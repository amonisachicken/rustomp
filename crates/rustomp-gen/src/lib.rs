// SPDX-License-Identifier: GPL-3.0-or-later

//! **插件侧代码生成**（S1c）：`rustomp.toml` → `plugin/src/generated/*.rs`。
//!
//! 生成什么：
//!
//! | 文件 | 内容 |
//! |---|---|
//! | `identity.rs` | 名字 / CLAP id / 三套 VST3 id（变体差异用 `cfg` 选择，文件是静态的） |
//! | `params.rs` | `Params` 结构体 + `Default` + `to_dsp()`（拍快照给电路） |
//! | `plugin_impl.rs` | `Plugin` / `ClapPlugin` / `Vst3Plugin` + 导出宏 + 界面钩子 |
//! | `mod.rs` | 汇总 |
//!
//! 不生成什么：**界面**。`editor()` 在没有 `gui` feature 时返回 `None`；
//! 要做界面时在手写的 `src/editor.rs` 里实现 `make(...)` 并开启 `gui` feature。
//!
//! 生成器**不懂 DSP**：`process` 只是"取参数快照 → `Engine::set_params` → `process` → 写活动量"。

pub mod emit;
pub mod manifest;

use std::path::{Path, PathBuf};

pub use manifest::{validate, Manifest};

/// 生成结果。
#[derive(Debug, Default)]
pub struct Report {
    /// 写出的文件（相对项目根）
    pub written: Vec<PathBuf>,
    /// 内容已是最新、无需改写的文件
    pub unchanged: Vec<PathBuf>,
    /// `--check` 模式下与磁盘不一致的文件
    pub drifted: Vec<PathBuf>,
}

impl Report {
    pub fn is_drift(&self) -> bool {
        !self.drifted.is_empty()
    }
}

/// 读 `<project>/rustomp.toml`。
pub fn load(project: &Path) -> Result<Manifest, String> {
    let path = project.join("rustomp.toml");
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("读不到 {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("解析 {} 失败：{e}", path.display()))
}

/// 生成到 `<project>/plugin/src/generated/`。
///
/// `check = true` 时只比对不写入（CI 防漂移）。
pub fn generate(project: &Path, check: bool) -> Result<Report, String> {
    let manifest = load(project)?;
    let errs = validate(&manifest);
    if !errs.is_empty() {
        return Err(format!(
            "rustomp.toml 校验失败：\n  - {}",
            errs.join("\n  - ")
        ));
    }

    // 素材文件必须真的在（否则 include_bytes! 会在 cargo 阶段才炸，且报错难懂）
    for (key, rel) in &manifest.assets {
        let path = project.join(rel);
        if !path.is_file() {
            return Err(format!(
                "[assets] {key} = \"{rel}\" 指向的文件不存在：{}",
                path.display()
            ));
        }
    }

    let dir = project.join("plugin/src/generated");
    let assets_rs = emit::assets_rs(&manifest);
    let mut files = vec![
        ("identity.rs", emit::identity_rs(&manifest)),
        ("params.rs", emit::params_rs(&manifest)),
        ("plugin_impl.rs", emit::plugin_impl_rs(&manifest)),
        (
            "mod.rs",
            emit::mod_rs(manifest.build.gui, !manifest.assets.is_empty()),
        ),
    ];
    if !assets_rs.is_empty() {
        files.push(("assets.rs", assets_rs));
    }
    if manifest.build.gui {
        // 界面相关：参数桥（[[params]] 派生）与面板声明（[ui] 派生）
        files.push(("bridge.rs", emit::bridge_rs(&manifest)));
        match emit::panel_rs(&manifest)? {
            Some(panel) => files.push(("panel.rs", panel)),
            None => {
                return Err(
                    "[build] gui = true 但 rustomp.toml 里没有 [ui] 段（面板声明无处来）".into(),
                )
            }
        }
    }

    let mut report = Report::default();
    for (name, content) in files {
        let path = dir.join(name);
        let rel = path.strip_prefix(project).unwrap_or(&path).to_path_buf();
        let old = std::fs::read_to_string(&path).ok();
        if old.as_deref() == Some(content.as_str()) {
            report.unchanged.push(rel);
            continue;
        }
        if check {
            report.drifted.push(rel);
            continue;
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("建目录失败：{e}"))?;
        std::fs::write(&path, content).map_err(|e| format!("写 {} 失败：{e}", path.display()))?;
        report.written.push(rel);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = r#"
[plugin]
package = "demo-plug"
name = "Demo Plug"
vendor = "HOP"
clap_id = "com.x.demo"
vst3_class_id = "DemoPlugIn000001"

[circuit]
plugin_type = "Gain"

[[params]]
id = "gain"
name = "Gain"
type = "float"
default = 0.5
circuit = "gain"
"#;

    /// 在临时目录里建一个最小项目；用完删掉。
    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn project() -> Tmp {
        let dir = std::env::temp_dir().join(format!(
            "rustomp-gen-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugin/src")).unwrap();
        std::fs::write(dir.join("rustomp.toml"), DEMO).unwrap();
        Tmp(dir)
    }

    #[test]
    fn writes_then_reports_unchanged() {
        let p = project();
        let r1 = generate(&p.0, false).unwrap();
        assert_eq!(r1.written.len(), 4, "{r1:?}");
        assert!(p.0.join("plugin/src/generated/plugin_impl.rs").is_file());
        // 再生成一次：内容一样 → unchanged，不该重写
        let r2 = generate(&p.0, false).unwrap();
        assert!(r2.written.is_empty());
        assert_eq!(r2.unchanged.len(), 4);
    }

    #[test]
    fn check_mode_detects_drift() {
        let p = project();
        generate(&p.0, false).unwrap();
        // 干净时 check 应通过
        assert!(!generate(&p.0, true).unwrap().is_drift());
        // 手改一个生成物 → check 必须报漂移
        let f = p.0.join("plugin/src/generated/params.rs");
        std::fs::write(&f, "// 手改过了\n").unwrap();
        let r = generate(&p.0, true).unwrap();
        assert!(r.is_drift());
        assert_eq!(r.drifted.len(), 1);
        // check 模式不能写盘
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "// 手改过了\n");
    }

    #[test]
    fn invalid_manifest_is_rejected() {
        let p = project();
        std::fs::write(
            p.0.join("rustomp.toml"),
            DEMO.replace("DemoPlugIn000001", "short"),
        )
        .unwrap();
        let err = generate(&p.0, false).unwrap_err();
        assert!(err.contains("16 字节"), "{err}");
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//
// 从插件项目的 rustomp.toml 生成 identity.rs 与 panel.rs。

use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let toml = format!("{manifest}/../rustomp.toml");
    let tool = format!("{manifest}/../../../tools/meta/plugin_meta.py");
    println!("cargo:rerun-if-changed={toml}");
    println!("cargo:rerun-if-changed={tool}");

    let variant = if std::env::var("CARGO_FEATURE_MONO").is_ok() {
        "mono"
    } else if std::env::var("CARGO_FEATURE_STEREO").is_ok() {
        "stereo"
    } else {
        "dual"
    };

    let run = |mode: &str| -> Vec<u8> {
        let mut cmd = Command::new("python3");
        cmd.arg(&tool).arg(&toml).arg(mode);
        if mode == "--rust" {
            cmd.arg("--variant").arg(variant);
        }
        let out = cmd.output().expect("运行 tools/meta/plugin_meta.py 失败（需要 python3 ≥3.11）");
        if !out.status.success() {
            panic!(
                "plugin_meta.py {mode} 失败：\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        out.stdout
    };

    let dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    std::fs::write(format!("{dir}/identity.rs"), run("--rust")).expect("写入 identity.rs 失败");
    std::fs::write(format!("{dir}/panel.rs"), run("--panel-rust")).expect("写入 panel.rs 失败");
}

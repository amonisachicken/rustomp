// SPDX-License-Identifier: GPL-3.0-or-later

//! `rustomp.toml` 的模型（只覆盖代码生成需要的字段）。
//!
//! 解析用 serde；**校验比解析更重要** —— 生成物会直接进发布路径，
//! 所以能在生成阶段挡住的错误（VST3 id 长度、参数 id 重复…）一律在这里报。

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub plugin: PluginMeta,
    #[serde(default)]
    pub au: Option<AuMeta>,
    #[serde(default)]
    pub build: BuildMeta,
    #[serde(default)]
    pub circuit: CircuitMeta,
    #[serde(default)]
    pub params: Vec<ParamMeta>,
    /// 存在即表示"有 GUI"（面板生成属 S1d，当前只是记录）
    #[serde(default)]
    pub ui: Option<toml::Value>,
    /// 素材：键名 → 相对项目根的路径（`panel` 是面板底图，其余是贴图）。
    /// BTreeMap 保证生成顺序稳定（可 diff）。
    #[serde(default)]
    pub assets: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginMeta {
    /// cargo 包名（小写短横线）
    pub package: String,
    /// 显示名
    pub name: String,
    pub vendor: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub email: String,
    pub clap_id: String,
    /// 恰好 16 字节；下面两个变体 id 可选
    pub vst3_class_id: String,
    #[serde(default)]
    pub vst3_class_id_mono: String,
    #[serde(default)]
    pub vst3_class_id_stereo: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuMeta {
    #[serde(default)]
    pub manufacturer_code: String,
    #[serde(default)]
    pub manufacturer_name: String,
    #[serde(default)]
    pub bundle_id: String,
    #[serde(default)]
    pub subtype_dual: String,
    #[serde(default)]
    pub subtype_mono: String,
    #[serde(default)]
    pub subtype_stereo: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildMeta {
    #[serde(default = "default_variants")]
    pub variants: Vec<String>,
    #[serde(default)]
    pub dual_variant: bool,
    /// 是否有 GUI。默认 false —— 这一阶段（进 GUI 之前）生成的是无界面插件。
    #[serde(default)]
    pub gui: bool,
}

impl Default for BuildMeta {
    fn default() -> Self {
        Self {
            variants: default_variants(),
            dual_variant: false,
            gui: false,
        }
    }
}

fn default_variants() -> Vec<String> {
    vec!["mono".into(), "stereo".into()]
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircuitMeta {
    /// 电路 crate 里实现 `rustomp_dsp::Circuit` 的类型名
    #[serde(default = "default_plugin_type")]
    pub plugin_type: String,
    /// 电路 crate 里的参数类型名
    #[serde(default = "default_params_type")]
    pub params_type: String,
    /// 电路 crate 名（默认由 `package` 派生：`<package>_circuit`）
    #[serde(default)]
    pub crate_name: String,
    /// 参数构造函数名（可选）。
    ///
    /// 设了就**不再**生成结构体字面量，而是生成
    /// `circuit_crate::<params_builder>(<按 manifest 顺序的所有参数>)`。
    /// 用于"分支电路"（`BranchParams { a, b, select_b }`）这类**不是平坦结构体**的参数。
    #[serde(default)]
    pub params_builder: String,
}

impl Default for CircuitMeta {
    fn default() -> Self {
        Self {
            plugin_type: default_plugin_type(),
            params_type: default_params_type(),
            crate_name: String::new(),
            params_builder: String::new(),
        }
    }
}

fn default_plugin_type() -> String {
    "Gain".into()
}
fn default_params_type() -> String {
    "Params".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    Float,
    Bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamFormat {
    Percent,
    Db,
    Plain,
    Enum,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamMeta {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ParamKind,
    /// float: 0..1 的位置；bool: true/false
    #[serde(default)]
    pub default: Option<toml::Value>,
    /// float 的取值范围（默认 0..1）
    #[serde(default)]
    pub range: Option<[f64; 2]>,
    #[serde(default)]
    pub format: Option<ParamFormat>,
    /// 平滑时间（毫秒）；float 默认 20，bool 无
    #[serde(default)]
    pub smoothing_ms: Option<f64>,
    /// 对应电路参数里的字段名（不填 = 只给界面用，不进 DSP）
    #[serde(default)]
    pub circuit: Option<String>,
    /// enum 显示名
    #[serde(default)]
    pub labels: Option<Vec<String>>,
}

// ---------------------------------------------------------------- 派生命名

impl Manifest {
    /// `sparklin-drive` → `sparklin_drive`
    pub fn crate_ident(&self) -> String {
        self.plugin.package.replace('-', "_")
    }

    /// 电路 crate 名：`sparklin_drive_circuit`
    pub fn circuit_crate(&self) -> String {
        if !self.circuit.crate_name.is_empty() {
            return self.circuit.crate_name.clone();
        }
        format!("{}_circuit", self.crate_ident())
    }

    /// `sparklin-drive` → `SparklinDrive`（Rust 类型名）
    pub fn struct_name(&self) -> String {
        self.plugin
            .package
            .split('-')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }

    pub fn vst3_id(&self, variant: &str) -> &str {
        match variant {
            "mono" if !self.plugin.vst3_class_id_mono.is_empty() => &self.plugin.vst3_class_id_mono,
            "stereo" if !self.plugin.vst3_class_id_stereo.is_empty() => {
                &self.plugin.vst3_class_id_stereo
            }
            _ => &self.plugin.vst3_class_id,
        }
    }

    /// 变体后缀：显示名追加、CLAP id 追加。
    pub fn variant_suffix(variant: &str) -> (&'static str, &'static str) {
        match variant {
            "mono" => (" Mono", ".mono"),
            "stereo" => (" Stereo", ".stereo"),
            _ => ("", ""),
        }
    }

    /// 实际会被生成的变体（恒含 dual，除非 `dual_variant = false` 且显式只要 mono/stereo）。
    pub fn variants(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.build.dual_variant || self.build.variants.is_empty() {
            out.push("dual".to_string());
        }
        for v in &self.build.variants {
            if v == "mono" || v == "stereo" {
                out.push(v.clone());
            }
        }
        out
    }

    /// `[ui]` 里的窗口尺寸（无 GUI 时给个默认值）。
    pub fn window_size(&self) -> (u32, u32) {
        let get = |k: &str| -> u32 {
            self.ui
                .as_ref()
                .and_then(|ui| ui.get(k))
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as u32
        };
        let w = get("window_width");
        let h = get("window_height");
        (if w == 0 { 400 } else { w }, if h == 0 { 600 } else { h })
    }

    /// 只含电路映射的参数（进 DSP 的那些）
    pub fn dsp_params(&self) -> Vec<&ParamMeta> {
        self.params.iter().filter(|p| p.circuit.is_some()).collect()
    }
}

// ---------------------------------------------------------------- 校验

/// 生成前的静态检查。返回全部问题（不是只报第一个），便于一次改完。
pub fn validate(m: &Manifest) -> Vec<String> {
    let mut errs = Vec::new();

    if m.plugin.package.is_empty()
        || !m
            .plugin
            .package
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        errs.push(format!(
            "[plugin] package 只能是小写字母/数字/短横线：{:?}",
            m.plugin.package
        ));
    }
    if m.plugin.name.trim().is_empty() {
        errs.push("[plugin] name 不能为空".into());
    }
    if m.plugin.vendor.trim().is_empty() {
        errs.push("[plugin] vendor 不能为空".into());
    }

    // VST3 id：恰好 16 字节
    for v in m.variants() {
        let id = m.vst3_id(&v);
        if id.len() != 16 {
            errs.push(format!(
                "VST3 class id（{v}）必须恰好 16 字节，现在是 {}：{:?}",
                id.len(),
                id
            ));
        }
    }

    // 参数 id 唯一 + 合法
    let mut seen = std::collections::BTreeSet::new();
    for p in &m.params {
        if p.id.is_empty()
            || !p
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            errs.push(format!("参数 id 只能是小写字母/数字/下划线：{:?}", p.id));
        }
        if !seen.insert(p.id.clone()) {
            errs.push(format!("参数 id 重复：{}", p.id));
        }
        if p.name.trim().is_empty() {
            errs.push(format!("参数 {} 的 name 不能为空", p.id));
        }
        if let Some(r) = p.range {
            if !(r[0] < r[1]) {
                errs.push(format!("参数 {} 的 range 非法：{r:?}", p.id));
            }
        }
        if p.kind == ParamKind::Bool && p.range.is_some() {
            errs.push(format!("参数 {} 是 bool，不应该有 range", p.id));
        }
        if let Some(c) = &p.circuit {
            if !c
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
            {
                errs.push(format!("参数 {} 的 circuit 字段名不合法：{:?}", p.id, c));
            }
        }
    }

    // 电路映射字段唯一（同一个 DSP 字段不能被两个参数写）
    let mut mapped = std::collections::BTreeMap::new();
    for p in m.dsp_params() {
        let field = p.circuit.clone().unwrap();
        if let Some(prev) = mapped.insert(field.clone(), p.id.clone()) {
            errs.push(format!(
                "电路字段 {field} 被多个参数映射：{prev} 与 {}",
                p.id
            ));
        }
    }

    if m.build.gui && m.ui.is_none() {
        errs.push("[build] gui = true 但缺少 [ui] 段".into());
    }

    errs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Manifest {
        toml::from_str(
            r#"
[plugin]
package = "demo"
name = "Demo"
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
"#,
        )
        .expect("示例 manifest 应该能解析")
    }

    #[test]
    fn derives_names() {
        let m = base();
        assert_eq!(m.crate_ident(), "demo");
        assert_eq!(m.circuit_crate(), "demo_circuit");
        assert_eq!(m.struct_name(), "Demo");
        assert_eq!(m.dsp_params().len(), 1);
    }

    #[test]
    fn kebab_case_struct_name() {
        let mut m = base();
        m.plugin.package = "sparklin-drive".into();
        assert_eq!(m.struct_name(), "SparklinDrive");
        assert_eq!(m.circuit_crate(), "sparklin_drive_circuit");
    }

    #[test]
    fn rejects_short_vst3_id() {
        let mut m = base();
        m.plugin.vst3_class_id = "short".into();
        let errs = validate(&m);
        assert!(errs.iter().any(|e| e.contains("16 字节")), "{errs:?}");
    }

    #[test]
    fn rejects_duplicate_param_ids_and_fields() {
        let mut m = base();
        let p = m.params[0].clone();
        m.params.push(p.clone());
        let errs = validate(&m);
        assert!(errs.iter().any(|e| e.contains("重复")), "{errs:?}");

        // 同一电路字段被两个不同参数映射
        let mut m = base();
        let mut p2 = m.params[0].clone();
        p2.id = "gain2".into();
        m.params.push(p2);
        let errs = validate(&m);
        assert!(errs.iter().any(|e| e.contains("电路字段")), "{errs:?}");
    }
}

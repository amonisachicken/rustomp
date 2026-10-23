// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 框架的**外部电路导入**。
//!
//! 设计：**先归一化到一份中间表示（IR），再由 IR 派生产物** ——
//!
//! ```text
//!  .bread / EasyEDA / KiCad / SPICE ...
//!        │  Importer（可插拔）
//!        ▼
//!   ImportedCircuit  ← 中间表示：元件 + 节点 + 值 + 模型
//!        ├── to_spice()     → ngspice 网表（喂给 rustomp-oracle 对拍）
//!        ├── to_skeleton()  → Rust 电路骨架（写 Circuit 实现的起点）
//!        └── to_json()      → 给其它工具消费
//! ```
//!
//! 这样每新增一种格式，只需要实现一个 [`Importer`]，下游全部复用。

use std::path::Path;

mod breadspice;

pub use breadspice::BreadSpice;

// ---------------------------------------------------------------- 中间表示

/// 元件类别（决定 SPICE 器件首字母与引脚顺序约定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Npn,
    Pnp,
    Jfet,
    Nmos,
    Pmos,
    /// 子电路（运放等），引脚顺序按 `X<name> <in+> <in-> <v+> <v-> <out> <model>` 约定
    Subckt,
    /// 电压源（`value` 已是完整的 SPICE 值表述）
    VoltageSource,
    Other,
}

impl ElementKind {
    pub fn spice_prefix(self) -> &'static str {
        match self {
            ElementKind::Resistor => "R",
            ElementKind::Capacitor => "C",
            ElementKind::Inductor => "L",
            ElementKind::Diode => "D",
            ElementKind::Npn | ElementKind::Pnp => "Q",
            ElementKind::Jfet => "J",
            ElementKind::Nmos | ElementKind::Pmos => "M",
            ElementKind::Subckt => "X",
            ElementKind::VoltageSource => "V",
            ElementKind::Other => "X",
        }
    }
}

/// 一个电路元件。
#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub kind: ElementKind,
    /// 按 SPICE 顺序排好的节点名（已把接地网换成 `0`）
    pub nodes: Vec<String>,
    /// 元件值（电阻 10k、电容 47n、电源 9 等）；子电路类为空
    pub value: String,
    /// SPICE 模型名（1N4148 / OP07A / …）
    pub model: Option<String>,
}

/// 一次导入的结果：中间表示。
#[derive(Debug, Clone)]
pub struct ImportedCircuit {
    pub name: String,
    /// 格式标识（例如 "breadspice"）
    pub format: &'static str,
    pub elements: Vec<Element>,
    /// 被判定为「地」的网（已映射成节点 `0`）
    pub ground_nets: Vec<String>,
    /// 导入过程中的提示/待人工确认项
    pub notes: Vec<String>,
}

impl ImportedCircuit {
    /// 生成 SPICE 网表**元件行**（不含分析语句与 `.end`，由调用方追加）。
    pub fn to_spice(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("* {} — imported from {}\n", self.name, self.format));
        for n in &self.notes {
            out.push_str(&format!("* note: {n}\n"));
        }
        if !self.ground_nets.is_empty() {
            out.push_str(&format!("* ground nets: {}\n", self.ground_nets.join(", ")));
        }
        for e in &self.elements {
            let nodes = e.nodes.join(" ");
            let tail = match (&e.model, e.value.as_str()) {
                (Some(m), _) => m.clone(),
                (None, v) if !v.is_empty() => v.to_string(),
                _ => String::new(),
            };
            out.push_str(&format!(
                "{}{} {} {}\n",
                e.kind.spice_prefix(),
                e.name,
                nodes,
                tail
            ));
        }
        out
    }

    /// 生成 Rust 电路骨架：把元件表作为数据给出，并标出待实现处。
    pub fn to_skeleton(&self) -> String {
        let mut out = String::new();
        out.push_str("// SPDX-License-Identifier: GPL-3.0-or-later\n");
        out.push_str(&format!("//! {} —— 由 rustomp-import 从 {} 生成。\n", self.name, self.format));
        out.push_str("//!\n//! 这不是可直接运行的实现：请把下面的元件表翻译成\n");
        out.push_str("//! `rustomp_dsp::Circuit` 的实现（线性用伴随模型 / 非线性用牛顿）。\n\n");
        out.push_str("use rustomp_dsp::Circuit;\n\n");
        out.push_str(&format!("pub const NAME: &str = {:?};\n\n", self.name));
        out.push_str("/// 元件表（节点名与导入结果一致）。\n");
        out.push_str("pub const ELEMENTS: &[(&str, &str, &[&str], &str)] = &[\n");
        for e in &self.elements {
            let nodes: Vec<String> = e.nodes.iter().map(|n| format!("{n:?}")).collect();
            let value = if let Some(m) = &e.model { m.clone() } else { e.value.clone() };
            out.push_str(&format!(
                "    ({:?}, {:?}, &[{}], {:?}),\n",
                e.name,
                e.kind.spice_prefix(),
                nodes.join(", "),
                value
            ));
        }
        out.push_str("];\n\n");
        if !self.notes.is_empty() {
            out.push_str("// 待确认：\n");
            for n in &self.notes {
                out.push_str(&format!("// - {n}\n"));
            }
        }
        out.push_str("\n// TODO: 实现 Circuit 的 new / set_sample_rate / set_params / reset / process。\n");
        out
    }

    /// 供其它工具消费的 JSON（手写，避免额外依赖）。
    pub fn to_json(&self) -> String {
        let mut s = String::from("{\n");
        s.push_str(&format!("  \"name\": {:?},\n", self.name));
        s.push_str(&format!("  \"format\": {:?},\n", self.format));
        s.push_str(&format!("  \"ground_nets\": {:?},\n", self.ground_nets));
        s.push_str(&format!("  \"notes\": {:?},\n", self.notes));
        s.push_str("  \"elements\": [\n");
        for (i, e) in self.elements.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"name\": {:?}, \"kind\": {:?}, \"nodes\": {:?}, \"value\": {:?}, \"model\": {:?}}}{}\n",
                e.name,
                e.kind.spice_prefix(),
                e.nodes,
                e.value,
                e.model,
                if i + 1 == self.elements.len() { "" } else { "," }
            ));
        }
        s.push_str("  ]\n}\n");
        s
    }

    /// 元件数量统计（导入日志用）。
    pub fn summary(&self) -> String {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for e in &self.elements {
            let k = e.kind.spice_prefix().to_string();
            match counts.iter_mut().find(|(n, _)| *n == k) {
                Some((_, c)) => *c += 1,
                None => counts.push((k, 1)),
            }
        }
        counts
            .iter()
            .map(|(k, c)| format!("{k}×{c}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// ---------------------------------------------------------------- 可插拔导入器

#[derive(Debug)]
pub enum ImportError {
    UnsupportedFormat(String),
    Parse(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::UnsupportedFormat(s) => write!(f, "不支持的电路格式：{s}"),
            ImportError::Parse(s) => write!(f, "解析失败：{s}"),
        }
    }
}
impl std::error::Error for ImportError {}

/// 一种外部电路格式的导入器。
pub trait Importer {
    /// 格式标识，例如 `"breadspice"`
    fn id(&self) -> &'static str;
    /// 是否能处理该文件（按扩展名/内容嗅探）
    fn matches(&self, path: &Path, content: &str) -> bool;
    /// 解析成中间表示
    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError>;
}

/// 已注册的导入器（新增格式只需在这里加一行）。
pub fn registry() -> &'static [&'static dyn Importer] {
    &[&BreadSpice]
}

/// 按内容自动选择导入器。
pub fn import_file(path: &Path) -> Result<ImportedCircuit, ImportError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ImportError::Parse(format!("读不到 {}: {e}", path.display())))?;
    for imp in registry() {
        if imp.matches(path, &content) {
            return imp.import(path, &content);
        }
    }
    Err(ImportError::UnsupportedFormat(format!(
        "{}（扩展名 {}）：没有匹配的导入器",
        path.display(),
        path.extension().and_then(|e| e.to_str()).unwrap_or("?")
    )))
}

// ---------------------------------------------------------------- 工具函数

/// 数值 + 单位 → SPICE 值（`4.7` + `kΩ` → `4.7k`）。
pub fn spice_value(value: &str, unit: Option<&str>) -> String {
    let unit = match unit {
        Some(u) => u,
        None => return value.trim().to_string(),
    };
    // 中文/希腊字符要按完整字符串匹配
    let suffix = match unit.trim() {
        "Ω" | "ohm" => "",
        "kΩ" | "k" => "k",
        "MΩ" | "meg" => "Meg",
        "GΩ" => "G",
        "pF" => "p",
        "nF" => "n",
        "µF" | "uF" | "μF" => "u",
        "mF" => "m",
        "F" => "",
        "µH" | "uH" | "μH" => "u",
        "mH" => "m",
        "H" => "",
        "V" => "",
        "mV" => "m",
        _ => return format!("{}{}", value.trim(), unit.trim()),
    };
    let v = value.trim().replace(',', ".");
    format!("{v}{suffix}")
}

/// 把元件名规范成 SPICE 引用名（保留字母数字下划线）。
pub fn sanitize_refdes(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// 网的 id → SPICE 节点名（不能以数字开头，非字母数字换成下划线）。
pub fn node_name(net: &str) -> String {
    let s: String = net
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    format!("n_{s}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spice_values() {
        assert_eq!(spice_value("4.7", Some("kΩ")), "4.7k");
        assert_eq!(spice_value("47", Some("nF")), "47n");
        assert_eq!(spice_value("1", Some("µF")), "1u");
        assert_eq!(spice_value("500", Some("kΩ")), "500k");
        assert_eq!(spice_value("9", Some("V")), "9");
        assert_eq!(spice_value("4,7", Some("kΩ")), "4.7k");
    }

    #[test]
    fn imports_the_example_circuit() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/sparklin-drive/circuit/netlist/fakedist.bread");
        let c = import_file(&path).expect("导入 fakedist.bread 失败");
        assert_eq!(c.format, "breadspice");
        assert_eq!(c.elements.len(), 22, "元件数不对：{}", c.summary());
        assert!(c.ground_nets.contains(&"rail_Lm".to_string()));
        let spice = c.to_spice();
        // 运放按 IN+/IN-/V+/V-/OUT 排序
        assert!(spice.contains("XU1 n_t11L n_t10L n_t10R n_t12L n_t11R OP07A"), "{spice}");
        // 电位器 50% → 两段 250k
        assert!(spice.contains("RP1A n_t10L n_t1L 250k"), "{spice}");
        assert!(spice.contains("RP2B n_t27R n_t24R 250k"), "{spice}");
        // 三只二极管的方向保留
        assert!(spice.contains("DD1 n_t1L n_t10L 1N4148"), "{spice}");
        assert!(spice.contains("DD3 n_t10L n_t5L 1N4148"), "{spice}");
        // 接地网映射到节点 0
        assert!(spice.contains("VB1 n_rail_Lp 0 9"), "{spice}");
        // 骨架里应包含元件表
        let sk = c.to_skeleton();
        assert!(sk.contains("pub const ELEMENTS"));
        assert!(sk.contains("\"Q\"") == false); // 本例没有三极管
    }

    #[test]
    fn node_names_are_safe() {
        assert_eq!(node_name("t11L"), "n_t11L");
        assert_eq!(node_name("rail_Lp"), "n_rail_Lp");
        assert_eq!(node_name("a-b.c"), "n_a_b_c");
    }
}

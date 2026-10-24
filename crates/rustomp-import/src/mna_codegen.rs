// SPDX-License-Identifier: GPL-3.0-or-later

//! **网表 → MNA 装配代码**（S1a）。
//!
//! 输入是 [`ImportedCircuit`]（SPICE 网表经 `spice` 导入器得到），输出是一段 Rust：
//! 节点编号常量 + `build(fs) -> Netlist<N>`。产物是**确定的**（同输入同输出），
//! 允许入库并做 `--check` 漂移检测。
//!
//! 做的事：
//! 1. **导线合并**：阻值 ≤ `WIRE_OHMS` 的电阻视为理想导线，把两端节点并成一个
//!    （breadspice 会把面包板的连接线生成成 `0.001 Ω` 电阻）；
//! 2. **节点编号**：地 = 0，其余按名字排序（保证 diff 稳定）；
//! 3. **模型映射**：闭集型号表（`1N4148` → `D_1N4148` …），未命中直接报错，
//!    不做静默兜底（ADR-013：替换必须可见）；
//! 4. **激励**：纯 DC 电压源 → **固定电位节点**；带 `SIN`/`AC` 的源 → **输入端口**；
//! 5. **运放子电路**：按 `<in+> <in-> <v+> <v-> <out>` 的引脚约定映射成库里的宏模型。

use crate::{ElementKind, ImportedCircuit};
use std::collections::{BTreeMap, BTreeSet};

/// 低于等于这个阻值的电阻视为导线（breadspice 的 `WIRE` 是 0.001 Ω）。
pub const WIRE_OHMS: f64 = 0.01;

/// 未知量上限（`Netlist<N>` 的 `N` 是编译期常量）。
const MAX_UNKNOWNS: usize = 64;

#[derive(Debug, Clone)]
pub struct Options {
    /// 输入端口对应的电压源名（默认：带 `SIN`/`AC` 的那个源）
    pub input: Option<String>,
    /// 输出节点名（默认：从 `* @rustomp out=…` 指令读）
    pub output: Option<String>,
    /// 生成代码顶部的说明（例如网表路径）
    pub source: String,
}

impl Default for Options {
    fn default() -> Self {
        Self { input: None, output: None, source: String::new() }
    }
}

/// 生成结果：代码 + 若干人类可读的提示。
#[derive(Debug)]
pub struct Generated {
    pub code: String,
    pub notes: Vec<String>,
    /// 未知量个数（= `Netlist<N>` 的 `N`）
    pub unknowns: usize,
}

/// 闭集型号表：SPICE 模型名 → (Rust 常量, 器件类别)
fn model_const(name: &str) -> Option<(&'static str, ModelKind)> {
    let key = name.trim().to_lowercase();
    let d = Some(("D_1N4148", ModelKind::Diode));
    match key.as_str() {
        "1n4148" => Some(("D_1N4148", ModelKind::Diode)),
        "1n34a" => Some(("D_1N34A", ModelKind::Diode)),
        "1n4007" => Some(("D_1N4007", ModelKind::Diode)),
        "1n5817" => Some(("D_1N5817", ModelKind::Diode)),
        "ledred" | "led_red" => Some(("LED_RED", ModelKind::Diode)),
        "ledgreen" | "led_green" => Some(("LED_GREEN", ModelKind::Diode)),
        "ledblue" | "led_blue" => Some(("LED_BLUE", ModelKind::Diode)),
        "op07" | "op07a" => Some(("OP07", ModelKind::OpAmp)),
        _ => d.filter(|_| false),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    Diode,
    OpAmp,
}

/// SPICE 数值（`4.7k` / `1meg` / `2.2u` / `4.7n`）→ f64。
pub fn parse_value(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    // 找第一个非"数字/小数点/指数"字符作为单位起点
    let mut end = lower.len();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let ok = c.is_ascii_digit()
            || c == '.'
            || ((c == 'e' || c == 'E') && i + 1 < bytes.len())
            || ((c == '+' || c == '-') && i > 0 && matches!(bytes[i - 1] as char, 'e' | 'E'));
        if !ok {
            end = i;
            break;
        }
        i += 1;
    }
    let num: f64 = lower[..end].parse().ok()?;
    let unit = lower[end..].trim();
    let mult = if unit.starts_with("meg") {
        1e6
    } else if unit.starts_with("mil") {
        25.4e-6
    } else {
        match unit.chars().next() {
            Some('t') => 1e12,
            Some('g') => 1e9,
            Some('k') => 1e3,
            Some('m') => 1e-3,
            Some('u') => 1e-6,
            Some('n') => 1e-9,
            Some('p') => 1e-12,
            Some('f') => 1e-15,
            _ => 1.0,
        }
    };
    Some(num * mult)
}

/// 并查集（导线合并用）
struct Union {
    parent: BTreeMap<String, String>,
}

impl Union {
    fn new() -> Self {
        Self { parent: BTreeMap::new() }
    }
    fn find(&mut self, x: &str) -> String {
        let p = self.parent.get(x).cloned().unwrap_or_else(|| x.to_string());
        if p == x {
            return p;
        }
        let root = self.find(&p);
        self.parent.insert(x.to_string(), root.clone());
        root
    }
    fn union(&mut self, a: &str, b: &str) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // 地优先当根，其余按字典序 —— 保证确定性
            let (root, child) = if ra == "0" || (rb != "0" && ra < rb) {
                (ra, rb)
            } else {
                (rb, ra)
            };
            self.parent.insert(child, root);
        }
    }
}

/// 把网表 IR 生成为 MNA 装配代码。
pub fn generate(circuit: &ImportedCircuit, opts: &Options) -> Result<Generated, String> {
    let mut notes = Vec::new();

    // ---- 1) 导线合并 ----
    let mut union = Union::new();
    for e in &circuit.elements {
        if e.kind == ElementKind::Resistor {
            if let Some(v) = parse_value(&e.value) {
                if v.abs() <= WIRE_OHMS {
                    union.union(&e.nodes[0], &e.nodes[1]);
                    notes.push(format!("{}（{} Ω）视为导线，合并两个节点", e.name, e.value));
                }
            }
        }
    }
    // 先把所有出现过的节点做一次彻底的路径压缩，之后只读
    let mut all_nodes: BTreeSet<String> = BTreeSet::new();
    for e in &circuit.elements {
        for n in &e.nodes {
            all_nodes.insert(n.clone());
        }
    }
    let canon_map: BTreeMap<String, String> = all_nodes
        .iter()
        .map(|n| (n.clone(), union.find(n)))
        .collect();
    let canon = |n: &str| -> String {
        canon_map.get(n).cloned().unwrap_or_else(|| n.to_string())
    };

    // ---- 2) 节点编号（地=0，其余按名字排序）----
    let mut names: BTreeSet<String> = BTreeSet::new();
    for e in &circuit.elements {
        for n in &e.nodes {
            names.insert(canon(n));
        }
    }
    names.remove("0");
    let mut node_index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, n) in names.iter().enumerate() {
        node_index.insert(n.clone(), i + 1);
    }
    let node_alias: BTreeMap<String, usize> = node_index
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();

    // 名字 → 编号（含地）
    let idx = |n: &str| -> usize {
        let c = canon(n);
        if c == "0" {
            0
        } else {
            *node_alias.get(&c).unwrap_or(&0)
        }
    };

    // ---- 3) 激励：DC 源 → 固定节点；SIN/AC 源 → 输入端口 ----
    let mut fixed_nodes: Vec<(usize, f64, String)> = Vec::new();
    let mut input_node: Option<(usize, String)> = None;
    for e in &circuit.elements {
        if e.kind != ElementKind::VoltageSource {
            continue;
        }
        if e.nodes[1] != "0" && canon(&e.nodes[1]) != "0" {
            return Err(format!(
                "{}：只支持接地的电压源（另一个节点必须是 0），这里是 {}",
                e.name, e.nodes[1]
            ));
        }
        let lower = e.value.to_lowercase();
        let is_signal = lower.contains("sin") || lower.contains("ac ") || lower.contains("pulse");
        let node = idx(&e.nodes[0]);
        if is_signal {
            if let Some((_, name)) = &input_node {
                return Err(format!(
                    "发现多个信号源（{} 和 {}）：请用 `* @rustomp in=<源名>` 指定输入",
                    name, e.name
                ));
            }
            input_node = Some((node, e.name.clone()));
            notes.push(format!("{} 作为输入端口（节点 {}）", e.name, e.nodes[0]));
        } else {
            let dc = parse_value(e.value.split_whitespace().last().unwrap_or(""))
                .ok_or_else(|| format!("{}：读不出直流电压 `{}`", e.name, e.value))?;
            fixed_nodes.push((node, dc, e.name.clone()));
        }
    }
    // 显式指定输入源时以它为准，并从固定节点里去掉
    if let Some(name) = &opts.input {
        let node = circuit
            .elements
            .iter()
            .find(|e| &e.name == name)
            .map(|e| idx(&e.nodes[0]))
            .ok_or_else(|| format!("找不到输入源 {name}"))?;
        fixed_nodes.retain(|(n, _, _)| *n != node);
        input_node = Some((node, name.clone()));
    }
    let (input_node, _) = input_node.ok_or_else(|| {
        "没找到输入源（带 SIN/AC/PULSE 的电压源），请用 `* @rustomp in=<源名>` 指定".to_string()
    })?;

    // ---- 4) 输出端口 ----
    let output_name = opts
        .output
        .clone()
        .or_else(|| {
            circuit
                .notes
                .iter()
                .find_map(|n| n.strip_prefix("@rustomp").and_then(|d| directive_value(d, "out")))
        })
        .ok_or_else(|| "没指定输出节点，请用 `* @rustomp out=<节点名>` 或 --output-node".to_string())?;
    let output_node = idx(&output_name);
    if output_node == 0 && canon(&output_name) == "0" {
        return Err(format!("输出节点 {output_name} 被合并到地了"));
    }

    // ---- 5) 元件 → 代码行 ----
    let mut body: Vec<String> = Vec::new();
    let mut branch = node_alias.values().copied().max().unwrap_or(0) + 1;
    let mut uses: BTreeSet<String> = BTreeSet::new();
    uses.insert("Netlist".into());
    uses.insert("GND".into());

    for e in &circuit.elements {
        match e.kind {
            ElementKind::Resistor => {
                let v = parse_value(&e.value)
                    .ok_or_else(|| format!("{}：读不出阻值 `{}`", e.name, e.value))?;
                if v.abs() <= WIRE_OHMS {
                    continue; // 已并节点
                }
                let (na, nb) = (idx(&e.nodes[0]), idx(&e.nodes[1]));
                if na == nb {
                    // 自环（电位器的"短接一半"就长这样）：对电路没有影响，跳过
                    notes.push(format!("{}（{} Ω）两端是同一个节点（自环），已跳过", e.name, e.value));
                    continue;
                }
                body.push(format!(
                    "    // {} ({} Ω) {}{}—{}{}",
                    e.name,
                    e.value,
                    if na == 0 { "*" } else { "" },
                    canon(&e.nodes[0]),
                    if nb == 0 { "*" } else { "" },
                    canon(&e.nodes[1]),
                ));
                body.push(format!(
                    "    net.resistor({}, {}, {});",
                    node_expr(na, &node_alias),
                    node_expr(nb, &node_alias),
                    fmt_f64(v)
                ));
            }
            ElementKind::Capacitor => {
                let v = parse_value(&e.value)
                    .ok_or_else(|| format!("{}：读不出容值 `{}`", e.name, e.value))?;
                body.push(format!(
                    "    // {} ({} F)\n    net.capacitor({}, {}, {});",
                    e.name,
                    e.value,
                    node_expr(idx(&e.nodes[0]), &node_alias),
                    node_expr(idx(&e.nodes[1]), &node_alias),
                    fmt_f64(v)
                ));
            }
            ElementKind::Diode => {
                let model = e
                    .model
                    .as_deref()
                    .ok_or_else(|| format!("{}：二极管没有模型名", e.name))?;
                let (const_name, kind) = model_const(model)
                    .ok_or_else(|| unknown_model_error(circuit, model, "二极管"))?;
                if kind != ModelKind::Diode {
                    return Err(format!("{}：模型 {model} 不是二极管", e.name));
                }
                uses.insert(const_name.into());
                body.push(format!(
                    "    // {} {} (阳极 {})\n    net.diode({}, {}, {});",
                    e.name,
                    model,
                    e.nodes[0],
                    const_name,
                    node_expr(idx(&e.nodes[0]), &node_alias),
                    node_expr(idx(&e.nodes[1]), &node_alias)
                ));
            }
            ElementKind::Subckt => {
                let model = e.model.as_deref().unwrap_or("");
                let (const_name, kind) =
                    model_const(model).ok_or_else(|| unknown_model_error(circuit, model, "子电路"))?;
                if kind != ModelKind::OpAmp {
                    return Err(format!("{}：子电路 {model} 目前只支持运放", e.name));
                }
                if e.nodes.len() < 5 {
                    return Err(format!(
                        "{}：运放子电路需要 5 个引脚 <in+> <in-> <v+> <v-> <out>，只有 {}",
                        e.name,
                        e.nodes.len()
                    ));
                }
                uses.insert(const_name.into());
                let br = branch;
                branch += 1;
                body.push(format!(
                    "    // {} {}（in+ in- v+ v- out）\n    net.opamp({}.model(1.0 / fs), {}, {}, {}, {});",
                    e.name,
                    model,
                    const_name,
                    node_expr(idx(&e.nodes[0]), &node_alias),
                    node_expr(idx(&e.nodes[1]), &node_alias),
                    node_expr(idx(&e.nodes[4]), &node_alias),
                    br
                ));
            }
            ElementKind::VoltageSource => { /* 已在第 3 步处理 */ }
            other => {
                return Err(format!(
                    "{}：暂不支持 {} 类元件（S1a 只覆盖 R/C/D/X/DC-V）",
                    e.name,
                    other.spice_prefix()
                ));
            }
        }
    }

    let unknowns = branch; // 节点 1..=max + 支路
    if unknowns > MAX_UNKNOWNS {
        return Err(format!("未知量 {unknowns} 超过上限 {MAX_UNKNOWNS}"));
    }

    // ---- 6) 输出 ----
    let mut code = String::new();
    code.push_str("// @generated by rustomp-import (netlist → MNA elements) — do not edit\n");
    // 注意：生成物会被 `include!` 进别的文件，所以只能用 `//` 而不是 `//!`（内层文档注释）
    code.push_str(&format!(
        "// 由 {} 生成。改动请改网表后重新生成。\n",
        if opts.source.is_empty() { "网表" } else { &opts.source }
    ));
    code.push_str("// 未知量 = 节点数 + 电压源支路数（地不占未知量）。\n\n");
    let mut use_list: Vec<String> = uses.iter().cloned().collect();
    use_list.sort();
    code.push_str(&format!("use rustomp_dsp::{{{}}};\n\n", use_list.join(", ")));
    code.push_str(&format!(
        "/// 未知量个数（`Netlist<N>` 的 N）\npub const N: usize = {unknowns};\n\n"
    ));
    code.push_str("/// 节点编号（0 = 地；其余按名字排序，保证 diff 稳定）\npub mod node {\n");
    for (name, index) in &node_alias {
        code.push_str(&format!(
            "    /// `{}`\n    pub const {}: usize = {};\n",
            name,
            const_name_for(name),
            index
        ));
    }
    code.push_str("}\n\n");
    code.push_str(&format!(
        "/// 输入端口节点（`process(x)` 用 `net.set_fixed(FIXED_IN, x)`）\npub const IN: usize = {};\n\
         /// 输出节点\npub const OUT: usize = {};\n",
        input_node, output_node
    ));
    // 固定节点按"电源轨在前、输入端在后"的顺序装配，所以输入端口的下标 = 电源轨个数
    let mut rails = fixed_nodes.clone();
    rails.sort_by(|a, b| a.0.cmp(&b.0));
    rails.dedup_by_key(|(n, _, _)| *n);
    code.push_str(&format!(
        "/// 输入端口在 `Netlist::fixed` 里的下标（电源轨排在它前面）\npub const FIXED_IN: usize = {};\n\n",
        rails.len()
    ));
    code.push_str("/// 按网表搭出电路（`fs` 是**内层**采样率，含过采样倍率）。\npub fn build(fs: f64) -> Netlist<N> {\n");
    code.push_str("    let mut net = Netlist::<N>::new();\n");
    for line in &body {
        code.push_str(line);
        code.push('\n');
    }
    // 固定节点：大电阻轨优先，顺序固定
    let mut fixed_sorted = fixed_nodes.clone();
    fixed_sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (node, dc, name) in &fixed_sorted {
        code.push_str(&format!(
            "    net.fix_node({}, {}); // {name}\n",
            node_expr(*node, &node_alias),
            fmt_f64(*dc)
        ));
    }
    code.push_str("    net.fix_node(IN, 0.0); // 输入端口，值由 process() 每拍写入\n");
    code.push_str("    net\n}\n");

    Ok(Generated { code, notes, unknowns })
}

fn directive_value(directive: &str, key: &str) -> Option<String> {
    directive
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(|v| v.to_string()))
}

fn const_name_for(node: &str) -> String {
    // n_t27R → N_T27R；纯数字/特殊字符前加 N_
    let mut out = String::new();
    for c in node.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
        out.insert_str(0, "N_");
    }
    out
}

fn node_expr(index: usize, alias: &BTreeMap<String, usize>) -> String {
    if index == 0 {
        return "GND".into();
    }
    // 找出这个名字对应的常量名
    for (name, i) in alias {
        if *i == index {
            return format!("node::{}", const_name_for(name));
        }
    }
    format!("{index}")
}

fn fmt_f64(v: f64) -> String {
    // `parse("4.7n")` 会得到 4.700000000000001e-9；先归一到 12 位有效数字去掉浮点噪声，
    // 再交给 `{:?}`（对 f64 往返精确，输出确定）。
    let rounded: f64 = format!("{v:.12e}").parse().unwrap_or(v);
    format!("{rounded:?}")
}

fn unknown_model_error(circuit: &ImportedCircuit, model: &str, kind: &str) -> String {
    let known: Vec<&str> = [
        "1N4148", "1N34A", "1N4007", "1N5817", "LedRed", "LedGreen", "LedBLUE", "OP07A",
    ]
    .to_vec();
    let mut msg = format!(
        "{kind}模型 `{model}` 不在闭集表里（已知：{}）",
        known.join(" / ")
    );
    let models: BTreeSet<&str> = circuit
        .elements
        .iter()
        .filter_map(|e| e.model.as_deref())
        .collect();
    msg.push_str(&format!("\n网表里出现的模型：{}", models.into_iter().collect::<Vec<_>>().join(", ")));
    msg.push_str("\n→ 要么换成库里的型号，要么先在 devices/ 里标定后加进 model_const()");
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spice_values() {
        let close = |a: Option<f64>, b: f64| (a.unwrap() - b).abs() < b.abs() * 1e-12;
        assert!(close(parse_value("4.7k"), 4700.0));
        assert!(close(parse_value("1meg"), 1e6));
        assert!(close(parse_value("2.2u"), 2.2e-6));
        assert!(close(parse_value("47n"), 47e-9));
        assert!(close(parse_value("9"), 9.0));
        assert!(close(parse_value("0.001"), 0.001));
        assert!(close(parse_value("1.5e3"), 1500.0));
        assert!(close(parse_value("100m"), 0.1));
        // 浮点噪声要在格式化时被消掉
        assert_eq!(fmt_f64(parse_value("4.7n").unwrap()), "4.7e-9");
        assert_eq!(fmt_f64(47000.0), "47000.0");
    }

    #[test]
    fn node_const_names_are_valid_rust() {
        assert_eq!(const_name_for("n_t27R"), "N_T27R");
        assert_eq!(const_name_for("in+"), "IN_");
        assert_eq!(const_name_for("1"), "N_1");
    }
}

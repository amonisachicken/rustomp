// SPDX-License-Identifier: GPL-3.0-or-later

//! **网表 → MNA 装配代码**（S1a/S1b）。
//!
//! 输入是 [`ImportedCircuit`]（SPICE 网表经 `spice` 导入器得到），输出是一段 Rust：
//! 节点常量 + `build()` + `Params` + 直接实现 `rustomp_dsp::Circuit` 的 `Ckt`。
//! 产物是**确定的**（同输入同输出），允许入库并做 `--check` 漂移检测。
//!
//! 做的事：
//! 1. **导线合并**：阻值 ≤ `WIRE_OHMS` 的电阻视为理想导线（breadspice 会把连线生成为 0.001 Ω）；
//! 2. **节点编号**：地 = 0，其余按名字排序（保证 diff 稳定）；
//! 3. **模型映射**：闭集型号表（`1N4148` → `D_1N4148`、`BC549C` → `BC549C` …），
//!    未命中直接报错，不做静默兜底（ADR-013：替换必须可见）；
//! 4. **激励**：纯 DC 电压源 → **固定电位节点**；带 `SIN`/`AC`/`PULSE` 的源 → **输入端口**；
//! 5. **参数化（S1b）**：`* @rustomp param <元素> <字段> pot max=<阻值> [invert]`、
//!    `* @rustomp switch <字段> <节点A> <节点B> closed=<r> open=<r>`；
//! 6. **运放/三极管**：运放按 `<in+> <in-> <v+> <v-> <out>` 引脚约定映射成库里的宏模型。
//!
//! 指令行（写在网表注释里）：
//!
//! ```text
//! * @rustomp in=VS1 out=n_t27R
//! * @rustomp param RP1A gain pot max=500k
//! * @rustomp param RP2B level pot max=500k invert
//! * @rustomp switch symmetric n_t10L n_t5L closed=0.001 open=1e12
//! ```

use crate::{ElementKind, ImportedCircuit};
use std::collections::{BTreeMap, BTreeSet};

/// 低于等于这个阻值的电阻视为导线（breadspice 的 `WIRE` 是 0.001 Ω）。
pub const WIRE_OHMS: f64 = 0.01;

/// 未知量上限（`Netlist<N>` 的 `N` 是编译期常量）。
const MAX_UNKNOWNS: usize = 64;

/// 实时路径默认的牛顿迭代次数。
///
/// **不能用 2**：方波/快速边沿下 2 次迭代会欠收敛，解进入次谐波极限环
/// （实测 880 Hz 方波时 f/8、f/16 分量高达 −0 dB，比基频只低 10 dB 出头）。
/// 3 次已足够（次谐波降到 −80 dB），这里取 3 作默认，需要余量可用 `newton=4`。
pub const DEFAULT_NEWTON_ITERS: usize = 3;

#[derive(Debug, Clone)]
pub struct Options {
    /// 输入端口对应的电压源名（默认：带 `SIN`/`AC`/`PULSE` 的那个源）
    pub input: Option<String>,
    /// 输出节点名（默认：从 `* @rustomp out=…` 指令读）
    pub output: Option<String>,
    /// 生成代码顶部的说明（例如网表文件名）
    pub source: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            source: String::new(),
        }
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
    let m = match key.as_str() {
        "1n4148" => ("D_1N4148", ModelKind::Diode),
        "1n34a" => ("D_1N34A", ModelKind::Diode),
        "1n4007" => ("D_1N4007", ModelKind::Diode),
        "1n5817" => ("D_1N5817", ModelKind::Diode),
        "ledred" | "led_red" => ("LED_RED", ModelKind::Diode),
        "ledgreen" | "led_green" => ("LED_GREEN", ModelKind::Diode),
        "ledblue" | "led_blue" => ("LED_BLUE", ModelKind::Diode),
        "op07" | "op07a" => ("OP07", ModelKind::OpAmp),
        // BJT（极性由常量名决定）
        "bc549c" => ("BC549C", ModelKind::BjtNpn),
        "bc559c" => ("BC559C", ModelKind::BjtPnp),
        "ac127" => ("AC127", ModelKind::BjtNpn),
        "ac128" => ("AC128", ModelKind::BjtPnp),
        "j201" => ("J201", ModelKind::Jfet),
        "2n7000" => ("NMOS_2N7000", ModelKind::Mosfet),
        _ => return None,
    };
    Some(m)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    Diode,
    OpAmp,
    BjtNpn,
    BjtPnp,
    Jfet,
    Mosfet,
}

/// SPICE 数值（`4.7k` / `1meg` / `2.2u` / `4.7n`）→ f64。
pub fn parse_value(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    let bytes = lower.as_bytes();
    let mut end = lower.len();
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

// ---------------------------------------------------------------- 指令

#[derive(Debug, Clone)]
struct PotParam {
    /// 网表里的电阻名（例如 `RP1A`）
    elem: String,
    /// 参数名（生成 `Params` 的字段）
    field: String,
    /// 电位器总阻值
    max: f64,
    /// true ⇒ `R = max·(1−t)`（分压器上臂），false ⇒ `R = max·t`
    invert: bool,
}

#[derive(Debug, Clone)]
struct SwitchSpec {
    field: String,
    node_a: String,
    node_b: String,
    closed: f64,
    open: f64,
}

#[derive(Debug, Default, Clone)]
struct Directives {
    input: Option<String>,
    output: Option<String>,
    /// 每样本的牛顿迭代次数（实时路径用）。**默认 3**：
    /// 2 次在方波/快速边沿下会欠收敛、进入次谐波极限环（DEVLOG E50）。
    newton: Option<usize>,
    /// 活动指示的参考电流（`* @rustomp activity_i=50u`）。**只驱动 GUI 的 LED**。
    /// 不同电路的二极管电流量级差很多（本电路 0.2 V 输入时只有 ~30 µA），所以必须可配。
    activity_i: Option<f64>,
    pots: Vec<PotParam>,
    switches: Vec<SwitchSpec>,
}

fn parse_directives(notes: &[String]) -> Result<Directives, String> {
    let mut d = Directives::default();
    for note in notes {
        let Some(rest) = note.strip_prefix("@rustomp") else {
            continue;
        };
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        // 形式一：`in=VS1 out=n_t27R`（键值对）
        if tokens.iter().any(|t| t.contains('=')) && !matches!(tokens[0], "param" | "switch") {
            for t in &tokens {
                if let Some(v) = t.strip_prefix("in=") {
                    d.input = Some(v.to_string());
                } else if let Some(v) = t.strip_prefix("out=") {
                    d.output = Some(v.to_string());
                } else if let Some(v) = t.strip_prefix("activity_i=") {
                    d.activity_i =
                        Some(parse_value(v).ok_or_else(|| format!("`activity_i={v}` 读不出数值"))?);
                } else if let Some(v) = t.strip_prefix("newton=") {
                    let n: usize = v.parse().map_err(|_| format!("`newton={v}` 不是整数"))?;
                    if !(2..=64).contains(&n) {
                        return Err(format!("newton={n} 不合理（允许 2..=64）"));
                    }
                    d.newton = Some(n);
                }
            }
            continue;
        }
        match tokens[0] {
            "param" => {
                // param <elem> <field> pot max=500k [invert]
                if tokens.len() < 5 || tokens[3] != "pot" {
                    return Err(format!(
                        "参数指令格式：`* @rustomp param <元素> <字段> pot max=<阻值> [invert]`，收到 `{}`",
                        rest.trim()
                    ));
                }
                let max = tokens[4..]
                    .iter()
                    .find_map(|t| t.strip_prefix("max="))
                    .and_then(parse_value)
                    .ok_or_else(|| format!("参数指令缺 `max=`：{rest}"))?;
                d.pots.push(PotParam {
                    elem: tokens[1].to_string(),
                    field: check_field(tokens[2])?,
                    max,
                    invert: tokens[4..].iter().any(|t| *t == "invert"),
                });
            }
            "switch" => {
                // switch <field> <nodeA> <nodeB> closed=0.001 open=1e12
                if tokens.len() < 4 {
                    return Err(format!(
                        "开关指令格式：`* @rustomp switch <字段> <节点A> <节点B> closed=<r> open=<r>`，收到 `{}`",
                        rest.trim()
                    ));
                }
                let kv = |key: &str| {
                    tokens
                        .iter()
                        .find_map(|t| t.strip_prefix(&format!("{key}=")))
                        .and_then(parse_value)
                };
                d.switches.push(SwitchSpec {
                    field: check_field(tokens[1])?,
                    node_a: tokens[2].to_string(),
                    node_b: tokens[3].to_string(),
                    closed: kv("closed").ok_or_else(|| format!("开关指令缺 `closed=`：{rest}"))?,
                    open: kv("open").ok_or_else(|| format!("开关指令缺 `open=`：{rest}"))?,
                });
            }
            other => return Err(format!("未知指令 `{other}`")),
        }
    }
    Ok(d)
}

fn check_field(name: &str) -> Result<String, String> {
    let ok = !name.is_empty()
        && name
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(name.to_string())
    } else {
        Err(format!("参数名 `{name}` 不是合法的 Rust 标识符"))
    }
}

// ---------------------------------------------------------------- 输入端口解析

/// 把 `in=<名>` 解析成**节点名**。
///
/// 两种写法都支持，因为两代设计器的习惯不同：
/// - **元件名**（breadspice 时代）：`* @rustomp in=VS1` —— 取那个电压源的正端节点；
/// - **节点名**（rustomp-circuit-designer）：`* @rustomp in=IN` —— 网表是纯拓扑，
///   没有激励源，输入节点由 `process(x)` 每拍写入。
fn resolve_input_name(
    circuit: &ImportedCircuit,
    name: &str,
    canon: &impl Fn(&str) -> String,
) -> Result<String, String> {
    if let Some(e) = circuit.elements.iter().find(|e| e.name == name) {
        if e.kind != ElementKind::VoltageSource {
            return Err(format!(
                "`in={name}` 命中元件 {}，但它不是电压源（输入端口必须是电压源或节点）",
                e.name
            ));
        }
        return Ok(e.nodes[0].clone());
    }

    let target = canon(name);
    if target == "0" {
        return Err(format!("`in={name}` 被合并到地（0），不能当输入端口"));
    }
    let known = circuit
        .elements
        .iter()
        .flat_map(|e| e.nodes.iter())
        .any(|n| canon(n) == target);
    if known {
        return Ok(name.to_string());
    }

    let sources = circuit
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::VoltageSource)
        .map(|e| e.name.clone())
        .collect::<Vec<_>>();
    Err(format!(
        "`in={name}` 既不是元件名也不是节点名（电压源：{}）",
        if sources.is_empty() {
            "无".to_string()
        } else {
            sources.join("、")
        }
    ))
}

// ---------------------------------------------------------------- 并查集

/// 每条运放子电路占一条电压源支路（生成前就要知道，用于排布节点编号）。
fn next_branch_estimate(circuit: &ImportedCircuit, _dirs: &Directives) -> usize {
    circuit
        .elements
        .iter()
        .filter(|e| {
            if e.kind != ElementKind::Subckt {
                return false;
            }
            model_const(e.model.as_deref().unwrap_or(""))
                .map(|(_, kind)| kind == ModelKind::OpAmp)
                .unwrap_or(false)
        })
        .count()
}

struct Union {
    parent: BTreeMap<String, String>,
}

impl Union {
    fn new() -> Self {
        Self {
            parent: BTreeMap::new(),
        }
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
        if ra == rb {
            return;
        }
        // 地优先当根，其余按字典序 —— 保证确定性
        let (root, child) = if ra == "0" || (rb != "0" && ra < rb) {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent.insert(child, root);
    }
}

// ---------------------------------------------------------------- 生成

/// 把网表 IR 生成为 MNA 装配代码。
pub fn generate(circuit: &ImportedCircuit, opts: &Options) -> Result<Generated, String> {
    let mut notes = Vec::new();
    let mut dirs = parse_directives(&circuit.notes)?;

    // 兼容老写法：`param` 里的元素名应是**网表里的元件名**（`RP1A`），
    // 但 rustomp-circuit-designer 早期版本写的是位号（`P1A`）。
    // 直接报错会让已经导出的网表全部失效，所以这里只补一层“加 R 前缀”的兼容。
    for p in &mut dirs.pots {
        if circuit.elements.iter().any(|e| e.name == p.elem) {
            continue;
        }
        let candidate = format!("R{}", p.elem);
        if circuit.elements.iter().any(|e| e.name == candidate) {
            notes.push(format!(
                "`param {}` 对应的是电阻 {}（老格式：指令里写的是位号）",
                p.elem, candidate
            ));
            p.elem = candidate;
        }
    }

    // ---- 1) 导线合并 ----
    let mut union = Union::new();
    for e in &circuit.elements {
        if e.kind == ElementKind::Resistor {
            // 被参数化的电阻即使是 0.001 Ω 也不是导线（开关的 closed 值会长这样）
            if dirs.pots.iter().any(|p| p.elem == e.name) {
                continue;
            }
            if let Some(v) = parse_value(&e.value) {
                if v.abs() <= WIRE_OHMS {
                    union.union(&e.nodes[0], &e.nodes[1]);
                    notes.push(format!("{}（{}）视为导线，合并两个节点", e.name, e.value));
                }
            }
        }
    }
    let mut all_nodes: BTreeSet<String> = BTreeSet::new();
    for e in &circuit.elements {
        for n in &e.nodes {
            all_nodes.insert(n.clone());
        }
    }
    for s in &dirs.switches {
        all_nodes.insert(s.node_a.clone());
        all_nodes.insert(s.node_b.clone());
    }
    let canon_map: BTreeMap<String, String> = all_nodes
        .iter()
        .map(|n| (n.clone(), union.find(n)))
        .collect();
    let canon = |n: &str| canon_map.get(n).cloned().unwrap_or_else(|| n.to_string());

    // ---- 2) 节点编号（地=0，其余按名字排序）----
    let mut names: BTreeSet<String> = BTreeSet::new();
    for e in &circuit.elements {
        for n in &e.nodes {
            names.insert(canon(n));
        }
    }
    for s in &dirs.switches {
        names.insert(canon(&s.node_a));
        names.insert(canon(&s.node_b));
    }
    names.remove("0");

    // 先找出"固定电位节点"对应的合并后节点名（电源轨、输入源），它们在编号时排最后：
    //   [未知节点 1..K] [电压源支路 K+1..K+B] [固定节点 K+B+1..]
    // 这样 LU 只需要处理前 K+B 行（固定节点的行列全零）—— 12 个未知量里砍掉 3 个尸体，
    // LU 代价 (9/12)³ ≈ 0.42（DEVLOG E57）。
    let mut fixed_names: BTreeSet<String> = BTreeSet::new();
    for e in &circuit.elements {
        if e.kind == ElementKind::VoltageSource {
            fixed_names.insert(canon(&e.nodes[0]));
        }
    }
    // 显式指定的输入节点**也算固定电位节点**：它由 `process(x)` 每拍写入
    // （`build()` 里 `fix_node(IN, 0.0)`）。如果不把它排到未知量之后，
    // LU 里就会出现一行全零的“未知量” → 解发散（实测 6e19 V）。
    let explicit_input = opts.input.clone().or_else(|| dirs.input.clone());
    let input_name: Option<String> = match &explicit_input {
        Some(n) => Some(resolve_input_name(&circuit, n, &canon)?),
        None => None,
    };
    if let Some(n) = &input_name {
        fixed_names.insert(canon(n));
    }
    for s in &dirs.switches {
        let _ = s;
    }
    let unknown_names: Vec<&String> = names.iter().filter(|n| !fixed_names.contains(*n)).collect();
    let fixed_only: Vec<&String> = names.iter().filter(|n| fixed_names.contains(*n)).collect();
    let n_unknown_nodes = unknown_names.len();
    let n_branches = next_branch_estimate(&circuit, &dirs);
    let mut node_index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, n) in unknown_names.iter().enumerate() {
        node_index.insert((*n).clone(), i + 1);
    }
    for (i, n) in fixed_only.iter().enumerate() {
        node_index.insert((*n).clone(), n_unknown_nodes + n_branches + i + 1);
    }
    let _ = (n_unknown_nodes, n_branches);
    let idx = |n: &str| -> usize {
        let c = canon(n);
        if c == "0" {
            0
        } else {
            *node_index.get(&c).unwrap_or(&0)
        }
    };

    // ---- 3) 激励：DC 源 → 固定节点；SIN/AC 源 → 输入端口 ----
    //
    // 输入端口有三种来源，优先级从高到低：
    //   ① `--input <名>`（CLI，测试/工具用）
    //   ② 网表里的 `* @rustomp in=<名>`
    //   ③ 自动：唯一一个 SIN/AC/PULSE 源
    //
    // **`in=` 既可以是元件名，也可以是节点名**。后者是 rustomp-circuit-designer 的写法：
    // 设计器导出的网表是一张**纯拓扑**（激励不画进网表，由 DSP 每拍注入），
    // 所以它只能写 `in=IN`。早期实现只认元件名，于是设计器的图会退化成
    // “无输入端口（纯直流电路）”—— 生成的 `Ckt` 直接忽略输入。
    let mut fixed_nodes: Vec<(usize, f64, String)> = Vec::new();
    let mut signal_sources: Vec<(usize, String)> = Vec::new();
    for e in &circuit.elements {
        if e.kind != ElementKind::VoltageSource {
            continue;
        }
        if idx(&e.nodes[1]) != 0 {
            return Err(format!(
                "{}：只支持接地的电压源（另一端必须是 0），这里是 {}",
                e.name, e.nodes[1]
            ));
        }
        let lower = e.value.to_lowercase();
        let is_signal = lower.contains("sin") || lower.contains("ac ") || lower.contains("pulse");
        let node = idx(&e.nodes[0]);
        if is_signal {
            signal_sources.push((node, e.name.clone()));
        } else {
            let dc = parse_value(e.value.split_whitespace().last().unwrap_or(""))
                .ok_or_else(|| format!("{}：读不出直流电压 `{}`", e.name, e.value))?;
            fixed_nodes.push((node, dc, e.name.clone()));
        }
    }

    let input_node: Option<(usize, String)> = match &explicit_input {
        Some(name) => {
            let node = idx(input_name.as_ref().expect("显式输入已解析成节点名"));
            fixed_nodes.retain(|(n, _, _)| *n != node);
            notes.push(format!(
                "`in={name}` → 输入端口节点 {node}（由 process(x) 每拍写入）"
            ));
            Some((node, name.clone()))
        }
        None => match signal_sources.len() {
            0 => None,
            1 => {
                let (node, name) = &signal_sources[0];
                notes.push(format!("{name} 作为输入端口（节点下标 {node}）"));
                Some((*node, name.clone()))
            }
            _ => {
                let list = signal_sources
                    .iter()
                    .map(|(_, n)| n.as_str())
                    .collect::<Vec<_>>()
                    .join("、");
                return Err(format!(
                    "发现多个信号源（{list}）：请用 `* @rustomp in=<源名或节点名>` 指定输入"
                ));
            }
        },
    };
    let has_input = input_node.is_some();

    // ---- 4) 输出端口 ----
    let output_name = opts.output.clone().or(dirs.output.clone()).ok_or_else(|| {
        "没指定输出节点，请用 `* @rustomp out=<节点名>` 或 --output-node".to_string()
    })?;
    let output_node = idx(&output_name);
    if output_node == 0 && canon(&output_name) == "0" {
        return Err(format!("输出节点 {output_name} 被合并到地了"));
    }

    // ---- 5) 元件 → 代码 ----
    let mut body: Vec<String> = Vec::new();
    let mut uses: BTreeSet<String> = BTreeSet::new();
    uses.insert("Netlist".into());
    uses.insert("GND".into());
    // 参数驱动的元件：(字段, 常量名, 阻值表达式)
    let mut param_writes: Vec<(String, String, String)> = Vec::new();
    let mut param_fields: Vec<(String, ParamType)> = Vec::new();
    // 支路紧跟在"未知节点"之后，固定节点排在最后（见上面的编号布局）
    let mut next_branch = n_unknown_nodes + 1;
    // 已经 push 进 `Netlist` 的元件个数（= 下一个元件的下标）
    let mut elem_count = 0usize;
    // 参数元件：(常量名, 下标) —— 生成模块级 const，供 set_params 用
    let mut param_consts: Vec<(String, usize)> = Vec::new();

    let mut declare_param = |field: &str, ty: ParamType| -> Result<(), String> {
        match param_fields.iter().find(|(f, _)| f == field) {
            Some((_, existing)) if *existing != ty => Err(format!(
                "参数 {field} 同时被声明为 {existing:?} 和 {ty:?}（一个参数只能有一种类型）"
            )),
            Some(_) => Ok(()),
            None => {
                param_fields.push((field.to_string(), ty));
                Ok(())
            }
        }
    };

    for e in &circuit.elements {
        match e.kind {
            ElementKind::Resistor => {
                let (na, nb) = (idx(&e.nodes[0]), idx(&e.nodes[1]));
                if na == nb {
                    notes.push(format!("{} 两端同节点（自环），已跳过", e.name));
                    continue;
                }
                if let Some(p) = dirs.pots.iter().find(|p| p.elem == e.name) {
                    declare_param(&p.field, ParamType::Pot)?;
                    let expr = if p.invert {
                        format!("{} * (1.0 - p.{})", fmt_f64(p.max), p.field)
                    } else {
                        format!("{} * p.{}", fmt_f64(p.max), p.field)
                    };
                    let const_name = format!("IDX_{}", e.name.to_uppercase());
                    body.push(format!(
                        "    // {} = 电位器「{}」：R = {}（max = {} Ω）",
                        e.name,
                        p.field,
                        if p.invert {
                            format!("max·(1−{})", p.field)
                        } else {
                            format!("max·{}", p.field)
                        },
                        fmt_f64(p.max)
                    ));
                    body.push(format!(
                        "    net.push_resistor({}, {}, {expr});",
                        node_expr(na, &node_index),
                        node_expr(nb, &node_index)
                    ));
                    param_consts.push((const_name.clone(), elem_count));
                    elem_count += 1;
                    // 注意：写入的表达式要用**新参数**（函数的形参 `params`）
                    param_writes.push((p.field.clone(), const_name, expr.replace("p.", "params.")));
                    continue;
                }
                let v = parse_value(&e.value)
                    .ok_or_else(|| format!("{}：读不出阻值 `{}`", e.name, e.value))?;
                if v.abs() <= WIRE_OHMS {
                    continue; // 已并节点
                }
                body.push(format!(
                    "    // {} ({} Ω) {}—{}",
                    e.name,
                    e.value,
                    canon(&e.nodes[0]),
                    canon(&e.nodes[1])
                ));
                body.push(format!(
                    "    net.resistor({}, {}, {});",
                    node_expr(na, &node_index),
                    node_expr(nb, &node_index),
                    fmt_f64(v)
                ));
                elem_count += 1;
            }
            ElementKind::Capacitor => {
                let v = parse_value(&e.value)
                    .ok_or_else(|| format!("{}：读不出容值 `{}`", e.name, e.value))?;
                body.push(format!("    // {} ({} F)", e.name, e.value));
                body.push(format!(
                    "    net.capacitor({}, {}, {});",
                    node_expr(idx(&e.nodes[0]), &node_index),
                    node_expr(idx(&e.nodes[1]), &node_index),
                    fmt_f64(v)
                ));
                elem_count += 1;
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
                    "    // {} {}（阳极 {}）",
                    e.name, model, e.nodes[0]
                ));
                body.push(format!(
                    "    net.diode({}, {}, {});",
                    const_name,
                    node_expr(idx(&e.nodes[0]), &node_index),
                    node_expr(idx(&e.nodes[1]), &node_index)
                ));
                elem_count += 1;
            }
            ElementKind::Npn | ElementKind::Pnp => {
                let model = e
                    .model
                    .as_deref()
                    .ok_or_else(|| format!("{}：三极管没有模型名", e.name))?;
                let (const_name, kind) = model_const(model)
                    .ok_or_else(|| unknown_model_error(circuit, model, "三极管"))?;
                let npn = match kind {
                    ModelKind::BjtNpn => true,
                    ModelKind::BjtPnp => false,
                    _ => return Err(format!("{}：模型 {model} 不是 BJT", e.name)),
                };
                uses.insert(const_name.into());
                body.push(format!(
                    "    // {} {}（{}：c b e）",
                    e.name,
                    model,
                    if npn { "NPN" } else { "PNP" }
                ));
                body.push(format!(
                    "    net.bjt({}, {}, {}, {}, {});",
                    const_name,
                    npn,
                    node_expr(idx(&e.nodes[0]), &node_index),
                    node_expr(idx(&e.nodes[1]), &node_index),
                    node_expr(idx(&e.nodes[2]), &node_index)
                ));
                elem_count += 1;
            }
            ElementKind::Jfet => {
                let model = e
                    .model
                    .as_deref()
                    .ok_or_else(|| format!("{}：JFET 没有模型名", e.name))?;
                let (const_name, kind) = model_const(model)
                    .ok_or_else(|| unknown_model_error(circuit, model, "JFET"))?;
                if kind != ModelKind::Jfet {
                    return Err(format!("{}：模型 {model} 不是 JFET", e.name));
                }
                uses.insert(const_name.into());
                body.push(format!("    // {} {}（d g s）", e.name, model));
                body.push(format!(
                    "    net.jfet({}, {}, {}, {});",
                    const_name,
                    node_expr(idx(&e.nodes[0]), &node_index),
                    node_expr(idx(&e.nodes[1]), &node_index),
                    node_expr(idx(&e.nodes[2]), &node_index)
                ));
                elem_count += 1;
            }
            ElementKind::Nmos => {
                let model = e
                    .model
                    .as_deref()
                    .ok_or_else(|| format!("{}：MOSFET 没有模型名", e.name))?;
                let (const_name, kind) = model_const(model)
                    .ok_or_else(|| unknown_model_error(circuit, model, "MOSFET"))?;
                if kind != ModelKind::Mosfet {
                    return Err(format!("{}：模型 {model} 不是 MOSFET", e.name));
                }
                uses.insert(const_name.into());
                body.push(format!("    // {} {}（d g s）", e.name, model));
                body.push(format!(
                    "    net.mosfet({}, {}, {}, {});",
                    const_name,
                    node_expr(idx(&e.nodes[0]), &node_index),
                    node_expr(idx(&e.nodes[1]), &node_index),
                    node_expr(idx(&e.nodes[2]), &node_index)
                ));
                elem_count += 1;
            }
            ElementKind::Subckt => {
                let model = e.model.as_deref().unwrap_or("");
                let (const_name, kind) = model_const(model)
                    .ok_or_else(|| unknown_model_error(circuit, model, "子电路"))?;
                if kind != ModelKind::OpAmp {
                    return Err(format!("{}：子电路 {model} 目前只支持运放", e.name));
                }
                if e.nodes.len() < 5 {
                    return Err(format!(
                        "{}：运放需要 5 个引脚 <in+> <in-> <v+> <v-> <out>，只有 {}",
                        e.name,
                        e.nodes.len()
                    ));
                }
                uses.insert(const_name.into());
                uses.insert("OP07".into());
                let br = next_branch;
                next_branch += 1;
                body.push(format!("    // {} {}（in+ in- v+ v- out）", e.name, model));
                body.push(format!(
                    "    net.opamp({}.model(1.0 / fs), {}, {}, {}, {});",
                    const_name,
                    node_expr(idx(&e.nodes[0]), &node_index),
                    node_expr(idx(&e.nodes[1]), &node_index),
                    node_expr(idx(&e.nodes[4]), &node_index),
                    br
                ));
                elem_count += 1;
            }
            ElementKind::VoltageSource => { /* 第 3 步处理 */ }
            other => {
                return Err(format!(
                    "{}：暂不支持 {} 类元件",
                    e.name,
                    other.spice_prefix()
                ));
            }
        }
    }

    // ---- 6) 开关（S1b）：生成一个可切换的电阻 ----
    for s in &dirs.switches {
        declare_param(&s.field, ParamType::Switch)?;
        let (na, nb) = (idx(&s.node_a), idx(&s.node_b));
        if na == nb {
            return Err(format!("开关 {} 的两个节点合并到同一个了", s.field));
        }
        let const_name = format!("IDX_SW_{}", s.field.to_uppercase());
        body.push(format!(
            "    // 开关「{}」：{} — {}（闭合 {} Ω / 断开 {} Ω）",
            s.field,
            canon(&s.node_a),
            canon(&s.node_b),
            fmt_f64(s.closed),
            fmt_f64(s.open)
        ));
        body.push(format!(
            "    net.push_resistor({}, {}, if p.{field} {{ {c} }} else {{ {o} }});",
            node_expr(na, &node_index),
            node_expr(nb, &node_index),
            field = s.field,
            c = fmt_f64(s.closed),
            o = fmt_f64(s.open),
        ));
        param_consts.push((const_name.clone(), elem_count));
        elem_count += 1;
        param_writes.push((
            s.field.clone(),
            const_name,
            format!(
                "if params.{0} {{ {1} }} else {{ {2} }}",
                s.field,
                fmt_f64(s.closed),
                fmt_f64(s.open)
            ),
        ));
    }

    // `N` 要覆盖所有出现过的下标（下标 0 是地，不上矩阵）：
    //   [未知节点 1..K][支路 K+1..K+B][固定节点 K+B+1..K+B+F]  ⇒ N = K+B+F+1
    let n_fixed_nodes = fixed_only.len();
    let max_index = n_unknown_nodes + n_branches + n_fixed_nodes;
    let unknowns = max_index + 1;
    // LU 处理"下标 0..=max_unknown_index"这些行：未知节点 1..K、支路 K+1..K+B
    // → 最大未知下标 = K+B → 行数 = K+B+1（下标 0 是地，白占一行但不影响正确性）
    let lu_rows = n_unknown_nodes + n_branches + 1;
    if unknowns > MAX_UNKNOWNS {
        return Err(format!("未知量 {unknowns} 超过上限 {MAX_UNKNOWNS}"));
    }

    // ---- 7) 生成代码 ----
    let mut code = String::new();
    code.push_str("// @generated by rustomp-import (netlist → MNA elements) — do not edit\n");
    code.push_str(&format!(
        "// 由 {} 生成。改动请改网表后重新生成。\n",
        if opts.source.is_empty() {
            "网表"
        } else {
            &opts.source
        }
    ));
    code.push_str("// 未知量 = 节点数 + 电压源支路数（地不占未知量）。\n\n");
    let mut use_list: Vec<String> = uses.iter().cloned().collect();
    use_list.sort();
    code.push_str(&format!(
        "use rustomp_dsp::{{{}}};\n\n",
        use_list.join(", ")
    ));
    code.push_str(&format!(
        "/// 总下标数（`Netlist<N>` 的 N；包含排在最后的固定电位节点）\npub const N: usize = {unknowns};\n\n"
    ));
    code.push_str(&format!(
        "/// LU 真正要处理的未知量个数（固定节点排在它之后，行列全零）\npub const UNKNOWNS: usize = {lu_rows};\n\n"
    ));
    let iters = dirs.newton.unwrap_or(DEFAULT_NEWTON_ITERS);
    // 注意：字符串里不要用「缩进续行」——续行后的空格会原样进入生成物
    code.push_str(&format!(
        "/// 实时路径的牛顿迭代次数（`* @rustomp newton=` 指定；默认 {DEFAULT_NEWTON_ITERS}）\n\n\
         /// 2 次不够：方波/快速边沿下会欠收敛 → 次谐波极限环（DEVLOG E50）。\n\
         pub const NEWTON_ITERS: usize = {iters};\n\n"
    ));

    // 参数结构体
    if !param_fields.is_empty() {
        code.push_str("/// 可调参数（电位器位置 0..1 / 开关布尔）。\n#[derive(Clone, Copy, PartialEq)]\npub struct Params {\n");
        for (field, ty) in &param_fields {
            code.push_str(&format!("    pub {field}: {},\n", ty.rust_ty()));
        }
        code.push_str(
            "}\n\nimpl Default for Params {\n    fn default() -> Self {\n        Self {\n",
        );
        for (field, ty) in &param_fields {
            code.push_str(&format!("            {field}: {},\n", ty.default_value()));
        }
        code.push_str("        }\n    }\n}\n\n");
    } else {
        code.push_str(
            "/// 无可调参数。\n#[derive(Clone, Copy, PartialEq, Default)]\npub struct Params;\n\n",
        );
    }

    // 参数元件的下标（供 set_params 定位）
    for (name, index) in &param_consts {
        code.push_str(&format!("/// `{name}` 在 `Netlist::elems` 里的下标\n#[allow(dead_code)]\npub const {name}: usize = {index};\n"));
    }
    if !param_consts.is_empty() {
        code.push('\n');
    }

    // 节点常量
    code.push_str("/// 节点编号（0 = 地；其余按名字排序，保证 diff 稳定）\npub mod node {\n");
    for (name, index) in &node_index {
        code.push_str(&format!(
            "    /// `{}`\n    pub const {}: usize = {};\n",
            name,
            const_name_for(name),
            index
        ));
    }
    code.push_str("}\n\n");

    if has_input {
        code.push_str(&format!(
            "/// 输入端口节点（`process(x)` 用 `net.set_fixed(FIXED_IN, x)`）\npub const IN: usize = {};\n",
            input_node.as_ref().unwrap().0
        ));
    } else {
        code.push_str("/// 无输入端口（纯直流电路）\npub const IN: usize = 0;\n");
    }
    code.push_str(&format!(
        "/// 输出节点\npub const OUT: usize = {};\n",
        output_node
    ));
    let mut rails = fixed_nodes.clone();
    rails.sort_by(|a, b| a.0.cmp(&b.0));
    rails.dedup_by_key(|(n, _, _)| *n);
    code.push_str(&format!(
        "/// 输入端口在 `Netlist::fixed` 里的下标（电源轨排在它前面）\npub const FIXED_IN: usize = {};\n\n",
        rails.len()
    ));

    // build()
    code.push_str("/// 按网表搭出电路（`fs` 是**内层**采样率，含过采样倍率）。\npub fn build(fs: f64, p: &Params) -> Netlist<N> {\n");
    code.push_str("    let mut net = Netlist::<N>::new();\n");
    code.push_str("    net.set_unknowns(UNKNOWNS);\n");
    code.push_str("    let _ = (fs, p);\n");
    for line in &body {
        code.push_str(line);
        code.push('\n');
    }
    for (node, dc, name) in &rails {
        code.push_str(&format!(
            "    net.fix_node({}, {}); // {name}\n",
            node_expr(*node, &node_index),
            fmt_f64(*dc)
        ));
    }
    if has_input {
        code.push_str("    net.fix_node(IN, 0.0); // 输入端口，值由 process() 每拍写入\n");
    }
    code.push_str("    net\n}\n\n");

    // Ckt：直接实现 Circuit
    code.push_str(
        "/// 生成的实时电路（直接实现 `rustomp_dsp::Circuit`）。\npub struct Ckt {\n    net: Netlist<N>,\n    params: Params,\n    fs: f64,\n    /// 削波活动量（0..1）：二极管最大正向电流 / ACTIVITY_I_REF\n    activity: f32,\n}\n\n",
    );
    let i_ref = dirs.activity_i.unwrap_or(50e-6);
    code.push_str(&format!(
        "/// 活动指示的参考电流（`* @rustomp activity_i=` 可改；**只驱动 GUI 的 LED**）\npub const ACTIVITY_I_REF: f64 = {};\n\n",
        fmt_f64(i_ref)
    ));
    code.push_str("impl rustomp_dsp::Circuit for Ckt {\n    type Params = Params;\n\n");
    code.push_str("    fn new(sample_rate: f64) -> Self {\n");
    code.push_str("        let params = Params::default();\n");
    code.push_str("        let mut net = build(sample_rate, &params);\n");
    code.push_str("        net.solve_dc();\n");
    code.push_str("        Self { net, params, fs: sample_rate, activity: 0.0 }\n    }\n\n");
    code.push_str("    fn set_sample_rate(&mut self, sample_rate: f64) {\n");
    code.push_str("        if (sample_rate - self.fs).abs() > 1e-9 {\n            *self = Self::new(sample_rate);\n        }\n    }\n\n");
    code.push_str("    fn set_params(&mut self, params: Params) {\n");
    code.push_str("        if params == self.params {\n            return;\n        }\n");
    for (field, const_name, expr) in &param_writes {
        code.push_str(&format!(
            "        if params.{field} != self.params.{field} {{\n            self.net.set_resistance({const_name}, {expr});\n        }}\n"
        ));
    }
    code.push_str("        self.params = params;\n    }\n\n");
    code.push_str("    fn reset(&mut self) {\n        self.net.reset();\n        self.net.solve_dc();\n    }\n\n");
    code.push_str("    fn process(&mut self, x: f64) -> f64 {\n");
    code.push_str("        let dt = 1.0 / self.fs;\n");
    if has_input {
        code.push_str("        self.net.set_fixed(FIXED_IN, x);\n");
    } else {
        code.push_str("        let _ = x;\n");
    }
    // ---- 削波活动量（给 GUI 的 LED）----
    code.push_str("        let v = self.net.step_fixed(dt, NEWTON_ITERS);\n");
    code.push_str("        // 削波活动量：网表里**所有二极管**正向电流的最大值（反向的不算）\n");
    code.push_str("        let mut imax = 0.0f64;\n");
    code.push_str("        for e in &self.net.elems {\n");
    code.push_str("            if let rustomp_dsp::Element::Diode { model, a, k } = e {\n");
    code.push_str("                let va = if *a == rustomp_dsp::GND { 0.0 } else { v[*a] };\n");
    code.push_str("                let vk = if *k == rustomp_dsp::GND { 0.0 } else { v[*k] };\n");
    code.push_str("                imax = imax.max(model.current(va - vk));\n");
    code.push_str("            }\n");
    code.push_str("        }\n");
    code.push_str("        self.activity = (imax / ACTIVITY_I_REF).clamp(0.0, 1.0) as f32;\n");
    code.push_str("        v[OUT]\n    }\n\n");
    code.push_str("    /// 削波活动量（0..1）→ 引擎做快起慢落包络 → GUI 的 LED\n");
    code.push_str("    fn activity(&self) -> f32 {\n        self.activity\n    }\n}\n");

    Ok(Generated {
        code,
        notes,
        unknowns,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParamType {
    Pot,
    Switch,
}

impl ParamType {
    fn rust_ty(self) -> &'static str {
        match self {
            ParamType::Pot => "f64",
            ParamType::Switch => "bool",
        }
    }
    fn default_value(self) -> &'static str {
        match self {
            ParamType::Pot => "0.5",
            ParamType::Switch => "false",
        }
    }
}

fn const_name_for(node: &str) -> String {
    let mut out = String::new();
    for c in node.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty()
        || out
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(true)
    {
        out.insert_str(0, "N_");
    }
    out
}

fn node_expr(index: usize, index_map: &BTreeMap<String, usize>) -> String {
    if index == 0 {
        return "GND".into();
    }
    for (name, i) in index_map {
        if *i == index {
            return format!("node::{}", const_name_for(name));
        }
    }
    format!("{index}")
}

fn fmt_f64(v: f64) -> String {
    // `parse("4.7n")` 会得到 4.700000000000001e-9；先归一到 12 位有效数字去掉浮点噪声。
    let rounded: f64 = format!("{v:.12e}").parse().unwrap_or(v);
    format!("{rounded:?}")
}

fn unknown_model_error(circuit: &ImportedCircuit, model: &str, kind: &str) -> String {
    let known = [
        "1N4148", "1N34A", "1N4007", "1N5817", "LedRed", "LedGreen", "LedBLUE", "OP07A", //
        "BC549C", "BC559C", "AC127", "AC128", "J201", "2N7000",
    ];
    let mut msg = format!(
        "{kind}模型 `{model}` 不在闭集表里（已知：{}）",
        known.join(" / ")
    );
    let models: BTreeSet<&str> = circuit
        .elements
        .iter()
        .filter_map(|e| e.model.as_deref())
        .collect();
    msg.push_str(&format!(
        "\n网表里出现的模型：{}",
        models.into_iter().collect::<Vec<_>>().join(", ")
    ));
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
        assert_eq!(fmt_f64(parse_value("4.7n").unwrap()), "4.7e-9");
        assert_eq!(fmt_f64(47000.0), "47000.0");
    }

    #[test]
    fn node_const_names_are_valid_rust() {
        assert_eq!(const_name_for("n_t27R"), "N_T27R");
        assert_eq!(const_name_for("in+"), "IN_");
        assert_eq!(const_name_for("1"), "N_1");
    }

    #[test]
    fn parses_param_directives() {
        let notes = vec![
            "@rustomp in=V1 out=out".to_string(),
            "@rustomp param RP1A gain pot max=500k".to_string(),
            "@rustomp param RP2B level pot max=500k invert".to_string(),
            "@rustomp switch symmetric a b closed=0.001 open=1e12".to_string(),
        ];
        let d = parse_directives(&notes).unwrap();
        assert_eq!(d.input.as_deref(), Some("V1"));
        assert_eq!(d.output.as_deref(), Some("out"));
        assert_eq!(d.pots.len(), 2);
        assert!(d.pots[1].invert);
        assert_eq!(d.pots[0].max, 500_000.0);
        assert_eq!(d.switches.len(), 1);
        assert_eq!(d.switches[0].closed, 0.001);
    }

    #[test]
    fn rejects_bad_param_syntax() {
        let notes = vec!["@rustomp param RP1A gain".to_string()];
        assert!(parse_directives(&notes).is_err());
        let notes = vec!["@rustomp param RP1A 9bad pot max=1k".to_string()];
        assert!(parse_directives(&notes).is_err());
    }
}

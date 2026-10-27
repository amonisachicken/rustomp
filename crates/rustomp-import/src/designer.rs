// SPDX-License-Identifier: GPL-3.0-or-later

//! **rustomp-circuit-designer 工程导入器**（`*.design.json`）。
//!
//! 为什么直接读工程文件，而不是只吃导出的 `.cir`：
//!
//! - `.cir` 是设计器的**导出物**，`.design.json` 才是它的**一等产物**（保存/导入/自动恢复都用它）。
//!   少一步手工导出，就少一次"图改了但忘了重新导出"的事故；
//! - 工程文件里还有 `.cir` 不携带的信息：端口符号用的是哪个 pin、测试点放在哪、
//!   电位器的字段别名（`alias`）—— 报错时能指名道姓（`P1 的抽头没接`）；
//! - 设计器的 lint（悬空引脚 / 无地 / 型号闭集）可以**原样带进导入提示**，
//!   而不是等 ngspice 跑了才发现。
//!
//! **几何规则必须与设计器完全一致**（`rustomp-circuit-designer/backend/src/netlist.rs`）：
//! 引脚世界坐标 = `rotate(mirror(pin_local)) + at`、端点落在导线中间算连（T 型）、
//! 自动网络名按"网络最小坐标"排序。漂移了就会"看起来对、网表却是错的"，
//! 所以 `tests/designer.rs` 拿设计器自己导出的 `.cir` 做逐行对拍。
//!
//! 引脚几何**不在本文件里硬编码**：运行时读设计器的 `symbols.json`
//! （`rustomp circuit-designer` 的仓库根下有 `symbols/symbols.json`），
//! 保证"用的是画这张图时的那份几何"。找不到就**报错并给出路径**，不猜。

use crate::{Element, ElementKind, ImportError, ImportedCircuit, Importer};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// 设计器工程文件（`.design.json`）的导入器。
pub struct CircuitDesigner;

impl Importer for CircuitDesigner {
    fn id(&self) -> &'static str {
        "circuit-designer"
    }

    fn matches(&self, path: &Path, content: &str) -> bool {
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_ascii_lowercase().ends_with(".design.json"))
            .unwrap_or(false)
        {
            return true;
        }
        // 内容嗅探：文件可能被改名，但 `parts` + `wires` + `symbol` 三个键是设计器的指纹
        // （`.bread` 有自己的 `format` 字段，SPICE 网表根本不以 `{` 开头）
        let t = content.trim_start();
        t.starts_with('{')
            && content.contains("\"parts\"")
            && content.contains("\"wires\"")
            && content.contains("\"symbol\"")
    }

    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError> {
        let table = SymbolTable::load(path)?;
        import_with_table(path, content, &table)
    }
}

// ---------------------------------------------------------------- 工程文件模型

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Part {
    #[allow(dead_code)]
    pub id: String,
    #[serde(rename = "ref")]
    pub r#ref: String,
    pub symbol: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    /// 字段别名（电位器用：`param <元素> <别名> pot …`）
    #[serde(default)]
    pub alias: Option<String>,
    pub at: Pt,
    #[serde(default)]
    pub rot: i32,
    #[serde(default = "no_mirror")]
    pub mirror: String,
}

fn no_mirror() -> String {
    "none".into()
}

#[derive(Debug, Clone, Deserialize)]
struct Wire {
    #[serde(default)]
    pub id: String,
    pub points: Vec<Pt>,
    /// 显式网络名（设计器目前只用来画图，不参与网表 —— 保持一致）
    #[serde(default)]
    #[allow(dead_code)]
    pub net: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Doc {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default)]
    pub wires: Vec<Wire>,
}

// ---------------------------------------------------------------- 符号表（设计器的 symbols.json）

/// 设计器 `symbols.json` 里本导入器用得到的部分。
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolTable {
    pub symbols: BTreeMap<String, Symbol>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Symbol {
    #[serde(default)]
    pub pins: Vec<Pin>,
    pub spice: SpiceSpec,
    /// 型号闭集（空 = 该符号不用型号）
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub category: String,
    /// 网络标签类符号（`earth` → `0`、`vcc` → `VCC`）
    #[serde(default, rename = "netName")]
    pub net_name: Option<String>,
    /// 端口符号（`in` / `out`）
    #[serde(default)]
    pub port: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pin {
    pub id: String,
    pub at: Pt,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpiceSpec {
    #[serde(default)]
    pub prefix: String,
    #[serde(default, rename = "pinOrder")]
    pub pin_order: Vec<String>,
}

impl SymbolTable {
    pub fn from_json(text: &str) -> Result<Self, ImportError> {
        serde_json::from_str(text)
            .map_err(|e| ImportError::Parse(format!("symbols.json 解析失败：{e}")))
    }

    /// 按"从工程文件向上找 → 当前目录 → 同级仓库"的顺序找设计器的 `symbols.json`。
    ///
    /// 也可以用 `RUSTOM_DESIGNER_SYMBOLS=<path>` 直接指定。
    pub fn load(design: &Path) -> Result<Self, ImportError> {
        let tried = candidate_paths(design);
        for p in &tried {
            if let Ok(text) = std::fs::read_to_string(p) {
                return Self::from_json(&text)
                    .map_err(|e| ImportError::Parse(format!("{}：{e}", p.display())));
            }
        }
        Err(ImportError::Parse(format!(
            "读不到 rustomp-circuit-designer 的 symbols.json（找过：{}）。\n\
             解决：① 用 `RUSTOM_DESIGNER_SYMBOLS=<...>/symbols/symbols.json` 指定；\
             ② 或把设计器仓库放在与本仓库同级的位置。",
            tried
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        )))
    }
}

fn candidate_paths(design: &Path) -> Vec<PathBuf> {
    if let Ok(p) = std::env::var("RUSTOM_DESIGNER_SYMBOLS") {
        return vec![PathBuf::from(p)];
    }
    let mut out: Vec<PathBuf> = Vec::new();
    // ① 从工程文件所在目录往上找（工程通常就在设计器仓库里）
    if let Some(dir) = design.parent() {
        let mut cur = Some(dir.to_path_buf());
        for _ in 0..6 {
            let Some(d) = cur else { break };
            out.push(d.join("symbols/symbols.json"));
            cur = d.parent().map(|p| p.to_path_buf());
        }
    }
    // ② 当前工作目录
    out.push(PathBuf::from("symbols/symbols.json"));
    // ③ 同级仓库（本仓库与设计器并列）
    out.push(PathBuf::from(
        "../rustomp-circuit-designer/symbols/symbols.json",
    ));
    out.dedup();
    out
}

// ---------------------------------------------------------------- 几何（与设计器逐位一致）

/// 局部 → 世界（必须与设计器 `netlist.rs::to_world` 和前端 `model.ts::toWorld` 一致）
pub fn to_world(local: Pt, part: &Part) -> Pt {
    let mut x = local.x;
    let mut y = local.y;
    match part.mirror.as_str() {
        "x" => x = -x,
        "y" => y = -y,
        _ => {}
    }
    let a = (part.rot as f64) * std::f64::consts::PI / 180.0;
    let (s, c) = a.sin_cos();
    // 屏幕坐标 y 向下；顺时针 ⇒ (x,y) → (x·cos − y·sin, x·sin + y·cos)
    Pt {
        x: c * x - s * y + part.at.x,
        y: s * x + c * y + part.at.y,
    }
}

/// 坐标量化精度（mm）：浮点相加会有 1e-15 的误差，量化后才能当 key
const Q: f64 = 1000.0;

fn key(p: Pt) -> (i64, i64) {
    ((p.x * Q).round() as i64, (p.y * Q).round() as i64)
}

/// 点是否落在线段上（含端点）
fn on_segment(p: Pt, a: Pt, b: Pt) -> bool {
    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    if cross.abs() > 1e-6 {
        return false;
    }
    let dot = (p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y);
    let len2 = (b.x - a.x).powi(2) + (b.y - a.y).powi(2);
    dot >= -1e-6 && dot <= len2 + 1e-6
}

struct Dsu {
    parent: HashMap<(i64, i64), (i64, i64)>,
}

impl Dsu {
    fn new() -> Self {
        Self {
            parent: HashMap::new(),
        }
    }

    fn find(&mut self, k: (i64, i64)) -> (i64, i64) {
        let p = *self.parent.get(&k).unwrap_or(&k);
        if p == k {
            self.parent.entry(k).or_insert(k);
            return k;
        }
        let root = self.find(p);
        self.parent.insert(k, root);
        root
    }

    fn union(&mut self, a: (i64, i64), b: (i64, i64)) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // 字典序小的当根：并查集形状与调用顺序无关（否则网表会随遍历顺序漂移）
            let (lo, hi) = if ra <= rb { (ra, rb) } else { (rb, ra) };
            self.parent.insert(hi, lo);
        }
    }

    /// 只读的查根（与 `find` 的路径压缩结果一致）
    fn root(&self, k: (i64, i64)) -> (i64, i64) {
        let mut cur = k;
        while let Some(&p) = self.parent.get(&cur) {
            if p == cur {
                break;
            }
            cur = p;
        }
        cur
    }
}

/// 参考编号自然序（`R2` < `R10`，且组内顺序与输入无关）
fn ref_key(r: &str) -> (String, i64, String) {
    let split = r.find(|c: char| c.is_ascii_digit()).unwrap_or(r.len());
    let (head, tail) = r.split_at(split);
    (
        head.to_string(),
        tail.parse().unwrap_or(0),
        tail.to_string(),
    )
}

// ---------------------------------------------------------------- 主流程

/// 用给定的符号表导入（测试/工具用；正常路径走 [`SymbolTable::load`]）。
pub fn import_with_table(
    path: &Path,
    content: &str,
    table: &SymbolTable,
) -> Result<ImportedCircuit, ImportError> {
    let doc: Doc = serde_json::from_str(content).map_err(|e| {
        ImportError::Parse(format!(
            "{}：不是合法的设计器工程文件（{e}）",
            path.display()
        ))
    })?;
    if doc.parts.is_empty() {
        return Err(ImportError::Parse(format!(
            "{}：工程里没有任何元件",
            path.display()
        )));
    }
    let mut errors: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // ---- 1) 参考编号唯一性（重复编号会让生成的 Rust 常量撞名）----
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for p in &doc.parts {
        *seen.entry(p.r#ref.as_str()).or_default() += 1;
    }
    for (r, n) in &seen {
        if *n > 1 {
            errors.push(format!("参考编号 {r} 出现 {n} 次"));
        }
    }

    // ---- 2) 导线：两端 union；端点落在别的线段上也算连上（T 型连接）----
    let mut dsu = Dsu::new();
    let segs: Vec<(Pt, Pt)> = doc
        .wires
        .iter()
        .filter(|w| w.points.len() >= 2)
        .map(|w| (w.points[0], w.points[1]))
        .collect();
    for w in &doc.wires {
        if w.points.len() < 2 {
            notes.push(format!("导线 {} 少于两个锚点，已忽略", w.id));
            continue;
        }
        dsu.union(key(w.points[0]), key(w.points[1]));
    }
    for (i, (a, b)) in segs.iter().enumerate() {
        for (j, (c, d)) in segs.iter().enumerate() {
            if i == j {
                continue;
            }
            for p in [a, b] {
                if on_segment(*p, *c, *d) {
                    dsu.union(key(*p), key(*c));
                }
            }
        }
    }

    // ---- 3) 引脚 → 世界坐标；落在导线上也 union ----
    struct PinWorld<'a> {
        part: &'a Part,
        pin_id: &'a str,
        at: Pt,
    }
    let mut pins: Vec<PinWorld> = Vec::new();
    let mut nc_pins: Vec<(String, String)> = Vec::new();
    let mut explicit: HashMap<(i64, i64), String> = HashMap::new();
    let (mut port_in, mut port_out): (Option<String>, Option<String>) = (None, None);

    for part in &doc.parts {
        let Some(sym) = table.symbols.get(&part.symbol) else {
            errors.push(format!(
                "{}：未知符号 `{}`（设计器的 symbols.json 里没有；设计器版本不一致？）",
                part.r#ref, part.symbol
            ));
            continue;
        };
        // 型号必须落在该符号的闭集里（与设计器的 lint 同一条规则）
        if let Some(model) = &part.model {
            if !sym.models.iter().any(|m| m == model) {
                errors.push(format!(
                    "{}：型号 {model} 不在闭集里（{}）",
                    part.r#ref,
                    sym.models.join("/")
                ));
            }
        } else if !sym.models.is_empty() {
            errors.push(format!(
                "{}：没选型号（{} 之一）",
                part.r#ref,
                sym.models.join("/")
            ));
        }

        for pin in &sym.pins {
            let at = to_world(pin.at, part);
            dsu.find(key(at));
            pins.push(PinWorld {
                part,
                pin_id: &pin.id,
                at,
            });
            if sym.category == "nc" {
                nc_pins.push((part.id.clone(), pin.id.clone()));
            }
            for (a, b) in &segs {
                if on_segment(at, *a, *b) {
                    dsu.union(key(at), key(*a));
                }
            }
        }

        if let Some(net_name) = &sym.net_name {
            if let Some(first) = sym.pins.first() {
                let k = to_world(first.at, part);
                if let Some(prev) = explicit.insert(key(k), net_name.clone()) {
                    if prev != *net_name {
                        notes.push(format!(
                            "{}：网络名 {prev} 与 {net_name} 冲突（取后者）",
                            part.r#ref
                        ));
                    }
                }
            }
        }
        if let Some(port) = &sym.port {
            if let Some(first) = sym.pins.first() {
                let k = to_world(first.at, part);
                let name = first.id.clone();
                match port.as_str() {
                    "in" => port_in = Some(name.clone()),
                    "out" => port_out = Some(name.clone()),
                    other => notes.push(format!("{}：未知端口类型 `{other}`，已忽略", part.r#ref)),
                }
                explicit.insert(key(k), name);
            }
        }
    }

    // ---- 4) 汇聚成网络 ----
    let mut nets: BTreeMap<(i64, i64), Vec<&PinWorld>> = BTreeMap::new();
    for pw in &pins {
        nets.entry(dsu.find(key(pw.at))).or_default().push(pw);
    }

    let mut named: HashMap<(i64, i64), String> = HashMap::new();
    for (k, name) in explicit {
        let root = dsu.find(k);
        named.insert(root, name);
    }

    // 自动命名：按网络最小坐标排序（同一张图永远得到同样的网名）
    let order: Vec<(i64, i64)> = nets.keys().copied().collect();
    let mut counter = 0;
    let mut net_name: HashMap<(i64, i64), String> = HashMap::new();
    for root in order.iter().copied() {
        if let Some(n) = named.get(&root) {
            net_name.insert(root, n.clone());
        } else {
            counter += 1;
            net_name.insert(root, format!("n{counter}"));
        }
    }

    // ---- 5) lint（error 阻断，warn 进 notes）----
    for (root, members) in &nets {
        if members.len() == 1 {
            let pw = members[0];
            let intentional = nc_pins
                .iter()
                .any(|(pid, pin)| *pid == pw.part.id && pin == pw.pin_id);
            if !intentional {
                let name = net_name.get(root).cloned().unwrap_or_default();
                notes.push(format!(
                    "{} 的引脚 {} 悬空（只连了自己，网络 {name}）",
                    pw.part.r#ref, pw.pin_id
                ));
            }
        }
    }
    if !net_name.values().any(|n| n == "0") {
        errors.push("没有接地符号（网表里没有 0 节点）".into());
    }
    if port_in.is_none() || port_out.is_none() {
        notes.push("缺少 IN 或 OUT 端口符号 → 生成的电路没有输入/输出端口".into());
    }

    if !errors.is_empty() {
        return Err(ImportError::Parse(format!(
            "{}：设计器工程有 {} 处问题：\n  - {}",
            path.display(),
            errors.len(),
            errors.join("\n  - ")
        )));
    }

    // ---- 6) 指令（与设计器导出的 `.cir` 同一条文本）----
    let mut parts_sorted: Vec<&Part> = doc.parts.iter().collect();
    parts_sorted.sort_by_key(|p| ref_key(&p.r#ref));

    if let (Some(i), Some(o)) = (&port_in, &port_out) {
        notes.push(format!("@rustomp in={i} out={o}"));
    }
    for p in &parts_sorted {
        if p.symbol != "potentiometer" {
            continue;
        }
        let field = p.alias.clone().unwrap_or_else(|| p.r#ref.to_lowercase());
        let max = p.value.clone().unwrap_or_else(|| "500k".into());
        // 指令里的元素名必须是**网表里的元件名**（电位器展开成两只电阻 → `RP1A`/`RP1B`），
        // 不是位号：rustomp-import 的 codegen 靠它找到要参数化的那条电阻。
        notes.push(format!(
            "@rustomp param R{}A {field} pot max={max}",
            p.r#ref
        ));
        notes.push(format!(
            "@rustomp param R{}B {field} pot max={max} invert",
            p.r#ref
        ));
    }

    // ---- 7) 元件 ----
    let mut elements: Vec<Element> = Vec::new();
    for p in &parts_sorted {
        let Some(sym) = table.symbols.get(&p.symbol) else {
            continue;
        };
        let net_of = |pin_id: &str| -> String {
            pins.iter()
                .find(|pw| pw.part.id == p.id && pw.pin_id == pin_id)
                .map(|pw| {
                    net_name
                        .get(&dsu.root(key(pw.at)))
                        .cloned()
                        .unwrap_or_else(|| "?".into())
                })
                .unwrap_or_else(|| "?".into())
        };
        let mut nodes: Vec<String> = sym.spice.pin_order.iter().map(|id| net_of(id)).collect();
        // MOS：SPICE 要 4 个节点 d g s b；rustomp 的 Nmos 衬底接源极（设计器 docs/05 §4）
        if p.symbol == "mosfet-n" && nodes.len() == 3 {
            nodes.push(nodes[2].clone());
        }

        // 电位器：展开成两段电阻（按 50% 抽头写标称值），与设计器一致
        if p.symbol == "potentiometer" && nodes.len() == 3 {
            let max = p.value.clone().unwrap_or_else(|| "500k".into());
            let half = half_value(&max);
            elements.push(Element {
                name: format!("R{}A", p.r#ref),
                kind: ElementKind::Resistor,
                nodes: vec![nodes[0].clone(), nodes[1].clone()],
                value: half.clone(),
                model: None,
            });
            elements.push(Element {
                name: format!("R{}B", p.r#ref),
                kind: ElementKind::Resistor,
                nodes: vec![nodes[1].clone(), nodes[2].clone()],
                value: half,
                model: None,
            });
            continue;
        }
        // 网络标签 / 端口 / 测试点 / NC：不产生元件
        if sym.spice.prefix.is_empty() {
            continue;
        }

        let kind = kind_of(&p.symbol, &sym.spice.prefix);
        let name = if sym.spice.prefix == "X" && !p.r#ref.starts_with('X') {
            // SPICE 靠首字母区分器件类型：运放实例必须以 X 开头
            format!("X{}", p.r#ref)
        } else {
            p.r#ref.clone()
        };
        // 电压源的值写全 `DC <v>`：设计器导出的正是这一形式（ngspice 也不会误会）
        let value = if kind == ElementKind::VoltageSource {
            format!("DC {}", p.value.clone().unwrap_or_else(|| "9".into()))
        } else {
            p.value.clone().unwrap_or_default()
        };
        elements.push(Element {
            name,
            kind,
            nodes,
            value,
            model: p.model.clone(),
        });
    }

    let name = if doc.name.is_empty() {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string()
    } else {
        doc.name.clone()
    };
    notes.insert(
        0,
        format!(
            "rustomp-circuit-designer 工程 v{}：{} 个元件 / {} 段导线",
            doc.version.max(1),
            doc.parts.len(),
            doc.wires.len()
        ),
    );

    Ok(ImportedCircuit {
        name,
        format: "circuit-designer",
        elements,
        ground_nets: vec!["0".into()],
        notes,
    })
}

fn kind_of(symbol: &str, prefix: &str) -> ElementKind {
    match symbol {
        "npn" => ElementKind::Npn,
        "pnp" => ElementKind::Pnp,
        "jfet-n" => ElementKind::Jfet,
        "mosfet-n" => ElementKind::Nmos,
        _ => match prefix {
            "R" => ElementKind::Resistor,
            "C" => ElementKind::Capacitor,
            "L" => ElementKind::Inductor,
            "D" => ElementKind::Diode,
            "Q" => ElementKind::Npn,
            "J" => ElementKind::Jfet,
            "M" => ElementKind::Nmos,
            "X" => ElementKind::Subckt,
            "V" => ElementKind::VoltageSource,
            _ => ElementKind::Other,
        },
    }
}

/// 电位器标称值：按 **50% 抽头** 展开（`max/2`），与设计器 `half_value` 逐字一致。
fn half_value(value: &str) -> String {
    let t = value.trim();
    let split = t.find(|c: char| {
        !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
    });
    let (num, unit) = match split {
        Some(i) => t.split_at(i),
        None => (t, ""),
    };
    match num.parse::<f64>() {
        Ok(v) => {
            let h = v / 2.0;
            let txt = format!("{h:.6}");
            let txt = txt.trim_end_matches('0').trim_end_matches('.').to_string();
            format!("{txt}{unit}")
        }
        Err(_) => value.to_string(),
    }
}

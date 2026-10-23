// SPDX-License-Identifier: GPL-3.0-or-later

//! **breadspice** 面包板工程（`*.bread`）导入器。
//!
//! breadspice 的工程里只记录「元件 + 引脚落在哪个插孔（node id）」，
//! 电气连通性由面包板的几何规则决定（见 breadspice 前端 `breadboardLayout.ts`）：
//!
//! - 端子排：`t{row}{a..e}` 横向连通成 net `t{row}L`；`t{row}{f..j}` → `t{row}R`
//! - 电源轨：`rail_{L|R}{p|m}_{i}` → net `rail_{L|R}{p|m}`（纵向整条连通）
//!
//! 这里按同样的规则把 node id 归并成 net，再翻译成中间表示。

use std::path::Path;

use serde_json::Value;

use crate::{sanitize_refdes, spice_value, Element, ElementKind, ImportedCircuit, ImportError, Importer};

pub struct BreadSpice;

impl Importer for BreadSpice {
    fn id(&self) -> &'static str {
        "breadspice"
    }

    fn matches(&self, path: &Path, content: &str) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        ext.eq_ignore_ascii_case("bread") || content.contains("breadspice-bread")
    }

    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError> {
        let doc: Value = serde_json::from_str(content)
            .map_err(|e| ImportError::Parse(format!("JSON 解析失败: {e}")))?;
        if doc.get("format").and_then(|v| v.as_str()) != Some("breadspice-bread") {
            return Err(ImportError::Parse(
                "不是 breadspice 工程文件（format 字段不匹配）".into(),
            ));
        }
        let items = doc
            .get("items")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ImportError::Parse("缺少 items".into()))?;

        let mut notes: Vec<String> = Vec::new();

        // ---- 1. 先把所有引脚的 node id 收集起来，判地 ----
        let mut ground_nets: Vec<String> = Vec::new();
        for item in items {
            let inst = match item.get("instance") {
                Some(i) => i,
                None => continue,
            };
            if inst.get("kind").and_then(|k| k.as_str()) == Some("gnd") {
                for pin in pins_of(inst) {
                    let net = net_of_node(&pin.1);
                    if !ground_nets.contains(&net) {
                        ground_nets.push(net);
                    }
                }
            }
        }

        // ---- 2. 逐个元件翻译 ----
        let mut elements: Vec<Element> = Vec::new();
        for item in items {
            let inst = match item.get("instance") {
                Some(i) => i,
                None => continue,
            };
            let kind_str = inst.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            let refdes = sanitize_refdes(inst.get("refdes").and_then(|v| v.as_str()).unwrap_or(""));
            if refdes.is_empty() {
                continue;
            }
            let value = inst.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let unit = inst.get("unit").and_then(|v| v.as_str());
            let pins = pins_of(inst);
            let node = |names: &[&str], idx: usize| -> Option<String> {
                // 优先按引脚名找，找不到退回第 idx 个引脚
                for want in names {
                    if let Some((_, n)) = pins.iter().find(|(p, _)| p.eq_ignore_ascii_case(want)) {
                        return Some(map_node(n, &ground_nets));
                    }
                }
                pins.get(idx).map(|(_, n)| map_node(n, &ground_nets))
            };
            let ordered = |names: &[&str]| -> Vec<String> {
                (0..names.len())
                    .map(|i| node(&names[i..i + 1], i).unwrap_or_else(|| "0".into()))
                    .collect()
            };

            match kind_str {
                "resistor" | "capacitor" | "inductor" => {
                    let kind = match kind_str {
                        "resistor" => ElementKind::Resistor,
                        "capacitor" => ElementKind::Capacitor,
                        _ => ElementKind::Inductor,
                    };
                    if unit.is_none() {
                        notes.push(format!("{refdes}: 没有单位，按原样输出值 {value:?}"));
                    }
                    elements.push(Element {
                        name: refdes,
                        kind,
                        nodes: vec![
                            node(&["1"], 0).unwrap_or_else(|| "0".into()),
                            node(&["2"], 1).unwrap_or_else(|| "0".into()),
                        ],
                        value: spice_value(value, unit),
                        model: None,
                    });
                }
                "potentiometer" => {
                    // 两级串联电阻：A = 总阻值 × percent（引脚 1-2），B = 其余（引脚 2-3）
                    let total = parse_ohm(value, unit);
                    let percent = inst
                        .get("params")
                        .and_then(|p| p.get("percent"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.5);
                    let n1 = node(&["1"], 0).unwrap_or_else(|| "0".into());
                    let n2 = node(&["2"], 1).unwrap_or_else(|| "0".into());
                    let n3 = node(&["3"], 2).unwrap_or_else(|| "0".into());
                    elements.push(Element {
                        name: format!("{refdes}A"),
                        kind: ElementKind::Resistor,
                        nodes: vec![n1, n2.clone()],
                        value: format_ohm(total * percent),
                        model: None,
                    });
                    elements.push(Element {
                        name: format!("{refdes}B"),
                        kind: ElementKind::Resistor,
                        nodes: vec![n2, n3],
                        value: format_ohm(total * (1.0 - percent)),
                        model: None,
                    });
                    notes.push(format!(
                        "{refdes}: 电位器按 {:.0}% 展开成两段电阻（A/B），参数化时对应抽头位置",
                        percent * 100.0
                    ));
                }
                "diode" | "led" => {
                    let model = match (kind_str, value) {
                        ("led", "red") => "LedRed".to_string(),
                        ("led", "green") => "LedGreen".to_string(),
                        ("led", "blue") => "LedBLUE".to_string(),
                        (_, v) => v.to_string(),
                    };
                    elements.push(Element {
                        name: refdes,
                        kind: ElementKind::Diode,
                        nodes: ordered(&["a", "k"]),
                        value: String::new(),
                        model: Some(model),
                    });
                }
                "opamp" | "opamp2" => {
                    let model = match value {
                        "OP07" => "OP07A".to_string(),
                        other => other.to_string(),
                    };
                    elements.push(Element {
                        name: refdes,
                        kind: ElementKind::Subckt,
                        nodes: ordered(&["IN+", "IN-", "V+", "V-", "OUT"]),
                        value: String::new(),
                        model: Some(model),
                    });
                }
                "npn" | "pnp" => {
                    elements.push(Element {
                        name: refdes,
                        kind: if kind_str == "npn" { ElementKind::Npn } else { ElementKind::Pnp },
                        nodes: ordered(&["C", "B", "E"]),
                        value: String::new(),
                        model: Some(value.to_string()),
                    });
                }
                "jfet" => {
                    elements.push(Element {
                        name: refdes,
                        kind: ElementKind::Jfet,
                        nodes: ordered(&["D", "G", "S"]),
                        value: String::new(),
                        model: Some(value.to_string()),
                    });
                }
                "nmos" | "pmos" => {
                    elements.push(Element {
                        name: refdes,
                        kind: if kind_str == "nmos" { ElementKind::Nmos } else { ElementKind::Pmos },
                        nodes: ordered(&["D", "G", "S"]),
                        value: String::new(),
                        model: Some(value.to_string()),
                    });
                }
                "power" => {
                    elements.push(Element {
                        name: refdes,
                        kind: ElementKind::VoltageSource,
                        nodes: ordered(&["+", "-"]),
                        value: spice_value(value, unit),
                        model: None,
                    });
                }
                "vsine" => {
                    let p = inst.get("params");
                    let get = |k: &str, d: &str| -> String {
                        p.and_then(|p| p.get(k))
                            .and_then(|v| v.as_str())
                            .unwrap_or(d)
                            .to_string()
                    };
                    let (dc, ac, freq, phase) =
                        (get("dc", "0"), get("ac", "0.2"), get("freq", "1k"), get("phase", "0"));
                    elements.push(Element {
                        name: refdes,
                        kind: ElementKind::VoltageSource,
                        nodes: ordered(&["+", "-"]),
                        value: format!("DC {dc} AC {ac} SIN({dc} {ac} {freq} 0 0 {phase})"),
                        model: None,
                    });
                }
                "wire" | "jumper" => {
                    elements.push(Element {
                        name: refdes,
                        kind: ElementKind::Resistor,
                        nodes: vec![
                            node(&["1"], 0).unwrap_or_else(|| "0".into()),
                            node(&["2"], 1).unwrap_or_else(|| "0".into()),
                        ],
                        value: "0.001".into(),
                        model: None,
                    });
                }
                "gnd" => { /* 已在第 1 步记入 ground_nets */ }
                other => {
                    notes.push(format!("{refdes}: 跳过不参与仿真的元件（kind = {other}）"));
                }
            }
        }

        if elements.is_empty() {
            return Err(ImportError::Parse("没有解析到任何元件".into()));
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported")
            .to_string();
        Ok(ImportedCircuit {
            name,
            format: "breadspice",
            elements,
            ground_nets,
            notes,
        })
    }
}

fn pins_of(inst: &Value) -> Vec<(String, String)> {
    inst.get("pins")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some((
                        p.get("name")?.as_str()?.to_string(),
                        p.get("node")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 面包板 node id → 电气 net（几何规则）。
fn net_of_node(node: &str) -> String {
    if let Some(rest) = node.strip_prefix("rail_") {
        // rail_Lp_11 → rail_Lp
        if let Some((head, _)) = rest.rsplit_once('_') {
            return format!("rail_{head}");
        }
    }
    if let Some(rest) = node.strip_prefix('t') {
        // t11L / t9e → t11L / t9L
        let letters = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        if let Some(group) = letters.chars().next() {
            let row = &rest[..rest.len() - letters.len()];
            let side = match group {
                'a'..='e' => 'L',
                'f'..='j' => 'R',
                _ => 'L',
            };
            return format!("t{row}{side}");
        }
    }
    node.to_string()
}

fn map_node(node: &str, ground: &[String]) -> String {
    let net = net_of_node(node);
    if ground.iter().any(|g| *g == net) {
        "0".to_string()
    } else {
        crate::node_name(&net)
    }
}

fn parse_ohm(value: &str, unit: Option<&str>) -> f64 {
    let v: f64 = value.trim().replace(',', ".").parse().unwrap_or(0.0);
    match unit.map(|u| u.trim()) {
        Some("kΩ") | Some("k") => v * 1e3,
        Some("MΩ") | Some("meg") => v * 1e6,
        Some("GΩ") => v * 1e9,
        _ => v,
    }
}

fn format_ohm(v: f64) -> String {
    if v >= 1e6 {
        format!("{}Meg", trim_num(v / 1e6))
    } else if v >= 1e3 {
        format!("{}k", trim_num(v / 1e3))
    } else {
        format!("{}", trim_num(v))
    }
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

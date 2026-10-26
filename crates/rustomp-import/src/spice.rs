// SPDX-License-Identifier: GPL-3.0-or-later

//! **SPICE 网表导入器**（`.cir` / `.net` / `.spice`）。
//!
//! 只做"网表 → 中间表示"，不做任何电路语义推断；语义（引脚、模型映射、固定节点）
//! 交给 `mna_codegen`。这样同一份 IR 也能被 SPICE 回写 / 骨架生成复用。
//!
//! 支持：`R C L D V I Q J M X` 元件、`.model`（只记名字）、`.subckt/.ends`（整段跳过）、
//! 续行 `+`、注释 `*`、以及 `* @rustomp ...` 指令行（原样收进 notes）。

use crate::{Element, ElementKind, ImportError, ImportedCircuit, Importer};
use std::path::Path;

pub struct SpiceNetlist;

impl Importer for SpiceNetlist {
    fn id(&self) -> &'static str {
        "spice"
    }

    fn matches(&self, path: &Path, content: &str) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if matches!(ext.as_str(), "cir" | "net" | "spice" | "sp") {
            return true;
        }
        // 内容嗅探：有元件行 + `.end`
        let has_element = content.lines().any(
            |l| matches!(l.chars().next(), Some(c) if "rclvdqjmx".contains(c.to_ascii_lowercase())),
        );
        has_element && content.to_lowercase().contains(".end")
    }

    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError> {
        let mut elements = Vec::new();
        let mut notes = Vec::new();
        let mut in_subckt: Option<String> = None;
        let mut title = String::new();

        for raw in join_continuations(content) {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('*') {
                // 指令行：`* @rustomp in=VS1 out=n_t27R`
                if let Some(directive) = rest.trim().strip_prefix("@rustomp") {
                    notes.push(format!("@rustomp{}", directive.trim_end()));
                }
                continue;
            }
            let lower = line.to_lowercase();
            if lower.starts_with(".subckt") {
                let name = line.split_whitespace().nth(1).unwrap_or("").to_string();
                in_subckt = Some(name);
                continue;
            }
            if lower.starts_with(".ends") {
                in_subckt = None;
                continue;
            }
            if lower.starts_with('.') {
                // .model/.tran/.op/.ac/.end …… 都不影响拓扑
                continue;
            }
            if in_subckt.is_some() {
                continue;
            }
            if title.is_empty() && !line.starts_with(|c: char| c.is_ascii_alphabetic()) {
                title = line.to_string();
                continue;
            }
            match parse_element(line) {
                Ok(Some(el)) => elements.push(el),
                Ok(None) => {}
                Err(e) => notes.push(format!("跳过无法解析的行：{line}（{e}）")),
            }
        }

        if elements.is_empty() {
            return Err(ImportError::Parse(format!(
                "{} 里没找到任何元件行",
                path.display()
            )));
        }
        if title.is_empty() {
            title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string();
        }

        Ok(ImportedCircuit {
            name: title,
            format: "spice",
            elements,
            ground_nets: vec!["0".into()],
            notes,
        })
    }
}

/// 把 `+` 续行接到上一行。
fn join_continuations(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('+') {
            if let Some(last) = out.last_mut() {
                last.push(' ');
                last.push_str(rest.trim());
                continue;
            }
        }
        out.push(line.to_string());
    }
    out
}

/// 解析一行元件。返回 `Ok(None)` 表示"认识但不需要保留"。
fn parse_element(line: &str) -> Result<Option<Element>, String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err("字段不足".into());
    }
    let name = tokens[0].to_string();
    let kind_char = name.chars().next().unwrap().to_ascii_lowercase();

    let el = match kind_char {
        'r' | 'c' | 'l' => {
            // R<name> a b value
            let kind = match kind_char {
                'r' => ElementKind::Resistor,
                'c' => ElementKind::Capacitor,
                _ => ElementKind::Inductor,
            };
            Element {
                name,
                kind,
                nodes: vec![tokens[1].to_string(), tokens[2].to_string()],
                value: tokens.get(3).copied().unwrap_or("0").to_string(),
                model: None,
            }
        }
        'd' => {
            // D<name> anode cathode model  （模型名可省略 = 默认二极管）
            let model = tokens.get(3).map(|s| s.to_string());
            Element {
                name,
                kind: ElementKind::Diode,
                nodes: vec![tokens[1].to_string(), tokens[2].to_string()],
                value: String::new(),
                model,
            }
        }
        'v' | 'i' => {
            // V<name> a b [DC] value [AC amp] [SIN(...)]
            let rest = tokens[3..].join(" ");
            Element {
                name,
                kind: ElementKind::VoltageSource,
                nodes: vec![tokens[1].to_string(), tokens[2].to_string()],
                value: rest,
                model: None,
            }
        }
        'q' => {
            if tokens.len() < 5 {
                return Err("BJT 字段不足".into());
            }
            let model = tokens.last().map(|s| s.to_string());
            let kind = ElementKind::Npn; // 极性由模型名决定，codegen 再判定
            Element {
                name,
                kind,
                nodes: vec![
                    tokens[1].to_string(),
                    tokens[2].to_string(),
                    tokens[3].to_string(),
                ],
                value: String::new(),
                model,
            }
        }
        'j' => Element {
            name,
            kind: ElementKind::Jfet,
            nodes: vec![
                tokens[1].to_string(),
                tokens[2].to_string(),
                tokens[3].to_string(),
            ],
            value: String::new(),
            model: tokens.get(4).map(|s| s.to_string()),
        },
        'm' => Element {
            name,
            kind: ElementKind::Nmos,
            nodes: vec![
                tokens[1].to_string(),
                tokens[2].to_string(),
                tokens[3].to_string(),
            ],
            value: String::new(),
            model: tokens.get(5).map(|s| s.to_string()),
        },
        'x' => {
            // X<name> n1 n2 … <subckt>
            let model = tokens.last().map(|s| s.to_string());
            let nodes: Vec<String> = tokens[1..tokens.len() - 1]
                .iter()
                .map(|s| s.to_string())
                .collect();
            Element {
                name,
                kind: ElementKind::Subckt,
                nodes,
                value: String::new(),
                model,
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(el))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Importer;

    const SAMPLE: &str = "\
* 测试网表
V1 in 0 DC 0 AC 0.2 SIN(0 0.2 1k 0 0 0)
R1 in x 1k
C1 x 0 4.7n
D1 x 0 1N4148
XU1 x 0 vcc 0 out OP07A
* @rustomp in=V1 out=out
.SUBCKT OP07A 3 2 7 4 6
RC1 7 80 8.842E+03
.ENDS OP07A
.MODEL 1N4148 D(IS=2.52n)
.end
";

    #[test]
    fn parses_elements_and_skips_subckt_body() {
        let c = SpiceNetlist.import(Path::new("t.cir"), SAMPLE).unwrap();
        assert_eq!(c.format, "spice");
        // V1 R1 C1 D1 XU1（子电路内的 RC1 应被跳过）
        assert_eq!(
            c.elements.len(),
            5,
            "{:?}",
            c.elements.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(
            c.notes.iter().any(|n| n.contains("in=V1")),
            "指令行应进 notes：{:?}",
            c.notes
        );
        let d = c.elements.iter().find(|e| e.name == "D1").unwrap();
        assert_eq!(d.model.as_deref(), Some("1N4148"));
        assert_eq!(d.nodes, vec!["x", "0"]);
    }

    #[test]
    fn sniffs_cir_extension() {
        assert!(SpiceNetlist.matches(Path::new("a.cir"), ""));
        assert!(!SpiceNetlist.matches(Path::new("a.bread"), "{}"));
    }
}

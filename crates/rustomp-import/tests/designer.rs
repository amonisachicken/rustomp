// SPDX-License-Identifier: GPL-3.0-or-later

//! **rustomp-circuit-designer 工程导入的逐行对拍**。
//!
//! 夹具三件套：
//!
//! | 文件 | 来源 |
//! |---|---|
//! | `tests/data/pretty-face.design.json` | 设计器真实工程（27 元件 / 30 导线，两只 AC127 的锗管法兹） |
//! | `tests/data/pretty-face.cir` | **设计器自己**导出的网表（同一份工程） |
//! | `tests/data/designer-symbols.json` | 设计器 `symbols/symbols.json` 的快照（引脚几何 + `pinOrder`） |
//!
//! 第 2 个文件是裁判：`rustomp-import` 直接读工程文件走出来的网表，
//! 必须与设计器的导出**逐行一致**。几何规则一旦漂了（变换、T 型接点、网名排序），
//! 这个测试立刻红 —— 这正是设计器 DEVLOG 里那条"看起来对、网表却是错的"的防线。
//!
//! 重新生成夹具（设计器仓库）：
//!
//! ```bash
//! cd rustomp-circuit-designer/backend && CD_PORT=3611 ./target/debug/circuit-designer-backend &
//! # POST /rpc {"method":"designer.netlist","params":<.design.json 的内容>} → result.cir
//! ```
//!
//! 注意：快照是有意冻结的（测试要能独立跑）——设计器改了符号几何时，
//! 运行期仍用**本机那份** `symbols.json`（`RUSTOM_DESIGNER_SYMBOLS` 可指定），
//! 快照只用来守住"算法与设计器一致"。

use rustomp_import::designer::{import_with_table, SymbolTable};
use rustomp_import::mna_codegen::{generate, Options};
use std::path::Path;

const DESIGN: &str = include_str!("data/pretty-face.design.json");
const DESIGNER_CIR: &str = include_str!("data/pretty-face.cir");
const SYMBOLS: &str = include_str!("data/designer-symbols.json");

/// 第二组夹具：最小可用图（比极简的 10k+100n 低通），用来盯住几何/端口/接地的常规路径
const LOWPASS: &str = include_str!("data/lowpass.design.json");
const LOWPASS_CIR: &str = include_str!("data/lowpass.cir");

fn import() -> rustomp_import::ImportedCircuit {
    let table = SymbolTable::from_json(SYMBOLS).expect("symbols.json 快照解析失败");
    import_with_table(Path::new("pretty-face.design.json"), DESIGN, &table)
        .expect("导入设计器工程失败")
}

/// 只留元件行（`R1 …` / `XU1 …`），去掉注释与 `.end`
fn element_lines(cir: &str) -> Vec<String> {
    cir.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('*') && !l.starts_with('.'))
        .map(|l| l.to_string())
        .collect()
}

#[test]
fn 与设计器导出的网表逐行一致() {
    for (name, design, golden) in [
        ("pretty-face", DESIGN, DESIGNER_CIR),
        ("lowpass", LOWPASS, LOWPASS_CIR),
    ] {
        let table = SymbolTable::from_json(SYMBOLS).unwrap();
        let imported = import_with_table(Path::new(name), design, &table).expect("导入失败");
        let ours = element_lines(&imported.to_spice());
        let theirs = element_lines(golden);
        assert_eq!(
            ours, theirs,
            "{name}：读工程文件得到的网表与设计器导出不一致\n我们的：{ours:#?}\n设计器的：{theirs:#?}"
        );
    }
}

#[test]
fn 指令与设计器一致() {
    let imported = import();
    let ours: Vec<&String> = imported
        .notes
        .iter()
        .filter(|n| n.starts_with("@rustomp"))
        .collect();
    let theirs: Vec<String> = DESIGNER_CIR
        .lines()
        .filter_map(|l| l.trim().strip_prefix("* "))
        .filter(|l| l.starts_with("@rustomp"))
        .map(|l| l.to_string())
        .collect();
    assert_eq!(ours, theirs.iter().collect::<Vec<_>>(), "指令行不一致");
}

#[test]
fn 元件分类正确() {
    let c = import();
    assert_eq!(c.format, "circuit-designer");
    assert_eq!(c.name, "pretty-face");
    let count = |k: &str| {
        c.elements
            .iter()
            .filter(|e| e.kind.spice_prefix() == k)
            .count()
    };
    assert_eq!(count("C"), 5, "{}", c.summary());
    assert_eq!(count("R"), 8, "电位器各展开成两段：{}", c.summary());
    assert_eq!(count("Q"), 2, "{}", c.summary());
    assert_eq!(count("V"), 1, "{}", c.summary());
    // AC127 是锗 PNP，但设计器里用的是 `npn` 符号 + AC127 型号 → 类别由符号决定
    assert!(c
        .elements
        .iter()
        .filter(|e| e.kind.spice_prefix() == "Q")
        .all(|e| e.kind == rustomp_import::ElementKind::Npn));
}

#[test]
fn 生成代码带输入端口而不是退化() {
    let c = import();
    let gen = generate(&c, &Options::default()).expect("生成 MNA 代码失败");
    // 关键回归：设计器网表是"纯拓扑"，没有任何 SIN/AC 源；
    // `in=IN` 必须被当成**节点名**，否则这里会退化成 "pub const IN: usize = 0"
    assert!(
        gen.code.contains("pub const IN: usize = "),
        "没生成输入端口：\n{}",
        gen.code
    );
    assert!(
        !gen.code.contains("无输入端口（纯直流电路）"),
        "输入端口退化成 0 了：\n{}",
        gen.code
    );
    assert!(
        gen.code.contains("self.net.set_fixed(FIXED_IN, x);"),
        "process() 没驱动输入"
    );
    assert!(
        gen.code.contains("net.fix_node(IN, 0.0)"),
        "build() 没固定输入节点"
    );
    assert_eq!(gen.unknowns, 11, "{:?}", gen.notes);
}

#[test]
fn 错误的工程给出可读错误() {
    let table = SymbolTable::from_json(SYMBOLS).unwrap();
    // ① 未知符号（设计器版本不一致）
    let doc = r#"{"version":1,"name":"x","parts":[{"id":"p1","ref":"R1","symbol":"flux-capacitor","at":{"x":0,"y":0}}],"wires":[]}"#;
    let e = import_with_table(Path::new("x.design.json"), doc, &table).unwrap_err();
    assert!(format!("{e}").contains("未知符号"), "{e}");

    // ② 没有接地
    let doc = r#"{"version":1,"name":"x","parts":[{"id":"p1","ref":"R1","symbol":"resistor","value":"10k","at":{"x":0,"y":0}},{"id":"p2","ref":"R2","symbol":"resistor","value":"10k","at":{"x":30,"y":0}}],"wires":[{"id":"w1","points":[{"x":12.5,"y":0},{"x":17.5,"y":0}]}]}"#;
    let e = import_with_table(Path::new("x.design.json"), doc, &table).unwrap_err();
    assert!(format!("{e}").contains("没有接地符号"), "{e}");
}

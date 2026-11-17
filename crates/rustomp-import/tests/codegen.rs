// SPDX-License-Identifier: GPL-3.0-or-later

//! **S1a 生成器的漂移守卫**：仓库里的生成物必须等于"现在重新生成"的结果。
//!
//! 生成物在 `crates/rustomp-dsp/tests/generated/sparklin_drive.rs`，
//! 由 `crates/rustomp-dsp/tests/sparklin_generated.rs` `include!` 后做 THD 回归。
//!
//! 更新流程：改网表 → `cargo run -p rustomp-import -- <网表> --mna <生成物路径>` → 跑本测试。

use rustomp_import::mna_codegen::{generate, Options};
use std::path::Path;

/// (夹具, 生成物, 生成物头部写的文件名)
const CASES: [(&str, &str, &str); 2] = [
    (
        "tests/fixtures/sparklin_drive.cir",
        "../rustomp-dsp/tests/generated/sparklin_drive.rs",
        "sparklin_drive.cir",
    ),
    (
        "tests/fixtures/bjt_stage.cir",
        "../rustomp-dsp/tests/generated/bjt_stage.rs",
        "bjt_stage.cir",
    ),
];

fn regenerate(fixture: &str, source: &str) -> String {
    let circuit = rustomp_import::import_file(Path::new(fixture)).expect("导入夹具网表失败");
    let opts = Options {
        input: None,
        output: None,
        source: source.into(),
    };
    generate(&circuit, &opts).expect("生成失败").code
}

/// 设计器工程文件（`.design.json`）→ 生成物。
///
/// 用快照的 `symbols.json`（而不是本机那份）：守卫必须能独自跑，不依赖设计器仓库在不在。
fn regenerate_designer(fixture: &str, source: &str) -> String {
    let table = rustomp_import::designer::SymbolTable::from_json(include_str!(
        "data/designer-symbols.json"
    ))
    .expect("symbols.json 快照解析失败");
    let content = std::fs::read_to_string(fixture).expect("读不到设计器工程夹具");
    let circuit = rustomp_import::designer::import_with_table(Path::new(fixture), &content, &table)
        .expect("导入设计器工程失败");
    let opts = Options {
        input: None,
        output: None,
        source: source.into(),
    };
    generate(&circuit, &opts).expect("生成失败").code
}

#[test]
fn designer_project_is_up_to_date() {
    // (夹具, 生成物)：lowpass = 线性最小例；pretty-face = 锗管法兹（非线性/饱和区的回归现场）
    for (fixture, golden) in [
        (
            "tests/data/lowpass.design.json",
            "../rustomp-dsp/tests/generated/lowpass.rs",
        ),
        (
            "tests/data/pretty-face.design.json",
            "../rustomp-dsp/tests/generated/pretty_face.rs",
        ),
        (
            "tests/data/diode-clip.design.json",
            "../rustomp-dsp/tests/generated/diode_clip.rs",
        ),
    ] {
        let source = fixture.rsplit_once('/').unwrap().1;
        let fresh = regenerate_designer(fixture, source);
        let repo = std::fs::read_to_string(golden).unwrap_or_default();
        assert_eq!(
            fresh, repo,
            "{golden} 与重新生成的结果不一致。请重新生成：\n  \
             RUSTOM_DESIGNER_SYMBOLS=<设计器>/symbols/symbols.json \
             cargo run -q -p rustomp-import -- {fixture} --mna {golden}"
        );
    }
}

#[test]
fn all_generated_files_are_up_to_date() {
    for (fixture, golden, source) in CASES {
        let fresh = regenerate(fixture, source);
        let repo = std::fs::read_to_string(golden).unwrap_or_default();
        assert_eq!(
            fresh, repo,
            "{golden} 与重新生成的结果不一致。请重新生成：\n  \
             cargo run -q -p rustomp-import -- {fixture} --mna {golden}"
        );
    }
}

#[test]
fn unknown_model_is_rejected_with_help() {
    let src = "\
* 未知型号
VS1 in 0 DC 0 AC 1
R1 in out 1k
D1 out 0 BS170
* @rustomp out=out
.end
";
    let circuit = rustomp_import::import_str("t.cir", src).expect("导入失败");
    let err = match generate(
        &circuit,
        &Options {
            input: None,
            output: None,
            source: "t.cir".into(),
        },
    ) {
        Ok(_) => panic!("未知型号 BS170 应该被拒绝"),
        Err(e) => e,
    };
    assert!(err.contains("BS170"), "错误信息里应点名未知型号：{err}");
    assert!(err.contains("闭集"), "应提示这是闭集库：{err}");
}

#[test]
fn wire_resistors_are_merged() {
    let src = "\
VS1 in 0 DC 0 AC 1
VCC vcc 0 9
R1 in a 1k
RW1 a b 0.001
R2 b 0 1k
* @rustomp out=b
.end
";
    let circuit = rustomp_import::import_str("t.cir", src).expect("导入失败");
    let gen = generate(
        &circuit,
        &Options {
            input: None,
            output: None,
            source: "t.cir".into(),
        },
    )
    .unwrap();
    // a、b 合并成一个节点 → 只剩一个节点常量，且没有 0.001Ω 的电阻
    // N = 节点数(3: a/in/vcc) + 1（下标 0 让给地）
    assert_eq!(gen.unknowns, 4, "未知量个数不对：{gen:?}");
    assert!(gen.code.contains("node::A"), "应保留合并后的节点 a");
    assert!(!gen.code.contains("node::B"), "b 应被合并进 a");
    assert!(!gen.code.contains("0.001"), "导线电阻不应出现在生成代码里");
    assert!(gen.notes.iter().any(|n| n.contains("视为导线")));
}

// ---------------------------------------------------------------------------
// P2：`M` 会被当成 milli（SPICE 语义没错），但"1MΩ 的偏置电位器"是常见笔误。
// 这里守两件事：电位器总阻值 < 1 Ω 直接拒绝；普通电阻的 `1M` 给 warning 但放行。
// ---------------------------------------------------------------------------

fn gen_from_spice(
    name: &str,
    content: &str,
) -> Result<rustomp_import::mna_codegen::Generated, String> {
    let circuit = rustomp_import::import_str(name, content).expect("导入失败");
    let opts = Options {
        input: None,
        output: None,
        source: name.into(),
    };
    generate(&circuit, &opts)
}

#[test]
fn pot_with_mega_typo_is_rejected_with_hint() {
    let err = gen_from_spice(
        "mega.cir",
        "* @rustomp in=IN out=OUT\n\
         * @rustomp param RP1A p1 pot max=1M\n\
         * @rustomp param RP1B p1 pot max=1M invert\n\
         V1 IN 0 DC 0\n\
         RP1A IN OUT 0.5M\n\
         RP1B OUT 0 0.5M\n\
         R1 OUT 0 1meg\n",
    )
    .err()
    .expect("`max=1M` 应该被拒绝");
    assert!(err.contains("1meg"), "错误提示应给出正确写法：{err}");
    assert!(err.contains("milli"), "错误提示应解释 M 的含义：{err}");
}

#[test]
fn meg_is_accepted() {
    let gen = gen_from_spice(
        "meg.cir",
        "* @rustomp in=IN out=OUT\n\
         * @rustomp param RP1A p1 pot max=1meg\n\
         V1 IN 0 DC 0\n\
         RP1A IN OUT 0.5meg\n\
         R1 OUT 0 1meg\n",
    )
    .expect("`max=1meg` 应该正常生成");
    assert!(
        gen.warnings.is_empty(),
        "不该有 warning：{:?}",
        gen.warnings
    );
}

#[test]
fn plain_resistor_with_mega_typo_warns_but_still_generates() {
    let gen = gen_from_spice(
        "warn.cir",
        "* @rustomp in=IN out=OUT\n\
         V1 IN 0 DC 0\n\
         R1 IN OUT 1M\n\
         R2 OUT 0 10k\n",
    )
    .expect("只警告，不阻断");
    assert_eq!(
        gen.warnings.len(),
        1,
        "应有 1 条 warning：{:?}",
        gen.warnings
    );
    assert!(gen.warnings[0].contains("R1"));
    assert!(gen.warnings[0].contains("1meg"));
}

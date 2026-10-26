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

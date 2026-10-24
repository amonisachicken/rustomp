// SPDX-License-Identifier: GPL-3.0-or-later

//! **S1a 生成器的漂移守卫**：仓库里的生成物必须等于"现在重新生成"的结果。
//!
//! 生成物在 `crates/rustomp-dsp/tests/generated/sparklin_drive.rs`，
//! 由 `crates/rustomp-dsp/tests/sparklin_generated.rs` `include!` 后做 THD 回归。
//!
//! 更新流程：改网表 → `cargo run -p rustomp-import -- <网表> --mna <生成物路径>` → 跑本测试。

use rustomp_import::mna_codegen::{generate, Options};
use std::path::Path;

const FIXTURE: &str = "tests/fixtures/sparklin_drive.cir";
const GOLDEN: &str = "../rustomp-dsp/tests/generated/sparklin_drive.rs";

fn regenerate() -> String {
    let circuit = rustomp_import::import_file(Path::new(FIXTURE)).expect("导入夹具网表失败");
    let opts = Options {
        input: None,
        output: None,
        source: "sparklin_drive.cir".into(),
    };
    generate(&circuit, &opts).expect("生成失败").code
}

#[test]
fn generated_file_is_up_to_date() {
    let fresh = regenerate();
    let golden = std::fs::read_to_string(GOLDEN).unwrap_or_default();
    if fresh != golden {
        // 给出第一处差异，方便定位
        let diff_line = fresh
            .lines()
            .zip(golden.lines())
            .position(|(a, b)| a != b)
            .map(|i| i + 1)
            .unwrap_or(0);
        panic!(
            "{GOLDEN} 与重新生成的结果不一致（第一处差异在第 {diff_line} 行）。\n\
             请重新生成：\n  cargo run -q -p rustomp-import -- {FIXTURE} --mna {GOLDEN}\n\
             --- 新生成 ---\n{}\n--- 仓库里 ---\n{}",
            fresh.lines().nth(diff_line.saturating_sub(1)).unwrap_or(""),
            golden.lines().nth(diff_line.saturating_sub(1)).unwrap_or("")
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
        &Options { input: None, output: None, source: "t.cir".into() },
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
    let gen = generate(&circuit, &Options { input: None, output: None, source: "t.cir".into() }).unwrap();
    // a、b 合并成一个节点 → 只剩一个节点常量，且没有 0.001Ω 的电阻
    // N = 节点数(3: a/in/vcc) + 1（下标 0 让给地）
    assert_eq!(gen.unknowns, 4, "未知量个数不对：{gen:?}");
    assert!(gen.code.contains("node::A"), "应保留合并后的节点 a");
    assert!(!gen.code.contains("node::B"), "b 应被合并进 a");
    assert!(!gen.code.contains("0.001"), "导线电阻不应出现在生成代码里");
    assert!(gen.notes.iter().any(|n| n.contains("视为导线")));
}

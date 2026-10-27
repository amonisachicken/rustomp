// SPDX-License-Identifier: GPL-3.0-or-later

//! **M-C 闭环**：设计器工程 → 生成的 `Ckt` → THD 与 **ngspice 同一份网表**对拍。
//!
//! ```text
//! rustomp-circuit-designer 工程（.design.json）
//!   → rustomp-import --mna → tests/generated/pretty_face.rs（本测试 include!）
//!   → 我们的 Ckt 跑 1 kHz 正弦 → THD
//!   ↕ 同一张网表（设计器导出的 .cir + VSIM + devices.lib）→ ngspice → THD
//! ```
//!
//! 这是 M-C 里程碑的定义（`docs/06-roadmap.md`）：**同一把尺子**量两边。
//! 判据取 `docs/component-library.md` §5 实验 2 的量级（1 kHz），
//! 但这个电路是真·硬削波（THD ~64%），绝对误差按 pp 计。

include!("generated/pretty_face.rs");

use rustomp_dsp::Circuit;
use rustomp_oracle::Oracle;

/// 设计器导出的网表（纯拓扑，没有激励源）
const CIR: &str = include_str!("../../rustomp-import/tests/data/pretty-face.cir");
/// 器件模型（AC127 / BC549C …）
const LIB: &str = include_str!("../../../devices/devices.lib");

const FS: f64 = 192_000.0;
const AMP: f64 = 0.1;
const F0: f64 = 1_000.0;
/// 分析窗：最后 20 ms（正好 20 个周期）
const TAIL: f64 = 0.02;

/// 我们的 `Ckt` 跑 `seconds` 秒，返回稳态窗内的输出
fn ours(seconds: f64) -> Vec<f64> {
    let mut c = Ckt::new(FS);
    let dt = 1.0 / FS;
    let n = (seconds / dt) as usize;
    let n_tail = (TAIL / dt) as usize;
    let mut out = Vec::with_capacity(n_tail);
    for i in 0..n {
        let t = i as f64 * dt;
        let x = AMP * (std::f64::consts::TAU * F0 * t).sin();
        let y = c.process(x);
        if i >= n - n_tail {
            out.push(y);
        }
    }
    out
}

/// ngspice 跑同一份网表（补上激励源），返回**重采样到 192 kHz** 的稳态输出
fn ngspice(oracle: &Oracle, seconds: f64) -> Vec<f64> {
    // 去掉设计器网表里的 `.end`，换成我们自己的分析语句
    let body: String = CIR
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .collect::<Vec<_>>()
        .join("\n");
    let netlist = format!(
        "* M-C closed loop\n{LIB}\nVSIM IN 0 DC 0 SIN(0 {AMP} {F0} 0 0 0)\n{body}\n\
         .tran 1e-6 {seconds} {}\n.end\n",
        seconds - TAIL
    );
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let v = raw.get("v(out)").expect("rawfile 里没有 v(out)");
    let t = raw.get("time").expect("rawfile 里没有 time");
    let span = t.last().unwrap() - t.first().unwrap();
    assert!(
        span > TAIL * 0.9,
        "ngspice 只保存了 {span:.4} s，不够一个分析窗"
    );
    let uni = raw.resample("v(out)", 1.0 / FS).expect("重采样失败");
    // 重采样后取最后 TAIL 秒
    let n_tail = (TAIL * FS) as usize;
    let _ = v;
    uni[uni.len() - n_tail..].to_vec()
}

fn thd(x: &[f64]) -> f64 {
    let h = rustomp_oracle::harmonics(x, FS, F0, 9);
    let hp = rustomp_oracle::harmonics_percent(&h);
    rustomp_oracle::thd_percent_from_percent(&hp)
}

#[test]
fn designer_project_thd_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    let seconds = 1.5;
    let a = ours(seconds);
    let b = ngspice(&oracle, seconds);
    let (ta, tb) = (thd(&a), thd(&b));
    println!("\n=== M-C 闭环（pretty-face，{AMP} V / {F0} Hz）===");
    println!("  我们的 Ckt : THD = {ta:.2} %");
    println!("  ngspice    : THD = {tb:.2} %");
    println!("  差         : {:+.2} pp", ta - tb);
    assert!(
        (ta - tb).abs() < 2.0,
        "THD 与 ngspice 差 {:+.2} pp（判据 < 2 pp）",
        ta - tb
    );
}

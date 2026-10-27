// SPDX-License-Identifier: GPL-3.0-or-later

//! **M-C 闭环（二极管削波）**：设计器工程 → 生成的 `Ckt` → THD 与 ngspice 对拍。
//!
//! 1N4148 ∥ 1N34A 反并联 + 10k 串联：不对称硬削波（THD 几个百分点），
//! 正好检验"非线性 + 器件模型"这条链，而且不含运放（ngspice 侧用同一份 `devices.lib`）。

include!("generated/diode_clip.rs");

use rustomp_dsp::Circuit;
use rustomp_oracle::Oracle;

const CIR: &str = include_str!("../../rustomp-import/tests/data/diode-clip.cir");
const LIB: &str = include_str!("../../../devices/devices.lib");

const FS: f64 = 192_000.0;
const AMP: f64 = 1.0;
const F0: f64 = 1_000.0;
const TAIL: f64 = 0.02;

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

fn ngspice(oracle: &Oracle, seconds: f64) -> Vec<f64> {
    let body: String = CIR
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .collect::<Vec<_>>()
        .join("\n");
    let netlist = format!(
        "* M-C closed loop (diode clip)\n{LIB}\nVSIM IN 0 DC 0 SIN(0 {AMP} {F0} 0 0 0)\n{body}\n\
         .tran 1e-6 {seconds} {}\n.end\n",
        seconds - TAIL
    );
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let uni = raw.resample("v(out)", 1.0 / FS).expect("重采样失败");
    uni[uni.len() - (TAIL * FS) as usize..].to_vec()
}

fn thd(x: &[f64]) -> f64 {
    let h = rustomp_oracle::harmonics(x, FS, F0, 9);
    rustomp_oracle::thd_percent_from_percent(&rustomp_oracle::harmonics_percent(&h))
}

#[test]
fn diode_clip_thd_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    let seconds = 0.3;
    let (ta, tb) = (thd(&ours(seconds)), thd(&ngspice(&oracle, seconds)));
    println!("\n=== M-C 闭环（diode-clip，{AMP} V / {F0} Hz）===");
    println!("  我们的 Ckt : THD = {ta:.3} %");
    println!("  ngspice    : THD = {tb:.3} %");
    println!("  差         : {:+.3} pp", ta - tb);
    assert!(
        (ta - tb).abs() < 1.0,
        "THD 与 ngspice 差 {:+.3} pp（判据 < 1 pp）",
        ta - tb
    );
}

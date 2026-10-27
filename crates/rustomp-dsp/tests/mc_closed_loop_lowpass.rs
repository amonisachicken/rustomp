// SPDX-License-Identifier: GPL-3.0-or-later

//! **M-C 闭环（线性对照）**：设计器工程 → 生成的 `Ckt` → THD 与 ngspice 对拍。
//!
//! 线性电路两边都应该是 ~0%：这条守的是**测量链本身**（激励、窗口、重采样、谐波提取）
//! 没搞错 —— 如果尺子歪了，这里会先炸。

include!("generated/lowpass.rs");

use rustomp_dsp::Circuit;
use rustomp_oracle::Oracle;

const CIR: &str = include_str!("../../rustomp-import/tests/data/lowpass.cir");
const LIB: &str = include_str!("../../../devices/devices.lib");

const FS: f64 = 192_000.0;
const AMP: f64 = 0.2;
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

fn ngspice(oracle: &Oracle, seconds: f64) -> (Vec<f64>, Vec<f64>) {
    let body: String = CIR
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .collect::<Vec<_>>()
        .join("\n");
    let netlist = format!(
        "* M-C closed loop (lowpass)\n{LIB}\nVSIM IN 0 DC 0 SIN(0 {AMP} {F0} 0 0 0)\n{body}\n\
         .tran 1e-6 {seconds} {}\n.end\n",
        seconds - TAIL
    );
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let uni_out = raw.resample("v(out)", 1.0 / FS).expect("重采样失败");
    let uni_in = raw.resample("v(in)", 1.0 / FS).expect("重采样失败");
    (uni_in, uni_out)
}

fn thd(x: &[f64]) -> f64 {
    let h = rustomp_oracle::harmonics(x, FS, F0, 9);
    let hp = rustomp_oracle::harmonics_percent(&h);
    rustomp_oracle::thd_percent_from_percent(&hp)
}

#[test]
fn linear_design_thd_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    let seconds = 0.6;
    let a = ours(seconds);
    let (_in, b) = ngspice(&oracle, seconds);
    let n_tail = (TAIL * FS) as usize;
    let b = &b[b.len() - n_tail..];
    let (ta, tb) = (thd(&a), thd(b));
    println!("\n=== M-C 闭环（lowpass，{AMP} V / {F0} Hz）===");
    println!("  我们的 Ckt : THD = {ta:.4} %");
    println!("  ngspice    : THD = {tb:.4} %");
    assert!(
        ta < 0.05 && tb < 0.05,
        "线性电路不该有失真：{ta:.4}% / {tb:.4}%"
    );
}

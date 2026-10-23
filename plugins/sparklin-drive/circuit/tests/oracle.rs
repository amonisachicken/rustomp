// SPDX-License-Identifier: GPL-3.0-or-later

//! **活体对拍**：直接让 ngspice 跑本插件的网表，与 WDF 实现用同一套测量函数比较。
//!
//! 与 `baseline.rs` 的区别：那边的期望值是从 ngspice 手工抄下来的常量；
//! 这里每次都重新跑 ngspice，因此网表或电路改动后能立刻发现偏差。
//!
//! 没装 ngspice 时自动跳过（不失败）。

use rustomp_dsp::Engine;
use rustomp_oracle::{
    harmonics, harmonics_percent, last_integer_periods, thd_percent_from_percent, Oracle,
};
use sparklin_drive_circuit::{Fakedist, Params};
use std::f64::consts::TAU;

const NETLIST: &str = include_str!("../netlist/netlist.cir");
const NHARM: usize = 9;
const PERIODS: usize = 10;

/// 用 WDF 实现跑同样的激励，返回输出波形（稳态最后 10 个周期）。
fn wdf_output() -> Vec<f64> {
    let fs = 48_000.0;
    let mut e = Engine::<Fakedist>::new(fs);
    e.set_params(Params::default());
    let mut ph: f64 = 0.0;
    for _ in 0..(3.0 * fs) as usize {
        e.process(0.2 * ph.sin());
        ph += TAU * 1000.0 / fs;
    }
    let n = (fs / 1000.0) as usize * PERIODS;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(e.process(0.2 * ph.sin()));
        ph += TAU * 1000.0 / fs;
    }
    out
}

#[test]
fn wdf_matches_ngspice_live() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice（可用环境变量 NGSPICE 指定路径）");
        return;
    };

    // 1) 跑 ngspice
    let body = NETLIST.trim_end().strip_suffix(".end").unwrap_or(NETLIST);
    let cir = format!("{body}\n.tran 5u 3 2.98\n.end\n");
    let raw = oracle.run_netlist(&cir).expect("ngspice 运行失败");
    // ngspice 的 tran 是变步长：先按名义步长重采样，再取整数周期
    let dt = raw.nominal_dt().expect("rawfile 里没有 time");
    let fs = 1.0 / dt;
    let out = raw.resample("v(n_t27r)", dt).expect("重采样失败");
    let seg = last_integer_periods(&out, fs, 1000.0, PERIODS);
    let h_ng = harmonics(seg, fs, 1000.0, NHARM);
    let p_ng = harmonics_percent(&h_ng);
    let thd_ng = thd_percent_from_percent(&p_ng);

    // 2) 跑 WDF，用**同一套**测量函数
    let wdf = wdf_output();
    let h_w = harmonics(&wdf, 48_000.0, 1000.0, NHARM);
    let p_w = harmonics_percent(&h_w);
    let thd_w = thd_percent_from_percent(&p_w);

    println!(
        "\nngspice: 基波 {:.4} V  THD(2..9) {:.3} %   (fs {:.0} Hz, {} 点)",
        h_ng[0],
        thd_ng,
        fs,
        seg.len()
    );
    println!("WDF    : 基波 {:.4} V  THD(2..9) {:.3} %", h_w[0], thd_w);
    println!("{:<6} {:>10} {:>10} {:>8}", "谐波", "WDF%", "ngspice%", "差");
    let mut worst = 0.0f64;
    for (i, (w, n)) in p_w.iter().zip(p_ng.iter()).enumerate() {
        let h = i + 2;
        println!("{:<6} {:>10.3} {:>10.3} {:>+8.3}", format!("h{h}"), w, n, w - n);
        worst = worst.max((w - n).abs());
    }

    assert!(
        (h_w[0] - h_ng[0]).abs() / h_ng[0] < 0.05,
        "基波差太多：WDF {:.4} vs ngspice {:.4}",
        h_w[0],
        h_ng[0]
    );
    assert!(worst < 0.5, "逐次谐波最大偏差 {worst:.3} pp 超过 0.5");
    assert!(
        (thd_w - thd_ng).abs() < 1.0,
        "THD 偏差 {:.3} pp 超过 1.0",
        (thd_w - thd_ng).abs()
    );
}

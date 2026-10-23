// SPDX-License-Identifier: GPL-3.0-or-later

//! 引擎级冒烟测试：把电路套进框架的 `Engine<C>`。
//!
//! 换成真实电路后，建议再加一个"与 ngspice 活体对拍"的测试
//! （参考 `sparklin-drive-legacy` 仓库的 `tests/oracle.rs`）。

use rustomp_dsp::Engine;
use std::f64::consts::TAU;
use __SLUG_UNDERSCORE___circuit::{Gain, Params};

#[test]
fn gain_passes_signal() {
    let fs = 48_000.0;
    let mut e = Engine::<Gain>::new(fs);
    e.set_params(Params { gain: 0.5 });

    let f0 = 1000.0;
    let mut ph: f64 = 0.0;
    // 先跑过过采样滤波器的延迟与建立过程
    for _ in 0..(0.1 * fs) as usize {
        e.process(0.2 * ph.sin());
        ph += TAU * f0 / fs;
    }
    let n = (fs / f0) as usize * 10;
    let mut peak = 0.0f64;
    for _ in 0..n {
        let y = e.process(0.2 * ph.sin());
        ph += TAU * f0 / fs;
        peak = peak.max(y.abs());
    }
    // 0.2 × 0.5 = 0.1
    assert!((peak - 0.1).abs() < 0.005, "输出峰值 {peak:.4}，期望 0.1");
}

#[test]
fn latency_is_reported() {
    assert_eq!(Engine::<Gain>::latency_samples(), 15);
}

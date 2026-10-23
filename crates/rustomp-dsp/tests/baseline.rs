// SPDX-License-Identifier: GPL-3.0-or-later

//! 与 ngspice 基线对拍（`circuits/fakedist/baseline.txt`）。
//!
//! 运行：`cargo test -p rustomp-dsp --test baseline -- --nocapture`
//!
//! 基线（ngspice-47，电位器 50%）：
//! - DC 增益 1.0000，工作点全部 4.5V
//! - 频响：50Hz +1.04dB / 100Hz +9.67 / 500Hz +23.72 / 1k +26.99 / 2k +28.29
//!         / 5k +27.96 / 10k +25.92（峰值 +28.42dB @ 2.72kHz）
//! - THD：输入峰值 0.2V → 27.57%；1mV → 0.283%

use rustomp_dsp::{Fakedist, Params};
use std::f64::consts::TAU;

const FS: f64 = 192_000.0;

/// 先用 3 秒把 0.5s 的直流恢复走完，再测量（与 `scripts/sim-baseline.sh` 一致）。
const WARMUP_S: f64 = 3.0;

fn db(x: f64) -> f64 {
    20.0 * x.log10()
}

/// 用小信号测某频点的增益（线性化对拍 ngspice 的 `.ac`）。
fn measure_gain(f: f64, amp: f64) -> f64 {
    let mut d = Fakedist::new(FS);
    d.set_params(Params::default());
    let t = 1.0 / FS;
    let mut ph: f64 = 0.0;
    let n_warm = (WARMUP_S * FS) as usize;
    for _ in 0..n_warm {
        d.process(amp * ph.sin());
        ph += TAU * f * t;
    }
    // 整数个周期，减少泄漏
    let cycles = (0.05 * f).round().max(8.0);
    let n = (cycles * FS / f).round() as usize;
    let (mut ss, mut sc) = (0.0, 0.0);
    for _ in 0..n {
        let s = ph.sin();
        let y = d.process(amp * s);
        ss += y * s;
        sc += y * ph.cos();
        ph += TAU * f * t;
    }
    let re = 2.0 * ss / n as f64;
    let im = 2.0 * sc / n as f64;
    (re * re + im * im).sqrt() / amp
}

/// 测 THD 与各次谐波（单点 DFT，1kHz，取整数周期）。
fn measure_harmonics(amp: f64, f0: f64, nh: usize) -> (f64, Vec<f64>) {
    let mut d = Fakedist::new(FS);
    d.set_params(Params::default());
    let t = 1.0 / FS;
    let mut ph: f64 = 0.0;
    let n_warm = (WARMUP_S * FS) as usize;
    for _ in 0..n_warm {
        d.process(amp * ph.sin());
        ph += TAU * f0 * t;
    }
    let n = (FS / f0).round() as usize; // 正好 1 个周期（192 点）
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n {
        buf.push(d.process(amp * ph.sin()));
        ph += TAU * f0 * t;
    }
    let mag = |f: f64| -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (k, v) in buf.iter().enumerate() {
            let a = TAU * f * k as f64 * t;
            re += v * a.cos();
            im -= v * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / n as f64
    };
    let fund = mag(f0);
    let hs: Vec<f64> = (2..=nh).map(|h| mag(f0 * h as f64) / fund * 100.0).collect();
    let sq: f64 = hs.iter().map(|x| x * x).sum();
    (sq.sqrt(), hs)
}

#[test]
fn dc_operating_point() {
    let mut d = Fakedist::new(FS);
    for _ in 0..(10.0 * FS) as usize {
        d.process(0.0);
    }
    let p = d.probe();
    println!(
        "直流工作点: v_out={:.5} V, v_inm={:.5} V (期望 4.50000)",
        p.v_out, p.v_inm
    );
    assert!((p.v_out - 4.5).abs() < 0.01, "v_out 不是 4.5V");
    assert!((p.v_inm - 4.5).abs() < 0.01, "v_inm 不是 4.5V");
}

#[test]
fn ac_response_matches_ngspice() {
    // (频率, ngspice 测得 dB)
    let reference = [
        (50.0, 1.04),
        (100.0, 9.67),
        (500.0, 23.72),
        (1000.0, 26.99),
        (2000.0, 28.29),
        (5000.0, 27.96),
        (10000.0, 25.92),
    ];
    println!("\n{:<10} {:>10} {:>10} {:>8}", "频率", "WDF(dB)", "ngspice", "误差");
    let mut worst = 0.0f64;
    for &(f, want) in &reference {
        let got = db(measure_gain(f, 1e-3));
        let err = (got - want).abs();
        worst = worst.max(err);
        println!("{:<10.0} {:>10.2} {:>10.2} {:>+8.2}", f, got, want, got - want);
    }
    println!("最大误差 {worst:.2} dB");
    assert!(worst < 1.0, "频响误差 {worst:.2} dB 超过 1dB");
}

#[test]
fn thd_and_harmonics_match_ngspice() {
    // ngspice 参考值取自 1µs 步长（1 MHz）的瞬态波形 + 同一套 DFT，
    // 而不是 `.four`（后者只用 1 个周期 + 线性插值，高次谐波偏低）：
    //   基波 0.4794 V, THD(2..9) = 27.616 %, h2=3.92 h3=22.47 h4=2.43 h5=11.96
    //   h6=1.837 h7=7.674 h8=1.483 h9=5.374
    println!(
        "\n{:<14} {:>9} {:>9} {:>9}",
        "输入峰值", "WDF THD%", "ngspice", "误差"
    );
    for &(amp, want_thd) in &[(0.2, 27.616), (1e-3, 0.283)] {
        let (thd, _) = measure_harmonics(amp, 1000.0, 9);
        println!(
            "{:<14} {:>9.3} {:>9.3} {:>+9.3}",
            amp,
            thd,
            want_thd,
            thd - want_thd
        );
        assert!(
            (thd - want_thd).abs() < 0.3,
            "THD 偏差过大：{thd:.3} vs {want_thd:.3}"
        );
    }

    // 逐次谐波（大信号）——这是最严的验证：非线性形状必须一致
    let (_, hs) = measure_harmonics(0.2, 1000.0, 9);
    let want = [3.92, 22.47, 2.43, 11.96, 1.837, 7.674, 1.483, 5.374];
    println!("\n{:<6} {:>9} {:>9} {:>8}", "谐波", "WDF%", "ngspice%", "误差");
    for (i, (got, w)) in hs.iter().zip(want.iter()).enumerate() {
        let h = i + 2;
        println!("{:<6} {:>9.3} {:>9.3} {:>+8.3}", format!("h{h}"), got, w, got - w);
        assert!(
            (got - w).abs() < 0.15,
            "h{h} 偏差过大：{got:.3} vs {w:.3}"
        );
    }
}

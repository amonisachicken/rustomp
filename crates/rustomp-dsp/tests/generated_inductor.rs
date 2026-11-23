// SPDX-License-Identifier: GPL-3.0-or-later

//! **M1 端到端**：网表里的 `L` 卡片 → 生成的 `net.inductor(...)` → 实时路径（`Engine`）。
//!
//! ```text
//! crates/rustomp-import/tests/fixtures/inductor_rl.cir（RL 低通：10 mH + 0.1 Ω 串阻 + 100 Ω 负载）
//!   → rustomp-import --mna → tests/generated/inductor_rl.rs → 本测试
//! ```
//!
//! 判据是**解析解**：`|H(f)| = R / |R + rdc + jωL|`。这条能一眼区分
//! "电感真的在工作"（1 kHz 时 0.847）和"电感被当成导线"（0.999）。

include!("generated/inductor_rl.rs");

use rustomp_dsp::Engine;
use rustomp_oracle::Oracle;

/// 生成器吃的同一份夹具（避免两边各写一份、悄悄漂移）
const FIXTURE: &str = include_str!("../../rustomp-import/tests/fixtures/inductor_rl.cir");
const LIB: &str = include_str!("../../../devices/devices.lib");
const FS: f64 = 48_000.0;
/// 夹具里的元件值（与 `inductor_rl.cir` 一致）
const R_LOAD: f64 = 100.0;
const R_DCR: f64 = 0.1;
const L: f64 = 0.01;

fn rms(xs: &[f64]) -> f64 {
    (xs.iter().map(|x| x * x).sum::<f64>() / xs.len().max(1) as f64).sqrt()
}

fn analytic_gain(f: f64) -> f64 {
    let wl = std::f64::consts::TAU * f * L;
    R_LOAD / ((R_LOAD + R_DCR).powi(2) + wl.powi(2)).sqrt()
}

/// 稳态增益（RMS 输出 / RMS 输入）。跳过预热，让过采样滤波器与电感电流进入稳态。
fn measured_gain(f: f64) -> f64 {
    const AMP: f64 = 0.2;
    let mut e = Engine::<Ckt>::new(FS);
    let dph = std::f64::consts::TAU * f / FS;
    let mut ph: f64 = 0.0;
    for _ in 0..(0.25 * FS) as usize {
        e.process(AMP * ph.sin());
        ph += dph;
    }
    let mut out = Vec::new();
    for _ in 0..(0.1 * FS) as usize {
        out.push(e.process(AMP * ph.sin()));
        ph += dph;
    }
    assert_eq!(e.guard_trips(), 0, "实时路径熔断了（f={f}）");
    assert_eq!(Engine::<Ckt>::latency_samples(), 15);
    rms(&out) / (AMP / 2f64.sqrt())
}

#[test]
fn generated_rl_matches_analytic() {
    println!("\n=== 生成电路：RL 低通（vs 解析解）===");
    let mut worst = 0.0f64;
    for &f in &[100.0, 1_000.0, 5_000.0] {
        let g = measured_gain(f);
        let a = analytic_gain(f);
        let err = (g - a).abs() / a;
        println!(
            "  f={f:>6.0} Hz  实测 {g:.4}  解析 {a:.4}  差 {:.2} %",
            err * 100.0
        );
        worst = worst.max(err);
    }
    assert!(worst < 0.03, "最大增益偏差 {:.2}% 超过 3%", worst * 100.0);
    // 防"电感被当成导线"：1 kHz 的增益必须明显低于 1
    let g1k = measured_gain(1_000.0);
    assert!(g1k < 0.9, "1 kHz 增益 {g1k:.4} —— 电感没起作用？");
}

/// 与 ngspice 比**稳态增益**（不比波形：`Engine` 有 15 样本延迟 + 后向欧拉的相位误差）。
#[test]
fn generated_rl_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    let body: String = FIXTURE
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .collect::<Vec<_>>()
        .join("\n");
    let netlist = format!("{LIB}\n{body}\n.tran 2e-5 0.4\n.end\n");
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let dt = 1.0 / FS;
    let v_out = raw.resample("v(out)", dt).expect("resample 失败");
    // 取最后 0.1 s（稳态）
    let tail = &v_out[v_out.len().saturating_sub((0.1 * FS) as usize)..];
    let gain_spice = rms(tail) / (0.2 / 2f64.sqrt());

    let gain_ours = measured_gain(1_000.0);
    let err = (gain_ours - gain_spice).abs() / gain_spice;
    println!("\n=== 生成电路：RL 低通（1 kHz，vs ngspice）===");
    println!(
        "  我们 {gain_ours:.4}  ngspice {gain_spice:.4}  差 {:.2} %",
        err * 100.0
    );
    assert!(err < 0.03, "与 ngspice 增益差 {:.2}% 超过 3%", err * 100.0);
}

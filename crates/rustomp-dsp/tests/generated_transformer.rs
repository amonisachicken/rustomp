// SPDX-License-Identifier: GPL-3.0-or-later

//! **M2 端到端**：网表里的 `L` + `K` → 生成的 `net.coupled_inductors(...)` → 实时路径。
//!
//! ```text
//! crates/rustomp-import/tests/fixtures/transformer.cir（2 绕组，k=0.999，匝比 2:1，次级 1 kΩ）
//!   → rustomp-import --mna → tests/generated/transformer.rs → 本测试
//! ```
//!
//! 判据与 ngspice 比**稳态输出 RMS**（不比波形：`Engine` 有 15 样本延迟 + 后向欧拉的相位误差）。

include!("generated/transformer.rs");

use rustomp_dsp::Engine;
use rustomp_oracle::Oracle;

const FIXTURE: &str = include_str!("../../rustomp-import/tests/fixtures/transformer.cir");
const LIB: &str = include_str!("../../../devices/devices.lib");
const FS: f64 = 48_000.0;
const F: f64 = 1_000.0;
const AMP: f64 = 0.2;

fn rms(xs: &[f64]) -> f64 {
    (xs.iter().map(|x| x * x).sum::<f64>() / xs.len().max(1) as f64).sqrt()
}

/// 我们的实时输出 RMS（跳过预热）。
fn our_output_rms() -> f64 {
    let mut e = Engine::<Ckt>::new(FS);
    let dph = std::f64::consts::TAU * F / FS;
    let mut ph: f64 = 0.0;
    for _ in 0..(0.25 * FS) as usize {
        e.process(AMP * ph.sin());
        ph += dph;
    }
    let mut out = Vec::new();
    for _ in 0..(0.2 * FS) as usize {
        out.push(e.process(AMP * ph.sin()));
        ph += dph;
    }
    assert_eq!(e.guard_trips(), 0, "实时路径熔断了");
    assert_eq!(Engine::<Ckt>::latency_samples(), 15);
    rms(&out)
}

#[test]
fn generated_transformer_matches_ngspice() {
    let ours = our_output_rms();
    println!("\n=== 生成电路：变压器（1 kHz，次级 1 kΩ）===");
    println!("  我们输出 RMS = {ours:.5} V");
    // 粗略先验：源 0.2 V、匝比 0.5、源内阻 100 Ω vs 折算 4 kΩ → 输出应在几十 mV 量级
    assert!(
        (0.02..0.15).contains(&ours),
        "输出 RMS {ours:.4} V 不在合理区间 —— 耦合可能没生效"
    );

    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    let body: String = FIXTURE
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .collect::<Vec<_>>()
        .join("\n");
    let netlist = format!("{LIB}\n{body}\n.tran 2e-5 0.5\n.end\n");
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let dt = 1.0 / FS;
    let v_out = raw.resample("v(s)", dt).expect("resample 失败");
    let tail = &v_out[v_out.len().saturating_sub((0.2 * FS) as usize)..];
    let spice = rms(tail);
    let err = (ours - spice).abs() / spice;
    println!("  ngspice 输出 RMS = {spice:.5} V，差 {:.2} %", err * 100.0);
    assert!(err < 0.03, "与 ngspice 差 {:.2}% 超过 3%", err * 100.0);
}

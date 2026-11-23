// SPDX-License-Identifier: GPL-3.0-or-later

//! 12AX7 共阴级的 CPU 基准（手动跑）：
//! `cargo test -p rustomp-dsp --release --test triode_cpu -- --ignored --nocapture`

include!("generated/triode_stage.rs");

use rustomp_dsp::Circuit;

#[test]
#[ignore = "基准：手动跑"]
fn realtime_cpu_cost() {
    const FS: f64 = 192_000.0;
    let dt = 1.0 / FS;
    let mut net = build(FS, &Params);
    net.solve_dc();
    for i in 0..10_000 {
        net.set_fixed(FIXED_IN, 0.02 * (i as f64 * 0.1).sin());
        net.step_fixed(dt, 4);
    }
    let n = 100_000usize;
    let t0 = std::time::Instant::now();
    for i in 0..n {
        net.set_fixed(FIXED_IN, 0.02 * (i as f64 * 0.1).sin());
        std::hint::black_box(net.step_fixed(dt, 4));
    }
    let ns = t0.elapsed().as_nanos() as f64 / n as f64;
    println!(
        "\n=== 12AX7 共阴级 CPU（6 未知量）===\n  {ns:.0} ns/样本 → 48 kHz 单核 {:.1}%",
        ns * 1e-9 * 48_000.0 * 100.0
    );
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! **BJT 体电阻的 CPU 代价**（基准，不进 CI）。
//!
//! 下游 Freedist（4×BJT 离散运放 fuzz）报告 P3：4× 实时路径 RTF 1.42，
//! 去掉 `rb/re/rc` 后 0.88，再配 2× 到 0.54。这里把这件事变成可复现的数字，
//! 方便以后改器件模型时对比。
//!
//! ```bash
//! cargo test -p rustomp-dsp --release --test bjt_body_bench -- --ignored --nocapture
//! ```
//!
//! 用的是设计器工程 `pretty-face`（两只锗管 AC127）生成的电路 —— 它不是 4 只管子，
//! 所以绝对 RTF 比 Freedist 小；要看的是**两者的比值**。

include!("generated/pretty_face.rs");

/// 跑 `n` 拍，返回每样本平均耗时（ns）。
fn bench(net: &mut Netlist<N>, fs: f64, label: &str) -> f64 {
    let dt = 1.0 / fs;
    let iters = NEWTON_ITERS;
    for _ in 0..2_000 {
        std::hint::black_box(net.step_fixed(dt, iters));
    }
    let n = 50_000usize;
    let t0 = std::time::Instant::now();
    for _ in 0..n {
        std::hint::black_box(net.step_fixed(dt, iters));
    }
    let ns = t0.elapsed().as_nanos() as f64 / n as f64;
    // RTF（相对实时）：每样本耗时 × 每秒样本数
    println!("  {label:<28} {ns:7.0} ns/样本   RTF {:.2}", ns * 1e-9 * fs);
    ns
}

#[test]
#[ignore = "基准：手动跑（cargo test --release --test bjt_body_bench -- --ignored --nocapture）"]
fn bjt_body_resistance_cost() {
    let fs = 192_000.0; // 48 kHz × 4（默认过采样）
    let p = Params::default();

    println!("\n=== BJT 体电阻的 CPU 代价（pretty-face，{fs:.0} Hz 内层）===");

    let mut with_body = build(fs, &p);
    with_body.solve_dc();
    let a = bench(&mut with_body, fs, "rb/re/rc 原样");

    let mut without = build(fs, &p);
    without.solve_dc();
    let stripped = without.strip_bjt_body_resistance();
    let b = bench(&mut without, fs, "strip_bjt_body_resistance");

    println!(
        "  改了 {stripped} 只管子；耗时比 {:.2}×（{:.0} → {:.0} ns）",
        a / b,
        a,
        b
    );
    // 只做宽松的健全性检查（机器差异大，不设性能门槛）
    assert!(a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0);
}

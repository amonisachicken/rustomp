// SPDX-License-Identifier: GPL-3.0-or-later

//! **首选设计器（rustomp-circuit-designer）= 工程文件直接进生成器**。
//!
//! ```text
//! crates/rustomp-import/tests/data/lowpass.design.json   ← 设计器的工程文件（不是导出的 .cir）
//!   → rustomp-import --mna → crates/rustomp-dsp/tests/generated/lowpass.rs
//!   → 本测试 include! 进来，用生成的 Ckt（实现了 rustomp_dsp::Circuit）
//! ```
//!
//! 这条链路守住三件事：
//!
//! 1. 生成的代码**能编译**（常量名 / 借用 / 类型全对）；
//! 2. **输入端口真的工作** —— 设计器导出的网表是"纯拓扑"（激励不画进网表，
//!    由 DSP 每拍注入），早期实现只认 `in=<源名>`，于是这种图会退化成
//!    "无输入端口（纯直流电路）"，生成的 `Ckt` 直接忽略输入；
//! 3. **数值对得上**：10k + 100n 的低通，fc ≈ 159 Hz，1 kHz 处应衰减到
//!    `1/√(1+(f/fc)²) ≈ 0.157`（与设计器自己的 ngspice 仿真同一个判据）。
//!
//! 生成物是否最新由 `cargo test -p rustomp-import` 的漂移守卫守着
//! （`tests/codegen.rs::designer_project_is_up_to_date`）。

include!("generated/lowpass.rs");

use rustomp_dsp::Circuit;

const FS: f64 = 192_000.0;

/// 跑 `seconds` 秒正弦，返回稳态段的 (输入峰峰值, 输出峰峰值)。
fn swing(amplitude: f64, freq: f64, seconds: f64) -> (f64, f64) {
    let mut c = Ckt::new(FS);
    let dt = 1.0 / FS;
    let n = (seconds * FS) as usize;
    let (mut in_lo, mut in_hi) = (f64::MAX, f64::MIN);
    let (mut out_lo, mut out_hi) = (f64::MAX, f64::MIN);
    for i in 0..n {
        let t = i as f64 * dt;
        let x = amplitude * (std::f64::consts::TAU * freq * t).sin();
        let y = c.process(x);
        assert!(y.is_finite(), "第 {i} 拍输出不是有限值");
        // 只看最后 25%（避开起始瞬态 —— DEVLOG E4 的老坑）
        if i > n * 3 / 4 {
            in_lo = in_lo.min(x);
            in_hi = in_hi.max(x);
            out_lo = out_lo.min(y);
            out_hi = out_hi.max(y);
        }
    }
    (in_hi - in_lo, out_hi - out_lo)
}

#[test]
fn 设计器工程生成的电路能编译且数值与理论一致() {
    // 通带：10 Hz ≪ fc=159 Hz → 理论衰减 1/√(1+(10/159)²) ≈ 0.998
    let (in_pp, out_pp) = swing(0.2, 10.0, 0.5);
    assert!(in_pp > 0.39, "输入没送进去：{in_pp}");
    let gain_pass = out_pp / in_pp;
    assert!(
        gain_pass > 0.99,
        "通带被压掉了（输入端口退化？）：{gain_pass:.3}"
    );

    // 阻带：1 kHz → 理论 1/√(1+(1000/159.15)²) = 0.157
    let (in_pp, out_pp) = swing(0.2, 1_000.0, 0.1);
    let gain = out_pp / in_pp;
    let fc = 1.0 / (std::f64::consts::TAU * 10_000.0 * 100e-9);
    let want = 1.0 / (1.0 + (1_000.0f64 / fc).powi(2)).sqrt();
    assert!(
        (gain - want).abs() < 0.02,
        "1 kHz 衰减 {gain:.3}，理论 {want:.3}（fc={fc:.1} Hz）"
    );
}

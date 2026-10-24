// SPDX-License-Identifier: GPL-3.0-or-later

//! **S1a/S1b 回归**：由代码生成器从**网表**自动生成的电路（含电位器/开关参数），
//! 必须和手写网表 / ngspice 基线一致。
//!
//! ```text
//! crates/rustomp-import/tests/fixtures/sparklin_drive.cir
//!   → rustomp-import --mna → tests/generated/sparklin_drive.rs
//!   → 本测试 include! 进来，用生成的 Ckt（实现了 rustomp_dsp::Circuit）
//! ```
//!
//! 生成物是否最新由 `cargo test -p rustomp-import` 的漂移守卫守着。

include!("generated/sparklin_drive.rs");

use rustomp_dsp::Circuit;
use rustomp_oracle::{harmonics, harmonics_percent, thd_percent};

const FS: f64 = 192_000.0;
const DT: f64 = 1.0 / FS;

/// 用生成的 `Ckt` 跑 `seconds` 秒，返回最后 `tail` 秒的输出。
fn run(params: Params, amplitude: f64, seconds: f64, tail: f64) -> Vec<f64> {
    let mut c = Ckt::new(FS);
    c.set_params(params);
    let n = (seconds / DT) as usize;
    let n_tail = (tail / DT) as usize;
    let mut out = Vec::with_capacity(n_tail);
    for i in 0..n {
        let t = i as f64 * DT;
        let x = amplitude * (std::f64::consts::TAU * 1_000.0 * t).sin();
        let y = c.process(x);
        if i >= n - n_tail {
            out.push(y);
        }
    }
    out
}

fn at(gain: f64, level: f64, symmetric: bool) -> Params {
    let mut p = Params::default();
    p.gain = gain;
    p.level = level;
    p.symmetric = symmetric;
    p
}

#[test]
fn generated_dc_operating_point() {
    let mut c = Ckt::new(FS);
    c.set_params(at(0.5, 0.5, false));
    // 无输入 → 工作点
    let y = c.process(0.0);
    println!("\n=== 生成电路（S1b）直流 ===");
    println!("  音量抽头 = {y:.6} V（应为 0，C3 隔直）");
    assert!(y.abs() < 1e-6, "抽头直流应为 0，实测 {y}");

    // 用 build() 直接看内部节点
    let mut net = build(FS, &at(0.5, 0.5, false));
    let v = net.solve_dc();
    println!(
        "  运放输出={:.6} V  IN+={:.6} V  IN−={:.6} V  偏置={:.6} V",
        v[node::N_T11R],
        v[node::N_T11L],
        v[node::N_T10L],
        v[node::N_T19R]
    );
    assert!((v[node::N_T11R] - 4.499935).abs() < 1e-3, "运放输出 {}", v[node::N_T11R]);
    assert!((v[node::N_T11L] - 4.499724).abs() < 1e-3);
    assert!((v[node::N_T19R] - 4.499986).abs() < 1e-3);
}

/// 参数默认值（gain=level=0.5、非对称）必须复现手写网表与 ngspice 基线。
#[test]
fn generated_thd_matches_baseline() {
    let out = run(at(0.5, 0.5, false), 0.2, 3.0, 0.02);
    let thd = thd_percent(&out, FS, 1_000.0, 9);
    let hp = harmonics_percent(&harmonics(&out, FS, 1_000.0, 9));
    println!("\n=== 生成电路（S1b）THD @0.2 V / 1 kHz ===");
    println!("  THD = {thd:.3} %（手写网表 27.579%，ngspice 27.5682%）");
    println!("  h2..h9 = {}", hp.iter().map(|x| format!("{x:.2}")).collect::<Vec<_>>().join(" / "));
    assert!((thd - 27.579).abs() < 0.2, "THD {thd:.3}% 与 27.579% 相差超过 0.2 pp");
}

/// **参数化生效**：对称模式（生成的 switch 短接 D3）必须把偶次谐波压下去。
///
/// legacy 实测：非对称 h2=3.92% → 对称 h2≈0.05%。
#[test]
fn symmetric_switch_kills_even_harmonics() {
    let asym = run(at(0.5, 0.5, false), 0.2, 3.0, 0.02);
    let sym = run(at(0.5, 0.5, true), 0.2, 3.0, 0.02);
    let h_asym = harmonics_percent(&harmonics(&asym, FS, 1_000.0, 9));
    let h_sym = harmonics_percent(&harmonics(&sym, FS, 1_000.0, 9));
    println!("\n=== 开关参数（对称 / 非对称）===");
    println!("  非对称 h2 = {:.3} %  h3 = {:.3} %", h_asym[0], h_asym[1]);
    println!("  对称   h2 = {:.3} %  h3 = {:.3} %", h_sym[0], h_sym[1]);
    assert!(h_asym[0] > 3.0, "非对称时 h2 应 ~3.9%，实测 {:.3}", h_asym[0]);
    assert!(h_sym[0] < 0.2, "对称时 h2 应接近 0（实测 {:.3}%）", h_sym[0]);
}

/// **参数化生效**：GAIN 电位器改变失真量。
#[test]
fn gain_pot_changes_clipping() {
    let mut thds = Vec::new();
    for gain in [0.15f64, 0.5, 1.0] {
        let out = run(at(gain, 0.5, false), 0.05, 1.5, 0.02);
        let thd = thd_percent(&out, FS, 1_000.0, 9);
        println!("  gain={gain:.2} → THD = {thd:.2} %");
        thds.push(thd);
    }
    println!("\n=== GAIN 电位器 ===");
    assert!(thds[0] < thds[1] && thds[1] < thds[2], "GAIN 越大失真应越大：{thds:?}");
    assert!(thds[2] > 10.0, "GAIN 拧满时应明显削波（实测 {:.1}%）", thds[2]);
}

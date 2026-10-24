// SPDX-License-Identifier: GPL-3.0-or-later

//! **S1a 回归**：由代码生成器从**网表**自动生成的电路，必须和手写网表/ngspice 基线一致。
//!
//! 生成链：
//!
//! ```text
//! crates/rustomp-import/tests/fixtures/sparklin_drive.cir   （legacy 网表快照）
//!   → rustomp-import --mna  →  crates/rustomp-dsp/tests/generated/sparklin_drive.rs
//!   → 本测试 include! 进来，跑 3 s 测 THD
//! ```
//!
//! 生成物是否最新由 `cargo test -p rustomp-import` 里的 `generated_file_is_up_to_date` 守着。

include!("generated/sparklin_drive.rs");

use rustomp_oracle::{harmonics_percent, thd_percent};

const FS: f64 = 192_000.0;
const DT: f64 = 1.0 / FS;

/// 跑 `seconds` 秒，返回最后 `tail` 秒的输出（音量抽头节点）。
fn run(amplitude: f64, seconds: f64, tail: f64) -> Vec<f64> {
    let mut net = build(FS);
    net.solve_dc();
    let n = (seconds / DT) as usize;
    let n_tail = (tail / DT) as usize;
    let mut out = Vec::with_capacity(n_tail);
    for i in 0..n {
        let t = i as f64 * DT;
        let x = amplitude * (std::f64::consts::TAU * 1_000.0 * t).sin();
        net.set_fixed(FIXED_IN, x);
        let v = net.step(DT);
        if i >= n - n_tail {
            out.push(v[OUT]);
        }
    }
    out
}

#[test]
fn generated_circuit_dc_operating_point() {
    let mut net = build(FS);
    let v = net.solve_dc();
    println!("\n=== 生成电路（S1a）直流工作点 ===");
    println!(
        "  运放输出={:.6} V  IN+={:.6} V  IN−={:.6} V  偏置={:.6} V  音量抽头={:.6} V",
        v[node::N_T11R],
        v[node::N_T11L],
        v[node::N_T10L],
        v[node::N_T19R],
        v[OUT]
    );
    // 与 ngspice 基线一致：运放输出 4.499935、IN+ 4.499724、偏置 4.499986
    assert!((v[node::N_T11R] - 4.499935).abs() < 1e-3, "OUT={}", v[node::N_T11R]);
    assert!((v[node::N_T11L] - 4.499724).abs() < 1e-3);
    assert!((v[node::N_T19R] - 4.499986).abs() < 1e-3);
    // 输出隔直：音量抽头在直流上必须是 0（C3 后面没有直流通路）
    assert!(v[OUT].abs() < 1e-6, "音量抽头直流 {} 应为 0", v[OUT]);
}

/// 生成电路的 THD 必须与手写网表（离线求解器）以及 ngspice 基线一致。
#[test]
fn generated_circuit_thd_matches_baseline() {
    let out = run(0.2, 3.0, 0.02);
    let thd = thd_percent(&out, FS, 1_000.0, 9);
    let hp = harmonics_percent(&rustomp_oracle::harmonics(&out, FS, 1_000.0, 9));
    println!("\n=== 生成电路（S1a）THD @0.2 V / 1 kHz ===");
    println!("  THD = {thd:.3} %（手写网表 27.579%，ngspice 27.5682%）");
    println!("  h2..h9 = {}", hp.iter().map(|x| format!("{x:.2}")).collect::<Vec<_>>().join(" / "));
    assert!(
        (thd - 27.579).abs() < 0.2,
        "生成电路 THD {thd:.3}% 与手写网表 27.579% 相差超过 0.2 pp —— 生成器可能漏/错元件"
    );
}

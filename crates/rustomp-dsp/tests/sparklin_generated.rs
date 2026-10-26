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
    assert!(
        (v[node::N_T11R] - 4.499935).abs() < 1e-3,
        "运放输出 {}",
        v[node::N_T11R]
    );
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
    println!(
        "  h2..h9 = {}",
        hp.iter()
            .map(|x| format!("{x:.2}"))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    assert!(
        (thd - 27.579).abs() < 0.2,
        "THD {thd:.3}% 与 27.579% 相差超过 0.2 pp"
    );
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
    assert!(
        h_asym[0] > 3.0,
        "非对称时 h2 应 ~3.9%，实测 {:.3}",
        h_asym[0]
    );
    assert!(
        h_sym[0] < 0.2,
        "对称时 h2 应接近 0（实测 {:.3}%）",
        h_sym[0]
    );
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
    assert!(
        thds[0] < thds[1] && thds[1] < thds[2],
        "GAIN 越大失真应越大：{thds:?}"
    );
    assert!(
        thds[2] > 10.0,
        "GAIN 拧满时应明显削波（实测 {:.1}%）",
        thds[2]
    );
}

/// **欠收敛回归**（DEVLOG E50）：方波是最容易暴露"牛顿迭代不够"的输入。
///
/// 2 次迭代时实测 880 Hz 方波会在 f/8、f/16 出现 −0…+19 dB 的**次谐波**
/// （比基频只低十几 dB），听感就是"低频噗噗声 + 发浑"。
/// 这里要求次谐波低于基频 60 dB —— 正常电路应当在 −80 dB 以下。
#[test]
fn no_subharmonics_under_square_input() {
    const F0: f64 = 880.0;
    let mut c = Ckt::new(FS);
    c.set_params(at(0.5, 0.5, false));

    let warm = (2.0 * FS) as usize; // 等直流恢复稳定
    let n = (0.2 * FS) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..(warm + n) {
        let t = i as f64 / FS;
        let x = if (t * F0).fract() < 0.5 { 0.5 } else { -0.5 };
        let y = c.process(x);
        if i >= warm {
            out.push(y);
        }
    }

    let fund = harmonics(&out, FS, F0, 1)[0];
    let subs: Vec<f64> = [2.0, 4.0, 8.0, 16.0]
        .iter()
        .map(|d| harmonics(&out, FS, F0 / d, 1)[0])
        .collect();
    let worst = subs.iter().cloned().fold(0.0f64, f64::max);
    let db = 20.0 * (worst / fund).log10();
    println!(
        "\n=== 方波次谐波（f/2,f/4,f/8,f/16）===\n  最大 {db:.1} dB（要求 < -60 dB）  实测 {:?}",
        subs.iter()
            .map(|v| format!("{:.0}", 20.0 * (v / fund).log10()))
            .collect::<Vec<_>>()
    );
    assert!(
        db < -60.0,
        "出现次谐波（{db:.1} dB）：牛顿迭代次数不够，求解器进入了极限环"
    );
}

/// **电位器端点回归**（DEVLOG E51）。
///
/// `level = 1` 时下臂电阻 = 0 Ω、`gain = 0` 时反馈电阻 = 0 Ω；
/// MNA 用导纳表示电阻，`1/0 = ∞` → 矩阵 NaN → 引擎熔断 → **整块静音**（用户实测：
/// 音量拉到 100% 就没声，99% 正常）。框架现在把电阻夹到 `MIN_RESISTANCE`（1 mΩ）。
#[test]
fn pot_extremes_stay_finite_and_monotonic() {
    let peak_at = |gain: f64, level: f64| -> f64 {
        let mut c = Ckt::new(FS);
        c.set_params(at(gain, level, false));
        let mut peak = 0.0f64;
        let mut nonfinite = 0;
        for i in 0..(0.05 * FS) as usize {
            let t = i as f64 / FS;
            let y = c.process(0.2 * (std::f64::consts::TAU * 1_000.0 * t).sin());
            if y.is_finite() {
                peak = peak.max(y.abs());
            } else {
                nonfinite += 1;
            }
        }
        assert_eq!(
            nonfinite, 0,
            "gain={gain} level={level} 出现 {nonfinite} 个非有限值"
        );
        peak
    };

    // 所有端点都必须有限（0/0 组合曾直接全 NaN）
    let mut p = Vec::new();
    for &(g, l) in &[
        (0.0, 0.0),
        (0.0, 0.5),
        (0.0, 1.0),
        (0.5, 0.0),
        (0.5, 0.5),
        (0.5, 1.0),
        (1.0, 0.5),
        (1.0, 1.0),
    ] {
        p.push((g, l, peak_at(g, l)));
    }
    println!("\n=== 电位器端点 ===");
    for (g, l, pk) in &p {
        println!("  gain={g:.1} level={l:.1} → 峰值 {pk:.4}");
    }

    // 音量单调：level=1 应该比 0.5 响，level=0 应该几乎没声
    let half = p.iter().find(|(g, l, _)| *g == 0.5 && *l == 0.5).unwrap().2;
    let full = p.iter().find(|(g, l, _)| *g == 0.5 && *l == 1.0).unwrap().2;
    let mute = p.iter().find(|(g, l, _)| *g == 0.5 && *l == 0.0).unwrap().2;
    assert!(
        full > half * 1.5,
        "level=1 应明显大于 level=0.5：{full:.3} vs {half:.3}"
    );
    assert!(mute < 1e-3, "level=0 应接近静音：{mute:.6}");
    // GAIN=0 → 反馈电阻(以及削波环)被短接 → 退化成单位增益缓冲，不应有增益
    let g0 = p.iter().find(|(g, l, _)| *g == 0.0 && *l == 0.5).unwrap().2;
    assert!(g0 < half, "GAIN=0 不应比 50% 更响：{g0:.3} vs {half:.3}");
}

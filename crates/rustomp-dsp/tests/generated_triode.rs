// SPDX-License-Identifier: GPL-3.0-or-later

//! **端到端**：网表里的 `X1 P G K 12AX7` → 生成的 `net.triode(T12AX7, …)` →
//! DC 工作点与 1 kHz 增益都要和 ngspice（Koren 参考实现）对上。
//!
//! ```text
//! crates/rustomp-import/tests/fixtures/triode_stage.cir（12AX7 共阴级）
//!   → rustomp-import --mna → tests/generated/triode_stage.rs → 本测试
//! ```
//!
//! 注意：Rust 侧 `T12AX7` 用的是 **Koren 官方 1996 参数**（MU=100 / KP=600），
//! 而 `tubes.txt` 里 12AX7 是**社区拟合值**（MU=100.26 / KP=17950）——
//! 两者同公式不同参数（见 `devices/triode-koren.inc` 文件头）。本测试统一用官方参数。

include!("generated/triode_stage.rs");

use rustomp_dsp::Circuit;
use rustomp_oracle::Oracle;

const FIXTURE: &str = include_str!("../../rustomp-import/tests/fixtures/triode_stage.cir");
const INC: &str = include_str!("../../../devices/triode-koren.inc");
const FS: f64 = 192_000.0;

/// ngspice 侧：同一张网表，但 X 行换成官方参数的 TRIODE 本体。
fn spice_netlist() -> String {
    let body = FIXTURE
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .map(|l| {
            if l.trim_start().starts_with("X1 ") {
                "X1 P G K TRIODE MU=100 EX=1.4 KG1=1060 KP=600 KVB=300".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{INC}\n{body}\n.op\n.end\n")
}

#[test]
fn generated_triode_stage_matches_ngspice() {
    let mut net = build(FS, &Params);
    let v = net.solve_dc();
    println!(
        "\n=== 生成电路：12AX7 共阴级（DC）===\n  Vp={:.3} V  Vk={:.4} V  Vg={:.4} V",
        v[node::P],
        v[node::K],
        v[node::G]
    );

    let Some(oracle) = Oracle::discover_or_skip() else {
        // 没有 ngspice 也要防退化：12AX7 共阴级应落在 Vp ≈ 200 V 附近
        assert!(
            (150.0..260.0).contains(&v[node::P]),
            "Vp={:.1} V 不像一个导通的共阴级",
            v[node::P]
        );
        return;
    };
    let raw = oracle
        .run_netlist(&spice_netlist())
        .expect("ngspice 运行失败");
    let (vp_s, vk_s) = (
        raw.get("v(p)").map(|x| x[0]).unwrap_or(f64::NAN),
        raw.get("v(k)").map(|x| x[0]).unwrap_or(f64::NAN),
    );
    println!(
        "  ngspice: Vp={vp_s:.3} V  Vk={vk_s:.4} V（差 {:.1} µV / {:.1} µV）",
        (v[node::P] - vp_s).abs() * 1e6,
        (v[node::K] - vk_s).abs() * 1e6
    );
    // 注意：屏极挂着 100 kΩ，**电流**上 0.03% 的模型差异会放大成 30 mV 的电压差。
    // 所以按电流判：Ia = (VHT − Vp)/Ra，相对误差 < 0.1%。
    let ia_ours = (300.0 - v[node::P]) / 100_000.0;
    let ia_spice = (300.0 - vp_s) / 100_000.0;
    let rel = (ia_ours - ia_spice).abs() / ia_spice;
    println!(
        "  Ia: Rust={:.4} mA  ngspice={:.4} mA（差 {:.3}%）",
        ia_ours * 1e3,
        ia_spice * 1e3,
        rel * 100.0
    );
    assert!(rel < 1e-3, "屏流相对误差 {:.3}% 超过 0.1%", rel * 100.0);
    assert!(
        (v[node::K] - vk_s).abs() < 3e-3,
        "Vk {:.4} vs ngspice {vk_s:.4} 差太多",
        v[node::K]
    );
}

/// 1 kHz 增益：Rust（瞬态测 RMS）vs ngspice（`.ac`）。
#[test]
fn generated_triode_stage_gain_matches_ngspice() {
    const AMP: f64 = 0.02;
    let mut net = build(FS, &Params);
    let v_dc = net.solve_dc()[node::P];
    let dt = 1.0 / FS;
    let dph = std::f64::consts::TAU * 1_000.0 / FS;
    let mut ph: f64 = 0.0;
    for _ in 0..(0.05 * FS) as usize {
        net.set_fixed(FIXED_IN, AMP * ph.sin());
        net.step_fixed(dt, 4);
        ph += dph;
    }
    let (mut acc, mut cnt) = (0.0f64, 0usize);
    for _ in 0..(0.02 * FS) as usize {
        net.set_fixed(FIXED_IN, AMP * ph.sin());
        let y = net.step_fixed(dt, 4)[node::P] - v_dc;
        ph += dph;
        acc += y * y;
        cnt += 1;
    }
    let ours = (acc / cnt as f64).sqrt() / (AMP / 2f64.sqrt());
    println!("\n=== 1 kHz 增益（生成电路）===\n  Rust = {ours:.2}×");
    assert!(
        (40.0..75.0).contains(&ours),
        "增益 {ours:.1}× 不在 12AX7 共阴级的合理区间"
    );

    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    let body = FIXTURE
        .lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .map(|l| {
            if l.trim_start().starts_with("X1 ") {
                "X1 P G K TRIODE MU=100 EX=1.4 KG1=1060 KP=600 KVB=300".to_string()
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let netlist = format!("{INC}\n{body}\n.ac lin 1 1k 1k\n.end\n");
    let raw = oracle.run_netlist(&netlist).expect("ngspice AC 运行失败");
    // 输入源 AC=0.2 → |v(p)| / 0.2 就是增益
    let spice = raw
        .get("v(p)")
        .map(|x| x[0].abs() / 0.2)
        .unwrap_or(f64::NAN);
    println!(
        "  ngspice（.ac）= {spice:.2}×（差 {:.1}%）",
        (ours - spice).abs() / spice * 100.0
    );
    assert!(
        (ours - spice).abs() / spice < 0.05,
        "增益 {ours:.2}× 与 ngspice {spice:.2}× 差超过 5%"
    );
}

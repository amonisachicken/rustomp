// SPDX-License-Identifier: GPL-3.0-or-later

//! **S1b 回归（器件映射）**：`Q` 卡片 → `net.bjt(...)` 的生成代码，工作点必须与 ngspice 一致。
//!
//! ```text
//! crates/rustomp-import/tests/fixtures/bjt_stage.cir（共射前级，BC549C）
//!   → rustomp-import --mna → tests/generated/bjt_stage.rs → 本测试
//! ```

include!("generated/bjt_stage.rs");

use rustomp_dsp::Circuit;
use rustomp_oracle::Oracle;

/// 直接拿**生成器吃的同一个网表文件**去跑 ngspice —— 避免两边各写一份、悄悄漂移。
const FIXTURE: &str = include_str!("../../rustomp-import/tests/fixtures/bjt_stage.cir");
const LIB: &str = include_str!("../../../devices/devices.lib");
const FS: f64 = 192_000.0;

/// 去掉 `.end`，好接上我们自己的分析语句
fn without_end(cir: &str) -> String {
    cir.lines()
        .filter(|l| !l.trim().eq_ignore_ascii_case(".end"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 与 ngspice 逐节点比工作点。
#[test]
fn generated_bjt_stage_matches_ngspice() {
    // 我们的解
    let mut net = build(FS, &Params);
    let v = net.solve_dc();
    let (vb, vc) = (v[node::B], v[node::C]);

    let Some(oracle) = Oracle::discover_or_skip() else {
        // 仍要防止生成器退化成"什么都不接"
        assert!(vb > 0.6 && vb < 0.8, "Vb={vb} 不像一个导通的 NPN 基极");
        return;
    };
    let netlist = format!("{LIB}\n{}\n.op\n.end\n", without_end(FIXTURE));
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let vb_spice = raw.get("v(b)").map(|x| x[0]).unwrap_or(f64::NAN);
    let vc_spice = raw.get("v(c)").map(|x| x[0]).unwrap_or(f64::NAN);

    println!("\n=== 生成电路（S1b：Q 卡片 → bjt）工作点 ===");
    println!(
        "  Vb: 我们={vb:.6}  ngspice={vb_spice:.6}  差={:+.1} µV",
        (vb - vb_spice) * 1e6
    );
    println!(
        "  Vc: 我们={vc:.6}  ngspice={vc_spice:.6}  差={:+.1} mV",
        (vc - vc_spice) * 1e3
    );
    // Vc 的差别来自集电极电流：bc549c 的 `Nk=0.6339` 我们没建模（DEVLOG E36 已记档），
    // 2 mA 处 Ic 偏 0.26% → 2.2k 上 11.8 mV。这是**器件模型**的已知偏差，不是生成器的问题。
    let ic_ours = (9.0 - vc) / 2200.0;
    let ic_spice = (9.0 - vc_spice) / 2200.0;
    println!(
        "  Ic: 我们={:.4} mA  ngspice={:.4} mA  差={:+.2}%（未建模 Nk，已知）",
        ic_ours * 1e3,
        ic_spice * 1e3,
        (ic_ours / ic_spice - 1.0) * 100.0
    );
    assert!((vb - vb_spice).abs() < 2e-4, "Vb 偏差过大");
    assert!(
        (ic_ours / ic_spice - 1.0).abs() < 0.01,
        "Ic 相对误差超过 1%"
    );
}

/// 生成的 `Ckt` 能实时跑：小信号输入下输出应该被反相放大（共射）。
#[test]
fn generated_bjt_stage_amplifies_and_inverts() {
    let mut c = Ckt::new(FS);
    let n = (0.05 * FS) as usize; // 50 ms
    let (mut min, mut max) = (f64::MAX, f64::MIN);
    for i in 0..n {
        let t = i as f64 / FS;
        let x = 2e-3 * (std::f64::consts::TAU * 1_000.0 * t).sin();
        let y = c.process(x);
        if i > n / 2 {
            min = min.min(y);
            max = max.max(y);
        }
    }
    let pp = max - min;
    println!("\n=== 生成电路（S1b）共射放大 ===");
    println!(
        "  输入 2 mV 峰值的 1 kHz → 输出峰峰 {:.3} V（增益 ≈ {:.0}×）",
        pp,
        pp / 4e-3
    );
    assert!(pp > 0.1, "共射级应该有明显增益（实测峰峰 {pp:.4} V）");
}

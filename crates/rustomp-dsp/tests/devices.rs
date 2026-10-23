// SPDX-License-Identifier: GPL-3.0-or-later

//! **器件级对拍**：库里二极管模型 vs ngspice 的 SPICE 模型，逐点比 V-I。
//!
//! 判据（`docs/component-library.md` §5）：`|I| ≥ 1µA` 相对误差 < 2%、绝对电压差 < 5 mV。
//! 没有 ngspice 时自动跳过。

use rustomp_dsp::{Diode, D_1N34A, D_1N4007, D_1N4148};
use rustomp_oracle::Oracle;

const LIB: &str = include_str!("../../../devices/devices.lib");

/// 电流源注入 + `.op`，取节点 `a` 的电压
fn spice_voltage(oracle: &Oracle, model: &str, current: f64) -> f64 {
    let netlist = format!("{LIB}\nI1 0 a {current:e}\nD1 a 0 {model}\n.op\n.end\n");
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let v = raw.get("v(a)").expect("rawfile 里没有 v(a)");
    *v.last().unwrap()
}

fn compare(oracle: &Oracle, d: Diode, model: &str) {
    let mut worst_rel = 0.0f64;
    let mut worst_abs = 0.0f64;
    for &i in &[1e-6, 1e-5, 1e-4, 1e-3, 1e-2] {
        let v_spice = spice_voltage(oracle, model, i);
        let v_ours = d.voltage_for_current(i);
        let rel = (v_ours - v_spice).abs() / v_spice.abs().max(1e-3);
        worst_rel = worst_rel.max(rel);
        worst_abs = worst_abs.max((v_ours - v_spice).abs());
        println!(
            "  {model:8} I={i:8.0e} A  ngspice={v_spice:.5} V  我们={v_ours:.5} V  差={:+.2}%",
            (v_ours - v_spice) / v_spice * 100.0
        );
    }
    assert!(worst_rel < 0.02, "{model}: 最大相对误差 {:.2}% 超过 2%", worst_rel * 100.0);
    assert!(worst_abs < 0.005, "{model}: 最大绝对误差 {worst_abs:.4} V 超过 5 mV");
}

#[test]
fn diodes_match_ngspice_dc() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    println!("\n=== 二极管 V-I 对拍 ===");
    compare(&oracle, D_1N4148, "1N4148");
    compare(&oracle, D_1N34A, "1N34A");
    compare(&oracle, D_1N4007, "1N4007");
}

/// 2N7000 目前只有 SPICE 侧的手写子电路（DSP 模型待实现），这里做标定冒烟：
/// 转移特性单调，且 Vgs=5V 时 Id 落在 0.15~0.30 A。
#[test]
fn handwritten_2n7000_subckt_is_calibrated() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    let points = [3.0f64, 4.0, 5.0, 6.0];
    let mut ids = Vec::new();
    for vgs in points {
        let netlist = format!("{LIB}\nVDS d 0 5\nVGS g 0 {vgs}\nX1 d g 0 N2N7000\n.op\n.end\n");
        let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
        let i = raw.get("i(vds)").map(|v| v.last().unwrap().abs()).unwrap_or(0.0);
        ids.push(i);
    }
    println!("\n=== 2N7000 手写子电路 ===");
    for (vgs, id) in points.iter().zip(&ids) {
        println!("  Vgs={vgs:.0}V  Id={id:.4} A");
    }
    assert!(ids.windows(2).all(|w| w[1] > w[0]), "转移特性不单调");
    assert!((0.15..0.30).contains(&ids[2]), "Vgs=5V 时 Id={} 不在 0.15~0.30 A", ids[2]);
}

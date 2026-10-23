// SPDX-License-Identifier: GPL-3.0-or-later

//! **器件级对拍**：库里二极管模型 vs ngspice 的 SPICE 模型，逐点比 V-I。
//!
//! 判据（`docs/component-library.md` §5）：`|I| ≥ 1µA` 相对误差 < 2%、绝对电压差 < 5 mV。
//! 没有 ngspice 时自动跳过。

use rustomp_dsp::{Diode, D_1N34A, D_1N4007, D_1N4148, NMOS_2N7000, NMOS_2N7000_BODY};
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

// ---------------------------------------------------------------- BJT

/// Vbe 扫描，与 ngspice 逐点比 Ic / Ib。
///
/// NPN：`VBE` 源扫基极、集电极 5 V。PNP 用"电压取反"的等价形式：
/// 我们的模型按 NPN 公式计算，PNP 传 `(-veb, -vcb)`。
fn compare_bjt(oracle: &Oracle, b: rustomp_dsp::Bjt, model: &str, npn: bool, sweep: &[f64]) {
    let mut worst = 0.0f64;
    for &v in sweep {
        let netlist = if npn {
            format!("{LIB}\nVBE b 0 {v}\nVCE c 0 5\nQ1 c b 0 {model}\n.op\n.end\n")
        } else {
            // PNP：发射极接 +v；基极、集电极各经 0V 源接地（便于分别测 Ib / Ic）
            format!("{LIB}\nVEB e 0 {v}\nVB b 0 0\nVC c 0 0\nQ1 c b e {model}\n.op\n.end\n")
        };
        let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
        let ic_spice = raw
            .get("i(vce)")
            .or_else(|| raw.get("i(vc)"))
            .map(|x| x.last().unwrap().abs())
            .unwrap_or(0.0);
        // 基极电流（NPN 用 i(vbe)，PNP 用 i(veb)）
        let ib_spice = raw
            .get(if npn { "i(vbe)" } else { "i(vb)" })
            .map(|x| x.last().unwrap().abs())
            .unwrap_or(0.0);
        // 我们的模型
        // PNP：SPICE 内部用 `type = -1` 把电压取反，等价于把 (veb, vcb) 直接喂给 NPN 公式
        let (ic, ib) = if npn {
            b.terminal(v, v - 5.0)
        } else {
            b.terminal(v, 0.0)
        };
        let ic = ic.abs();
        let ib = ib.abs();
        if ic_spice < 1e-9 {
            continue;
        }
        let rel_ic = (ic - ic_spice).abs() / ic_spice;
        let rel_ib = if ib_spice > 1e-12 {
            (ib - ib_spice).abs() / ib_spice
        } else {
            0.0
        };
        worst = worst.max(rel_ic.max(rel_ib));
        let kind = if npn { "NPN" } else { "PNP" };
        println!(
            "  {model:8} {kind} Vbe={v:.3}  Ic: {ic_spice:.4e}/{ic:.4e} ({:+.2}%)  Ib: {ib_spice:.3e}/{ib:.3e} ({:+.2}%)",
            (ic - ic_spice) / ic_spice * 100.0,
            (ib - ib_spice) / ib_spice.max(1e-15) * 100.0
        );
    }
    assert!(worst < 0.05, "{model}: Ic/Ib 最大相对误差 {:.2}% 超过 5%", worst * 100.0);
}

#[test]
fn bjts_match_ngspice_dc() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    println!("\n=== BJT V-I 对拍（Vce=5V, Vbc 随动）===");
    compare_bjt(&oracle, rustomp_dsp::BC549C, "BC549C", true, &[0.55, 0.60, 0.65, 0.70]);
    compare_bjt(&oracle, rustomp_dsp::AC127, "AC127", true, &[0.05, 0.10, 0.15, 0.20]);
    compare_bjt(&oracle, rustomp_dsp::AC128, "AC128", false, &[0.05, 0.10, 0.15, 0.20]);
}

// ---------------------------------------------------------------- JFET

#[test]
fn jfet_matches_ngspice_dc() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    println!("\n=== J201：转移特性（Vds=5V，扫 Vgs）===");
    let mut worst = 0.0f64;
    for &vg in &[-0.55, -0.45, -0.30, -0.15, 0.0] {
        let netlist = format!("{LIB}\nVG g 0 {vg}\nVD d 0 5\nJ1 d g 0 J201\n.op\n.end\n");
        let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
        let i_spice = raw.get("i(vd)").map(|v| v.last().unwrap().abs()).unwrap_or(0.0);
        let i_ours = rustomp_dsp::J201.drain_current(vg, 5.0);
        if i_spice < 1e-9 {
            continue;
        }
        let rel = (i_ours - i_spice).abs() / i_spice;
        worst = worst.max(rel);
        println!("  Vgs={vg:+.2}  ngspice Id={i_spice:.5e}  我们={i_ours:.5e}  差={:+.2}%", rel * 100.0);
    }
    println!("=== J201：输出特性（Vgs=0，扫 Vds）===");
    for &vd in &[0.2, 0.5, 1.0, 3.0] {
        let netlist = format!("{LIB}\nVG g 0 0\nVD d 0 {vd}\nJ1 d g 0 J201\n.op\n.end\n");
        let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
        let i_spice = raw.get("i(vd)").map(|v| v.last().unwrap().abs()).unwrap_or(0.0);
        let i_ours = rustomp_dsp::J201.drain_current(0.0, vd);
        let rel = (i_ours - i_spice).abs() / i_spice;
        worst = worst.max(rel);
        println!("  Vds={vd:.1}  ngspice Id={i_spice:.5e}  我们={i_ours:.5e}  差={:+.2}%", rel * 100.0);
    }
    assert!(worst < 0.05, "J201 最大相对误差 {:.2}% 超过 5%", worst * 100.0);
}

// ---------------------------------------------------------------- MOSFET

/// 2N7000：DSP 侧 `Nmos` 与 SPICE 手写子电路（`N2N7000`）逐点对拍。
///
/// 子电路结构：沟道（level-1）+ 体二极管 + Rd/Rs，与 Rust 侧 `Nmos` + `NMOS_2N7000_BODY` 对应。
#[test]
fn mosfet_2n7000_matches_ngspice_dc() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    let sim = |vgs: f64, vds: f64| -> f64 {
        let netlist =
            format!("{LIB}\nVD d 0 {vds}\nVG g 0 {vgs}\nX1 d g 0 N2N7000\n.op\n.end\n");
        let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
        raw.get("i(vd)").map(|v| v.last().unwrap().abs()).unwrap_or(0.0)
    };

    let mut worst = 0.0f64;
    println!("\n=== 2N7000：转移特性（Vds=5V）===");
    for &vgs in &[2.0f64, 3.0, 4.0, 5.0, 6.0] {
        let i_spice = sim(vgs, 5.0);
        let i_ours = NMOS_2N7000.drain_current(vgs, 5.0);
        if i_spice < 1e-9 {
            continue;
        }
        let rel = (i_ours - i_spice).abs() / i_spice;
        worst = worst.max(rel);
        println!("  Vgs={vgs:.0}V  ngspice={i_spice:.5e}  我们={i_ours:.5e}  差={:+.2}%", rel * 100.0);
    }
    println!("=== 2N7000：三极管区（Vgs=5V）===");
    for &vds in &[0.2f64, 0.5, 1.0] {
        let i_spice = sim(5.0, vds);
        let i_ours = NMOS_2N7000.drain_current(5.0, vds);
        let rel = (i_ours - i_spice).abs() / i_spice;
        worst = worst.max(rel);
        println!("  Vds={vds:.1}V  ngspice={i_spice:.5e}  我们={i_ours:.5e}  差={:+.2}%", rel * 100.0);
    }

    // 体二极管：Vgs=0、Vds=-1V 时电流全部走体二极管（源→漏）
    println!("=== 2N7000：体二极管（Vgs=0）===");
    for &vds in &[-0.6f64, -0.8, -1.0] {
        let i_spice = sim(0.0, vds);
        let i_ours = NMOS_2N7000_BODY.current(-vds); // 二极管正向电压 = −vds
        let rel = (i_ours - i_spice).abs() / i_spice.max(1e-15);
        worst = worst.max(rel);
        println!("  Vds={vds:.1}V  ngspice={i_spice:.5e}  我们={i_ours:.5e}  差={:+.2}%", rel * 100.0);
    }

    assert!(worst < 0.05, "2N7000 最大相对误差 {:.2}% 超过 5%", worst * 100.0);
}

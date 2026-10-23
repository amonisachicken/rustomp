// SPDX-License-Identifier: GPL-3.0-or-later

//! **MNA 装配器对拍**：同一电路分别用 `Netlist` 和 ngspice 求解，逐点比较节点电压 / 波形。
//!
//! 覆盖：非线性单口（二极管削波）、三端器件（BJT 共射、JFET 共源）、
//! 动态元件（RC + 削波）、运放（非反相放大，装进代数环）。
//!
//! 没有 ngspice 时自动跳过（运放那组不需要 ngspice）。

use rustomp_dsp::{Netlist, BC549C, D_1N4148, J201, GND, OP07};
use rustomp_oracle::Oracle;

const LIB: &str = include_str!("../../../devices/devices.lib");

/// 跑一个 `.op` 网表，取出某个节点电压。
fn spice_op(oracle: &Oracle, body: &str, var: &str) -> f64 {
    let netlist = format!("{LIB}\n{body}\n.op\n.end\n");
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    if std::env::var("MNA_DEBUG").is_ok() {
        eprintln!("vars = {:?}", raw.vars);
    }
    raw.get(var).map(|v| v[0]).unwrap_or_else(|| panic!("ngspice 没有输出 {var}，有 {:?}", raw.vars))
}

/// 二极管削波：`Vin → R → out`，两只 1N4148 反并联到地。
#[test]
fn diode_clipper_dc_matches_ngspice() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    let mut worst = 0.0f64;
    println!("\n=== 二极管削波（DC，DC 扫描）===");
    for &vin in &[0.2f64, 0.4, 0.6, 1.0, -0.4, -1.0] {
        let body = format!("V1 in 0 {vin}\nR1 in out 1k\nD1 out 0 1N4148\nD2 0 out 1N4148");
        let v_spice = spice_op(&oracle, &body, "v(out)");

        // 节点：1=in, 2=out；支路：3
        let mut net = Netlist::<4>::new();
        net.resistor(1, 2, 1000.0)
            .diode(D_1N4148, 2, GND)
            .diode(D_1N4148, GND, 2)
            .voltage_source(3, 1, GND, vin);
        let v = net.solve_dc();
        let err = (v[2] - v_spice).abs();
        worst = worst.max(err);
        println!("  Vin={vin:+.1} V  ngspice={v_spice:.5}  我们={:.5}  差={:.1} µV", v[2], err * 1e6);
    }
    assert!(worst < 1e-4, "二极管削波最大偏差 {:.1} µV 超过 0.1 mV", worst * 1e6);
}

/// BJT 共射偏置：`VCC=9V, Rc=4.7k, Rb=470k`，比较基极/集电极电压。
#[test]
fn bjt_common_emitter_dc_matches_ngspice() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    let body = "VCC vcc 0 9\nRC vcc c 4.7k\nRB vcc b 470k\nQ1 c b 0 BC549C";
    let (vb_spice, vc_spice) = (
        spice_op(&oracle, body, "v(b)"),
        spice_op(&oracle, body, "v(c)"),
    );

    // 节点：1=vcc, 2=c, 3=b；支路：4
    let mut net = Netlist::<5>::new();
    net.resistor(1, 2, 4700.0)
        .resistor(1, 3, 470_000.0)
        .bjt(BC549C, true, 2, 3, GND)
        .voltage_source(4, 1, GND, 9.0);
    let v = net.solve_dc();

    println!("\n=== BJT 共射偏置（DC）===");
    println!("  Vb: ngspice={vb_spice:.5}  我们={:.5}  差={:.1} µV", v[3], (v[3] - vb_spice).abs() * 1e6);
    println!("  Vc: ngspice={vc_spice:.5}  我们={:.5}  差={:.1} µV", v[2], (v[2] - vc_spice).abs() * 1e6);
    assert!((v[3] - vb_spice).abs() < 2e-4, "Vb 偏差过大");
    assert!((v[2] - vc_spice).abs() < 2e-3, "Vc 偏差过大");
}

/// JFET 共源：`VDD=9V, Rd=2.2k, Rg=1M`，比较漏极电压。
#[test]
fn jfet_common_source_dc_matches_ngspice() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    let body = "VDD vdd 0 9\nRD vdd d 2.2k\nRG g 0 1meg\nJ1 d g 0 J201";
    let vd_spice = spice_op(&oracle, body, "v(d)");

    // 节点：1=vdd, 2=d, 3=g；支路：4
    let mut net = Netlist::<5>::new();
    net.resistor(1, 2, 2200.0)
        .resistor(3, GND, 1e6)
        .jfet(J201, 2, 3, GND)
        .voltage_source(4, 1, GND, 9.0);
    let v = net.solve_dc();
    println!("\n=== JFET 共源（DC）===");
    println!("  Vd: ngspice={vd_spice:.5}  我们={:.5}  差={:.1} µV", v[2], (v[2] - vd_spice).abs() * 1e6);
    assert!((v[2] - vd_spice).abs() < 1e-3, "Vd 偏差 {:.2} mV", (v[2] - vd_spice).abs() * 1e3);
}

/// 瞬态：`Vin → R(10k) → out`，`C(100n) → 地`，再并一只 1N4148 削波。
///
/// 我们的积分器是后向欧拉（一阶），ngspice 默认梯形（二阶），
/// 所以不追求逐位相等，只要求波形偏差 < 3%。
#[test]
fn rc_with_diode_transient_matches_ngspice() {
    let Some(oracle) = Oracle::discover() else {
        eprintln!("跳过：本机没有 ngspice");
        return;
    };
    const FS: f64 = 48_000.0;
    let dt = 1.0 / FS;
    let body = format!(
        "V1 in 0 PULSE(0 1 0 1n 1n 3m 6m)\nR1 in out 10k\nC1 out 0 100n\nD1 out 0 1N4148\n.tran {dt} 6m 0\n"
    );
    let raw = oracle
        .run_netlist(&format!("{LIB}\n{body}.end\n"))
        .expect("ngspice 运行失败");
    // ngspice 的 `.tran` 是**变步长**（5n 只是上限），必须按固定 dt 重采样后才能逐点比
    let v_spice = raw.resample("v(out)", dt).expect("resample 失败");
    let t_spice = raw.get("time").expect("没有 time");

    // 节点：1=in, 2=out；支路：3
    let mut net = Netlist::<4>::new();
    net.resistor(1, 2, 10_000.0)
        .capacitor(2, GND, 100e-9)
        .diode(D_1N4148, 2, GND)
        .voltage_source(3, 1, GND, 0.0);

    let _ = t_spice;
    let n = (6e-3 / dt) as usize;
    let n = n.min(v_spice.len());
    let mut worst = 0.0f64;
    let mut checked = 0;
    for i in 0..n {
        let t = i as f64 * dt;
        // PULSE(0 1 0 1n 1n 3m 6m)：0 起的上升沿立即到 1，3 ms 后降到 0
        let vin = if t < 3e-3 { 1.0 } else { 0.0 };
        net.set_voltage(0, vin);
        let v = net.step(dt);
        if i >= 48 {
            worst = worst.max((v[2] - v_spice[i]).abs());
            checked += 1;
        }
    }
    println!("\n=== RC + 二极管 瞬态（48 kHz，后向欧拉 vs 梯形）===");
    println!("  检查 {checked} 点，最大偏差 {:.2} mV", worst * 1e3);
    assert!(checked > 200, "采样点太少");
    assert!(worst < 0.03, "瞬态最大偏差 {:.1} mV 超过 30 mV", worst * 1e3);
}

/// 运放非反相放大（10 倍）：`R1=10k (out→m)`、`R2=1k (m→VREF)`。
///
/// 注意 **R2 要接 VREF（4.5 V 偏置）而不是地** —— 运放模型是"绝对电压"的，
/// 反馈网络必须以同一偏置为参考，否则它会试图把 4.5 V 放大 11 倍而直接饱和。
///
/// 这里验证的是**装配器把运放更新式解进代数环**：DC 增益必须是 1 + R1/R2 = 11，
/// 而不是被单极点拖成别的值。同时检查 4× 过采样下的 -3dB 带宽。
#[test]
fn opamp_noninverting_stage() {
    const VREF: f64 = 4.5;
    let fs = 48_000.0 * 4.0;
    let dt = 1.0 / fs;
    let model = OP07.model(dt);

    // 节点：1=in, 2=m, 3=out, 4=VREF；支路：5=运放输出, 6=输入源, 7=VREF 源
    let build = || {
        let mut net = Netlist::<8>::new();
        net.resistor(3, 2, 10_000.0) // 反馈 R1：out → m
            .resistor(2, 4, 1_000.0) // R2：m → VREF（闭环增益 1 + R1/R2 = 11）
            .opamp(model, 1, 2, 3, 5)
            .voltage_source(6, 1, GND, VREF)
            .voltage_source(7, 4, GND, VREF);
        net
    };
    let mut net = build();

    net.set_voltage(0, VREF + 0.05); // 输出 = VREF + 0.55
    let v = net.solve_dc();
    let gain_dc = (v[3] - VREF) / 0.05;
    println!("\n=== 运放非反相级（10×）===");
    println!("  DC 增益 = {gain_dc:.3}（期望 11）");
    assert!((gain_dc - 11.0).abs() < 0.05, "DC 增益 {gain_dc:.3} 应 ≈ 11（代数环没解对？）");

    // 运放饱和：输入给 1 V，输出应被钉在摆幅上限附近
    let mut sat = build();
    sat.set_voltage(0, VREF + 1.0);
    let v_sat = sat.solve_dc();
    println!("  饱和测试：Vin=VREF+1V → Vout={:.3}（上限 {:.1}）", v_sat[3], OP07.vout_max);
    assert!((v_sat[3] - OP07.vout_max).abs() < 0.05, "饱和时应钉在 {:.1} V", OP07.vout_max);
    drop(net);

    // 交流：测 -3dB（GBW/k = 600k/11 ≈ 54.5 kHz）
    let mut net = build();
    let measure = |f: f64, net: &mut Netlist<8>| -> f64 {
        net.reset();
        net.set_voltage(1, VREF);
        let n = ((60.0 * fs / f).round() as usize).max(64);
        let (mut pk_out, mut pk_in) = (0.0f64, 0.0f64);
        for i in 0..n {
            let t = i as f64 * dt;
            let vin = VREF + 1e-3 * (std::f64::consts::TAU * f * t).sin();
            net.set_voltage(0, vin);
            let v = net.step(dt);
            if i > n / 2 {
                pk_out = pk_out.max((v[3] - VREF).abs());
                pk_in = pk_in.max((vin - VREF).abs());
            }
        }
        pk_out / pk_in
    };
    let mut f3 = None;
    let mut last = 0.0;
    for i in 0..=15 {
        let f = 10_000.0 * (1.15_f64).powi(i);
        let h = measure(f, &mut net);
        println!("  f={f:>8.0} Hz  |H|={h:>6.2}");
        if f3.is_none() && h < 11.0 / std::f64::consts::SQRT_2 {
            f3 = Some(f);
        }
        last = h;
    }
    let f3 = f3.expect("-3dB 点未找到");
    // 模拟原型是 54.5 kHz；192 kHz 采样下离散化会把它压到 ~40 kHz（已知、已记档：
    // 详见 devices/opamp.rs 顶部"应在 ≥4× 过采样下运行"——48 kHz 主机的 4× 正是 192 kHz）
    println!("  -3dB ≈ {f3:.0} Hz（模拟原型 54.5 kHz；192 kHz 采样下偏低属已知离散化）");
    assert!((34_000.0..50_000.0).contains(&f3), "-3dB 点 {f3:.0} Hz 偏离太多");
    // 近 Nyquist 处增益不会降到 0：`v_out` 是 `vd[n]` 的代数函数（无延迟），
    // 因此存在一个 `b/(2+bk)` 的地板。这是"零延迟进代数环"的代价 ——
    // 该频段会被引擎的抽取滤波器滤掉，音频带内不受影响。
    println!("  80 kHz 处 |H|={last:.2}（近 Nyquist 地板，非零是设计取舍）");
    assert!(last < 6.5, "高频端应明显滚降（实测 {last:.2}）");
}

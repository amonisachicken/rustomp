// SPDX-License-Identifier: GPL-3.0-or-later

//! **MNA 装配器对拍**：同一电路分别用 `Netlist` 和 ngspice 求解，逐点比较节点电压 / 波形。
//!
//! 覆盖：非线性单口（二极管削波）、三端器件（BJT 共射、JFET 共源）、
//! 动态元件（RC + 削波）、运放（非反相放大，装进代数环）。
//!
//! 没有 ngspice 时自动跳过（运放那组不需要 ngspice）。

use rustomp_dsp::{Netlist, BC549C, BC559C, D_1N4148, GND, J201, NMOS_2N7000, OP07};
use rustomp_oracle::Oracle;

const LIB: &str = include_str!("../../../devices/devices.lib");

/// 跑一个 `.op` 网表，取出某个节点电压。
fn spice_op(oracle: &Oracle, body: &str, var: &str) -> f64 {
    let netlist = format!("{LIB}\n{body}\n.op\n.end\n");
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    if std::env::var("MNA_DEBUG").is_ok() {
        eprintln!("vars = {:?}", raw.vars);
    }
    raw.get(var)
        .map(|v| v[0])
        .unwrap_or_else(|| panic!("ngspice 没有输出 {var}，有 {:?}", raw.vars))
}

/// 二极管削波：`Vin → R → out`，两只 1N4148 反并联到地。
#[test]
fn diode_clipper_dc_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
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
        println!(
            "  Vin={vin:+.1} V  ngspice={v_spice:.5}  我们={:.5}  差={:.1} µV",
            v[2],
            err * 1e6
        );
    }
    assert!(
        worst < 1e-4,
        "二极管削波最大偏差 {:.1} µV 超过 0.1 mV",
        worst * 1e6
    );
}

/// BJT 共射偏置：`VCC=9V, Rc=4.7k, Rb=470k`，比较基极/集电极电压。
#[test]
fn bjt_common_emitter_dc_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
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
    println!(
        "  Vb: ngspice={vb_spice:.5}  我们={:.5}  差={:.1} µV",
        v[3],
        (v[3] - vb_spice).abs() * 1e6
    );
    println!(
        "  Vc: ngspice={vc_spice:.5}  我们={:.5}  差={:.1} µV",
        v[2],
        (v[2] - vc_spice).abs() * 1e6
    );
    assert!((v[3] - vb_spice).abs() < 2e-4, "Vb 偏差过大");
    assert!((v[2] - vc_spice).abs() < 2e-3, "Vc 偏差过大");
}

/// JFET 共源：`VDD=9V, Rd=2.2k, Rg=1M`，比较漏极电压。
#[test]
fn jfet_common_source_dc_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
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
    println!(
        "  Vd: ngspice={vd_spice:.5}  我们={:.5}  差={:.1} µV",
        v[2],
        (v[2] - vd_spice).abs() * 1e6
    );
    assert!(
        (v[2] - vd_spice).abs() < 1e-3,
        "Vd 偏差 {:.2} mV",
        (v[2] - vd_spice).abs() * 1e3
    );
}

/// 瞬态：`Vin → R(10k) → out`，`C(100n) → 地`，再并一只 1N4148 削波。
///
/// 我们的积分器是后向欧拉（一阶），ngspice 默认梯形（二阶），
/// 所以不追求逐位相等，只要求波形偏差 < 3%。
#[test]
fn rc_with_diode_transient_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
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
    assert!(
        worst < 0.03,
        "瞬态最大偏差 {:.1} mV 超过 30 mV",
        worst * 1e3
    );
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
    assert!(
        (gain_dc - 11.0).abs() < 0.05,
        "DC 增益 {gain_dc:.3} 应 ≈ 11（代数环没解对？）"
    );

    // 运放饱和：输入给 1 V，输出应被钉在摆幅上限附近
    let mut sat = build();
    sat.set_voltage(0, VREF + 1.0);
    let v_sat = sat.solve_dc();
    println!(
        "  饱和测试：Vin=VREF+1V → Vout={:.3}（上限 {:.1}）",
        v_sat[3], OP07.vout_max
    );
    assert!(
        (v_sat[3] - OP07.vout_max).abs() < 0.05,
        "饱和时应钉在 {:.1} V",
        OP07.vout_max
    );
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
    assert!(
        (34_000.0..50_000.0).contains(&f3),
        "-3dB 点 {f3:.0} Hz 偏离太多"
    );
    // 近 Nyquist 处增益不会降到 0：`v_out` 是 `vd[n]` 的代数函数（无延迟），
    // 因此存在一个 `b/(2+bk)` 的地板。这是"零延迟进代数环"的代价 ——
    // 该频段会被引擎的抽取滤波器滤掉，音频带内不受影响。
    println!("  80 kHz 处 |H|={last:.2}（近 Nyquist 地板，非零是设计取舍）");
    assert!(last < 6.5, "高频端应明显滚降（实测 {last:.2}）");
}

// ---------------------------------------------------------------------------
// 固定电位端子（电源轨 / 输入端口）上的非线性器件
//
// 下游 Freedist 报告（P0）：`stamp_ports` / `multiport` 里，固定节点的**列**被
// 从 Y 里跳过，但 `i_eq = i − J·v0` 仍然减了全部列的 `J·v0`。固定列在 LHS 不存在、
// 在 RHS 却成了常数误差电流 —— 只要固定端接的是**正偏结**（PNP 射极接 VCC），
// 误差就是几十安量级，工作点直接落到伪解上。
//
// 这一组测试守着两件事：
//   ① 报告里的最小复现（PNP 射极固定 9 V、基极固定 8.3 V）必须与 ngspice 一致；
//   ② **等价性不变量**：同一个电位用「固定节点」和「理想电压源」两种方式给，
//      解必须相同。这条不依赖任何魔法数字，能同时守住 BJT / JFET / MOSFET 两条路径。
// ---------------------------------------------------------------------------

/// 报告里的最小复现：PNP 射极接固定 9 V 轨、基极固定 8.3 V。
///
/// ngspice 同网表：`vc ≈ 1.51 V`；带 bug 时落到 −147 V 附近的伪解。
#[test]
fn pnp_emitter_on_fixed_rail_matches_ngspice() {
    // 节点：0=GND，1=集电极（未知），2=基极（固定），3=射极（固定）
    let mut net = Netlist::<4>::new();
    net.set_unknowns(2);
    net.fix_node(3, 9.0); // 射极 = VCC
    net.fix_node(2, 8.3); // 基极 → Vbe = 0.7 V
    net.resistor(1, GND, 2200.0);
    net.bjt(BC559C, false, 1, 2, 3);
    let v = net.solve_dc();

    println!("\n=== 固定电源轨上的 PNP（DC）===");
    println!("  Vc = {:.4} V", v[1]);

    if let Some(oracle) = Oracle::discover_or_skip() {
        let body = "VCC vcc 0 9\nVB b 0 8.3\nRC c 0 2.2k\nQ1 c b vcc BC559C";
        let vc_spice = spice_op(&oracle, body, "v(c)");
        println!(
            "  ngspice={vc_spice:.5}  差={:.2} mV",
            (v[1] - vc_spice).abs() * 1e3
        );
        assert!(
            (v[1] - vc_spice).abs() < 2e-3,
            "Vc = {:.4} V，ngspice = {vc_spice:.5} V —— 差得太多",
            v[1]
        );
    }
    assert!(
        (0.5..3.0).contains(&v[1]),
        "Vc = {:.4} V 落在伪解上（应为 ~1.51 V）",
        v[1]
    );
}

/// 等价性不变量：`fix_node(n, V)` 与"理想电压源把 n 钉在 V"必须解出同一个电路。
#[test]
fn fixed_node_equals_ideal_voltage_source() {
    // ---- ① BJT（PNP：射极 + 基极都钉死）----
    let mut fixed = Netlist::<4>::new();
    fixed.set_unknowns(2);
    fixed.fix_node(3, 9.0);
    fixed.fix_node(2, 8.3);
    fixed.resistor(1, GND, 2200.0);
    fixed.bjt(BC559C, false, 1, 2, 3);
    let v_fixed = fixed.solve_dc();

    // 节点：0=GND，1=c，2=b，3=e；支路：4=基极源，5=射极源
    let mut sourced = Netlist::<6>::new();
    sourced
        .resistor(1, GND, 2200.0)
        .bjt(BC559C, false, 1, 2, 3)
        .voltage_source(4, 2, GND, 8.3)
        .voltage_source(5, 3, GND, 9.0);
    let v_sourced = sourced.solve_dc();

    println!("\n=== 固定节点 vs 理想电压源（等价性）===");
    println!(
        "  BJT  PNP  Vc: fixed={:.6}  source={:.6}",
        v_fixed[1], v_sourced[1]
    );
    assert!(
        (v_fixed[1] - v_sourced[1]).abs() < 1e-6,
        "BJT 两种接法不一致：{:.6} vs {:.6}",
        v_fixed[1],
        v_sourced[1]
    );

    // ---- ② JFET（漏极固定 9 V、栅极固定 −0.5 V）----
    let mut fixed = Netlist::<4>::new();
    fixed.set_unknowns(2);
    fixed.fix_node(2, -0.5);
    fixed.fix_node(3, 9.0);
    fixed.resistor(1, GND, 1000.0);
    fixed.jfet(J201, 3, 2, 1);
    let v_fixed = fixed.solve_dc();

    let mut sourced = Netlist::<6>::new();
    sourced
        .resistor(1, GND, 1000.0)
        .jfet(J201, 3, 2, 1)
        .voltage_source(4, 2, GND, -0.5)
        .voltage_source(5, 3, GND, 9.0);
    let v_sourced = sourced.solve_dc();

    println!(
        "  JFET      Vs: fixed={:.6}  source={:.6}",
        v_fixed[1], v_sourced[1]
    );
    assert!(
        (v_fixed[1] - v_sourced[1]).abs() < 1e-6,
        "JFET 两种接法不一致：{:.6} vs {:.6}",
        v_fixed[1],
        v_sourced[1]
    );

    // ---- ③ MOSFET（漏极固定 9 V、栅极固定 3 V）----
    let mut fixed = Netlist::<4>::new();
    fixed.set_unknowns(2);
    fixed.fix_node(2, 3.0);
    fixed.fix_node(3, 9.0);
    fixed.resistor(1, GND, 1000.0);
    fixed.mosfet(NMOS_2N7000, 3, 2, 1);
    let v_fixed = fixed.solve_dc();

    let mut sourced = Netlist::<6>::new();
    sourced
        .resistor(1, GND, 1000.0)
        .mosfet(NMOS_2N7000, 3, 2, 1)
        .voltage_source(4, 2, GND, 3.0)
        .voltage_source(5, 3, GND, 9.0);
    let v_sourced = sourced.solve_dc();

    println!(
        "  MOSFET    Vs: fixed={:.6}  source={:.6}",
        v_fixed[1], v_sourced[1]
    );
    assert!(
        (v_fixed[1] - v_sourced[1]).abs() < 1e-6,
        "MOSFET 两种接法不一致：{:.6} vs {:.6}",
        v_fixed[1],
        v_sourced[1]
    );
}

/// 瞬态也要一致：固定输入节点（插件生成的 `build()` 就是这么做的）与理想源驱动。
#[test]
fn fixed_input_equals_ideal_source_in_transient() {
    let dt = 1.0 / 192_000.0; // 4× 过采样下的电路采样率

    // 固定输入：基极电位每拍用 `set_fixed` 写入
    let mut fixed = Netlist::<4>::new();
    fixed.set_unknowns(2);
    let idx_b = fixed.fix_node(2, 8.3);
    fixed.fix_node(3, 9.0);
    fixed.resistor(1, GND, 2200.0);
    fixed.bjt(BC559C, false, 1, 2, 3);
    fixed.solve_dc();

    // 理想源：同一个基极电位用电压源给（支路 4）
    let mut sourced = Netlist::<6>::new();
    sourced
        .resistor(1, GND, 2200.0)
        .bjt(BC559C, false, 1, 2, 3)
        .voltage_source(4, 2, GND, 8.3)
        .voltage_source(5, 3, GND, 9.0);
    sourced.solve_dc();

    let mut worst = 0.0f64;
    for i in 0..400 {
        let vb = 8.3 + 0.05 * (i as f64 * 0.05).sin();
        fixed.set_fixed(idx_b, vb);
        sourced.set_voltage(0, vb);
        let a = fixed.step(dt);
        let b = sourced.step(dt);
        worst = worst.max((a[1] - b[1]).abs());
    }
    println!("\n=== 瞬态等价性（PNP，400 拍）===");
    println!("  集电极最大偏差 = {:.3} µV", worst * 1e6);
    assert!(worst < 1e-3, "瞬态偏差 {:.1} mV 太大", worst * 1e3);
}

/// DC 解必须真的满足 KCL（P1）：`solve_dc()` 之后 `last_dc_resid` 要达标，
/// 而且**不该**靠"换起点重试"来兜（正常电路一次就该收敛，否则说明起点或收敛判据有问题）。
#[test]
fn dc_solution_satisfies_kcl() {
    // ① 二极管削波
    let mut net = Netlist::<4>::new();
    net.resistor(1, 2, 1000.0)
        .diode(D_1N4148, 2, GND)
        .diode(D_1N4148, GND, 2)
        .voltage_source(3, 1, GND, 0.6);
    net.solve_dc();
    println!("\n=== DC 残差（KCL 复检）===");
    println!(
        "  二极管削波：resid={:.2e} iters={} retried={}",
        net.last_dc_resid, net.last_dc_iters, net.last_dc_retried
    );
    assert!(net.last_dc_resid.is_finite() && net.last_dc_resid <= 1e-6);

    // ② BJT 共射
    let mut net = Netlist::<5>::new();
    net.resistor(1, 2, 4700.0)
        .resistor(1, 3, 470_000.0)
        .bjt(BC549C, true, 2, 3, GND)
        .voltage_source(4, 1, GND, 9.0);
    net.solve_dc();
    println!(
        "  BJT 共射：resid={:.2e} iters={} retried={}",
        net.last_dc_resid, net.last_dc_iters, net.last_dc_retried
    );
    assert!(net.last_dc_resid.is_finite() && net.last_dc_resid <= 1e-6);
    assert!(!net.last_dc_retried, "正常电路不该走重试通道");

    // ③ 固定电源轨上的 PNP（P0 的电路：以前这里会落到伪解）
    let mut net = Netlist::<4>::new();
    net.set_unknowns(2);
    net.fix_node(3, 9.0);
    net.fix_node(2, 8.3);
    net.resistor(1, GND, 2200.0);
    net.bjt(BC559C, false, 1, 2, 3);
    net.solve_dc();
    println!(
        "  PNP 固定轨：resid={:.2e} iters={} retried={}",
        net.last_dc_resid, net.last_dc_iters, net.last_dc_retried
    );
    assert!(net.last_dc_resid.is_finite() && net.last_dc_resid <= 1e-6);
    assert!(!net.last_dc_retried, "P0 修好后这里不该再需要重试");
}

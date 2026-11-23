// SPDX-License-Identifier: GPL-3.0-or-later

//! **MNA 装配器对拍**：同一电路分别用 `Netlist` 和 ngspice 求解，逐点比较节点电压 / 波形。
//!
//! 覆盖：非线性单口（二极管削波）、三端器件（BJT 共射、JFET 共源）、
//! 动态元件（RC + 削波）、运放（非反相放大，装进代数环）。
//!
//! 没有 ngspice 时自动跳过（运放那组不需要 ngspice）。

use rustomp_dsp::{Element, Netlist, BC549C, BC559C, D_1N4148, GND, J201, NMOS_2N7000, OP07};
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

// ---------------------------------------------------------------------------
// 电感（M0）：理想 L + 绕线电阻 `rdc`，后向欧拉伴生模型。
//
// 验收用**解析解**为主（比 oracle 更硬），ngspice 只作交叉验证：
//   RL 阶跃  i(t) = (V/R)·(1 − e^(−t/τ))，τ = L/(R + rdc)
//   LC 振铃  f0 = 1/(2π√(LC))
//   DC       电感 = rdc（理想 L 在 DC 是短路，会毁掉矩阵）
// ---------------------------------------------------------------------------

/// RL 阶跃：与解析解比，并确认**一阶收敛**（dt 变小误差应显著变小）。
#[test]
fn inductor_rl_step_matches_analytic() {
    const V: f64 = 1.0;
    const R: f64 = 100.0;
    const RDC: f64 = 0.1;
    const L: f64 = 10e-3;
    let tau = L / (R + RDC);
    let i_ss = V / (R + RDC);

    let mut worsts = Vec::new();
    for &dt in &[tau / 20.0, tau / 80.0] {
        // 节点：1 = 源正（in），2 = out，支路 3
        let mut net = Netlist::<4>::new();
        net.inductor(1, 2, L, RDC)
            .resistor(2, GND, R)
            .voltage_source(3, 1, GND, 0.0);
        net.solve_dc();
        net.set_voltage(0, V);

        let n = (5.0 * tau / dt) as usize;
        let mut worst = 0.0f64;
        for k in 1..=n {
            let v = net.step(dt);
            let t = k as f64 * dt;
            // 流过 R 的电流 = 电感电流（串联）
            let i = v[2] / R;
            let exact = i_ss * (1.0 - (-t / tau).exp());
            worst = worst.max((i - exact).abs() / i_ss);
        }
        println!(
            "  RL 阶跃 dt=τ/{:.0}：最大相对误差 {:.2} %",
            tau / dt,
            worst * 100.0
        );
        assert!(
            worst < 0.03,
            "dt={dt:.3e} 时误差 {:.2}% 超过 3%",
            worst * 100.0
        );
        worsts.push(worst);
    }
    // 后向欧拉是一阶：dt 缩小 4 倍，误差应明显下降
    assert!(worsts[1] < worsts[0] * 0.5, "一阶收敛不成立：{:?}", worsts);
}

/// 同一个 RL 阶跃与 ngspice 对拍（ngspice 默认梯形 + 变步长，这里按固定 dt 重采样）。
#[test]
fn inductor_rl_step_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    const R: f64 = 100.0;
    const RDC: f64 = 0.1;
    const L: f64 = 10e-3;
    let tau = L / (R + RDC);
    let dt = tau / 40.0;

    // ⚠️ ngspice 的 `L` **没有** DCR 参数（第 4 个 token 会被当成别的意思，实测差了 10×），
    // 所以 DCR 必须用串联电阻表达 —— 这也是真实网表里变压器/电感的常见写法。
    let body = format!(
        "V1 in 0 PULSE(0 1 0 1n 1n 1 2)\nL1 in mid {L}\nRL mid out {RDC}\nR1 out 0 {R}\n.tran {dt} {}\n",
        6.0 * tau
    );
    let raw = oracle
        .run_netlist(&format!("{LIB}\n{body}.end\n"))
        .expect("ngspice 运行失败");
    let i_spice: Vec<f64> = raw
        .resample("v(out)", dt)
        .expect("resample 失败")
        .iter()
        .map(|v| v / R)
        .collect();

    // 节点：1 = in，2 = out，支路 3
    let mut net = Netlist::<4>::new();
    net.inductor(1, 2, L, RDC)
        .resistor(2, GND, R)
        .voltage_source(3, 1, GND, 0.0);
    net.solve_dc();
    net.set_voltage(0, 1.0);

    let mut worst = 0.0f64;
    let mut worst_at = (0usize, 0.0f64, 0.0f64);
    let mut checked = 0;
    let i_ss = 1.0 / (R + RDC);
    for (k, &i_sp) in i_spice.iter().enumerate().skip(1) {
        let v = net.step(dt);
        let i = v[2] / R;
        let err = (i - i_sp).abs() / i_ss;
        if err > worst {
            worst = err;
            worst_at = (k, i, i_sp);
        }
        checked += 1;
    }
    println!(
        "  最差点 k={} 我们={:.6e} A  ngspice={:.6e} A",
        worst_at.0, worst_at.1, worst_at.2
    );
    println!("\n=== 电感 RL 阶跃（vs ngspice，梯形）===");
    println!("  检查 {checked} 点，最大相对偏差 {:.2} %", worst * 100.0);
    assert!(checked > 100, "采样点太少");
    assert!(
        worst < 0.05,
        "与 ngspice 偏差 {:.2}% 超过 5%",
        worst * 100.0
    );
}

/// LC 谐振频率：`f0 = 1/(2π√(LC))`（用 `v(mid)` 穿过稳态值的时刻量周期）。
#[test]
fn inductor_lc_ring_frequency_matches_analytic() {
    const L: f64 = 0.1;
    const C: f64 = 1e-6;
    const RDC: f64 = 1e-3; // 1 mΩ：Q ≈ 1000，两个周期内几乎不衰减
    let f0 = 1.0 / (2.0 * std::f64::consts::PI * (L * C).sqrt());
    let dt = 1.0 / 192_000.0;

    // 节点：1 = in，2 = mid，支路 3
    let mut net = Netlist::<4>::new();
    net.inductor(1, 2, L, RDC)
        .capacitor(2, GND, C)
        .voltage_source(3, 1, GND, 0.0);
    net.solve_dc();
    net.set_voltage(0, 1.0);

    // 阶跃后 v(mid) 在 0..2 V 之间振铃，穿过 1 V（= 稳态值）的时刻间隔是半个周期
    let n = (5.0 / f0 / dt) as usize; // 5 个周期
    let mut crossings: Vec<f64> = Vec::new();
    let mut prev = net.step(dt)[2] - 1.0;
    for k in 2..=n {
        let v = net.step(dt);
        let cur = v[2] - 1.0;
        if prev * cur < 0.0 {
            // 线性插值求过零点
            let t0 = (k - 1) as f64 * dt;
            crossings.push(t0 + dt * prev.abs() / (prev.abs() + cur.abs()));
        }
        prev = cur;
    }
    assert!(crossings.len() >= 4, "过零点太少（{} 个）", crossings.len());
    let half_periods: Vec<f64> = crossings.windows(2).map(|w| w[1] - w[0]).collect();
    let measured = 1.0 / (2.0 * half_periods[0]);
    let err = (measured - f0).abs() / f0;
    println!("\n=== 电感 + 电容 振铃 ===");
    println!(
        "  f0（解析）={f0:.2} Hz，实测 {measured:.2} Hz（差 {:.2} %）",
        err * 100.0
    );
    println!("  后续半周期：{half_periods:?}");
    assert!(err < 0.01, "谐振频率偏差 {:.2}% 超过 1%", err * 100.0);
}

/// DC 时电感 = 绕线电阻；`reset()` 后靠 DC 解初始化 `i_prev`，**首拍不许跳**。
#[test]
fn inductor_dc_is_winding_resistance_and_reset_has_no_thump() {
    // 10 V → L(rdc=2 Ω) → R(8 Ω) → GND：DC 电流 1 A，L 上 2 V、out 8 V
    let mut net = Netlist::<4>::new();
    net.inductor(1, 2, 1.0, 2.0)
        .resistor(2, GND, 8.0)
        .voltage_source(3, 1, GND, 10.0);
    let v = net.solve_dc();
    assert!((v[2] - 8.0).abs() < 1e-6, "out = {:.6} V，应为 8 V", v[2]);
    assert!((v[1] - v[2] - 2.0).abs() < 1e-6, "L 上应降 2 V");
    assert!(net.last_dc_resid <= 1e-6, "DC 残差 {:e}", net.last_dc_resid);
    assert!(!net.last_dc_retried, "正常电路不该走重试通道");

    // DC 解应把电感电流提交成 1 A
    let i_dc = match &net.elems[0] {
        Element::Inductor { i_prev, .. } => *i_prev,
        other => panic!("第一个元件不是电感：{other:?}"),
    };
    assert!((i_dc - 1.0).abs() < 1e-6, "DC 电流 {i_dc:.6} A，应为 1 A");

    // 复位后重解 → 首拍必须还在稳态（否则插件加载/切回时会"砰"一声）
    net.reset();
    net.solve_dc();
    let v1 = net.step(1.0 / 192_000.0);
    assert!(
        (v1[2] - 8.0).abs() < 1e-3,
        "复位后首拍 out = {:.4} V，跳变了",
        v1[2]
    );
}

// ---------------------------------------------------------------------------
// 变压器（M2）：耦合电感矩阵。判据用**解析解**，不依赖 oracle：
//   · 匝比        |V2/V1| = k·√(L2/L1)（空载）
//   · 反射阻抗    Z_in = rdc1 + jωL1 + (ωM)² / (rdc2 + Z_L + jωL2)
//   · 极性        交换次级两个端子 ⇒ 输出反相
//   · 中心抽头    4 绕组（初/次级各两段），抽头分压 ≈ 0.5、总电感不变
//   · 固定端等价  绕组接 `fix_node` 与接理想电压源必须一致（P0 那类 bug 的复发点）
// ---------------------------------------------------------------------------
use rustomp_dsp::{inductance_matrix, MAX_WINDINGS};

/// 正弦驱动（`sources[0]`）：返回 `(node 的稳态 RMS, 源支路电流 RMS)`。
fn drive_sine<const N: usize>(
    net: &mut Netlist<N>,
    node: usize,
    amp: f64,
    f: f64,
    fs: f64,
    branch: usize,
) -> (f64, f64) {
    let dt = 1.0 / fs;
    let dph = std::f64::consts::TAU * f * dt;
    let mut ph: f64 = 0.0;
    for _ in 0..(0.3 * fs) as usize {
        net.set_voltage(0, amp * ph.sin());
        net.step(dt);
        ph += dph;
    }
    let n = (0.2 * fs) as usize;
    let (mut sv, mut si) = (0.0f64, 0.0f64);
    for _ in 0..n {
        net.set_voltage(0, amp * ph.sin());
        let v = net.step(dt);
        ph += dph;
        sv += v[node] * v[node];
        let i = net.source_current(branch);
        si += i * i;
    }
    ((sv / n as f64).sqrt(), (si / n as f64).sqrt())
}

/// 带符号的稳态增益：把输出投影到输入正弦上（RMS 会把极性吃掉）。
fn drive_sine_gain<const N: usize>(
    net: &mut Netlist<N>,
    node: usize,
    amp: f64,
    f: f64,
    fs: f64,
    branch: usize,
) -> f64 {
    let dt = 1.0 / fs;
    let dph = std::f64::consts::TAU * f * dt;
    let mut ph: f64 = 0.0;
    for _ in 0..(0.3 * fs) as usize {
        net.set_voltage(0, amp * ph.sin());
        net.step(dt);
        ph += dph;
    }
    let n = (0.2 * fs) as usize;
    let (mut xy, mut xx) = (0.0f64, 0.0f64);
    let _ = branch;
    for _ in 0..n {
        let x = amp * ph.sin();
        net.set_voltage(0, x);
        let v = net.step(dt);
        ph += dph;
        xy += v[node] * x;
        xx += x * x;
    }
    xy / xx // 投影系数 = 同相增益（负值 = 反相）
}

fn k_pair(a: usize, b: usize, k: f64, dst: &mut [[f64; MAX_WINDINGS]; MAX_WINDINGS]) {
    dst[a][b] = k;
    dst[b][a] = k;
}

/// 匝比与极性：空载次级电压 = `k·√(L2/L1)` × 初级电压；交换端子则反相。
#[test]
fn transformer_turns_ratio_and_polarity() {
    const F: f64 = 100.0;
    const FS: f64 = 192_000.0;
    const AMP: f64 = 1.0;
    let (l1, l2, k) = (1.0f64, 0.25f64, 0.999f64);
    let expect = k * (l2 / l1).sqrt(); // 0.4995

    for &swap in &[false, true] {
        // 节点：1 = 初级+，2 = 次级+；支路 3 = 源
        let mut net = Netlist::<4>::new();
        let mut kk = [[0.0; MAX_WINDINGS]; MAX_WINDINGS];
        k_pair(0, 1, k, &mut kk);
        let lmat = inductance_matrix(&[l1, l2, 0.0, 0.0], &kk, 2);
        let sec = if swap { (GND, 2) } else { (2, GND) };
        net.coupled_inductors(&[(1, GND, 2.0), (sec.0, sec.1, 0.5)], &lmat)
            .resistor(2, GND, 1e6) // 空载：只给一个高阻，避免次级被自身 rdc 短接
            .voltage_source(3, 1, GND, 0.0);
        net.solve_dc();
        let ratio = drive_sine_gain(&mut net, 2, AMP, F, FS, 3);
        let signed = if swap { -expect } else { expect };
        println!("\n=== 变压器匝比（swap={swap}）===\n  V2/V1 = {ratio:.5}（期望 {signed:.5}）");
        assert!(
            (ratio - signed).abs() < 0.01 * expect.abs(),
            "匝比 {ratio:.5} 偏离 {signed:.5}"
        );
    }
}

/// 反射阻抗：`Z_in = rdc1 + jωL1 + (ωM)² / (rdc2 + Z_L + jωL2)`。
#[test]
fn transformer_reflected_impedance() {
    const FS: f64 = 192_000.0;
    let (l1, l2, k) = (10.0f64, 2.5f64, 0.999f64);
    let (rdc1, rdc2, r_load) = (1.0f64, 1.0f64, 1e3f64);
    let f = 1_000.0f64;
    let w = std::f64::consts::TAU * f;
    let m = k * (l1 * l2).sqrt();

    // 解析：Z_in = rdc1 + jωL1 + (ωM)²/(rdc2 + r_load + jωL2)
    let z2 = (rdc2 + r_load, w * l2);
    let num = (w * m) * (w * m); // (ωM)² 是实数
    let den = z2.0 * z2.0 + z2.1 * z2.1;
    let add = (num * z2.0 / den, -num * z2.1 / den);
    let z_in = ((rdc1 + add.0).powi(2) + (w * l1 + add.1).powi(2)).sqrt();

    // 节点：1 = 初级+，2 = 次级+；支路 3 = 源
    let mut net = Netlist::<4>::new();
    let mut kk = [[0.0; MAX_WINDINGS]; MAX_WINDINGS];
    k_pair(0, 1, k, &mut kk);
    let lmat = inductance_matrix(&[l1, l2, 0.0, 0.0], &kk, 2);
    net.coupled_inductors(&[(1, GND, rdc1), (2, GND, rdc2)], &lmat)
        .resistor(2, GND, r_load)
        .voltage_source(3, 1, GND, 0.0);
    net.solve_dc();
    let (v1, i1) = drive_sine(&mut net, 1, 1.0, f, FS, 3);
    let z_meas = v1 / i1;
    let err = (z_meas - z_in).abs() / z_in;
    println!("\n=== 变压器反射阻抗（1 kHz，负载 1 kΩ）===");
    println!(
        "  |Z_in| 实测 {z_meas:.1} Ω，解析 {z_in:.1} Ω，差 {:.2} %",
        err * 100.0
    );
    println!(
        "  （折算到初级的负载 ≈ (L1/L2)·1 kΩ = {:.0} Ω）",
        l1 / l2 * r_load
    );
    assert!(err < 0.03, "|Z_in| 偏差 {:.2}% 超过 3%", err * 100.0);
    // 再确认它确实≈ 4×负载（匝比 2:1 的平方）
    assert!((z_meas - 4.0 * r_load).abs() / (4.0 * r_load) < 0.05);
}

/// 6 脚变压器（初/次级各带中心抽头 → 4 个绕组）：抽头分压 ≈ 0.5，匝比 2:1。
#[test]
fn transformer_center_tap_four_windings() {
    const F: f64 = 100.0;
    const FS: f64 = 192_000.0;
    // 初级总 1 H（两段各 0.25 H），次级总 0.25 H（两段各 0.0625 H）
    let seg_l = [0.25, 0.25, 0.0625, 0.0625];
    let mut kk = [[0.0; MAX_WINDINGS]; MAX_WINDINGS];
    k_pair(0, 1, 0.9999, &mut kk); // 同一绕组的两个半段：几乎完全耦合
    k_pair(2, 3, 0.9999, &mut kk);
    for i in 0..2 {
        for j in 2..4 {
            k_pair(i, j, 0.999, &mut kk); // 初 ↔ 次
        }
    }
    let lmat = inductance_matrix(&seg_l, &kk, 4);

    // 节点：1 = 初级上端，2 = 初级抽头；3 = 次级上端，4 = 次级抽头；支路 5 = 源
    let mut net = Netlist::<6>::new();
    net.coupled_inductors(
        &[
            (1, 2, 1.0),   // 初级上半
            (2, GND, 1.0), // 初级下半
            (3, 4, 0.5),   // 次级上半
            (4, GND, 0.5), // 次级下半
        ],
        &lmat,
    )
    .resistor(3, GND, 1e6) // 空载
    .voltage_source(5, 1, GND, 0.0);
    net.solve_dc();
    let (v1, _) = drive_sine(&mut net, 1, 1.0, F, FS, 5);
    let (v2, _) = drive_sine(&mut net, 2, 1.0, F, FS, 5);
    let (v3, _) = drive_sine(&mut net, 3, 1.0, F, FS, 5);
    let (v4, _) = drive_sine(&mut net, 4, 1.0, F, FS, 5);
    let tap_p = v2 / v1;
    let tap_s = v4 / v3;
    let ratio = v3 / v1;
    println!("\n=== 6 脚变压器（4 绕组，抽头 50%）===");
    println!("  抽头分压 初级 {tap_p:.4} / 次级 {tap_s:.4}（期望 0.5）");
    println!("  匝比 V(次级上)/V(初级上) = {ratio:.4}（期望 0.5）");
    assert!((tap_p - 0.5).abs() < 0.02, "初级抽头 {tap_p:.4} 偏离 0.5");
    assert!((tap_s - 0.5).abs() < 0.02, "次级抽头 {tap_s:.4} 偏离 0.5");
    assert!((ratio - 0.5).abs() < 0.02, "匝比 {ratio:.4} 偏离 0.5");
}

/// 固定端等价性（P0 类 bug 的复发点）：绕组端子接 `fix_node` vs 接理想电压源。
#[test]
fn transformer_fixed_rail_equals_ideal_source() {
    let (l1, l2, k) = (1.0f64, 0.25f64, 0.999f64);
    let mut kk = [[0.0; MAX_WINDINGS]; MAX_WINDINGS];
    k_pair(0, 1, k, &mut kk);
    let lmat = inductance_matrix(&[l1, l2, 0.0, 0.0], &kk, 2);
    let fs = 192_000.0f64;
    let dt = 1.0 / fs;

    // (a) 初级下端固定在 9 V；节点 1=初上，2=初下，3=次上；支路 5 = 源
    let mut fixed = Netlist::<6>::new();
    fixed
        .coupled_inductors(&[(1, 2, 2.0), (3, GND, 0.5)], &lmat)
        .resistor(3, GND, 1e3)
        .voltage_source(5, 1, GND, 0.0);
    fixed.fix_node(2, 9.0);
    fixed.solve_dc();

    // (b) 同一个 9 V 用理想电压源给（支路 4）
    let mut sourced = Netlist::<6>::new();
    sourced
        .coupled_inductors(&[(1, 2, 2.0), (3, GND, 0.5)], &lmat)
        .resistor(3, GND, 1e3)
        .voltage_source(5, 1, GND, 0.0)
        .voltage_source(4, 2, GND, 9.0);
    sourced.solve_dc();

    println!(
        "  DC: fixed v1={:.6} v2={:.6} v3={:.6} | sourced v1={:.6} v2={:.6} v3={:.6}",
        fixed.v[1], fixed.v[2], fixed.v[3], sourced.v[1], sourced.v[2], sourced.v[3]
    );
    let dph = std::f64::consts::TAU * 1_000.0 * dt;
    let mut ph: f64 = 0.0;
    let mut worst = 0.0f64;
    let mut first_bad: Option<(usize, f64, f64, f64, f64)> = None;
    let mut worst = 0.0f64;
    for k in 0..400 {
        let x = 0.2 * ph.sin();
        ph += dph;
        fixed.set_voltage(0, x);
        sourced.set_voltage(0, x);
        let a = fixed.step(dt);
        let b = sourced.step(dt);
        let mut d = 0.0f64;
        for node in [1usize, 2, 3] {
            d = d.max((a[node] - b[node]).abs());
        }
        worst = worst.max(d);
        if (k + 1) % 100 == 0 {
            println!(
                "    {:.1} ms: 本段最差 {:.3} mV",
                (k + 1) as f64 * dt * 1e3,
                d * 1e3
            );
        }
    }
    println!("\n=== 变压器：固定端 vs 理想源（400 拍）===");
    println!("  最大节点差 = {:.3} mV（有界，不随时间增长）", worst * 1e3);
    // 两种写法**代数等价**，但未知量集合不同（固定端被消去 vs 作为支路未知量），
    // 数值条件不同 —— 实测差在 1 mV 量级且**不随时间增长**（与 BJT 的同类测试一致）。
    // 真正要抓的是**结构性错误**：如果固定列被漏掉，端口电压会差 9 V，
    // 节点差会是伏特/安培级，而不是毫伏级。所以阈值取 2 mV。
    assert!(
        worst < 2e-3,
        "固定端与理想源差得太多（{:.3} mV）—— 像是固定列没进 RHS",
        worst * 1e3
    );
}

/// DC（绕组 = rdc）与复位后的首拍（`i_prev` 由 DC 解初始化，不许跳）。
#[test]
fn transformer_dc_and_reset() {
    // 10 V → R(1k) → 初级(rdc=2) → GND；次级空载
    let (l1, l2, k) = (1.0f64, 0.25f64, 0.999f64);
    let mut kk = [[0.0; MAX_WINDINGS]; MAX_WINDINGS];
    k_pair(0, 1, k, &mut kk);
    let lmat = inductance_matrix(&[l1, l2, 0.0, 0.0], &kk, 2);

    let mut net = Netlist::<5>::new();
    net.resistor(1, 2, 1e3)
        .coupled_inductors(&[(2, GND, 2.0), (3, GND, 0.5)], &lmat)
        .resistor(3, GND, 1e6)
        .voltage_source(4, 1, GND, 10.0);
    let v = net.solve_dc();
    // 初级电流 ≈ 10 V / (1000 + 2 + 1e6?) —— 次级空载但 DC 也是 rdc=0.5 Ω 的环路
    // DC 下两个绕组互不影响：初级回路 1000 + 2 = 1002 Ω → 9.98 mA，L 上 19.96 mV
    let i_primary = (10.0 - v[2]) / 1e3;
    println!("\n=== 变压器 DC ===");
    println!(
        "  V(初上) = {:.4} V，初级电流 = {:.3} mA",
        v[2],
        i_primary * 1e3
    );
    assert!((i_primary - 10.0 / 1002.0).abs() < 1e-5);
    assert!(net.last_dc_resid <= 1e-6, "DC 残差 {:e}", net.last_dc_resid);
    assert!(!net.last_dc_retried);

    net.reset();
    net.solve_dc();
    let v1 = net.step(1.0 / 192_000.0);
    assert!(
        (v1[2] - v[2]).abs() < 1e-4,
        "复位后首拍跳变：{:.4} V → {:.4} V",
        v[2],
        v1[2]
    );
}

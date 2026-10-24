// SPDX-License-Identifier: GPL-3.0-or-later

//! **Sparklin Drive 回归验证（MNA 路径）**。
//!
//! 同一条网表（`sparklin-drive-legacy/plugins/sparklin-drive/circuit/netlist/netlist.cir`）
//! 现在用 **通用 MNA 装配器** 重新搭一遍，与三份独立数据对拍：
//!
//! 1. **ngspice 基线**（`baseline.txt`，完整 OP07A 子电路）—— 见下方常量；
//! 2. **legacy WDF 实现**（`fakedist.rs`，一维牛顿 + 梯形伴随）—— 同样在常量里；
//! 3. 本仓库的器件模型（二极管/BJT/... 都已在 `tests/devices.rs` 对过 ngspice）。
//!
//! 通过标准：DC 工作点 < 1 mV、THD 与基线相差 < 1 个百分点、小信号近似线性。
//!
//! 电路（取自网表，节点号见 [`Net`]）：
//!
//! ```text
//!  VCC 9V ─ R2 47k ─┬─ R3 47k ─ GND        (Vbias = 4.5V)
//!                   └─ R4 470k ─ IN+ ─ C1 4.7n ─ IN
//!                                 │
//!        R5 4.7k ─┬─ IN− ─────── OP07 ──── OUT ─┬─ C3 1µ ─ P2 500k(LEVEL) ─ 出
//!                 │                            │
//!               C2 47n                        P1 250k (GAIN, 50%)
//!                 │                            │
//!                GND                           └── 削波环：D1(1 只) ∥ D2+D3(串，反)
//! ```

use rustomp_dsp::{Netlist, D_1N4148, GND, OP07};
use rustomp_oracle::{harmonics, harmonics_percent, thd_percent};

/// 引擎的工作采样率：48 kHz × 4 过采样
const FS: f64 = 192_000.0;
const DT: f64 = 1.0 / FS;

// ---- 基线常量（sparklin-drive-legacy/.../netlist/baseline.txt，ngspice-47）----
/// DC 工作点：OUT（= n_t1l = n_t11r）
const BASE_DC_OUT: f64 = 4.499_935;
/// DC 工作点：IN−（n_t10l）
const BASE_DC_INM: f64 = 4.499_724;
/// DC 工作点：偏置分压中点（n_t19r）
const BASE_DC_BIAS: f64 = 4.499_986;
/// THD @ 0.2 V / 1 kHz（ngspice）
const BASE_THD_200MV: f64 = 27.5682;
/// THD @ 1 mV / 1 kHz（ngspice）
const BASE_THD_1MV: f64 = 0.283_043;
/// 稳态 OUT 摆幅（2.98–3.00 s）：min / max
const BASE_OUT_MIN: f64 = 3.459_39;
const BASE_OUT_MAX: f64 = 5.120_9;

/// 节点编号（0 = 地）
mod net {
    pub const IN: usize = 1; // n_t21L 输入（C1 左侧）
    pub const INP: usize = 2; // n_t11L 运放同相输入
    pub const INM: usize = 3; // n_t10L 运放反相输入
    pub const R5: usize = 4; // n_t1R  R5/C2 中点
    pub const OUT: usize = 5; // n_t1L = n_t11R 运放输出
    pub const MID: usize = 6; // n_t5L  两只串联二极管的中点
    pub const VOL: usize = 7; // n_t24R C3 之后
    pub const WIPER: usize = 8; // n_t27R 音量抽头（= 输出）
    pub const BIAS: usize = 9; // n_t19R 4.5V 偏置
    pub const VCC: usize = 10; // n_rail_Lp/Rp

    // 支路未知量
    pub const BR_OPAMP: usize = 11;
    pub const BR_VCC: usize = 12;
    pub const BR_IN: usize = 13;
}

/// 元件值（Ω / F），与网表一一对应
struct Values {
    r2: f64,
    r3: f64,
    r4: f64,
    c1: f64,
    r5: f64,
    c2: f64,
    gain_pot: f64,
    c3: f64,
    level_pot: f64,
    vcc: f64,
}

impl Default for Values {
    fn default() -> Self {
        Self {
            r2: 47_000.0,
            r3: 47_000.0,
            r4: 470_000.0,
            c1: 4.7e-9,
            r5: 4_700.0,
            c2: 47e-9,
            gain_pot: 250_000.0, // 500k 电位器放在 50%
            c3: 1e-6,
            level_pot: 250_000.0, // 同上
            vcc: 9.0,
        }
    }
}

/// 搭电路。`net` 用 `Netlist<14>`：10 个节点 + 3 条电压源支路。
fn build(v: &Values, input: f64) -> Netlist<14> {
    let mut net = Netlist::<14>::new();
    let model = OP07.model(DT);

    // 偏置分压（保留完整节点，不用 Thevenin 等效 —— 要的是真装配）
    net.resistor(net::VCC, net::BIAS, v.r2)
        .resistor(net::BIAS, GND, v.r3)
        .resistor(net::INP, net::BIAS, v.r4)
        .capacitor(net::INP, net::IN, v.c1)
        // 反馈网络：OUT → P1 → IN−，以及 IN− → R5 → C2 → GND
        .resistor(net::INM, net::OUT, v.gain_pot)
        .resistor(net::INM, net::R5, v.r5)
        .capacitor(net::R5, GND, v.c2)
        // 削波环：D1 一只（OUT→IN−）；D2+D3 两只串联反向（IN−→MID→OUT）
        .diode(D_1N4148, net::OUT, net::INM)
        .diode(D_1N4148, net::INM, net::MID)
        .diode(D_1N4148, net::MID, net::OUT)
        // 输出隔直 + 音量电位器（抽头直接作为输出，无负载）
        .capacitor(net::OUT, net::VOL, v.c3)
        .resistor(net::VOL, net::WIPER, v.level_pot)
        .resistor(net::WIPER, GND, v.level_pot)
        // 运放：p=INP, m=INM, out=OUT，输出用一条支路未知量
        .opamp(model, net::INP, net::INM, net::OUT, net::BR_OPAMP)
        // 电源与输入源
        .voltage_source(net::BR_VCC, net::VCC, GND, v.vcc)
        .voltage_source(net::BR_IN, net::IN, GND, input);

    net
}

/// 跑 `seconds` 秒，返回最后 `tail` 秒的输出波形（音量抽头）。
fn run(v: &Values, amplitude: f64, freq: f64, seconds: f64, tail: f64) -> Vec<f64> {
    let mut net = build(v, 0.0);
    let n = (seconds / DT) as usize;
    let n_tail = (tail / DT) as usize;
    let mut out = Vec::with_capacity(n_tail);
    for i in 0..n {
        let t = i as f64 * DT;
        let x = amplitude * (std::f64::consts::TAU * freq * t).sin();
        net.set_voltage(1, x); // 第 1 个电压源 = 输入
        let node = net.step(DT);
        if i >= n - n_tail {
            out.push(node[net::WIPER]);
        }
    }
    out
}

/// 0.2 V / 1 kHz 跑满 3 s 的稳态输出，两个测试共用（`OnceLock` 保证只算一次）。
fn run_200mv_settled() -> &'static Vec<f64> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Vec<f64>> = OnceLock::new();
    CACHE.get_or_init(|| run(&Values::default(), 0.2, 1_000.0, 3.0, 0.02))
}

/// 直流工作点：与 ngspice 基线逐节点比较（应到 mV 以内）。
#[test]
fn dc_operating_point_matches_baseline() {
    let v = Values::default();
    let mut net = build(&v, 0.0);
    let node = net.solve_dc();
    let out = node[net::OUT];

    println!("\n=== Sparklin Drive（MNA）直流工作点 ===");
    for (name, ours, base) in [
        ("OUT  ", out, BASE_DC_OUT),
        ("IN−  ", node[net::INM], BASE_DC_INM),
        ("BIAS ", node[net::BIAS], BASE_DC_BIAS),
    ] {
        println!("  {name} 我们={ours:.6}  ngspice={base:.6}  差={:+.3} mV", (ours - base) * 1e3);
    }

    // 未削波时 DC 增益 = 1（C2 隔直）→ OUT ≈ IN+ ≈ 4.5 V
    assert!((out - BASE_DC_OUT).abs() < 1e-3, "OUT 偏差 {:.3} mV", (out - BASE_DC_OUT).abs() * 1e3);
    assert!(
        (node[net::INM] - BASE_DC_INM).abs() < 1e-3,
        "IN− 偏差 {:.3} mV",
        (node[net::INM] - BASE_DC_INM).abs() * 1e3
    );
    assert!(
        (node[net::BIAS] - BASE_DC_BIAS).abs() < 1e-3,
        "偏置中点偏差 {:.3} mV",
        (node[net::BIAS] - BASE_DC_BIAS).abs() * 1e3
    );
    // 输出隔直：音量抽头在直流上应为 0
    assert!(node[net::WIPER].abs() < 1e-3, "输出直流偏移 {:.3} mV", node[net::WIPER] * 1e3);
}

/// THD @ 0.2 V / 1 kHz：与 ngspice 基线（27.57%）和 legacy WDF（27.65%）比较。
///
/// 跑 3 s 才到稳态：C3(1 µF) × 电平电位器(500k) 的直流恢复时间常数约 0.5 s
/// （非对称削波会在 C3 上充出直流，这就是"阻塞失真"的来源）。
#[test]
fn thd_at_200mv_matches_baseline() {
    let out = run_200mv_settled();
    let thd = thd_percent(out, FS, 1_000.0, 9);
    let hp = harmonics_percent(&harmonics(out, FS, 1_000.0, 9));
    let (min, max) = out.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));

    println!("\n=== Sparklin Drive（MNA）THD @ 0.2 V / 1 kHz ===");
    println!("  THD = {thd:.3} %   （ngspice 基线 {BASE_THD_200MV:.3}%，legacy WDF 27.650%）");
    println!("  h2..h9 = {}", hp.iter().map(|x| format!("{x:.2}")).collect::<Vec<_>>().join(" / "));
    println!("  音量抽头摆幅 = {min:+.4} .. {max:+.4} V");

    assert!(
        (thd - BASE_THD_200MV).abs() < 1.0,
        "THD {thd:.3}% 与基线 {BASE_THD_200MV:.3}% 相差超过 1 个百分点"
    );
}

/// 1 mV 输入 ≈ 线性：THD 应与基线的 0.283% 同量级（证明失真来自二极管而非数值误差）。
#[test]
fn small_signal_thd_matches_baseline() {
    let v = Values::default();
    let out = run(&v, 1e-3, 1_000.0, 3.0, 0.02);
    let thd = thd_percent(&out, FS, 1_000.0, 9);
    // 峰值取"去均值后的幅度"：C3 还在充电时直流偏移会混进绝对值
    let mean = out.iter().sum::<f64>() / out.len() as f64;
    let amp = out.iter().fold(0.0f64, |a, &x| a.max((x - mean).abs()));
    println!("\n=== Sparklin Drive（MNA）小信号 @ 1 mV ===");
    println!("  THD = {thd:.4} %（基线 {BASE_THD_1MV:.4}%）  输出峰值 {:.4} V", amp);
    assert!(thd < 0.6, "小信号 THD {thd:.4}% 偏大（应 ≈0.283%）");
    // 基线在 1 kHz 是 +26.99 dB（= 22.4×，已含音量电位器的 1/2 分压）
    println!("  增益 = {:.1}×（基线 1 kHz = +26.99 dB = 22.4×）", amp / 1e-3);
    assert!((0.005..0.05).contains(&amp), "小信号输出幅度 {amp:.4} V 不合理");
}

/// 削波的不对称性：OUT 摆幅应以 4.5 V 为中心偏向负侧（1 只 vs 2 只二极管）。
#[test]
fn clipping_is_asymmetric() {
    let out = run_200mv_settled();
    let (min, max) = out.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    let mean = out.iter().sum::<f64>() / out.len() as f64;

    // 音量电位器把 OUT 的摆幅减半，所以与基线的 3.459/5.121 差一个 2 倍分压 + 隔直
    println!("\n=== 削波不对称性 ===");
    println!("  输出 min={min:+.4} max={max:+.4} mean={mean:+.4}");
    println!("  （基线 OUT 摆幅 {BASE_OUT_MIN:.3}..{BASE_OUT_MAX:.3}，经 C3 隔直 + 1/2 分压后到输出）");

    assert!(max > 0.2 && min < -0.2, "输出摆幅不足：{min:.3}..{max:.3}");
    // 以稳态均值为中心比较两侧摆幅：负向（单只二极管那半边）应该更大
    let (up, down) = (max - mean, mean - min);
    println!("  相对均值：上摆 {up:.4} V / 下摆 {down:.4} V");
    assert!(down > up * 1.02, "应偏向负侧（1 只二极管的半边先削波）：上 {up:.4} / 下 {down:.4}");
}

/// 频响：50 Hz ~ 10 kHz，与 `baseline.txt` 的 `.ac` 结果比较。
///
/// 这条测试同时校验三件事：输入高通（C1×R4，≈72 Hz）、反馈网络低频转折
/// （C2×R5，≈720 Hz）、以及**运放 GBW 造成的高频滚降**（OP07 的性格）。
#[test]
fn frequency_response_matches_baseline() {
    // (频率, 基线增益 dB @ 音量抽头)
    let points = [
        (50.0, 1.04),
        (100.0, 9.67),
        (500.0, 23.72),
        (1_000.0, 26.99),
        (2_000.0, 28.29),
        (5_000.0, 27.96),
        (10_000.0, 25.92),
    ];
    let v = Values::default();
    println!("\n=== Sparklin Drive（MNA）频响（小信号）===");
    let mut worst = 0.0f64;
    for &(f, base_db) in &points {
        // 低频要跑更久：C3 × 音量电位器的 0.5 s 直流恢复若没settle，
        // 残余的指数衰减会在 50 Hz 附近泄漏进基波 bin（实测能让 50 Hz 偏高 4 dB）
        let cycles: f64 = 20.0;
        let warmup: f64 = if f < 200.0 { 3.0 } else { 1.5 };
        let seconds: f64 = warmup.max(cycles / f);
        let out = run(&v, 1e-3, f, seconds, cycles / f);
        let mean = out.iter().sum::<f64>() / out.len() as f64;
        let fund = harmonics(&out, FS, f, 1)[0];
        let _ = mean;
        let db = 20.0 * (fund / 1e-3).log10();
        let err = (db - base_db).abs();
        worst = worst.max(err);
        println!("  {f:>7.0} Hz  我们={db:+6.2} dB  基线={base_db:+6.2} dB  差={:+.2} dB", db - base_db);
    }
    // 高频端受运放离散化影响（192 kHz 下 GBW 等效偏低），留 1.5 dB 余量
    assert!(worst < 1.0, "频响最大偏差 {worst:.2} dB 超过 1.0 dB");
}

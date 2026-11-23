// SPDX-License-Identifier: GPL-3.0-or-later

//! 电子三极管（Koren 模型）：**Rust 器件模型 ↔ ngspice 参考实现 ↔ 数据手册**三方对照。
//!
//! ngspice 侧的参考实现在 `devices/triode-koren.inc`（Koren `Tube.lib` / `Tube97.lib` 的
//! 逐字翻译，已验证过 12AX7/12AT7/12AY7/12AU7/6SN7 的静态工作点）。
//!
//! ```bash
//! cargo test -p rustomp-dsp --release --test triode -- --nocapture
//! ```

use rustomp_dsp::{Netlist, Triode, TriodeLaw, GND, T12AT7, T12AU7, T12AX7, T12AY7, T6SN7};
use rustomp_oracle::Oracle;

const LIB: &str = include_str!("../../../devices/triode-koren.inc");

/// 型号 → (ngspice 本体, 参数行)
fn oracle_spec(model: &Triode) -> (&'static str, String) {
    let body = match model.law {
        TriodeLaw::V96 => "TRIODE",
        TriodeLaw::V97 => "TRIODE97",
    };
    let p = format!(
        "MU={} EX={} KG1={} KP={} KVB={} VCT={}",
        model.mu, model.ex, model.kg1, model.kp, model.kvb, model.vct
    );
    (body, p)
}

/// ngspice 里的屏流（串一个 0 V 源读电流）。
fn spice_plate(oracle: &Oracle, model: &Triode, va: f64, vg: f64) -> f64 {
    let (body, params) = oracle_spec(model);
    let net = format!(
        "{LIB}\nVP p 0 {va}\nVG g 0 {vg}\nVA p p1 0\nX1 p1 g 0 {body} {params}\n.op\n.end\n"
    );
    let raw = oracle.run_netlist(&net).expect("ngspice 运行失败");
    // i(VA) 的符号：正电流从 p 流向 p1（= 流入管子），这里取绝对值
    raw.get("i(va)").map(|v| v[0].abs()).unwrap_or_else(|| {
        panic!("没有 i(va)，有 {:?}", raw.vars);
    })
}

fn models() -> Vec<(&'static str, Triode)> {
    vec![
        ("12AX7", T12AX7),
        ("12AT7", T12AT7),
        ("12AU7", T12AU7),
        ("12AY7", T12AY7),
        ("6SN7", T6SN7),
    ]
}

/// 静态曲线：Rust 与 ngspice 在多个工作点逐点比（相对误差 < 1%）。
#[test]
fn plate_curve_matches_ngspice() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    println!("\n=== 三极管静态曲线（Rust vs ngspice）===");
    let mut worst = 0.0f64;
    for (name, m) in models() {
        // 每个型号选 3 个有代表性的工作点（含截止附近）
        let points: &[(f64, f64)] = if name == "12AU7" {
            &[(250.0, -8.5), (250.0, -4.0), (100.0, -4.0)]
        } else if name == "6SN7" {
            &[(250.0, -8.0), (250.0, -4.0), (100.0, -4.0)]
        } else {
            &[(250.0, -2.0), (250.0, -1.0), (100.0, -0.5)]
        };
        for &(va, vg) in points {
            let ours = m.plate(va, vg).ip;
            let spice = spice_plate(&oracle, &m, va, vg);
            let denom = spice.abs().max(1e-9);
            let rel = (ours - spice).abs() / denom;
            worst = worst.max(rel);
            println!(
                "  {name:6} Va={va:5.0} Vg={vg:5.1}  Rust={:9.3} mA  ngspice={:9.3} mA  差 {:.3}%",
                ours * 1e3,
                spice * 1e3,
                rel * 100.0
            );
        }
    }
    assert!(worst < 0.01, "最大相对误差 {:.2}% 超过 1%", worst * 100.0);
}

/// 小信号参数：`mu = gm·Rp` 必须贴近型号的 `MU`，并与手册对得上。
#[test]
fn small_signal_matches_datasheet() {
    println!("\n=== 三极管小信号（Va=250）===");
    // (型号, Vg, 手册: Ia mA / gm mA/V / Rp kΩ)
    let cases: [(&str, Triode, f64, f64, f64, f64); 4] = [
        ("12AX7", T12AX7, -2.0, 1.2, 1.6, 62.5),
        ("12AT7", T12AT7, -2.0, 10.0, 5.5, 15.0),
        ("12AU7", T12AU7, -8.5, 10.5, 2.2, 7.7),
        ("6SN7", T6SN7, -8.0, 9.0, 2.6, 7.7),
    ];
    for (name, m, vg, ia_ds, gm_ds, rp_ds) in cases {
        let e = m.plate(250.0, vg);
        let rp = 1.0 / e.gp;
        let mu = e.gm * rp;
        println!(
            "  {name:6} Ia={:6.2} mA（手册 {ia_ds:5.1}） gm={:5.2}（{gm_ds:4.1}） Rp={:5.1}k（{rp_ds:4.1}） mu={:6.1}",
            e.ip * 1e3,
            e.gm * 1e3,
            rp / 1e3,
            mu
        );
        // Koren 模型里 MU 是"渐近放大系数"，工作点的 gm·Rp 会略低（12AX7 实测 ~90）。
        // 只要求两者同量级；手册 mu 在打印里对照。
        assert!(
            (mu - m.mu).abs() / m.mu < 0.25,
            "{name}: gm·Rp = {mu:.1} 偏离 MU = {:.1} 太多",
            m.mu
        );
        // 工作点与手册同量级（Koren 模型是曲线拟合，允许 40%）
        assert!(
            (e.ip * 1e3 - ia_ds).abs() / ia_ds < 0.4,
            "{name}: Ia {:.2} mA 与手册 {ia_ds} 差太多",
            e.ip * 1e3
        );
    }
}

/// 共阴放大级：12AX7，Ra=100k / Rk=1.5k / VHT=300（经典数值：Vp≈196 V，增益≈60）。
#[test]
fn common_cathode_stage() {
    const FS: f64 = 192_000.0; // 4× 过采样的内层速率
                               // 节点 1..6，支路排在最后（8/9）—— 支路号与节点号不能撞，否则会互相覆盖
    let mut net = Netlist::<10>::new();
    // 节点：1=ht, 2=p, 3=g, 4=k, 5=in, 6=栅极 stopper；支路 8=HT 源, 9=输入源
    net.resistor(1, 2, 100_000.0)
        .resistor(4, GND, 1_500.0)
        .capacitor(4, GND, 22e-6)
        .capacitor(5, 6, 22e-9)
        .resistor(6, 3, 100_000.0) // 栅极 stopper：米勒电容要靠它才显出低通
        .resistor(3, GND, 1_000_000.0)
        .voltage_source(8, 1, GND, 300.0)
        .voltage_source(9, 5, GND, 0.0)
        .triode(T12AX7, 2, 3, 4);
    let v = net.solve_dc();
    println!(
        "\n=== 12AX7 共阴级（Ra=100k / Rk=1.5k / VHT=300）===\n  Vp={:.2} V  Vk={:.3} V  Ia={:.3} mA",
        v[2],
        v[4],
        (300.0 - v[2]) / 100_000.0 * 1e3
    );
    assert!(
        (v[2] - 196.0).abs() < 25.0,
        "Vp={:.1} V 偏离经典值 ~196 V",
        v[2]
    );
    assert!((v[4] - 1.55).abs() < 0.4, "Vk={:.3} V 偏离 ~1.55 V", v[4]);

    // 1 kHz 小信号增益（输入经 22n 耦合到栅极，源在 5）
    let vp_dc = v[2];
    let gain = |f: f64| {
        const AMP: f64 = 0.02;
        let mut n = net.clone();
        let dt = 1.0 / FS;
        let dph = std::f64::consts::TAU * f / FS;
        let mut ph: f64 = 0.0;
        for _ in 0..(0.05 * FS) as usize {
            n.set_voltage(1, AMP * ph.sin());
            n.step_fixed(dt, 4);
            ph += dph;
        }
        let (mut acc, mut cnt) = (0.0f64, 0usize);
        for _ in 0..(0.02 * FS) as usize {
            n.set_voltage(1, AMP * ph.sin());
            let out = n.step_fixed(dt, 4)[2] - vp_dc;
            ph += dph;
            acc += out * out;
            cnt += 1;
        }
        (acc / cnt as f64).sqrt() / (AMP / 2f64.sqrt())
    };
    let g1k = gain(1_000.0);
    let g20k = gain(20_000.0);
    println!(
        "  增益 @1kHz = {g1k:.1}×（理论 gm·(Ra∥Rp) ≈ 60）  @20kHz = {g20k:.1}×（Cgp 米勒 + 100k 栅阻）"
    );
    assert!((g1k - 60.0).abs() < 12.0, "1 kHz 增益 {g1k:.1} 偏离 ~60");
    // 米勒电容 ≈ Cgp·(1+A) ≈ 2.4p×61 ≈ 146 pF，配 100 k 栅阻 → 转角 ≈11 kHz
    assert!(
        g20k < g1k * 0.85,
        "20 kHz 应因 Cgp 的米勒效应明显下降（{g20k:.1} vs {g1k:.1}）"
    );
}

/// 栅流只在**正栅**时出现（负栅 → 0），且量级合理。
#[test]
fn grid_current_only_when_positive() {
    let (i_neg, _) = T12AX7.grid(-1.0);
    let (i_zero, _) = T12AX7.grid(0.0);
    let (i_pos, g_pos) = T12AX7.grid(1.0);
    println!("\n=== 栅流 ===\n  Vg=-1 → {i_neg:.3e} A   Vg=0 → {i_zero:.3e} A   Vg=+1 → {i_pos:.3e} A（g={g_pos:.3e} S）");
    assert_eq!(i_neg, 0.0);
    assert_eq!(i_zero, 0.0);
    assert!(
        i_pos > 1e-4 && i_pos < 5e-3,
        "Vg=+1 时栅流 {i_pos:.3e} A 不合理"
    );
}

/// DC 解后复位，首拍不应跳（电容历史由 DC 解初始化）。
#[test]
fn dc_and_reset_have_no_thump() {
    let mut net = Netlist::<6>::new();
    net.resistor(1, 2, 100_000.0)
        .resistor(4, GND, 1_500.0)
        .voltage_source(5, 1, GND, 300.0)
        .triode(T12AX7, 2, 3, 4)
        .resistor(3, GND, 1_000_000.0);
    let v = net.solve_dc();
    net.reset();
    net.solve_dc();
    let v1 = net.step(1.0 / 192_000.0);
    assert!(
        (v1[2] - v[2]).abs() < 1e-3,
        "复位后首拍 Vp {:.3} → {:.3} 跳变了",
        v[2],
        v1[2]
    );
}

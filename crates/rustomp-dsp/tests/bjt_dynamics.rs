// SPDX-License-Identifier: GPL-3.0-or-later

//! **BJT 动态档（C1 结电容 + C2 渡越时间）的守卫**。
//!
//! 判据取自 `docs/component-library.md` §5（"用数据决定精度"）：
//! 与 ngspice 的同一型号同偏置下 特征频率 `ft` 相差 < 20%。
//!
//! ngspice 参考值（本机实测，`.ac` 扫 h21 到 0 dB）：
//!
//! | 型号 | 偏置 | ngspice `ft` |
//! |---|---|---|
//! | BC549C | `Vbe=0.664 V`（Ic≈1.16 mA）、`Vce=5 V` | **119.7 MHz** |
//! | AC127 | `Vbe=0.15 V`（Ic≈1.8 mA）、`Vce=8.2 V` | **0.25 MHz**（锗管确实很慢） |
//!
//! 另外守住"扩散电荷的开关"：锗管的 `Tf=600 ns` ⇔ 扩散电容 ≈41 nF，
//! 当前求解器（3 次迭代预算、无结电压限制）稳不住，所以默认只上 C1（见 DEVLOG E82）。

use rustomp_dsp::devices::{Bjt, AC127, AC128, BC549C, BC559C};
use rustomp_oracle::Oracle;

const LIB: &str = include_str!("../../../devices/devices.lib");

/// 用 ngspice 量 `ft`：共射、集电极经 0 V 源到电源，基极 AC 1 V，
/// `h21 = |i(vce)/i(vbe)|` 穿越 1 的频率（对数插值）。
fn ngspice_ft(oracle: &Oracle, model: &str, vbe: f64, vcc: f64) -> f64 {
    let netlist = format!(
        "{LIB}\nVCC vcc 0 DC {vcc}\nVCE c vcc DC 0\nVBE b 0 DC {vbe} AC 1\n\
         Q1 c b 0 {model}\n.ac dec 20 1k 2G\n.end\n"
    );
    let raw = oracle.run_netlist(&netlist).expect("ngspice 运行失败");
    let f = raw.get("frequency").expect("rawfile 里没有 frequency");
    let ic = raw.get_complex("i(vce)").expect("没有 i(vce) 的复数结果");
    let ib = raw.get_complex("i(vbe)").expect("没有 i(vbe) 的复数结果");
    assert_eq!(f.len(), ic.len());
    let h21: Vec<f64> = ic
        .iter()
        .zip(ib)
        .map(|((icr, ici), (ibr, ibi))| {
            let mag_c = icr.hypot(*ici);
            let mag_b = ibr.hypot(*ibi);
            mag_c / mag_b
        })
        .collect();
    for i in 1..h21.len() {
        if h21[i - 1] >= 1.0 && h21[i] < 1.0 {
            // log–log 线性插值
            let (f1, f2) = (f[i - 1], f[i]);
            let (m1, m2) = (h21[i - 1].log10(), h21[i].log10());
            let t = m1 / (m1 - m2);
            return 10f64.powf(f1.log10() + t * (f2.log10() - f1.log10()));
        }
    }
    panic!("h21 没有穿越 1（可能扫频范围不够）：{h21:?}");
}

/// `ft = gm / (2π(Cbe + Cbc))`（小信号，单位 Hz）
fn ft(b: &Bjt, vbe: f64, vbc: f64) -> f64 {
    let (gm, cbe, cbc) = b.small_signal(vbe, vbc);
    gm / (2.0 * std::f64::consts::PI * (cbe + cbc))
}

/// **真对拍**：有 ngspice 才算（CI 里装了 ngspice 就会跑）。
#[test]
fn ft_matches_ngspice_live() {
    let Some(oracle) = Oracle::discover_or_skip() else {
        return;
    };
    // BC549C：Vbe=0.664 V（Ic≈1.16 mA）、Vce=5 V
    let want = ngspice_ft(&oracle, "BC549C", 0.664, 5.0);
    let ours = ft(&BC549C, 0.664, 0.664 - 5.0);
    println!(
        "BC549C ft：我们 {:.1} MHz / ngspice {:.1} MHz（差 {:+.1}%）",
        ours / 1e6,
        want / 1e6,
        (ours / want - 1.0) * 100.0
    );
    assert!(
        (ours / want - 1.0).abs() < 0.15,
        "BC549C 的 ft 与 ngspice 差 {:+.1}%",
        (ours / want - 1.0) * 100.0
    );

    // AC127：Vbe=0.15 V（Ic≈1.8 mA）、Vce=8.2 V；模型的扩散电荷默认关，这里临时打开
    let mut ge = AC127;
    ge.diffusion = true;
    let want = ngspice_ft(&oracle, "AC127", 0.15, 8.2);
    let ours = ft(&ge, 0.15, -5.0);
    println!(
        "AC127  ft：我们 {:.3} MHz / ngspice {:.3} MHz（差 {:+.1}%）",
        ours / 1e6,
        want / 1e6,
        (ours / want - 1.0) * 100.0
    );
    assert!(
        (ours / want - 1.0).abs() < 0.15,
        "AC127 的 ft 与 ngspice 差 {:+.1}%",
        (ours / want - 1.0) * 100.0
    );
}

#[test]
fn ft_of_bc549c_matches_ngspice() {
    let f = ft(&BC549C, 0.664, 0.664 - 5.0);
    println!("BC549C ft = {:.1} MHz（ngspice 119.7 MHz）", f / 1e6);
    assert!(
        (95e6..=145e6).contains(&f),
        "BC549C 的 ft={:.1} MHz 与 ngspice 的 119.7 MHz 差得太多",
        f / 1e6
    );
}

#[test]
fn ft_of_ac127_matches_ngspice_when_diffusion_on() {
    // 扩散电荷默认关（见文件头）；这里临时打开，只为验证 Tf 的量级对得上
    let mut b = AC127;
    b.diffusion = true;
    let f = ft(&b, 0.15, -5.0);
    println!(
        "AC127 ft（Tf 打开）= {:.3} MHz（ngspice 0.25 MHz）",
        f / 1e6
    );
    assert!(
        (0.15e6..=0.40e6).contains(&f),
        "AC127 的 ft={:.3} MHz 与 ngspice 的 0.25 MHz 差得太多",
        f / 1e6
    );
}

#[test]
fn diffusion_flag_controls_transit_charge() {
    // 开关语义：`diffusion = false` = 只上 C1（结电容），`true` = 外加 C2（Tf/Tr 渡越电荷）。
    // 锗管的 `Tf=600 ns` 在 mA 级就有 ~40 nF 的扩散电容，开关效果非常明显。
    let (_, cbe_off, _) = {
        let mut b = AC127;
        b.diffusion = false;
        b.small_signal(0.15, -5.0)
    };
    let (_, cbe_on, _) = AC127.small_signal(0.15, -5.0);
    println!(
        "AC127 Cbe：C1 only = {:.1} pF，C1+C2 = {:.1} nF",
        cbe_off * 1e12,
        cbe_on * 1e9
    );
    assert!(cbe_off < 1e-9, "关掉扩散电荷后 Cbe 应只有结电容（pF 级）");
    assert!(
        cbe_on > 20.0 * cbe_off,
        "打开扩散电荷后 Cbe 应显著变大（Tf·gm）"
    );
}

#[test]
fn junction_charge_starts_at_zero_and_is_monotone() {
    for b in [BC549C, AC127] {
        assert_eq!(b.charges(0.0, 0.0).0, 0.0);
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=60 {
            let v = -2.0 + 0.1 * k as f64;
            let (qbe, _) = b.charges(v, 0.0);
            assert!(qbe.is_finite(), "vbe={v}: Qbe={qbe:e}");
            assert!(
                qbe > prev,
                "vbe={v}: Qbe={qbe:e} 应单调增（上一步 {prev:e}）"
            );
            prev = qbe;
        }
    }
}

#[test]
fn junction_capacitance_is_continuous_at_fc() {
    // 电荷公式在 v = Fc·Vj 处分段：左右差分得到的电容必须一致（C¹ 连续）
    let (b, vj) = (BC549C, BC549C.vje);
    let vc = BC549C.fc * vj;
    let h = 1e-9;
    let dq = |v: f64| (b.charges(v, 0.0).0 - b.charges(v - h, 0.0).0) / h;
    let left = dq(vc - 1e-6);
    let right = dq(vc + 1e-6);
    println!(
        "Cje @Fc·Vj: 左 {:.2} pF / 右 {:.2} pF",
        left * 1e12,
        right * 1e12
    );
    assert!(
        (left - right).abs() / right < 0.01,
        "Fc·Vj 处电容不连续：{left:e} vs {right:e}"
    );
}

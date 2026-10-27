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

use rustomp_dsp::devices::{Bjt, AC127, BC549C};

/// `ft = gm / (2π(Cbe + Cbc))`（小信号，单位 Hz）
fn ft(b: &Bjt, vbe: f64, vbc: f64) -> f64 {
    let (gm, cbe, cbc) = b.small_signal(vbe, vbc);
    gm / (2.0 * std::f64::consts::PI * (cbe + cbc))
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
fn germanium_diffusion_is_gated_off() {
    // 默认（C1 only）：Cbe 应该就是结电容量级（几十 pF），不是 41 nF
    let (_, cbe, cbc) = AC127.small_signal(0.15, -5.0);
    println!(
        "AC127（C1 only）Cbe={:.1} pF Cbc={:.1} pF",
        cbe * 1e12,
        cbc * 1e12
    );
    assert!(cbe < 1e-9, "扩散电荷没被关掉？Cbe={:.3} nF", cbe * 1e9);
    assert!(
        cbe > 10e-12 && cbc > 10e-12,
        "结电容不见了：{cbe:e} {cbc:e}"
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

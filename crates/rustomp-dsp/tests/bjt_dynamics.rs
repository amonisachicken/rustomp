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

/// 新热路径（`eval`：一次求值 + 解析雅可比）必须与旧参考实现（`terminal`/`charges`）一致。
#[test]
fn eval_matches_reference_terminal_and_charges() {
    // 电荷是在**本征结**上的：要和 `charges(端电压)` 对比，必须先让端=本征（rb/re/rc=0）。
    // 电流部分不受影响（`terminal` 自带内层牛顿）。
    let mut ge = AC127;
    ge.diffusion = true;
    let mut no_r = |mut b: Bjt| -> Bjt {
        b.rb = 0.0;
        b.re = 0.0;
        b.rc = 0.0;
        b
    };
    let with_r = [BC549C, BC559C, ge, AC128];
    let no_r_models = [no_r(BC549C), no_r(BC559C), no_r(ge), no_r(AC128)];
    for (b, b0) in with_r.iter().zip(no_r_models.iter()) {
        let c = b.consts();
        let mut worst_i: f64 = 0.0;
        let mut worst_q: f64 = 0.0;
        for &vbe in &[-0.3, 0.0, 0.05, 0.12, 0.2, 0.3, 0.45, 0.62] {
            for &vbc in &[-5.0, -1.0, -0.2, 0.0, 0.2, 0.5] {
                let ev = b.eval(&c, vbe, vbc, true);
                let (ic, ib) = b.terminal(vbe, vbc);
                let rel = |a: f64, r: f64| (a - r).abs() / (1e-12 + r.abs());
                worst_i = worst_i.max(rel(ev.ic, ic)).max(rel(ev.ib, ib));
                assert!(
                    rel(ev.ic, ic) < 1e-8 && rel(ev.ib, ib) < 1e-8,
                    "电流不一致：vbe={vbe} vbc={vbc} → ic {:e} vs {:e}、ib {:e} vs {:e}",
                    ev.ic,
                    ic,
                    ev.ib,
                    ib
                );
                // 电荷：端=本征的模型（rb/re/rc 已清零）才有可直接对比的参考
                let c0 = b0.consts();
                let ev0 = b0.eval(&c0, vbe, vbc, true);
                let (qbe, qbc) = b0.charges(vbe, vbc);
                worst_q = worst_q.max(rel(ev0.qbe, qbe)).max(rel(ev0.qbc, qbc));
                assert!(
                    rel(ev0.qbe, qbe) < 1e-8 && rel(ev0.qbc, qbc) < 1e-8,
                    "q 不一致：vbe={vbe} vbc={vbc} → {:e}/{:e} vs {:e}/{:e}",
                    ev0.qbe,
                    ev0.qbc,
                    qbe,
                    qbc
                );
            }
        }
        println!("eval vs 参考：最大电流差 {worst_i:.2e}、电荷差 {worst_q:.2e}");
    }
}

/// 解析雅可比必须等于数值差分（否则牛顿会走错方向）。
#[test]
fn analytic_jacobian_matches_fd() {
    let mut ge = AC127;
    ge.diffusion = true;
    for b in [BC549C, BC559C, ge, AC128] {
        let c = b.consts();
        let h = 1e-6;
        for &vbe in &[0.0, 0.1, 0.2, 0.35, 0.55] {
            for &vbc in &[-5.0, -0.5, 0.0, 0.3] {
                let ev = b.eval(&c, vbe, vbc, true);
                let (ic_p, ib_p) = b.terminal(vbe + h, vbc);
                let (ic_m, ib_m) = b.terminal(vbe - h, vbc);
                let fd_dic = (ic_p - ic_m) / (2.0 * h);
                let fd_dib = (ib_p - ib_m) / (2.0 * h);
                let (ic_p, ib_p) = b.terminal(vbe, vbc + h);
                let (ic_m, ib_m) = b.terminal(vbe, vbc - h);
                let fd_dic_bc = (ic_p - ic_m) / (2.0 * h);
                let fd_dib_bc = (ib_p - ib_m) / (2.0 * h);
                let close = |a: f64, n: f64| (a - n).abs() <= 1e-4 * (1.0 + n.abs());
                assert!(
                    close(ev.dic_dvbe, fd_dic)
                        && close(ev.dic_dvbc, fd_dic_bc)
                        && close(ev.dib_dvbe, fd_dib)
                        && close(ev.dib_dvbc, fd_dib_bc),
                    "雅可比不一致 @ vbe={vbe} vbc={vbc}：\
                     dic/dvbe 解析 {:e} / 差分 {:e}，dic/dvbc {:e}/{:e}，\
                     dib/dvbe {:e}/{:e}，dib/dvbc {:e}/{:e}",
                    ev.dic_dvbe,
                    fd_dic,
                    ev.dic_dvbc,
                    fd_dic_bc,
                    ev.dib_dvbe,
                    fd_dib,
                    ev.dib_dvbc,
                    fd_dib_bc
                );
            }
        }
    }
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

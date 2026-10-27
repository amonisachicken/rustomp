// SPDX-License-Identifier: GPL-3.0-or-later

//! BJT：Gummel-Poon 静态模型（静态 **D3**，见 `docs/component-library.md`）。
//!
//! 包含推进过程中的关键因素：
//!
//! - 正向/反向传输电流（`Is, Nf, Nr`）
//! - 基极电流分量 `Ise/Ne`、`Isc/Nc`（**锗管的漏电就靠这两项**）
//! - 电流增益 `Bf/Br` 与 Early 效应 `Vaf/Var`
//! - 大电流 β 塌陷 `Ikf/Ikr`
//! - 体电阻 `Rb/Re/Rc`（在**端**电压与**本征**结电压之间做一次 2 维牛顿）
//!
//! 未包含（按精度表属动态档 C1/C2）：`Cje/Cjc/Tf/Tr`，以及 `IRB/RBM`（电流相关基区电阻）。
//!
//! 约定：所有电流方向为**流入端**（NPN 正向工作时 `Ic>0, Ib>0, Ie<0`）。

use super::diode::VT;

/// Gummel-Poon 静态参数（缺省值取 SPICE 默认）。
#[derive(Debug, Clone, Copy)]
pub struct Bjt {
    pub is: f64,
    pub bf: f64,
    pub nf: f64,
    pub vaf: f64,
    pub ikf: f64,
    pub ise: f64,
    pub ne: f64,
    pub br: f64,
    pub nr: f64,
    pub var: f64,
    pub ikr: f64,
    pub isc: f64,
    pub nc: f64,
    pub rb: f64,
    pub re: f64,
    pub rc: f64,
}

impl Default for Bjt {
    fn default() -> Self {
        Self {
            is: 1e-16,
            bf: 100.0,
            nf: 1.0,
            vaf: f64::INFINITY,
            ikf: f64::INFINITY,
            ise: 0.0,
            ne: 1.5,
            br: 1.0,
            nr: 1.0,
            var: f64::INFINITY,
            ikr: f64::INFINITY,
            isc: 0.0,
            nc: 2.0,
            rb: 0.0,
            re: 0.0,
            rc: 0.0,
        }
    }
}

/// 结电压的指数函数：**超过 `vcrit` 后按切线线性延续**（值、一阶导都连续）。
///
/// 为什么不直接把 `exp` 钉个上限：硬截断会把**导数钉成 0**，牛顿的雅可比
/// 立刻变成奇异矩阵（迭代原地打转，最后返回一个离谱的电流）。
///
/// 为什么必须是 `vcrit` 而不是一个固定上限：`vcrit = vt·ln(vt/(√2·is))` 是
/// "电流开始不小时"的结电压（与 SPICE 同一定义）。硅管 `is≈1e-15` ⇒ vcrit≈0.74 V，
/// 正常工作电压全在指数区，模型**逐位不变**；钽管 `is=5µ` ⇒ vcrit≈0.21 V，
/// 牛顿稍一过冲就进了线性区 —— 而线性区的斜率（≈0.7 S）是有限的，能收敛。
#[inline(always)]
fn exp_lim(v: f64, vt: f64, isat: f64) -> f64 {
    if !(isat > 0.0) {
        return (v / vt).exp();
    }
    let vcrit = vt * (vt / (std::f64::consts::SQRT_2 * isat)).ln();
    if v <= vcrit {
        (v / vt).exp()
    } else {
        // ec = exp(vcrit/vt) = vt/(√2·isat)；线性段斜率 = ec/vt（连续可导）
        let ec = vcrit / vt;
        let ec = ec.exp();
        ec * (1.0 + (v - vcrit) / vt)
    }
}

impl Bjt {
    /// **本征**器件：给定结电压 (vbe, vbc)，返回 (Ic, Ib)。
    ///
    /// 不含 `Rb/Re/Rc`。公式与 SPICE3 的 Gummel-Poon 一致（不含 `Nk`）。
    #[inline]
    pub fn intrinsic(&self, vbe: f64, vbc: f64) -> (f64, f64) {
        let vte = self.nf * VT;
        let vtc = self.nr * VT;
        let e_be = exp_lim(vbe, vte, self.is);
        let e_bc = exp_lim(vbc, vtc, self.is);

        // 基区电荷因子 qb（Early + 大电流塌陷）
        let c1 = 1.0 - vbc / self.vaf - vbe / self.var;
        let q1 = if c1 > 1e-12 { 1.0 / c1 } else { 1e12 };

        let mut q2 = 0.0;
        if self.ikf.is_finite() {
            q2 += self.is / self.ikf * (e_be - 1.0);
        }
        if self.ikr.is_finite() {
            q2 += self.is / self.ikr * (e_bc - 1.0);
        }
        let qb = 0.5 * q1 * (1.0 + (1.0 + 4.0 * q2).max(0.0).sqrt());

        // 传输电流与两个结电流
        let ict = self.is / qb * (e_be - e_bc);
        let ibe = self.is / self.bf * (e_be - 1.0)
            + self.ise * (exp_lim(vbe, self.ne * VT, self.ise) - 1.0);
        let ibc = self.is / self.br * (e_bc - 1.0)
            + self.isc * (exp_lim(vbc, self.nc * VT, self.isc) - 1.0);

        (ict - ibc, ibe + ibc)
    }

    /// **端**器件：给定端电压 (vbe, vbc)，返回 (Ic, Ib)，内部把 `Rb/Re/Rc` 的压降算进去。
    ///
    /// 解 `f(vbe_i, vbc_i) = 0`，其中
    /// `vbe_t = vbe_i + Ib·Rb − Ie·Re`、`vbc_t = vbc_i + Ib·Rb − Ic·Rc`。
    /// 雅可比用**对残差本身**的中心差分（最不容易写错的形式）。
    pub fn terminal(&self, vbe: f64, vbc: f64) -> (f64, f64) {
        if self.rb == 0.0 && self.re == 0.0 && self.rc == 0.0 {
            return self.intrinsic(vbe, vbc);
        }
        let residual = |vb_i: f64, vc_i: f64| -> (f64, f64) {
            let (ic, ib) = self.intrinsic(vb_i, vc_i);
            let ie = -(ic + ib);
            (
                vb_i + ib * self.rb - ie * self.re - vbe,
                vc_i + ib * self.rb - ic * self.rc - vbc,
            )
        };

        let (mut vb_i, mut vc_i) = (vbe, vbc);
        let h = 1e-7;
        for _ in 0..60 {
            let (f1, f2) = residual(vb_i, vc_i);
            if f1.abs() < 1e-12 && f2.abs() < 1e-12 {
                break;
            }
            let (f1_b, f2_b) = residual(vb_i + h, vc_i);
            let (f1_c, f2_c) = residual(vb_i, vc_i + h);
            let j11 = (f1_b - f1) / h;
            let j21 = (f2_b - f2) / h;
            let j12 = (f1_c - f1) / h;
            let j22 = (f2_c - f2) / h;
            let det = j11 * j22 - j12 * j21;
            if det.abs() < 1e-30 {
                break;
            }
            let d1 = (f1 * j22 - f2 * j12) / det;
            let d2 = (f1 * j21 - f2 * j11) / -det;
            // 阻尼：避免大信号下步子过大
            let (d1, d2) = (d1.clamp(-0.2, 0.2), d2.clamp(-0.2, 0.2));
            vb_i -= d1;
            vc_i -= d2;
            if d1.abs() < 1e-12 && d2.abs() < 1e-12 {
                break;
            }
        }
        self.intrinsic(vb_i, vc_i)
    }
}

// ---------------------------------------------------------------- 库内型号（闭集）

/// BC549C（硅 NPN 低噪声）：来自 breadspice 的 `models.lib`。
pub const BC549C: Bjt = Bjt {
    is: 7.049e-15,
    bf: 493.2,
    nf: 1.0,
    vaf: 23.89,
    ikf: 0.1542,
    ise: 99.2e-15,
    ne: 1.829,
    br: 2.886,
    nr: 1.0,
    var: f64::INFINITY,
    ikr: 5.426,
    isc: 7.371e-12,
    nc: 1.508,
    rb: 0.0,
    re: 0.0,
    rc: 1.175,
};

/// BC559C（硅 **PNP**，BC549C 的互补管）：`Is=1.02f Bf=401.6 Vaf=34.62 Rc=1.151`
///
/// PNP 接法：把 `(veb, vcb)` 传进 [`Bjt::terminal`]，返回电流取负。
pub const BC559C: Bjt = Bjt {
    is: 1.02e-15,
    bf: 401.6,
    nf: 1.0,
    vaf: 34.62,
    ikf: 74.73e-3,
    ise: 38.26e-12,
    ne: 5.635,
    br: 9.011,
    nr: 1.0,
    var: f64::INFINITY,
    ikr: 0.1469,
    isc: 1.517e-15,
    nc: 1.831,
    rb: 0.0,
    re: 0.0,
    rc: 1.151,
};

/// AC127（锗 NPN）：大漏电（`Is=5µ`、`Isc=5µ`）+ 低 `Vf` + 大 `Rb=100Ω`。
/// 这些正是"锗味"的来源（见 `docs/component-library.md` §3）。
pub const AC127: Bjt = Bjt {
    is: 5e-6,
    bf: 160.0,
    nf: 1.0,
    vaf: 50.0,
    ikf: 0.3,
    ise: 600e-9,
    ne: 2.0,
    br: 5.0,
    nr: 1.0,
    var: 50.0,
    ikr: f64::INFINITY,
    isc: 5e-6,
    nc: 2.0,
    rb: 100.0,
    re: 0.0,
    rc: 1.5,
};

/// AC128（锗 PNP）。PNP 在与 `AC127` 相同的公式下工作 —— 使用时把端电压取反即可
/// （`Ic/Ib` 的方向按"流入端"约定，见模块文档）。
pub const AC128: Bjt = Bjt {
    is: 5e-6,
    bf: 90.0,
    nf: 1.0,
    vaf: 40.0,
    ikf: 3.0,
    ise: 200e-9,
    ne: 1.5,
    br: 5.0,
    nr: 1.0,
    var: 40.0,
    ikr: f64::INFINITY,
    isc: 1e-6,
    nc: 2.0,
    rb: 7.0,
    re: 0.1,
    rc: 0.2,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silicon_and_germanium_turn_on_differently() {
        // 同一集电极电流下，锗管所需 Vbe 明显更低
        let v_si = solve_vbe(&BC549C, 1e-3);
        let v_ge = solve_vbe(&AC127, 1e-3);
        assert!(
            v_ge < v_si - 0.15,
            "Ge Vbe={v_ge:.3} 应显著低于 Si Vbe={v_si:.3}"
        );
    }

    #[test]
    fn germanium_is_leakier() {
        // 反向偏置下的漏电：锗管 Is 大 9 个数量级
        let (ic_si, _) = BC549C.intrinsic(-0.1, 0.0);
        let (ic_ge, _) = AC127.intrinsic(-0.1, 0.0);
        assert!(ic_ge.abs() > 1e-6, "Ge 反偏漏电 {ic_ge:e} 应 > 1µA");
        assert!(ic_si.abs() < 1e-12, "Si 反偏漏电 {ic_si:e} 应 < 1pA");
    }

    #[test]
    fn beta_near_bf_in_mid_range() {
        // 中电流区，hFE 应接近 Bf（硅管）
        let vbe = solve_vbe(&BC549C, 1e-3);
        let (ic, ib) = BC549C.terminal(vbe, vbe - 5.0);
        let hfe = ic / ib;
        assert!(
            (hfe / BC549C.bf - 1.0).abs() < 0.6,
            "hFE={hfe:.1}, Bf={}",
            BC549C.bf
        );
    }

    /// **大信号下不能爆**：`exp` 必须有界且**导数不为零**。
    ///
    /// 这条守的是一个真实事故（DEVLOG E81）：`exp` 曾用 `min(80)` 硬截断 →
    /// 导数被钉成 0 → `terminal()` 的内层牛顿拿不到方向 → 返回 1e17 A 级的电流 →
    /// 锗管法兹（AC127）的直流工作点直接跑到 −1.6e34 V。
    /// 现在按 `vcrit` 做切线延续（与 SPICE 同源的思路）。
    #[test]
    fn large_signal_stays_finite_and_monotone() {
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=50 {
            let vbe = -0.5 + 0.1 * k as f64; // −0.5 … 4.5 V
            let (ic, ib) = AC127.terminal(vbe, vbe - 5.0); // Vce = 5 V
            assert!(
                ic.is_finite() && ib.is_finite(),
                "vbe={vbe}: ic={ic:e} ib={ib:e}"
            );
            assert!(
                ic.abs() < 10.0,
                "vbe={vbe}: ic={ic:e} 明显不合理（应 ≤ 几安）"
            );
            assert!(ib.abs() < 1.0, "vbe={vbe}: ib={ib:e} 明显不合理");
            assert!(
                ic > prev,
                "vbe={vbe}: ic={ic:e} 应随 Vbe 单调不减（上一步 {prev:e}）"
            );
            prev = ic;
        }
        // 参考量级：AC127（Is=5µ）在 Vbe=0.2V 时约 2 mA
        let (ic, _) = AC127.terminal(0.2, 0.2 - 5.0);
        assert!(
            (1e-3..1e-2).contains(&ic),
            "Vbe=0.2V 时 ic={ic:e} 应在 mA 量级"
        );
    }

    /// 饱和区（B-C 结正偏）也要有界：Ge 管的 `Isc=5µA` 在深饱和时能反向灌电流，
    /// 但量级必须是"毫安"，不能是天文数字。
    #[test]
    fn saturation_region_is_bounded() {
        for k in 0..=10 {
            let vbc = -0.5 + 0.1 * k as f64;
            let (ic, ib) = AC127.terminal(0.2, vbc);
            assert!(
                ic.is_finite() && ib.is_finite(),
                "vbc={vbc}: ic={ic:e} ib={ib:e}"
            );
            assert!(
                ic.abs() < 0.1 && ib.abs() < 0.05,
                "vbc={vbc}: ic={ic:e} ib={ib:e}"
            );
        }
    }

    /// 给定 Vce=5V、集电极电流目标，二分求 Vbe（测试用）
    fn solve_vbe(b: &Bjt, target_ic: f64) -> f64 {
        let (mut lo, mut hi) = (-0.5_f64, 1.0_f64);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            let (ic, _) = b.terminal(mid, mid - 5.0);
            if ic < target_ic {
                lo = mid
            } else {
                hi = mid
            }
        }
        0.5 * (lo + hi)
    }
}

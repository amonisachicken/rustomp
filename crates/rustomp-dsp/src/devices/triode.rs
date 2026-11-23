// SPDX-License-Identifier: GPL-3.0-or-later

//! 电子三极管：**Koren 模型**（两种本体，见 [`TriodeLaw`]）。
//!
//! 参数与本体都来自 Norman Koren 的官方模型库（`Tube.lib` 1996 / `Tube97.lib` 1997-12）：
//!
//! ```text
//! V96（Tube.lib）：
//!   E1 = (Va/KP)·log( 1 + exp( KP·(1/MU + (Vg + VCT)/sqrt(KVB + Va²)) ) )
//!   Ia = 2·E1^EX / KG1                       （E1 > 0，否则 0）
//! V97（Tube97.lib，仅 12AX7 / 12AU7 / 6SN7 用）：
//!   分母换成 sqrt(KVB² + Va²)，电流换成 2·(E1/KG1)^EX
//! ```
//!
//! 两个本体**不能混用参数**：6SN7 的官方参数（KG1=232.6 / KVB=7.3）配 V96 会给出
//! 53 mA（正确值 9 mA），必须配 V97。
//!
//! 栅极：负栅不导通；正栅时用"二极管 + 串联 `RGI`"（与 SPICE 模型里的 `D3 + RGI` 同一套），
//! 内部小牛顿解出结电压 —— 过载时的栅流（格子阻塞失真的来源）由此产生。
//!
//! 极间电容 `CCG / CGP / CCP` 由 MNA 侧按伴生模型装配（`CGP` 的米勒效应是共阴级高频
//! 滚降的主因，不能省）。

use super::diode::VT;

/// 两种 Koren 本体（参数必须与本体配套）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriodeLaw {
    /// Koren 1996（`Tube.lib`）：分母 `sqrt(KVB + Va²)`，`Ia = 2·E1^EX/KG1`
    V96,
    /// Koren 12/97（`Tube97.lib`）：分母 `sqrt(KVB² + Va²)`，`Ia = 2·(E1/KG1)^EX`
    V97,
}

/// 三极管模型（Koren 参数 + 栅流 + 极间电容）。
#[derive(Debug, Clone, Copy)]
pub struct Triode {
    pub mu: f64,
    pub ex: f64,
    pub kg1: f64,
    pub kp: f64,
    pub kvb: f64,
    /// 栅偏置偏移（Koren 1997-12 引入；V96 参数通常为 0）
    pub vct: f64,
    pub law: TriodeLaw,
    /// 栅极等效二极管：反向饱和电流（A）
    pub ig_is: f64,
    /// 栅极等效二极管：发射系数
    pub ig_n: f64,
    /// 栅极串联电阻（Ω），对应 SPICE 模型里的 `RGI`
    pub ig_rs: f64,
    /// 阴极-栅极电容（F）
    pub ccg: f64,
    /// 栅极-屏极电容（F，米勒效应的来源）
    pub cgp: f64,
    /// 阴极-屏极电容（F）
    pub ccp: f64,
}

/// 屏流求值结果。
#[derive(Debug, Clone, Copy, Default)]
pub struct PlateEval {
    pub ip: f64,
    /// ∂Ip/∂Vg（跨导）
    pub gm: f64,
    /// ∂Ip/∂Va（输出电导，`1/Rp`）
    pub gp: f64,
}

impl Triode {
    /// 屏流与小信号参数：`vpk = V(屏) − V(阴)`，`vgk = V(栅) − V(阴)`。
    ///
    /// 公式与 ngspice 侧 `devices/triode-koren.inc` 逐字一致（那边是验证过的参考实现）。
    pub fn plate(&self, vpk: f64, vgk: f64) -> PlateEval {
        // 分母：V96 用 sqrt(KVB + Va²)，V97 用 sqrt(KVB² + Va²)
        let inv_kvb_sq = match self.law {
            TriodeLaw::V96 => self.kvb + vpk * vpk,
            TriodeLaw::V97 => self.kvb * self.kvb + vpk * vpk,
        };
        let s = inv_kvb_sq.max(1e-12).sqrt();
        let s3 = s * s * s;
        let x = 1.0 / self.mu + (vgk + self.vct) / s;
        let u = self.kp * x;
        // L = log(1+exp(u))，用稳定的 softplus：u 大时 L≈u
        let (logistic, l) = if u > 30.0 {
            (1.0, u)
        } else {
            let e = u.exp();
            (e / (1.0 + e), (1.0 + e).ln())
        };
        let e1 = (vpk / self.kp) * l;
        if e1 <= 0.0 {
            return PlateEval::default();
        }

        // dE1/dVa、dE1/dVg（解析）
        let du_dva = -self.kp * (vgk + self.vct) * vpk / s3;
        let du_dvg = self.kp / s;
        let de1_dva = l / self.kp + (vpk / self.kp) * logistic * du_dva;
        let de1_dvg = (vpk / self.kp) * logistic * du_dvg;

        // Ia 与 dIa/dE1
        let (ip, dip_de1) = match self.law {
            TriodeLaw::V96 => (
                2.0 * e1.powf(self.ex) / self.kg1,
                2.0 * self.ex * e1.powf(self.ex - 1.0) / self.kg1,
            ),
            TriodeLaw::V97 => {
                let r = e1 / self.kg1;
                (
                    2.0 * r.powf(self.ex),
                    2.0 * self.ex * r.powf(self.ex - 1.0) / self.kg1,
                )
            }
        };
        PlateEval {
            ip,
            gm: dip_de1 * de1_dvg,
            gp: dip_de1 * de1_dva,
        }
    }

    /// 栅极电流与电导（`vgk <= 0` 时不导通）。
    ///
    /// 与 SPICE 模型的 `RGI + D3` 等价：解 `vgk = vj + RGI·I(vj)`。
    /// 用**二分**而不是牛顿：`RGI=2 kΩ` + `Is=1 nA` 时方程极硬，牛顿从 `vj=1 V`
    /// 起步会把指数打到 1e7 A 量级、20 次迭代也拉不回来（实测栅流偏大 1000 倍）。
    /// 二分只在正栅时发生（正常音频没有），代价可以忽略。
    pub fn grid(&self, vgk: f64) -> (f64, f64) {
        if vgk <= 0.0 || self.ig_rs <= 0.0 || self.ig_is <= 0.0 {
            return (0.0, 0.0);
        }
        let n_vt = self.ig_n * VT;
        let junction = |vj: f64| {
            let x = (vj / n_vt).min(60.0);
            let e = x.exp();
            (self.ig_is * (e - 1.0), self.ig_is * e / n_vt)
        };
        // f(vj) = vj + Rs·I(vj) − vgk 单调递增；f(0) = −vgk < 0，f(vgk) > 0
        let (mut lo, mut hi) = (0.0f64, vgk);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            let (i, _) = junction(mid);
            if mid + self.ig_rs * i < vgk {
                lo = mid;
            } else {
                hi = mid;
            }
            if hi - lo < 1e-9 * (1.0 + hi.abs()) {
                break;
            }
        }
        let vj = 0.5 * (lo + hi);
        let (i, g) = junction(vj);
        (i.max(0.0), g / (1.0 + self.ig_rs * g))
    }

    /// 该型号是否带极间电容（测试/诊断用）。
    pub fn has_caps(&self) -> bool {
        self.ccg > 0.0 || self.cgp > 0.0 || self.ccp > 0.0
    }
}

// ---------------------------------------------------------------- 库内型号（闭集）

/// 12AX7（高 μ 双三极管）：Koren `Tube.lib`（1996）。
///
/// Va=250 V / Vg=−2 V → 0.95 mA，gm 1.67 mA/V，Rp 53.7 kΩ（手册 1.2 mA / 1.6 / 62.5 k）
pub const T12AX7: Triode = Triode {
    mu: 100.0,
    ex: 1.4,
    kg1: 1060.0,
    kp: 600.0,
    kvb: 300.0,
    vct: 0.0,
    law: TriodeLaw::V96,
    ig_is: 1e-9,
    ig_n: 1.0,
    ig_rs: 2000.0,
    ccg: 2.3e-12,
    cgp: 2.4e-12,
    ccp: 0.9e-12,
};

/// 12AT7（中 μ、高跨导）：Koren `Tube.lib`（1996）。
///
/// Va=250 / Vg=−2 → 12.8 mA，gm 7.2，Rp 7.9 kΩ（手册 10 mA / 5.5 / 15 k）
pub const T12AT7: Triode = Triode {
    mu: 60.0,
    ex: 1.35,
    kg1: 460.0,
    kp: 300.0,
    kvb: 300.0,
    vct: 0.0,
    law: TriodeLaw::V96,
    ig_is: 1e-9,
    ig_n: 1.0,
    ig_rs: 2000.0,
    ccg: 2.3e-12,
    cgp: 2.2e-12,
    ccp: 1.0e-12,
};

/// 12AU7（低 μ 双三极管）：Koren `Tube.lib`（1996）。
///
/// Va=250 / Vg=−8.5 → 10.4 mA，gm 2.48，Rp 7.3 kΩ（手册 10.5 mA / 2.2 / 7.7 k）
pub const T12AU7: Triode = Triode {
    mu: 21.5,
    ex: 1.3,
    kg1: 1180.0,
    kp: 84.0,
    kvb: 300.0,
    vct: 0.0,
    law: TriodeLaw::V96,
    ig_is: 1e-9,
    ig_n: 1.0,
    ig_rs: 2000.0,
    ccg: 2.3e-12,
    cgp: 2.2e-12,
    ccp: 1.0e-12,
};

/// 12AY7（低 μ、低噪）：**社区拟合值**（Koren 官方库没有这只）。
///
/// 公式与本文件的 V96 一致（多一个 `VCT=0.5` 的栅偏置偏移），来源见
/// `rustomp/devices/triode-koren.inc` 的文件头与 DEVLOG。用它时建议拿 [`T6SN7`]
/// （同为中 μ、官方参数）做参照：两者在 Va=250 的屏流应在同一量级。
pub const T12AY7: Triode = Triode {
    mu: 44.16,
    ex: 1.113,
    kg1: 1192.4,
    kp: 409.96,
    kvb: 300.0,
    vct: 0.5,
    law: TriodeLaw::V96,
    ig_is: 1e-9,
    ig_n: 1.0,
    ig_rs: 2000.0,
    ccg: 2.0e-12,
    cgp: 2.0e-12,
    ccp: 1.0e-12,
};

/// 6SN7（中 μ 双三极管）：Koren `Tube97.lib`（1997-12，**V97 本体**）。
///
/// Va=250 / Vg=−8 → 8.9 mA，gm ≈2.6，Rp ≈7.7 kΩ（手册 9 mA / 2.6 / 7.7 k）。
/// 12AY7 的参照型号（官方参数、可追溯）。
pub const T6SN7: Triode = Triode {
    mu: 21.17,
    ex: 1.33,
    kg1: 232.6,
    kp: 156.08,
    kvb: 7.3,
    vct: 0.0,
    law: TriodeLaw::V97,
    ig_is: 1e-9,
    ig_n: 1.0,
    ig_rs: 2000.0,
    ccg: 2.6e-12,
    cgp: 2.6e-12,
    ccp: 0.7e-12,
};

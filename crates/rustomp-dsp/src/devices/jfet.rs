// SPDX-License-Identifier: GPL-3.0-or-later

//! JFET：Shichman-Hodges 静态模型（静态 **D3**）。
//!
//! 包含：夹断电压 `Vto`、跨导参数 `Beta`、沟道长度调制 `Lambda`、体电阻 `Rd/Rs`。
//! 未包含（属 C1/C2 动态档）：`Cgs/Cgd`、栅结二极管（`Is/N`）—— 音频电路中栅流通常 <nA。
//!
//! 约定：N 沟道。`vgs = vg − vs`、`vds = vd − vs`，`Id` 为**流入漏极**的电流。
//! P 沟道使用时把各电压取反（与 BJT 的 PNP 处理一致）。

/// JFET 静态参数。
#[derive(Debug, Clone, Copy)]
pub struct Jfet {
    /// 夹断电压（N 沟道通常为负）
    pub vto: f64,
    /// 跨导参数（饱和区 `Id = Beta·(Vgs−Vto)²`）
    pub beta: f64,
    /// 沟道长度调制系数
    pub lambda: f64,
    /// 漏极体电阻
    pub rd: f64,
    /// 源极体电阻
    pub rs: f64,
}

impl Default for Jfet {
    fn default() -> Self {
        Self { vto: -0.6, beta: 1e-3, lambda: 0.0, rd: 0.0, rs: 0.0 }
    }
}

impl Jfet {
    /// **本征**沟道电流（不含 `Rd/Rs`），`vgs/vds` 为本征结电压。
    #[inline]
    pub fn channel(&self, vgs: f64, vds: f64) -> f64 {
        // 反向工作（vds < 0）按对称性交换漏源
        if vds < 0.0 {
            return -self.channel(vgs - vds, -vds);
        }
        let vov = vgs - self.vto;
        if vov <= 0.0 {
            return 0.0;
        }
        let id = if vov <= vds {
            // 饱和区
            self.beta * vov * vov * (1.0 + self.lambda * vds)
        } else {
            // 三极管区
            self.beta * vds * (2.0 * vov - vds) * (1.0 + self.lambda * vds)
        };
        id.max(0.0)
    }

    /// **端**电流：给定端电压 (vgs, vds)，把 `Rs/Rd` 的压降算进去。
    ///
    /// 体电阻为一维反馈（`Id` 已知即可算压降），所以只需**标量**牛顿，比 BJT 简单得多。
    #[inline]
    pub fn drain_current(&self, vgs: f64, vds: f64) -> f64 {
        let rsum = self.rd + self.rs;
        if rsum == 0.0 && self.rs == 0.0 {
            return self.channel(vgs, vds);
        }
        // 初值：直接按端电压估一个
        let mut id = self.channel(vgs, vds);
        for _ in 0..40 {
            let vds_i = vds - id * rsum;
            let vgs_i = vgs - id * self.rs;
            let f = self.channel(vgs_i, vds_i) - id;
            // 数值导数（对 id），步长随量级缩放
            let h = 1e-12_f64.max(id.abs() * 1e-6);
            let f2 = self.channel(vgs - (id + h) * self.rs, vds - (id + h) * rsum) - (id + h);
            let df = (f2 - f) / h;
            if df.abs() < 1e-15 {
                break;
            }
            let step = f / df;
            id -= step;
            if step.abs() < 1e-15 * (1.0 + id.abs()) {
                break;
            }
        }
        id
    }

    /// 由本征参数构造（`rd/rs` 常为 0）。
    pub fn new(vto: f64, beta: f64, lambda: f64, rd: f64, rs: f64) -> Self {
        Self { vto, beta, lambda, rd, rs }
    }
}

// ---------------------------------------------------------------- 库内型号（闭集）

/// J201（N 沟道 JFET，低噪声高增益，踏板常用）。
/// `Beta=1.621m Vto=-0.6 Lambda=2.236m Rd=1 Rs=1`
pub const J201: Jfet = Jfet {
    vto: -0.6,
    beta: 1.621e-3,
    lambda: 2.236e-3,
    rd: 1.0,
    rs: 1.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idss_matches_datasheet_range() {
        // Vgs=0 时的饱和电流（Idss）：J201 典型 0.2~1 mA
        let idss = J201.channel(0.0, 5.0);
        assert!((0.2e-3..1.2e-3).contains(&idss), "Idss={idss:e} 超出 J201 范围");
    }

    #[test]
    fn pinches_off_below_vto() {
        assert_eq!(J201.channel(-0.7, 5.0), 0.0);
        assert!(J201.channel(-0.5, 5.0) > 0.0);
    }

    #[test]
    fn triode_and_saturation_are_continuous() {
        // 在 Vds = Vgs-Vto 处两段公式应连续
        let vov = 0.4;
        let vgs = J201.vto + vov;
        let a = J201.channel(vgs, vov - 1e-6);
        let b = J201.channel(vgs, vov + 1e-6);
        assert!((a - b).abs() / b < 1e-4, "三极管/饱和区不连续：{a:e} vs {b:e}");
    }

    #[test]
    fn series_resistance_reduces_current() {
        let ideal = Jfet { rd: 0.0, rs: 0.0, ..J201 };
        let with_r = J201.drain_current(0.0, 0.5); // 三极管区，电阻影响明显
        let without = ideal.drain_current(0.0, 0.5);
        assert!(with_r < without, "串联电阻应降低电流：{with_r:e} vs {without:e}");
    }
}

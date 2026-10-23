// SPDX-License-Identifier: GPL-3.0-or-later

//! MOSFET：`Shichman-Hodges`（level 1）静态模型（静态 **D3**）。
//!
//! 只建模**沟道**；体二极管（源→漏）由电路把 [`crate::devices::Diode`] 单独接上 ——
//! 与 SPICE 侧手写的 `N2N7000` 子电路结构一致（体二极管 + 沟道 + Rd/Rs）。
//!
//! 约定：N 沟道；`vgs = vg − vs`、`vds = vd − vs`，`Id` 为**流入漏极**的电流。
//! P 沟道把电压取反（同 BJT/JFET 的处理）。
//!
//! 精度档：静态 D3、动态 C1（结电容 `Cgs/Cgd` 待补）。
//! 2N7000 的参数已用 ngspice 手写子电路标定（见 `devices/handwritten.lib` 与 DEVLOG E35）。

/// N 沟道 MOSFET 静态参数。
#[derive(Debug, Clone, Copy)]
pub struct Nmos {
    /// 阈值电压
    pub vto: f64,
    /// 跨导参数（`KP`，饱和区 `Id = KP/2·(Vgs−Vto)²`）
    pub kp: f64,
    /// 沟道长度调制
    pub lambda: f64,
    /// 漏极体电阻
    pub rd: f64,
    /// 源极体电阻
    pub rs: f64,
}

impl Default for Nmos {
    fn default() -> Self {
        Self { vto: 1.6, kp: 0.035, lambda: 0.02, rd: 2.0, rs: 1.0 }
    }
}

impl Nmos {
    /// **本征**沟道电流（不含 `Rd/Rs`）。
    #[inline]
    pub fn channel(&self, vgs: f64, vds: f64) -> f64 {
        if vds < 0.0 {
            // 反向：交换漏源（MOSFET 结构对称）
            return -self.channel(vgs - vds, -vds);
        }
        let vov = vgs - self.vto;
        if vov <= 0.0 {
            return 0.0;
        }
        let id = if vov <= vds {
            // 饱和区
            0.5 * self.kp * vov * vov * (1.0 + self.lambda * vds)
        } else {
            // 三极管区
            self.kp * (vov * vds - 0.5 * vds * vds) * (1.0 + self.lambda * vds)
        };
        id.max(0.0)
    }

    /// **端**电流：给定端电压，把 `Rd/Rs` 压降算进去（标量牛顿，同 JFET）。
    #[inline]
    pub fn drain_current(&self, vgs: f64, vds: f64) -> f64 {
        let rsum = self.rd + self.rs;
        if rsum == 0.0 {
            return self.channel(vgs, vds);
        }
        let mut id = self.channel(vgs, vds);
        for _ in 0..40 {
            let f = self.channel(vgs - id * self.rs, vds - id * rsum) - id;
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

    /// 导通电阻（深三极管区近似，`Vgs` 足够大时）
    pub fn rds_on(&self, vgs: f64) -> f64 {
        let vov = vgs - self.vto;
        if vov <= 0.0 {
            return f64::INFINITY;
        }
        (1.0 / (self.kp * vov)).max(0.0) + self.rd + self.rs
    }
}

// ---------------------------------------------------------------- 库内型号（闭集）

/// 2N7000（N 沟道增强型）：参数由 ngspice 手写子电路标定（DEVLOG E35）。
///
/// 标定目标：`Vgs=5V, Vds=5V` 时 `Id ≈ 0.15~0.25 A`；`Vgs=10V` 时 `Rds(on)` 约 3~6 Ω。
pub const NMOS_2N7000: Nmos = Nmos {
    vto: 1.6,
    kp: 0.035,
    lambda: 0.02,
    rd: 2.0,
    rs: 1.0,
};

/// 2N7000 的体二极管（源→漏，即 bulk-drain 结）。
///
/// 串联电阻取 `Rd + Rs = 3 Ω` —— 体二极管电流和沟道电流走同一条串阻路径。
pub const NMOS_2N7000_BODY: crate::devices::Diode =
    crate::devices::Diode::from_spice(1e-12, 1.5, 3.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_calibration_points() {
        // 与 devices/handwritten.lib 的 N2N7000 子电路标定值对比（Vds=5V）
        let id5 = NMOS_2N7000.drain_current(5.0, 5.0);
        assert!((0.15..0.30).contains(&id5), "Id@Vgs=5V = {id5:.4} A 不在 0.15~0.30");
        let id3 = NMOS_2N7000.drain_current(3.0, 5.0);
        assert!((0.025..0.055).contains(&id3), "Id@Vgs=3V = {id3:.4} A");
    }

    #[test]
    fn rds_on_in_datasheet_range() {
        // Vgs=10V 时 Rds(on) 应在几 Ω 量级
        let r = NMOS_2N7000.rds_on(10.0);
        assert!((2.0..8.0).contains(&r), "Rds(on)={r:.2} Ω");
    }

    #[test]
    fn off_below_threshold() {
        assert_eq!(NMOS_2N7000.channel(1.5, 5.0), 0.0);
        assert!(NMOS_2N7000.channel(1.7, 5.0) > 0.0);
    }

    #[test]
    fn triode_saturation_continuous() {
        let vov = 3.4;
        let a = NMOS_2N7000.channel(NMOS_2N7000.vto + vov, vov - 1e-6);
        let b = NMOS_2N7000.channel(NMOS_2N7000.vto + vov, vov + 1e-6);
        assert!((a - b).abs() / b < 1e-4, "三极管/饱和区不连续");
    }
}

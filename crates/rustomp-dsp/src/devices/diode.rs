// SPDX-License-Identifier: GPL-3.0-or-later

//! 二极管：`Is / N / Rs`（静态 **D2**，见 `docs/component-library.md`）。
//!
//! `Rs` 是串联体电阻，用内层牛顿解出结电压（`v = vj + Rs·I(vj)`）。
//! 结电容（`Cjo/M/tt`）暂不建模：实测在 1 kHz 削波场景下影响 <0.06 pp（DEVLOG E26）。

/// 通用二极管模型。
#[derive(Debug, Clone, Copy)]
pub struct Diode {
    /// 反向饱和电流 `Is`
    pub is: f64,
    /// 发射系数 × 热电压（`N·Vt`）
    pub n_vt: f64,
    /// 串联体电阻 `Rs`
    pub rs: f64,
    /// `1/(N·Vt)`，预计算（热路径只做乘法）
    inv_n_vt: f64,
}

/// 热电压 @ **27 °C（300.15 K）** —— 与 SPICE 的默认参考温度 `TNOM` 对齐。
///
/// 用 300 K 会带来约 **−0.05%** 的系统性偏差（实测见 `tests/devices.rs`），
/// 统一到 27 °C 后与 ngspice 基本重合。
pub const VT: f64 = 0.025_865;

impl Diode {
    /// 用 SPICE 参数构造（`n_ideal` 是发射系数，内部乘上 `VT`）。
    pub const fn from_spice(is: f64, n_ideal: f64, rs: f64) -> Self {
        let n_vt = n_ideal * VT;
        Self { is, n_vt, rs, inv_n_vt: 1.0 / n_vt }
    }

    /// 由两个同型号二极管**串联**得到等效器件（电流相同、电压对半分 ⇒ `N` 与 `Rs` 各翻倍）。
    pub const fn series(self) -> Self {
        Self::from_spice(self.is, 2.0 * self.n_vt / VT, 2.0 * self.rs)
    }

    /// 结电压 → (结电流, 结电导)，不含 `Rs`。
    #[inline(always)]
    fn junction(&self, vj: f64) -> (f64, f64) {
        let x = (vj * self.inv_n_vt).min(60.0);
        let e = x.exp();
        (self.is * (e - 1.0), self.is * e * self.inv_n_vt)
    }

    /// 端电压 → 端电流（含 `Rs` 压降）。
    #[inline]
    pub fn current(&self, v: f64) -> f64 {
        self.current_and_conductance(v).0
    }

    /// 端电压 → (端电流, 小信号电导 dI/dV)。
    #[inline]
    pub fn current_and_conductance(&self, v: f64) -> (f64, f64) {
        // 解 vj + Rs·I(vj) = v
        let mut vj = v.min(1.2);
        for _ in 0..12 {
            let (i, g) = self.junction(vj);
            let f = vj + self.rs * i - v;
            let df = 1.0 + self.rs * g;
            let step = f / df;
            vj -= step;
            if step.abs() <= 1e-9 * (1.0 + vj.abs()) {
                break;
            }
        }
        let (i, g) = self.junction(vj);
        // dI/dV = g / (1 + Rs·g)
        (i, g / (1.0 + self.rs * g))
    }

    /// 给定电流反解端电压（测试/参数拟合用；单调函数，二分保证收敛）。
    pub fn voltage_for_current(&self, target: f64) -> f64 {
        let (mut lo, mut hi) = (-2.0_f64, 2.0_f64);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if self.current(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }
}

// ---------------------------------------------------------------- 库内型号（闭集）

/// 1N4148（硅小信号）：`Is=2.52n N=1.752 Rs=.568`
pub const D_1N4148: Diode = Diode { is: 2.52e-9, n_vt: 1.752 * VT, rs: 0.568, inv_n_vt: 1.0 / (1.752 * VT) };

/// 1N34A（锗点接触）：`Is=2.6u N=1.6 Rs=6.5`
/// 低拐点（Vf@1mA ≈ 0.25 V）+ 大 Rs ⇒ 圆头削波，是"锗味"的来源。
pub const D_1N34A: Diode = Diode { is: 2.6e-6, n_vt: 1.6 * VT, rs: 6.5, inv_n_vt: 1.0 / (1.6 * VT) };

/// 1N4007（硅整流）：`Is=7.028n N=1.808 Rs=.034`
/// 大芯片 ⇒ 极小 Rs，拐点更"硬"、能承受更大电流。
pub const D_1N4007: Diode = Diode { is: 7.02767e-9, n_vt: 1.80803 * VT, rs: 0.0341512, inv_n_vt: 1.0 / (1.80803 * VT) };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_voltages_are_plausible() {
        // 与 ngspice 实测值对齐（DEVLOG E34）
        assert!((D_1N34A.voltage_for_current(1e-3) - 0.253).abs() < 0.02, "1N34A Vf@1mA");
        assert!((D_1N4007.voltage_for_current(1e-3) - 0.555).abs() < 0.02, "1N4007 Vf@1mA");
        // 1N4148 由 breadspice 模型给出，同量级即可
        let v = D_1N4148.voltage_for_current(1e-3);
        assert!((0.55..0.75).contains(&v), "1N4148 Vf@1mA={v}");
    }

    #[test]
    fn germanium_is_lower_than_silicon() {
        // 同一电流下锗管压降更低
        assert!(D_1N34A.voltage_for_current(1e-4) < D_1N4148.voltage_for_current(1e-4));
    }

    #[test]
    fn series_doubles_threshold() {
        let two = D_1N4148.series();
        let v1 = D_1N4148.voltage_for_current(1e-3);
        let v2 = two.voltage_for_current(1e-3);
        assert!((v2 - 2.0 * v1).abs() < 0.01, "串联压降 {v2} vs 2×{v1}");
    }

    #[test]
    fn conductance_matches_numeric_derivative() {
        for d in [D_1N4148, D_1N34A, D_1N4007] {
            for &v in &[0.1, 0.3, 0.5, 0.65] {
                let h = 1e-6;
                let num = (d.current(v + h) - d.current(v - h)) / (2.0 * h);
                let (_, g) = d.current_and_conductance(v);
                let rel = (num - g).abs() / num.max(1e-12);
                assert!(rel < 1e-4, "v={v} 解析电导与数值不符 (rel={rel:e})");
            }
        }
    }
}

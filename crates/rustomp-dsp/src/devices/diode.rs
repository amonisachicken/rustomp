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
        Self {
            is,
            n_vt,
            rs,
            inv_n_vt: 1.0 / n_vt,
        }
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
        // 解 vj + Rs·I(vj) = v（牛顿，二次收敛）
        //
        // 收敛阈值 1e-9（相对 vj）：**不能放松**。
        // 试过 1e-6 省一次 `exp`（实时路径快 ~15%），但高电流处（如 1N34A 在 40 mA）
        // 内层解电压只准到 ~1.4 µV，导致 `current()` 的相对误差 ~3e-5 ——
        // 比数值微分测试用的扰动（h=1e-6 V 对应的电流变化 7e-6）还大，雅可比直接失真。
        // 精度优先：实时路径的多余开销在稀疏 LU 里再找回来。
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
    ///
    /// 上界要**自动撑开**：LED 的 Vf 可以到 3.7 V，写死 `[-2, 2]` 会在 2.0 V 处截断
    /// （绿 LED @10 mA 曾因此差 9%）。
    pub fn voltage_for_current(&self, target: f64) -> f64 {
        let mut lo = -2.0_f64;
        let mut hi = 0.1_f64;
        let mut guard = 0;
        while self.current(hi) < target && guard < 80 {
            hi *= 1.5;
            guard += 1;
        }
        for _ in 0..100 {
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
pub const D_1N4148: Diode = Diode {
    is: 2.52e-9,
    n_vt: 1.752 * VT,
    rs: 0.568,
    inv_n_vt: 1.0 / (1.752 * VT),
};

/// 1N34A（锗点接触）：`Is=2.6u N=1.6 Rs=6.5`
/// 低拐点（Vf@1mA ≈ 0.25 V）+ 大 Rs ⇒ 圆头削波，是"锗味"的来源。
pub const D_1N34A: Diode = Diode {
    is: 2.6e-6,
    n_vt: 1.6 * VT,
    rs: 6.5,
    inv_n_vt: 1.0 / (1.6 * VT),
};

/// 1N4007（硅整流）：`Is=7.028n N=1.808 Rs=.034`
/// 大芯片 ⇒ 极小 Rs，拐点更"硬"、能承受更大电流。
pub const D_1N4007: Diode = Diode {
    is: 7.02767e-9,
    n_vt: 1.80803 * VT,
    rs: 0.0341512,
    inv_n_vt: 1.0 / (1.80803 * VT),
};

/// 1N5817（**肖特基**）：`Is=31.7µ N=1.373 Rs=.051`
///
/// 低 Vf（@1 A ≈ 0.42 V）+ 快恢复：电源反接保护、检波、硬削波。
pub const D_1N5817: Diode = Diode {
    is: 31.7e-6,
    n_vt: 1.373 * VT,
    rs: 0.051,
    inv_n_vt: 1.0 / (1.373 * VT),
};

/// 红色 LED：`Is=93.2p N=3.73 Rs=42m`（Vf@20 mA ≈ 1.85 V）
pub const LED_RED: Diode = Diode {
    is: 93.2e-12,
    n_vt: 3.73 * VT,
    rs: 0.042,
    inv_n_vt: 1.0 / (3.73 * VT),
};

/// 绿色 LED：`Is=93.2p N=4.61 Rs=42m`（Vf@20 mA ≈ 2.29 V）
pub const LED_GREEN: Diode = Diode {
    is: 93.2e-12,
    n_vt: 4.61 * VT,
    rs: 0.042,
    inv_n_vt: 1.0 / (4.61 * VT),
};

/// 蓝色 LED：`Is=93.2p N=7.47 Rs=42m`（Vf@20 mA ≈ 3.71 V）
pub const LED_BLUE: Diode = Diode {
    is: 93.2e-12,
    n_vt: 7.47 * VT,
    rs: 0.042,
    inv_n_vt: 1.0 / (7.47 * VT),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_voltages_are_plausible() {
        // 与 ngspice 实测值对齐（DEVLOG E34）
        assert!(
            (D_1N34A.voltage_for_current(1e-3) - 0.253).abs() < 0.02,
            "1N34A Vf@1mA"
        );
        assert!(
            (D_1N4007.voltage_for_current(1e-3) - 0.555).abs() < 0.02,
            "1N4007 Vf@1mA"
        );
        // 1N4148 由 breadspice 模型给出，同量级即可
        let v = D_1N4148.voltage_for_current(1e-3);
        assert!((0.55..0.75).contains(&v), "1N4148 Vf@1mA={v}");
        // 肖特基比硅管低、LED 按颜色递增
        let schottky = D_1N5817.voltage_for_current(1e-3);
        assert!(schottky < 0.4, "1N5817 Vf@1mA={schottky} 应 < 0.4");
        let (r, g, b) = (
            LED_RED.voltage_for_current(20e-3),
            LED_GREEN.voltage_for_current(20e-3),
            LED_BLUE.voltage_for_current(20e-3),
        );
        assert!((1.7..2.0).contains(&r), "红 LED Vf@20mA={r}");
        assert!((2.1..2.5).contains(&g), "绿 LED Vf@20mA={g}");
        assert!((3.4..4.0).contains(&b), "蓝 LED Vf@20mA={b}");
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

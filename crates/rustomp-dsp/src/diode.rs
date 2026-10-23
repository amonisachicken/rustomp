// SPDX-License-Identifier: GPL-3.0-or-later

//! 1N4148 二极管模型（含串联体电阻 `Rs`）。
//!
//! 参数取自 breadspice 的 `models.lib`：
//! `.model 1N4148 D(Is=2.52n Rs=.568 N=1.752 Cjo=4p M=.4 tt=20n)`
//!
//! 本实现包含 **Is / N / Rs** 三个对静态 I-V 有决定性影响的参数。
//! `Cjo` / `tt` 是结电容与渡越时间，属于动态/高频效应，留到后续版本（M2+）。
//!
//! ## 为什么要 Rs
//! `Rs` 是大电流下的体电阻：几十 mA 时会带来几十 mV 压降，**明显软化削波拐点**。
//! E1 的 spike 只用了 `Is/N`，曲线偏硬，不能用于最终产品（见 DEVLOG E2 §3.9）。

/// 内层牛顿迭代上限（热启动后通常 1~2 次就收敛）
const MAX_INNER_ITERS: usize = 16;

/// 1N4148 的参数。
#[derive(Debug, Clone, Copy)]
pub struct Diode1n4148 {
    /// 反向饱和电流 `Is`
    pub is: f64,
    /// 发射系数 × 热电压（`N*Vt`），常温约 0.0453 V
    pub n_vt: f64,
    /// 串联体电阻 `Rs`
    pub rs: f64,
    /// `1/(N*Vt)` —— 预计算，热路径里只做乘法（除法约贵 4~8 倍）
    inv_n_vt: f64,
}

impl Default for Diode1n4148 {
    fn default() -> Self {
        Self::new(2.52e-9, 1.752 * 0.025_852, 0.568)
    }
}

impl Diode1n4148 {
    /// 用原始参数构造（派生值在此预计算）。
    pub fn new(is: f64, n_vt: f64, rs: f64) -> Self {
        Self { is, n_vt, rs, inv_n_vt: 1.0 / n_vt }
    }

    /// 两只同型号二极管**串联**：电流相同、电压对半分，Rs 相加。
    pub fn series(self) -> Self {
        Self::new(self.is, 2.0 * self.n_vt, 2.0 * self.rs)
    }
}

impl Diode1n4148 {
    /// 结电压 → 结电流（不含 Rs），带指数上限保护。
    #[inline(always)]
    fn junction(&self, vj: f64) -> (f64, f64) {
        let x = (vj * self.inv_n_vt).min(60.0);
        let e = x.exp();
        // 全乘法，无除法
        (self.is * (e - 1.0), self.is * e * self.inv_n_vt)
    }

    /// 端电压（含 Rs 压降）→ 端电流。
    ///
    /// 需要解 `v = vj + Rs*I(vj)`。用牛顿法在 `vj` 上迭代，
    /// 以 `vj = v` 为初值（`Rs` 很小时收敛 2~3 步）。
    #[inline]
    pub fn current(&self, v: f64) -> f64 {
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
        self.junction(vj).0
    }

    /// 端电压 → (端电流, 小信号电导 dI/dV)，**带热启动**。
    ///
    /// `vj_guess` 传入上一拍的结电压、传出本拍的解。相邻样本的解几乎相同，
    /// 因此通常 1~2 次迭代就收敛（实测单次成本从最高 315 ns 降到 ~20 ns）。
    ///
    /// 精度不变：容差仍是 `1e-9`（相对），只是初值更好；若初值离得太远
    /// （瞬态）会退回解析初值 `min(v, 1.2)`，行为与无热启动版本一致。
    #[inline]
    pub fn current_and_conductance_warm(&self, v: f64, vj_guess: &mut f64) -> (f64, f64) {
        let start = if (v - *vj_guess).abs() < 0.5 {
            *vj_guess
        } else {
            v.min(1.2)
        };
        let mut vj = start;
        for _ in 0..MAX_INNER_ITERS {
            let (i, g) = self.junction(vj);
            let f = vj + self.rs * i - v;
            let df = 1.0 + self.rs * g;
            let step = f / df;
            vj -= step;
            if step.abs() <= 1e-9 * (1.0 + vj.abs()) {
                break;
            }
        }
        *vj_guess = vj;
        let (i, g) = self.junction(vj);
        (i, g / (1.0 + self.rs * g))
    }

    /// 端电压 → (端电流, 小信号电导 dI/dV)。电导也用同一套牛顿结果。
    #[inline]
    pub fn current_and_conductance(&self, v: f64) -> (f64, f64) {
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
        // dI/dV = g / (1 + Rs*g)
        (i, g / (1.0 + self.rs * g))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_voltage_matches_spice() {
        // 0.65V 附近的电流应在 1mA 量级（1N4148 典型值）
        let d = Diode1n4148::default();
        let i = d.current(0.65);
        assert!(i > 1e-4 && i < 1e-2, "0.65V 时电流 {i} 不合理");
    }

    #[test]
    fn reverse_current_is_tiny() {
        let d = Diode1n4148::default();
        assert!(d.current(-1.0).abs() < 1e-8);
    }

    #[test]
    fn warm_start_matches_cold_start() {
        // 热启动只改变初值，**不改变解**。逐点对比，误差必须可忽略。
        let d = Diode1n4148::default();
        let mut vj = 0.0;
        let mut worst = 0.0f64;
        let mut v = -1.0;
        while v < 2.0 {
            let (i_cold, g_cold) = d.current_and_conductance(v);
            let (i_warm, g_warm) = d.current_and_conductance_warm(v, &mut vj);
            // 绝对底噪 1e-12 A / 1e-12 S，避免深反偏区"相对误差"失去意义
            worst = worst.max((i_warm - i_cold).abs() / (i_cold.abs() + 1e-12));
            worst = worst.max((g_warm - g_cold).abs() / (g_cold.abs() + 1e-12));
            v += 0.0005;
        }
        assert!(worst < 1e-6, "热启动解与冷启动最大相对误差 {worst:e}");
    }

    #[test]
    fn conductance_matches_numeric_derivative() {
        let d = Diode1n4148::default();
        for &v in &[0.3, 0.5, 0.6, 0.7, 0.8] {
            let h = 1e-6;
            let num = (d.current(v + h) - d.current(v - h)) / (2.0 * h);
            let (_, g) = d.current_and_conductance(v);
            let rel = (num - g).abs() / num.max(1e-12);
            assert!(rel < 1e-4, "v={v} 解析电导 {g} 与数值 {num} 不符 (rel={rel})");
        }
    }
}

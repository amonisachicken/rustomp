// SPDX-License-Identifier: GPL-3.0-or-later

//! 运放：**单极点宏模型**（CPU 友好精度，见 `docs/component-library.md` §3）。
//!
//! ```text
//!  v+ ──┐
//!       ├─ vd ──► A(s) = A0/(1 + s/ωp) ──► [压摆率] ──► [输出钳位] ──► vout
//!  v- ──┘
//! ```
//!
//! 两个关键设计（都来自 Sparklin Drive 的教训）：
//!
//! **① 不要用显式欧拉**：闭环时间常数 `1/(ωp·A0)` 在 192 kHz 下比采样周期还小，
//! 显式积分会发散。这里用**精确极点离散**（`a = e^{−ωp·dt}`），把更新式写成
//! `v_out[n] = a·v_out[n-1] + b·vd[n]`（`b = (1−a)·A0`）。
//!
//! **② 不要用梯形法**：Tustin 会把频率轴扭曲，54 kHz 处等效频率偏高约 36%，
//! 让开环运放比真实"更快"。精确极点离散在音频段与模拟原型的极点完全一致。
//!
//! **③ 应在 ≥4× 过采样下运行**：54 kHz 已接近 192 kHz 的 Nyquist，
//! 离散化会把 -3dB 点拉低约 30%（4× 下误差 <2%）。`rustomp-engine` 的过采样器正好满足。
//!
//! 更新式对 `vd[n]` 是**线性**的 → **MNA 装配器可直接把它解进代数环**（无延迟），
//! 状态也只需一个数。
//!
//! 未建模（按 `docs/component-library.md` §3 可省）：输入偏置电流、失调、共模范围、噪声。
//! **不做双运放**：需要时用两个实例。

/// 运放宏模型参数。
#[derive(Debug, Clone, Copy)]
pub struct OpAmp {
    /// 开环直流增益
    pub a0: f64,
    /// 增益带宽积（Hz）
    pub gbw: f64,
    /// 压摆率（V/s）
    pub sr: f64,
    /// 输出上限 / 下限（单电源 9 V 时约距轨 1.2 V）
    pub vout_max: f64,
    pub vout_min: f64,
}

impl Default for OpAmp {
    fn default() -> Self {
        Self { a0: 5e5, gbw: 0.6e6, sr: 0.25e6, vout_max: 7.8, vout_min: 1.2 }
    }
}

impl OpAmp {
    /// 主极点角频率（rad/s）
    #[inline]
    pub fn omega_p(&self) -> f64 {
        std::f64::consts::TAU * self.gbw / self.a0
    }

    #[inline]
    pub fn clamp_out(&self, v: f64) -> f64 {
        v.clamp(self.vout_min, self.vout_max)
    }

    /// 按时间步长预计算梯形系数。
    pub fn model(&self, dt: f64) -> OpAmpModel {
        let a = (-self.omega_p() * dt).exp();
        OpAmpModel {
            p: *self,
            a,
            b: (1.0 - a) * self.a0,
            max_step: self.sr * dt,
        }
    }
}

/// 已按采样率预计算的运放模型（含梯形系数）。
#[derive(Debug, Clone, Copy)]
pub struct OpAmpModel {
    pub p: OpAmp,
    /// 极点系数 `e^{−ωp·dt}`
    pub a: f64,
    /// `vd[n]` 的系数 `(1−a)·A0`
    pub b: f64,
    /// 每步最大变化量（压摆率 × dt）
    pub max_step: f64,
}

/// 运放状态：只有输出电压一个数。
#[derive(Debug, Clone, Copy)]
pub struct OpAmpState {
    pub v_out: f64,
}

impl OpAmpState {
    pub fn at(v: f64) -> Self {
        Self { v_out: v }
    }
    pub fn reset(&mut self) {
        self.v_out = 0.0;
    }
}

impl OpAmpModel {
    /// **未钳位**的输出（`vd` 的线性函数）。MNA 装配器要的就是这个关系。
    #[inline]
    pub fn out_unclamped(&self, st: &OpAmpState, vd: f64) -> f64 {
        self.a * st.v_out + self.b * vd
    }

    /// 应用压摆率与输出钳位（非线性、事后处理；饱和不频繁时足够准确）。
    #[inline]
    pub fn post(&self, v_out_prev: f64, v: f64) -> f64 {
        let dv = (v - v_out_prev).clamp(-self.max_step, self.max_step);
        self.p.clamp_out(v_out_prev + dv)
    }

    /// 独立使用（没有 MNA 时）：给定 `vd`，更新状态并返回输出。
    #[inline]
    pub fn tick(&self, st: &mut OpAmpState, vd: f64) -> f64 {
        let raw = self.out_unclamped(st, vd);
        let out = self.post(st.v_out, raw);
        st.v_out = out;
        out
    }

    /// **线性反馈**的解析解（离线验证用；MNA 里由装配器解同一个代数环）。
    ///
    /// 假定反馈网络给出 `vd = c0 − k·vout`（`k` = 反馈系数 = 1/闭环增益），代入更新式：
    /// `vout = (a·vout_prev + b·c0) / (1 + b·k)`
    #[inline]
    pub fn solve_feedback(&self, st: &mut OpAmpState, c0: f64, k: f64) -> f64 {
        let raw = (self.a * st.v_out + self.b * c0) / (1.0 + self.b * k);
        let out = self.post(st.v_out, raw);
        st.v_out = out;
        out
    }

    /// 单位增益跟随器。
    #[inline]
    pub fn follower(&self, st: &mut OpAmpState, vin: f64) -> f64 {
        self.solve_feedback(st, vin, 1.0)
    }

}

// ---------------------------------------------------------------- 库内型号（闭集）

/// OP07（精密、低带宽：GBW 0.6 MHz、压摆率 0.25 V/µs —— **这个"慢"是音色的一部分**）。
pub const OP07: OpAmp = OpAmp {
    a0: 5e5,
    gbw: 0.6e6,
    sr: 0.25e6,
    vout_max: 7.8,
    vout_min: 1.2,
};

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 192_000.0;
    /// 单电源 9 V 的虚拟地（信号都以 4.5 V 为中心）
    const VREF: f64 = 4.5;

    #[test]
    fn dc_gain_settles() {
        // 开环：差分 10 µV × A0 = 5 V，时间常数 1/ωp ≈ 0.27 s
        let m = OP07.model(DT);
        let mut st = OpAmpState::at(VREF);
        for _ in 0..(3.0 / DT) as usize {
            m.tick(&mut st, 1e-5); // ×A0 = 5.0 V
        }
        assert!((st.v_out - 5.0).abs() < 0.05, "v_out={}", st.v_out);
    }

    #[test]
    fn output_is_clamped() {
        let m = OP07.model(DT);
        let mut st = OpAmpState::at(VREF);
        for _ in 0..(0.1 / DT) as usize {
            m.tick(&mut st, 1.0);
        }
        assert_eq!(st.v_out, OP07.vout_max);
    }

    #[test]
    fn slew_rate_limits_step() {
        let m = OP07.model(DT);
        let mut st = OpAmpState::at(VREF);
        let before = st.v_out;
        m.tick(&mut st, 10.0);
        assert!(st.v_out - before <= OP07.sr * DT + 1e-12);
    }

    #[test]
    fn unity_gain_follower_tracks_input() {
        // 隐式闭环：应该稳定且几乎无误差（显式欧拉在这里会发散）
        let m = OP07.model(DT);
        let mut st = OpAmpState::at(VREF);
        let mut worst = 0.0f64;
        for i in 0..(0.1 / DT) as usize {
            let t = i as f64 * DT;
            let vin = VREF + (std::f64::consts::TAU * 1000.0 * t).sin();
            let out = m.follower(&mut st, vin);
            if t > 0.02 {
                worst = worst.max((out - vin).abs());
            }
        }
        assert!(worst < 0.01, "跟随误差 {worst:.4} V");
    }

    #[test]
    fn closed_loop_bandwidth_is_gbw_over_gain() {
        // 非反相 11 倍：闭环 -3dB 带宽应 ≈ GBW/11 ≈ 54.5 kHz。
        // **在 4× 过采样下测**（引擎的实际工作点）：基带 192 kHz 时 54 kHz 已接近 Nyquist，
        // 离散化本身会让 -3dB 点偏低 30% 左右，那是采样率的限制而不是模型的错。
        const DT: f64 = 1.0 / (192_000.0 * 4.0);
        let m = OP07.model(DT);
        let gain = 11.0;
        let k = 1.0 / gain;
        let mut st = OpAmpState::at(VREF);

        let mut measure = |f: f64| -> f64 {
            st.reset();
            st.v_out = VREF;
            let n = ((80.0 / (f * DT)).round() as usize).max(64);
            let (mut pk_out, mut pk_in) = (0.0f64, 0.0f64);
            for i in 0..n {
                let t = i as f64 * DT;
                let vin = VREF + 1e-3 * (std::f64::consts::TAU * f * t).sin();
                // 非反相：vd = (vin−VREF) − k·(vout−VREF) = c0 − k·vout
                let c0 = (vin - VREF) + k * VREF;
                let out = m.solve_feedback(&mut st, c0, k);
                if i > n / 2 {
                    pk_out = pk_out.max((out - VREF).abs());
                    pk_in = pk_in.max((vin - VREF).abs());
                }
            }
            pk_out / pk_in
        };

        let dc = measure(1000.0);
        println!("  低频 |H| = {dc:.3}（期望 ≈ 11）");
        let mut f3 = None;
        for i in 0..=20 {
            let f = 5_000.0 * (1.12_f64).powi(i);
            let h = measure(f);
            println!("  f={f:>8.0} Hz  |H|={h:>6.2}  ({:+.1} dB)", 20.0 * (h / gain).log10());
            if f3.is_none() && h < dc / std::f64::consts::SQRT_2 {
                f3 = Some(f);
            }
        }
        assert!((dc - gain).abs() < 0.6, "低频增益 {dc:.2} 应 ≈ 11");
        let f3 = f3.expect("-3dB 点未找到（滚降太慢）");
        assert!((45_000.0..65_000.0).contains(&f3), "-3dB 点 {f3:.0} Hz 应 ≈ 54 kHz");
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! 4 倍过采样（多相 FIR）。
//!
//! 目的：非线性削波会产生远超 Nyquist 的谐波。在 48 kHz 下直接跑电路会把
//! 高次谐波**折叠**回音频带（听感上是"沙砾感/金属味"），所以要在 4 倍率下处理。
//!
//! 实现：一条 Kaiser 窗 sinc 原型滤波器（64 抽头），
//! - 插值：多相分解，每相 16 抽头，每个输入样本产出 4 个输出
//! - 抽取：同一原型在 4 倍率上做低通，每 4 个样本取 1 个
//!
//! 群延迟：(N-1)/2 个 4 倍率样本 = (N-1)/8 个基带样本，
//! 插值 + 抽取合计 `(N-1)/4` 个基带样本（N=64 时 ≈ 15.75 → 报 16）。

/// 原型滤波器抽头数（必须是 4 的倍数）。
const TAPS: usize = 64;
/// 每个相位的抽头数。
const PHASE_TAPS: usize = TAPS / 4;
/// 归一化截止频率（相对于 4 倍率采样率）。略低于 1/8 以留过渡带。
const CUTOFF: f64 = 0.5 / 4.0 * 0.90;
/// Kaiser 窗参数（≈ −80 dB 阻带）。
const KAISER_BETA: f64 = 8.0;

/// 4 倍过采样器（单声道）。
pub struct Oversampler4x {
    /// 插值：输入历史（环形）
    up_hist: [f64; PHASE_TAPS],
    up_pos: usize,
    /// 抽取：4 倍率历史（环形）
    down_hist: [f64; TAPS],
    down_pos: usize,
    /// 原型滤波器
    proto: [f64; TAPS],
}

impl Default for Oversampler4x {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler4x {
    pub fn new() -> Self {
        let mut proto = [0.0; TAPS];
        // 理想低通：h[n] = 2*fc * sinc(2*fc*(n - (N-1)/2))
        let m = (TAPS - 1) as f64 / 2.0;
        let mut sum = 0.0;
        for (i, h) in proto.iter_mut().enumerate() {
            let x = i as f64 - m;
            let s = if x.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * 2.0 * CUTOFF * x).sin()
                    / (std::f64::consts::PI * 2.0 * CUTOFF * x)
            };
            // Kaiser 窗
            let r = x / m;
            let w = if r.abs() > 1.0 {
                0.0
            } else {
                kaiser(r, KAISER_BETA)
            };
            *h = 2.0 * CUTOFF * s * w;
            sum += *h;
        }
        // 归一化到直流增益 1
        for h in proto.iter_mut() {
            *h /= sum;
        }
        Self {
            up_hist: [0.0; PHASE_TAPS],
            up_pos: 0,
            down_hist: [0.0; TAPS],
            down_pos: 0,
            proto,
        }
    }

    /// 插值：1 个基带样本 → 4 个 4 倍率样本。
    #[inline]
    pub fn upsample(&mut self, x: f64, out: &mut [f64; 4]) {
        self.up_hist[self.up_pos] = x;
        // 相位 p 用到原型里下标模 4 == p 的抽头
        for (p, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for k in 0..PHASE_TAPS {
                // 历史样本从新到旧
                let idx = (self.up_pos + PHASE_TAPS - k) % PHASE_TAPS;
                acc += self.proto[k * 4 + p] * self.up_hist[idx];
            }
            // 零插值后要补偿 4 倍增益
            *o = acc * 4.0;
        }
        self.up_pos = (self.up_pos + 1) % PHASE_TAPS;
    }

    /// 抽取：4 个 4 倍率样本 → 1 个基带样本。
    #[inline]
    pub fn downsample(&mut self, inp: &[f64; 4]) -> f64 {
        for v in inp {
            self.down_hist[self.down_pos] = *v;
            self.down_pos = (self.down_pos + 1) % TAPS;
        }
        // 只在最后一个 4 倍率样本上算一次 FIR 输出
        let mut acc = 0.0;
        for k in 0..TAPS {
            let idx = (self.down_pos + TAPS - 1 - k) % TAPS;
            acc += self.proto[k] * self.down_hist[idx];
        }
        acc
    }

    /// 插值 + 抽取带来的总延迟（基带样本数）。
    pub const fn latency_samples() -> u32 {
        ((TAPS - 1) / 4) as u32
    }
}

/// 零阶修正贝塞尔函数（Kaiser 窗用），级数展开足够精确。
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let y = x * x / 4.0;
    for k in 1..25 {
        term *= y / (k as f64 * k as f64);
        sum += term;
        if term < 1e-16 * sum {
            break;
        }
    }
    sum
}

fn kaiser(r: f64, beta: f64) -> f64 {
    let a = 1.0 - r * r;
    if a <= 0.0 {
        0.0
    } else {
        bessel_i0(beta * a.sqrt()) / bessel_i0(beta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    /// 直通：通带内的正弦经过 上采样→下采样 后应保持幅度（忽略延迟）。
    #[test]
    fn passband_is_flat() {
        let fs = 48_000.0;
        for &f in &[100.0, 1000.0, 5000.0, 10000.0] {
            let mut os = Oversampler4x::new();
            let mut ph: f64 = 0.0;
            let (mut peak, mut peak_ref) = (0.0f64, 0.0f64);
            for n in 0..8000 {
                let x = 0.1 * ph.sin();
                let mut up = [0.0; 4];
                os.upsample(x, &mut up);
                let y = os.downsample(&up);
                ph += TAU * f / fs;
                if n > 1000 {
                    peak = peak.max(y.abs());
                    peak_ref = peak_ref.max(x.abs());
                }
            }
            let gain = peak / peak_ref;
            assert!((gain - 1.0).abs() < 0.02, "f={f} 通带增益 {gain:.4} 偏离 1");
        }
    }

    /// 阻带：接近 Nyquist 的分量应被压掉（否则会产生镜像/折叠）。
    #[test]
    fn stopband_is_attenuated() {
        let mut os = Oversampler4x::new();
        let fs = 48_000.0;
        let f = 22_000.0; // 基带 Nyquist 附近
        let mut ph: f64 = 0.0;
        let mut peak = 0.0f64;
        for _ in 0..4800 {
            let x = ph.sin();
            let mut up = [0.0; 4];
            os.upsample(x, &mut up);
            let y = os.downsample(&up);
            ph += TAU * f / fs;
            if ph > TAU * 20.0 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.3, "22 kHz 未被压掉：{peak:.3}");
    }
}

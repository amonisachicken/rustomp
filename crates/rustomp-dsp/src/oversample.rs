// SPDX-License-Identifier: GPL-3.0-or-later

//! 多相 FIR 过采样（2× / 4×）。
//!
//! 目的：非线性削波会产生远超 Nyquist 的谐波。在 48 kHz 下直接跑电路会把
//! 高次谐波**折叠**回音频带（听感上是"沙砾感/金属味"），所以要在更高的内部倍率下处理。
//!
//! 实现：一条 Kaiser 窗 sinc 原型滤波器（`TAPS` 抽头，`F` 个相位），
//! - 插值：多相分解，每相 `TAPS/F` 抽头，每个输入样本产出 `F` 个输出
//! - 抽取：同一原型在 `F` 倍率上做低通，每 `F` 个样本取 1 个
//!
//! 群延迟：(N-1)/2 个 F 倍率样本 = (N-1)/(2F) 个基带样本，
//! 插值 + 抽取合计 `(N-1)/F` 个基带样本。
//!
//! | 类型 | F | TAPS | 往返延迟（基带样本） | 用途 |
//! |---|---|---|---|---|
//! | [`Oversampler4x`] | 4 | 64 | 15.75（上报 15） | 默认：混叠最低 |
//! | [`Oversampler2x`] | 2 | 32 | 15.5（上报 15） | 立体声省一半 CPU，代价是高音折叠变多 |
//!
//! **为什么 2× 的延迟能跟 4× 一样**：(N-1)/F —— 抽头数减半、倍数也减半，
//! 商不变。所以从 4× 切到 2× **宿主侧的上报延迟不用改**（都是 15 样本）。

/// Kaiser 窗参数（≈ −80 dB 阻带）。
const KAISER_BETA: f64 = 8.0;

/// 通用多相过采样器（单声道）：`F` 倍、`TAPS` 抽头（`TAPS` 必须是 `F` 的倍数）。
///
/// 历史：E11 起只有硬编码的 4×；E91 抽成 const 泛型，让 2× 与 4× 共用同一份实现
/// （同一套窗函数、同一套多相结构，A/B 时差异只来自倍数本身）。
pub struct Polyphase<const F: usize, const TAPS: usize> {
    /// 插值：输入历史（环形，只用前 `TAPS/F` 个槽）
    up_hist: [f64; TAPS],
    up_pos: usize,
    /// 抽取：F 倍率历史（环形）
    down_hist: [f64; TAPS],
    down_pos: usize,
    /// 原型滤波器
    proto: [f64; TAPS],
}

/// 过采样器契约：`Engine` 只依赖这四个东西。
pub trait Oversampler: Sized {
    /// 过采样倍数
    const FACTOR: usize;
    /// 插值 + 抽取带来的总延迟（基带样本，向下取整）
    const LATENCY: u32;

    fn new_oversampler() -> Self;
    /// 1 个基带样本 → [`Self::FACTOR`] 个内部样本（只写 `out[..FACTOR]`）
    fn upsample(&mut self, x: f64, out: &mut [f64]);
    /// [`Self::FACTOR`] 个内部样本（`inp[..FACTOR]`）→ 1 个基带样本
    fn downsample(&mut self, inp: &[f64]) -> f64;
}

impl<const F: usize, const TAPS: usize> Default for Polyphase<F, TAPS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const F: usize, const TAPS: usize> Polyphase<F, TAPS> {
    pub fn new() -> Self {
        assert!(F >= 1, "过采样倍数必须 ≥ 1");
        assert_eq!(TAPS % F, 0, "TAPS 必须是 F 的倍数");
        let mut proto = [0.0; TAPS];
        // 理想低通：h[n] = 2·fc·sinc(2·fc·(n − (N−1)/2))，fc 归一化到 F 倍率
        let cutoff = 0.45 / F as f64;
        let m = (TAPS - 1) as f64 / 2.0;
        let mut sum = 0.0;
        for (i, h) in proto.iter_mut().enumerate() {
            let x = i as f64 - m;
            let s = if x.abs() < 1e-12 {
                1.0
            } else {
                (std::f64::consts::PI * 2.0 * cutoff * x).sin()
                    / (std::f64::consts::PI * 2.0 * cutoff * x)
            };
            let r = x / m;
            let w = if r.abs() > 1.0 {
                0.0
            } else {
                kaiser(r, KAISER_BETA)
            };
            *h = 2.0 * cutoff * s * w;
            sum += *h;
        }
        // 归一化到直流增益 1
        for h in proto.iter_mut() {
            *h /= sum;
        }
        Self {
            up_hist: [0.0; TAPS],
            up_pos: 0,
            down_hist: [0.0; TAPS],
            down_pos: 0,
            proto,
        }
    }

    /// 该配置的往返延迟（基带样本，向下取整）
    pub const fn latency_samples() -> u32 {
        ((TAPS - 1) / F) as u32
    }
}

impl<const F: usize, const TAPS: usize> Oversampler for Polyphase<F, TAPS> {
    const FACTOR: usize = F;
    const LATENCY: u32 = ((TAPS - 1) / F) as u32;

    #[inline]
    fn new_oversampler() -> Self {
        Self::new()
    }

    #[inline]
    fn upsample(&mut self, x: f64, out: &mut [f64]) {
        debug_assert!(out.len() >= F);
        let phase_taps = TAPS / F;
        self.up_hist[self.up_pos] = x;
        // 相位 p 用到原型里下标模 F == p 的抽头
        for (p, o) in out.iter_mut().enumerate().take(F) {
            let mut acc = 0.0;
            for k in 0..phase_taps {
                // 历史样本从新到旧
                let idx = (self.up_pos + phase_taps - k) % phase_taps;
                acc += self.proto[k * F + p] * self.up_hist[idx];
            }
            // 零插值后要补偿 F 倍增益
            *o = acc * F as f64;
        }
        self.up_pos = (self.up_pos + 1) % phase_taps;
    }

    #[inline]
    fn downsample(&mut self, inp: &[f64]) -> f64 {
        debug_assert!(inp.len() >= F);
        for v in inp.iter().take(F) {
            self.down_hist[self.down_pos] = *v;
            self.down_pos = (self.down_pos + 1) % TAPS;
        }
        // 只在最后一个 F 倍率样本上算一次 FIR 输出
        let mut acc = 0.0;
        for k in 0..TAPS {
            let idx = (self.down_pos + TAPS - 1 - k) % TAPS;
            acc += self.proto[k] * self.down_hist[idx];
        }
        acc
    }
}

/// 4 倍过采样（默认）：64 抽头，往返延迟 15.75 基带样本。
pub type Oversampler4x = Polyphase<4, 64>;
/// 2 倍过采样：32 抽头，往返延迟 15.5 基带样本（**与 4× 上报的 15 相同**）。
pub type Oversampler2x = Polyphase<2, 32>;

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

    /// 通带平坦：`F` 倍上/下采样后幅度应保持（忽略延迟）
    fn passband_flat<O: Oversampler>(fs: f64) {
        for &f in &[100.0, 1000.0, 5000.0, 10000.0] {
            let mut os = O::new_oversampler();
            let mut up = [0.0f64; 8];
            let mut ph: f64 = 0.0;
            let (mut peak, mut peak_ref) = (0.0f64, 0.0f64);
            for n in 0..8000 {
                let x = 0.1 * ph.sin();
                os.upsample(x, &mut up);
                let y = os.downsample(&up);
                ph += TAU * f / fs;
                if n > 1000 {
                    peak = peak.max(y.abs());
                    peak_ref = peak_ref.max(x.abs());
                }
            }
            let gain = peak / peak_ref;
            assert!(
                (gain - 1.0).abs() < 0.02,
                "F={} f={f} 通带增益 {gain:.4} 偏离 1",
                O::FACTOR
            );
        }
    }

    /// 阻带：基带 Nyquist 附近的分量应被压掉
    fn stopband_attenuated<O: Oversampler>(fs: f64) {
        let mut os = O::new_oversampler();
        let mut up = [0.0f64; 8];
        let f = 22_000.0;
        let mut ph: f64 = 0.0;
        let mut peak = 0.0f64;
        for _ in 0..4800 {
            let x = ph.sin();
            os.upsample(x, &mut up);
            let y = os.downsample(&up);
            ph += TAU * f / fs;
            if ph > TAU * 20.0 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.3, "F={} 22 kHz 未被压掉：{peak:.3}", O::FACTOR);
    }

    #[test]
    fn 四倍通带平坦() {
        passband_flat::<Oversampler4x>(48_000.0);
    }

    #[test]
    fn 四倍阻带衰减() {
        stopband_attenuated::<Oversampler4x>(48_000.0);
    }

    #[test]
    fn 二倍通带平坦() {
        passband_flat::<Oversampler2x>(48_000.0);
    }

    #[test]
    fn 二倍阻带衰减() {
        stopband_attenuated::<Oversampler2x>(48_000.0);
    }

    /// 2× 与 4× 上报的延迟必须一致（切倍数不改宿主侧的补偿量）
    #[test]
    fn 延迟一致() {
        assert_eq!(Oversampler4x::LATENCY, 15);
        assert_eq!(Oversampler2x::LATENCY, 15);
        assert_eq!(Oversampler4x::FACTOR, 4);
        assert_eq!(Oversampler2x::FACTOR, 2);
    }
}

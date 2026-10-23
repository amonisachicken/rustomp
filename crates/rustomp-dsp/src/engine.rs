// SPDX-License-Identifier: GPL-3.0-or-later

//! 把「电路 + 4 倍过采样」打包成插件直接可用的引擎。

use crate::fakedist::{Fakedist, Params};
use crate::oversample::Oversampler4x;

/// 过采样倍数。
pub const OVERSAMPLING: usize = 4;

/// 单声道处理引擎。
pub struct Engine {
    dsp: Fakedist,
    os: Oversampler4x,
    fs: f64,
}

impl Engine {
    pub fn new(fs: f64) -> Self {
        let mut e = Self {
            dsp: Fakedist::new(fs * OVERSAMPLING as f64),
            os: Oversampler4x::new(),
            fs,
        };
        e.set_sample_rate(fs);
        e
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        self.fs = fs;
        self.dsp.set_sample_rate(fs * OVERSAMPLING as f64);
        self.os = Oversampler4x::new();
    }

    pub fn set_params(&mut self, p: Params) {
        self.dsp.set_params(p);
    }

    pub fn reset(&mut self) {
        self.dsp.reset();
        self.os = Oversampler4x::new();
    }

    /// 插件应向上报的延迟（基带样本）。
    pub const fn latency_samples() -> u32 {
        Oversampler4x::latency_samples()
    }

    /// 处理一个基带样本。
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let mut up = [0.0f64; OVERSAMPLING];
        self.os.upsample(x, &mut up);
        for v in up.iter_mut() {
            *v = self.dsp.process(*v);
        }
        self.os.downsample(&up)
    }

    /// 内部电路（测试/调试用）。
    pub fn circuit(&self) -> &Fakedist {
        &self.dsp
    }

    pub fn sample_rate(&self) -> f64 {
        self.fs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn thd(amp: f64, f0: f64, fs: f64) -> f64 {
        let mut e = Engine::new(fs);
        e.set_params(Params::default());
        let t = 1.0 / fs;
        let mut ph: f64 = 0.0;
        for _ in 0..(3.0 * fs) as usize {
            e.process(amp * ph.sin());
            ph += TAU * f0 * t;
        }
        let n = (fs / f0).round() as usize * 4; // 4 个周期
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            buf.push(e.process(amp * ph.sin()));
            ph += TAU * f0 * t;
        }
        let mean = buf.iter().sum::<f64>() / n as f64;
        let mag = |f: f64| -> f64 {
            let (mut re, mut im) = (0.0, 0.0);
            for (k, v) in buf.iter().enumerate() {
                let a = TAU * f * k as f64 * t;
                re += (v - mean) * a.cos();
                im -= (v - mean) * a.sin();
            }
            2.0 * (re * re + im * im).sqrt() / n as f64
        };
        let fund = mag(f0);
        let sq: f64 = (2..=9).map(|h| mag(f0 * h as f64).powi(2)).sum();
        sq.sqrt() / fund * 100.0
    }

    /// 48 kHz + 4x 过采样后，THD 应仍与 192 kHz 直接运行的基线一致。
    #[test]
    fn thd_survives_oversampling() {
        let got = thd(0.2, 1000.0, 48_000.0);
        println!("48k + 4x 过采样 THD(2..9) = {got:.3} %（基线 27.616 %）");
        assert!((got - 27.616).abs() < 1.5, "THD {got:.3} 与基线差太多");
    }

    /// 延迟应当是固定常数，不能随采样率变化。
    #[test]
    fn latency_is_constant() {
        assert_eq!(Engine::latency_samples(), 15);
    }

    /// 宿主采样率 44.1k / 48k / 96k / 192k 下都必须给出同样的稳态结果。
    ///
    /// 这一条很关键：**直接在 48k 跑电路会失效**（隐含求解欠解析，3kHz 输入时输出归零，
    /// 见 `examples/os_probe.rs`）。Engine 固定 4x，所以宿主侧安全。
    #[test]
    fn works_at_all_host_rates() {
        for &fs in &[44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            for &f0 in &[1000.0f64, 3000.0] {
                let got = thd(0.2, f0, fs);
                println!("host fs={fs:.0} f0={f0:.0} → THD {got:.2} %");
                assert!(got > 20.0 && got < 32.0, "fs={fs} f0={f0} THD={got:.2} 异常");
            }
        }
    }
}

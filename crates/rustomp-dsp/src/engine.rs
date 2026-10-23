// SPDX-License-Identifier: GPL-3.0-or-later

//! 把「电路 + 4 倍过采样」打包成插件直接可用的引擎。

use crate::fakedist::{Fakedist, Params};
use crate::oversample::Oversampler4x;

/// 过采样倍数。
pub const OVERSAMPLING: usize = 4;

/// 输出电平保护阈值（远超任何可能的合法信号）
const GUARD_LIMIT: f64 = 100.0;

/// 与过采样链路等长的干路延迟（bypass 时用，保证与湿路相位对齐）
const DRY_LEN: usize = Oversampler4x::latency_samples() as usize;

/// 单声道处理引擎。
pub struct Engine {
    dsp: Fakedist,
    os: Oversampler4x,
    fs: f64,
    params: Params,
    /// 干路延迟线（与湿路对齐，避免 bypass 切换时的梳状滤波）
    dry: [f64; DRY_LEN],
    dry_pos: usize,
    /// bypass 交叉淡化量：0 = 干，1 = 湿
    mix: f64,
    mix_coef: f64,
    /// 削波指示包络（0..1，给 GUI 的 LED）
    clip_env: f32,
    clip_decay: f64,
}

impl Engine {
    pub fn new(fs: f64) -> Self {
        let mut e = Self {
            dsp: Fakedist::new(fs * OVERSAMPLING as f64),
            os: Oversampler4x::new(),
            fs,
            params: Params::default(),
            dry: [0.0; DRY_LEN],
            dry_pos: 0,
            mix: 1.0,
            mix_coef: 0.0,
            clip_env: 0.0,
            clip_decay: 0.0,
        };
        e.set_sample_rate(fs);
        e
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        self.fs = fs;
        self.dsp.set_sample_rate(fs * OVERSAMPLING as f64);
        self.os = Oversampler4x::new();
        // bypass 切换 5 ms 交叉淡化；LED 释放 ~150 ms
        self.mix_coef = 1.0 - (-1.0 / (0.005 * fs)).exp();
        self.clip_decay = (-1.0 / (0.15 * fs)).exp();
        self.reset();
    }

    pub fn set_params(&mut self, p: Params) {
        self.params = p;
        self.dsp.set_params(p);
    }

    /// 削波指示电平（0..1），供 GUI 的 LED 使用
    #[inline]
    pub fn clip_indicator(&self) -> f32 {
        self.clip_env
    }

    pub fn reset(&mut self) {
        self.dsp.reset();
        self.os = Oversampler4x::new();
        self.dry = [0.0; DRY_LEN];
        self.dry_pos = 0;
        self.mix = if self.params.bypass { 0.0 } else { 1.0 };
        self.clip_env = 0.0;
    }

    /// 插件应向上报的延迟（基带样本）。
    pub const fn latency_samples() -> u32 {
        Oversampler4x::latency_samples()
    }

    /// 处理一个基带样本。
    ///
    /// 兜底：万一出现非有限值或离谱电平（数值意外），**立刻复位到直流工作点**
    /// 并本样本输出静音。绝不让一个坏样本变成永久自激。
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let mut up = [0.0f64; OVERSAMPLING];
        self.os.upsample(x, &mut up);
        for v in up.iter_mut() {
            *v = self.dsp.process(*v);
        }
        let wet = self.os.downsample(&up);
        if !wet.is_finite() || wet.abs() > GUARD_LIMIT {
            self.reset();
            return 0.0;
        }

        // 干路（延迟对齐后取）
        let dry = self.dry[self.dry_pos];
        self.dry[self.dry_pos] = x;
        self.dry_pos = (self.dry_pos + 1) % DRY_LEN;

        // true-bypass：淡入淡出，电路仍在运行（避免接回时状态陈旧产生爆音）
        let target = if self.params.bypass { 0.0 } else { 1.0 };
        self.mix += (target - self.mix) * self.mix_coef;
        let y = dry * (1.0 - self.mix) + wet * self.mix;

        // 削波指示：|u| 超过 ~0.55 V 起亮，慢释放
        let u = self.dsp.clip_voltage();
        let t = ((u - 0.55) / 0.8).clamp(0.0, 1.0) as f32;
        self.clip_env = ((self.clip_env as f64 * self.clip_decay) as f32).max(t);

        y
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

    /// 瞬态鲁棒性（回归测试）：曾经在噪声突发 / 直流阶跃后锁死成 ~10 kHz 自激。
    ///
    /// 根因：牛顿在大信号下被指数项甩飞 → 状态被污染到 1e16 → 硬钳位维持成极限环。
    /// 修复：阻尼牛顿 + 残差判据 + 失败兜底 + 去掉破坏一致性的 v_out 钳位。
    #[test]
    fn no_self_oscillation_after_transients() {
        // 输入停止后，输出必须回落到接近 0；且全程有界
        let cases: [(&str, Box<dyn Fn(usize, f64) -> f64>); 4] = [
            ("直流阶跃", Box::new(|_, t: f64| if (1.0..2.0).contains(&t) { 3.0 } else { 0.0 })),
            ("脉冲", Box::new(|_, t: f64| if (1.0..1.001).contains(&t) { 5.0 } else { 0.0 })),
            ("噪声突发", Box::new(|i: usize, t: f64| {
                if (1.0..2.0).contains(&t) {
                    let mut s = i as u64;
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    ((s % 2000) as f64 / 1000.0 - 1.0) * 2.0
                } else {
                    0.0
                }
            })),
            ("方波", Box::new(|_, t: f64| if (t * 220.0).fract() < 0.5 { 2.0 } else { -2.0 })),
        ];
        for (name, gen) in cases.iter() {
            for &gain in &[0.0, 0.5, 1.0] {
                let fs = 48_000.0;
                let mut e = Engine::new(fs);
                e.set_params(Params { gain, level: 0.5, ..Params::default() });
                let n = (4.0 * fs) as usize;
                let mut peak = 0.0f64;
                let mut tail = 0.0f64;
                let mut zc = 0u32;
                let mut prev = 0.0;
                for i in 0..n {
                    let t = i as f64 / fs;
                    let y = e.process(gen(i, t));
                    assert!(y.is_finite(), "{name} gain={gain} 出现非有限值");
                    peak = peak.max(y.abs());
                    if i > n * 19 / 20 {
                        tail = tail.max(y.abs());
                        if (prev >= 0.0) != (y >= 0.0) {
                            zc += 1;
                        }
                        prev = y;
                    }
                }
                assert!(peak < 20.0, "{name} gain={gain} 电平失控 peak={peak}");
                // 末尾 5% 是纯静音输入，方波除外（它一直在响）
                if *name != "方波" {
                    assert!(
                        tail < 0.05,
                        "{name} gain={gain} 输入停止后仍有 {tail:.3} 输出（自激）"
                    );
                } else {
                    let zcr = zc as f64 / (n as f64 * 0.05);
                    assert!(zcr < 0.05, "{name} gain={gain} 尾段过零率 {zcr:.3} 异常（自激）");
                }
            }
        }
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

// SPDX-License-Identifier: GPL-3.0-or-later

//! 把「任意电路 + 4 倍过采样 + true-bypass + 活动指示」打包成插件可用的引擎。
//!
//! 对电路一无所知：只要求实现 [`Circuit`]。

use crate::circuit::Circuit;
use crate::oversample::Oversampler4x;

/// 过采样倍数。
pub const OVERSAMPLING: usize = 4;

/// 输出电平保护阈值（远超任何可能的合法信号）
const GUARD_LIMIT: f64 = 100.0;

/// 与过采样链路等长的干路延迟（bypass 时用，保证与湿路相位对齐）
const DRY_LEN: usize = Oversampler4x::latency_samples() as usize;

/// 单声道处理引擎。
pub struct Engine<C: Circuit> {
    dsp: C,
    os: Oversampler4x,
    fs: f64,
    params: C::Params,
    /// 干路延迟线（与湿路对齐，避免 bypass 切换时的梳状滤波）
    dry: [f64; DRY_LEN],
    dry_pos: usize,
    /// bypass 交叉淡化量：0 = 干，1 = 湿
    mix: f64,
    mix_coef: f64,
    /// 活动指示包络（0..1，给 GUI 的 LED）
    activity_env: f32,
    activity_decay: f64,
}

impl<C: Circuit> Engine<C> {
    pub fn new(fs: f64) -> Self {
        let mut e = Self {
            dsp: C::new(fs * OVERSAMPLING as f64),
            os: Oversampler4x::new(),
            fs,
            params: C::Params::default(),
            dry: [0.0; DRY_LEN],
            dry_pos: 0,
            mix: 1.0,
            mix_coef: 0.0,
            activity_env: 0.0,
            activity_decay: 0.0,
        };
        e.set_sample_rate(fs);
        e
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        self.fs = fs;
        self.dsp.set_sample_rate(fs * OVERSAMPLING as f64);
        self.os = Oversampler4x::new();
        // bypass 切换 5 ms 交叉淡化；指示释放 ~150 ms
        self.mix_coef = 1.0 - (-1.0 / (0.005 * fs)).exp();
        self.activity_decay = (-1.0 / (0.15 * fs)).exp();
        self.reset();
    }

    pub fn set_params(&mut self, p: C::Params) {
        self.params = p;
        self.dsp.set_params(p);
    }

    /// 活动/削波指示电平（0..1），供 GUI 使用
    #[inline]
    pub fn activity_indicator(&self) -> f32 {
        self.activity_env
    }

    pub fn reset(&mut self) {
        self.dsp.reset();
        self.os = Oversampler4x::new();
        self.dry = [0.0; DRY_LEN];
        self.dry_pos = 0;
        self.mix = 1.0;
        self.activity_env = 0.0;
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
        self.mix += (1.0 - self.mix) * self.mix_coef;
        let y = dry * (1.0 - self.mix) + wet * self.mix;

        // 活动指示：快起慢落
        let t = self.dsp.activity().clamp(0.0, 1.0);
        self.activity_env = ((self.activity_env as f64 * self.activity_decay) as f32).max(t);

        y
    }

    /// 内部电路（测试/调试用）。
    pub fn circuit(&self) -> &C {
        &self.dsp
    }

    pub fn sample_rate(&self) -> f64 {
        self.fs
    }
}

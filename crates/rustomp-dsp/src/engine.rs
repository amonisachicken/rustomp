// SPDX-License-Identifier: GPL-3.0-or-later

//! 把「任意电路 + 多相过采样 + true-bypass + 活动指示」打包成插件可用的引擎。
//!
//! 对电路一无所知：只要求实现 [`Circuit`]。过采样器是**类型参数**
//! （默认 [`Oversampler4x`]，可换 [`Oversampler2x`] —— 见 E91），
//! 因为"倍数"是音质/CPU 的取舍，属于插件侧决策，不属于电路。

use crate::circuit::Circuit;
use crate::oversample::{Oversampler, Oversampler2x, Oversampler4x};

/// 默认过采样器。开 `oversample-2x` feature 时全局改成 2×（省一半 CPU）。
#[cfg(not(feature = "oversample-2x"))]
pub type DefaultOversampler = Oversampler4x;
/// 默认过采样器（`feature = "oversample-2x"`）。
#[cfg(feature = "oversample-2x")]
pub type DefaultOversampler = Oversampler2x;

/// 内部样本缓冲的最大长度（= 支持的最大倍数）。
pub const MAX_FACTOR: usize = 4;

/// 干路延迟线的最大长度（够任何倍数的 `latency_samples()` 用）。
const MAX_DRY: usize = 64;

/// 输出电平保护阈值（远超任何可能的合法信号）
const GUARD_LIMIT: f64 = 100.0;

/// 单声道处理引擎。
pub struct Engine<C: Circuit, O: Oversampler = DefaultOversampler> {
    dsp: C,
    os: O,
    fs: f64,
    params: C::Params,
    /// 干路延迟线（与湿路对齐，避免 bypass 切换时的梳状滤波）
    dry: [f64; MAX_DRY],
    /// 干路实际长度 = `O::LATENCY`（按倍数选择）
    dry_len: usize,
    dry_pos: usize,
    /// bypass 交叉淡化量：0 = 干，1 = 湿
    mix: f64,
    /// 目标（0 或 1）：由 [`Engine::set_bypass`] 设置，`mix` 每样本向它靠拢
    mix_target: f64,
    mix_coef: f64,
    /// 活动指示包络（0..1，给 GUI 的 LED）
    activity_env: f32,
    activity_decay: f64,
    /// **还没吃到过宿主的参数**（`new`/`reset`/采样率变化之后为 true）。
    ///
    /// 为什么需要它（E90）：`C::new()` 用 `Params::default()` 解了直流工作点，
    /// 而宿主在第一次 `process` 前就会把工程里存的参数打进来。若两者相差较多
    /// （电位器拧到 25% 之类），牛顿要从"错误偏置"迁移过去 —— 实测要多花 ~20 次
    /// 迭代，甚至有落进发散区、此后全靠兜底的风险。所以第一口参数到达时
    /// **重解一次工作点**（代价：一次 `solve_dc`，只发生在加载/复位时）。
    fresh: bool,
}

impl<C: Circuit, O: Oversampler> Engine<C, O> {
    pub fn new(fs: f64) -> Self {
        let mut e = Self {
            dsp: C::new(fs * O::FACTOR as f64),
            os: O::new_oversampler(),
            fs,
            params: C::Params::default(),
            dry: [0.0; MAX_DRY],
            dry_len: (O::LATENCY as usize).max(1),
            dry_pos: 0,
            mix: 1.0,
            mix_target: 1.0,
            mix_coef: 0.0,
            activity_env: 0.0,
            activity_decay: 0.0,
            fresh: true,
        };
        e.set_sample_rate(fs);
        e
    }

    pub fn set_sample_rate(&mut self, fs: f64) {
        self.fs = fs;
        self.dsp.set_sample_rate(fs * O::FACTOR as f64);
        self.os = O::new_oversampler();
        // bypass 切换 5 ms 交叉淡化；指示释放 ~150 ms
        self.mix_coef = 1.0 - (-1.0 / (0.005 * fs)).exp();
        self.activity_decay = (-1.0 / (0.15 * fs)).exp();
        self.reset();
    }

    /// **true-bypass**：`true` = 干路直通（湿路继续运行，避免接回时状态陈旧）。
    ///
    /// 切换是 5 ms 交叉淡化（`mix_coef`），并且干路走与过采样链路等长的延迟线，
    /// 所以切换瞬间没有咔哒、也没有梳状滤波。
    pub fn set_bypass(&mut self, bypass: bool) {
        self.mix_target = if bypass { 0.0 } else { 1.0 };
    }

    /// 当前是否处于 bypass（`mix` 已经走到干路）
    pub fn is_bypassed(&self) -> bool {
        self.mix < 0.5
    }

    pub fn set_params(&mut self, p: C::Params) {
        self.params = p;
        self.dsp.set_params(p);
        if self.fresh {
            // 第一次拿到真参数：把直流工作点按**真参数**重解一遍（见 `fresh` 的说明）
            self.fresh = false;
            self.dsp.reset();
        }
    }

    /// 活动/削波指示电平（0..1），供 GUI 使用
    ///
    /// 乘上旁通权重：**踩下 true-bypass 时灯自然熄灭**（否则电路还在跑，
    /// 灯会继续闪，用户看到的就是"已经 bypass 了灯还亮着"）。
    #[inline]
    pub fn activity_indicator(&self) -> f32 {
        self.activity_env * self.mix as f32
    }

    pub fn reset(&mut self) {
        self.dsp.reset();
        self.os = O::new_oversampler();
        self.dry = [0.0; MAX_DRY];
        self.dry_pos = 0;
        self.mix = 1.0;
        self.mix_target = 1.0;
        self.activity_env = 0.0;
        self.fresh = true;
    }

    /// 插件应向上报的延迟（基带样本）。**与过采样倍数无关地保持稳定**
    /// （4× 与 2× 都报 15，见 [`crate::oversample`] 的表格）。
    pub const fn latency_samples() -> u32 {
        O::LATENCY
    }

    /// 当前过采样倍数（诊断/日志用）。
    pub const fn oversampling() -> usize {
        O::FACTOR
    }

    /// 处理一个基带样本。
    ///
    /// 兜底：万一出现非有限值或离谱电平（数值意外），**立刻复位到直流工作点**
    /// 并本样本输出静音。绝不让一个坏样本变成永久自激。
    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let mut up = [0.0f64; MAX_FACTOR];
        let n = O::FACTOR;
        {
            let inner = &mut up[..n];
            self.os.upsample(x, inner);
            for v in inner.iter_mut() {
                *v = self.dsp.process(*v);
            }
        }
        let wet = self.os.downsample(&up[..n]);
        if !wet.is_finite() || wet.abs() > GUARD_LIMIT {
            self.reset();
            return 0.0;
        }

        // 干路（延迟对齐后取）
        let dry = self.dry[self.dry_pos];
        self.dry[self.dry_pos] = x;
        self.dry_pos = (self.dry_pos + 1) % self.dry_len;

        // true-bypass：向目标靠拢（5 ms 交叉淡化），电路仍在运行
        self.mix += (self.mix_target - self.mix) * self.mix_coef;
        // 指数淡化有无穷长的尾巴；到位后吸附，避免"永远差一点"
        if (self.mix - self.mix_target).abs() < 1e-5 {
            self.mix = self.mix_target;
        }
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

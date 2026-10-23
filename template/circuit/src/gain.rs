// SPDX-License-Identifier: GPL-3.0-or-later

//! 示例电路：**纯增益**（直通 × 增益）。
//!
//! 它存在的意义是让模板"能跑、能听、能测"：把这里换成你的真实电路即可。
//! 真正的电路实现请参考 `plugins/sparklin-drive/circuit/src/fakedist.rs`
//! （运放宏模型 + 反馈环内二极管削波 + 一维牛顿）。

use rustomp_dsp::Circuit;

/// 电路参数。
#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// 增益（线性），0..=2
    pub gain: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

/// 纯增益电路。
pub struct Gain {
    /// 采样率（这里用不到，但真实电路通常需要）
    pub sample_rate: f64,
    params: Params,
}

impl Gain {
    pub fn params(&self) -> Params {
        self.params
    }
}

impl Circuit for Gain {
    type Params = Params;

    fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            params: Params::default(),
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
    }

    fn set_params(&mut self, params: Self::Params) {
        self.params = params;
    }

    fn reset(&mut self) {}

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        // TODO: 换成你的电路求解
        x * self.params.gain
    }
}

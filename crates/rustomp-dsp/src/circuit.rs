// SPDX-License-Identifier: GPL-3.0-or-later

//! 电路与框架之间**唯一的契约**。
//!
//! 框架（过采样、bypass、削波指示包络、NaN 熔断）只依赖这个 trait，
//! 对具体电路一无所知 —— 这就是"框架无状态"的落点。
//!
//! 插件项目里每个电路实现一次 `Circuit`（例如 `fakedist.rs`），
//! 框架就可以把它套进 `Engine<C>` 直接产出可用的效果器。

/// 一个可实时运行的电路实现。
pub trait Circuit {
    /// 电路自己的参数类型（电位器位置、开关状态等）。
    type Params: Copy + Default;

    /// 以 `sample_rate` 构造（此时通常是过采样后的速率）。
    fn new(sample_rate: f64) -> Self
    where
        Self: Sized;

    /// 采样率变化（必须重置内部状态）。
    fn set_sample_rate(&mut self, sample_rate: f64);

    /// 更新参数。**必须足够便宜**（插件会逐样本调用）。
    fn set_params(&mut self, params: Self::Params);

    /// 回到直流工作点（避免加载/旁通切换时的爆音）。
    fn reset(&mut self);

    /// 处理一个样本。
    fn process(&mut self, x: f64) -> f64;

    /// 活动指示的**瞬时**值（0..1），例如削波强度。`Engine` 会做释放包络。
    fn activity(&self) -> f32 {
        0.0
    }
}

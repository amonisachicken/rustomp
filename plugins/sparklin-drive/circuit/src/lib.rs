// SPDX-License-Identifier: GPL-3.0-or-later

//! Sparklin Drive 的电路实现（fakedist v1）。
//!
//! 这里只包含**这个插件专属**的东西：元件值、拓扑、求解方式。
//! 通用机制（过采样、bypass、指示包络、数值熔断）都在 `rustomp-dsp` 框架里。

pub mod fakedist;

pub use fakedist::{ComponentValues, Fakedist, OpAmpModel, Params, Probe};

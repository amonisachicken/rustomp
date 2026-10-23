// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 框架的 DSP 核心：**与具体电路无关**的通用机制。
//!
//! - [`Circuit`]：电路实现的唯一契约（框架对具体电路一无所知）
//! - [`Engine`]：4x 过采样 + true-bypass + 活动指示 + 数值熔断
//! - [`devices`]：元件库（闭集器件模型；当前有二极管，BJT/JFET/MOS/运放待补）
//! - [`Oversampler4x`]：多相 FIR 过采样
//!
//! 具体电路（例如 Sparklin Drive 的 `fakedist`）住在各自的插件项目里。

pub mod circuit;
pub mod devices;
pub mod engine;
pub mod oversample;

pub use circuit::Circuit;
pub use devices::{Diode, D_1N34A, D_1N4007, D_1N4148};
pub use engine::Engine;
pub use oversample::Oversampler4x;

// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 框架的 DSP 核心：**与具体电路无关**的通用机制。
//!
//! - [`Circuit`]：电路实现的唯一契约（框架对具体电路一无所知）
//! - [`Branch`]：把两份完整电路打包成一个「开关可切换」的电路（拓扑切换）
//! - [`Engine`]：4x 过采样 + true-bypass + 活动指示 + 数值熔断
//! - [`devices`]：元件库（闭集器件模型：二极管/BJT/JFET/MOSFET/运放）
//! - [`mna`]：通用 MNA 装配器（器件 + 拓扑 → 可求解电路）
//! - [`Oversampler4x`]：多相 FIR 过采样
//!
//! 具体电路（例如 Sparklin Drive 的 `fakedist`）住在各自的插件项目里。

pub mod branch;
pub mod circuit;
pub mod devices;
pub mod engine;
pub mod mna;
pub mod oversample;

pub use branch::{Branch, BranchParams};
pub use circuit::Circuit;
pub use devices::{
    Bjt, Diode, Jfet, Nmos, OpAmp, OpAmpModel, OpAmpState, AC127, AC128, BC549C, BC559C, D_1N34A,
    D_1N4007, D_1N4148, D_1N5817, J201, LED_BLUE, LED_GREEN, LED_RED, NMOS_2N7000,
    NMOS_2N7000_BODY, OP07,
};
pub use engine::Engine;
pub use mna::{Element, Mna, Netlist, GND};
pub use oversample::Oversampler4x;

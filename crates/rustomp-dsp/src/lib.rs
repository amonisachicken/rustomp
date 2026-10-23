// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 的 DSP 核心：电路建模（wave-digital / 状态变量 + 牛顿法）。
//!
//! 设计原则（见 DEVLOG ADR-001）：
//! - 线性储能元件用梯形法伴随模型（与 WDF 单端口等价）
//! - 非线性节点用**一维**牛顿法（未知量必须尽量少）
//! - 运放用单极点宏模型，环内**不留一采样延迟**
//! - 音频线程零分配、状态可 `reset()` 到直流工作点

pub mod diode;
pub mod fakedist;
pub mod oversample;
pub mod engine;

pub use diode::Diode1n4148;
pub use fakedist::{ComponentValues, Fakedist, OpAmpModel, Params, Probe};
pub use oversample::Oversampler4x;
pub use engine::Engine;

// SPDX-License-Identifier: GPL-3.0-or-later

//! __NAME__ 的电路实现。
//!
//! **这是本插件唯一需要写硬功夫的地方。**
//! 通用机制（过采样、bypass、指示包络、数值熔断）都在 `rustomp-dsp` 框架里，
//! 你只需要实现 [`rustomp_dsp::Circuit`]。
//!
//! 上手路线：
//! 1. 把电路写成网表（`new-plugin.sh` 已建好 `circuit/netlist/` 目录），
//!    用 `scripts/make-netlist.sh` + `scripts/sim-baseline.sh` 拿到 ngspice 基线；
//! 2. 线性部分用梯形伴随模型（`i[n] = (2C/T)(v[n]−v[n−1]) − i[n−1]`）；
//!    非线性部分用一维牛顿（参考 `sparklin-drive-legacy` 仓库的 `circuit/src/fakedist.rs`）；
//! 3. 写一个 `tests/` 对拍测试，把 h2–h9 锁住。

pub mod gain;

pub use gain::{Gain, Params};

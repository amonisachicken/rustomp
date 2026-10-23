// SPDX-License-Identifier: GPL-3.0-or-later

//! 元件库：**闭集器件模型**（型号固定，参数与 SPICE 对齐）。
//!
//! 每个器件只负责"给定端电压 → 电流与电导"，**不含拓扑、不含求解器**。
//! 拓扑由电路实现（`Circuit`）或以后的 MNA 装配器负责。
//!
//! 精度档位见 `docs/component-library.md`：
//! 静态 `D1..D3`、动态 `C0..C2`。当前进度：
//!
//! | 器件 | 状态 |
//! |---|---|
//! | 二极管 `1N4148` / `1N34A` / `1N4007` | ✅ 静态完整（Is/N/Rs） |
//! | BJT `BC549C` / `AC127` / `AC128` | ✅ 静态完整（Gummel-Poon D3，含 Rb/Re/Rc） |
//! | JFET `J201` | ✅ 静态完整（Shichman-Hodges D3，含 Rd/Rs） |
//! | MOSFET `2N7000` | ⬜（SPICE 侧已手写子电路并标定） |
//! | 运放（通用） | ⬜（参数已定，实现随 MNA） |

pub mod bjt;
pub mod diode;
pub mod jfet;

pub use bjt::{Bjt, AC127, AC128, BC549C};
pub use jfet::{Jfet, J201};
pub use diode::{Diode, D_1N34A, D_1N4007, D_1N4148};

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
//! | 二极管 `1N4148` / `1N34A` / `1N4007` / `1N5817`(肖特基) | ✅ 静态完整（Is/N/Rs） |
//! | LED `LED_RED` / `LED_GREEN` / `LED_BLUE` | ✅ 静态完整（N=3.73/4.61/7.47，Vf@20mA ≈ 1.85/2.29/3.71 V） |
//! | BJT `BC549C` / `AC127` / `AC128` / `BC559C`(PNP) | ✅ 静态完整（Gummel-Poon D3，含 Rb/Re/Rc） |
//! | JFET `J201` | ✅ 静态完整（Shichman-Hodges D3，含 Rd/Rs） |
//! | MOSFET `2N7000` | ✅ 静态完整（level-1 D3，含 Rd/Rs；体二极管单独接） |
//! | 运放 `OP07`（通用单运放） | ✅ 参数 + 单极点状态（随 MNA 使用） |

pub mod bjt;
pub mod diode;
pub mod jfet;
pub mod mosfet;
pub mod opamp;

pub use bjt::{Bjt, AC127, AC128, BC549C, BC559C};
pub use diode::{Diode, D_1N34A, D_1N4007, D_1N4148, D_1N5817, LED_BLUE, LED_GREEN, LED_RED};
pub use jfet::{Jfet, J201};
pub use mosfet::{Nmos, NMOS_2N7000, NMOS_2N7000_BODY};
pub use opamp::{OpAmp, OpAmpModel, OpAmpState, OP07};

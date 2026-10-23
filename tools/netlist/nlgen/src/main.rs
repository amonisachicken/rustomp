// SPDX-License-Identifier: GPL-3.0-or-later
//!
//! 把 breadspice 的 `Circuit` JSON 编译成 ngspice 网表。
//!
//! 复用 breadspice 后端的 `build_netlist`，而不是自己重写一遍 —— 电位器展开成
//! 两段电阻、运放按引脚名排序、模型库注入这些细节都由上游保证。
//!
//! 用法：
//!   nlgen <circuit.json> > netlist.cir

use breadboard_backend::domain::Circuit;
use breadboard_backend::netlist::build_netlist;

fn main() {
    let path = std::env::args().nth(1).expect("用法: nlgen <circuit.json>");
    let text = std::fs::read_to_string(&path).expect("读不到 circuit json");
    let circuit: Circuit = serde_json::from_str(&text).expect("Circuit JSON 解析失败");
    let netlist = build_netlist(&circuit);
    print!("{}", netlist.text);
    eprintln!("器件 {} 个", netlist.devices.len());
}

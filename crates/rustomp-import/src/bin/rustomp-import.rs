// SPDX-License-Identifier: GPL-3.0-or-later
//! 外部电路导入 CLI。
//!
//! 用法：
//!   rustomp-import <input.bread>                  # 只打印摘要
//!   rustomp-import <input> --spice out.cir        # 输出 SPICE 元件行
//!   rustomp-import <input> --skeleton out.rs      # 输出 Rust 电路骨架
//!   rustomp-import <input> --json out.json        # 输出中间表示

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: {} <input> [--spice f] [--skeleton f] [--json f]", args[0]);
        std::process::exit(1);
    }
    let input = PathBuf::from(&args[1]);
    let mut spice: Option<PathBuf> = None;
    let mut skeleton: Option<PathBuf> = None;
    let mut json: Option<PathBuf> = None;
    let mut i = 2;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--spice" => spice = Some(PathBuf::from(&args[i + 1])),
            "--skeleton" => skeleton = Some(PathBuf::from(&args[i + 1])),
            "--json" => json = Some(PathBuf::from(&args[i + 1])),
            other => {
                eprintln!("未知参数 {other}");
                std::process::exit(1);
            }
        }
        i += 2;
    }

    let circuit = match rustomp_import::import_file(&input) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("导入失败: {e}");
            std::process::exit(2);
        }
    };

    println!("已导入 {}（格式 {}）", input.display(), circuit.format);
    println!("  元件 {} 个：{}", circuit.elements.len(), circuit.summary());
    if !circuit.ground_nets.is_empty() {
        println!("  接地网：{}", circuit.ground_nets.join(", "));
    }
    for n in &circuit.notes {
        println!("  注意：{n}");
    }

    if let Some(p) = spice {
        std::fs::write(&p, circuit.to_spice()).expect("写 SPICE 失败");
        println!("  → SPICE 元件行：{}", p.display());
    }
    if let Some(p) = skeleton {
        std::fs::write(&p, circuit.to_skeleton()).expect("写骨架失败");
        println!("  → Rust 骨架：{}", p.display());
    }
    if let Some(p) = json {
        std::fs::write(&p, circuit.to_json()).expect("写 JSON 失败");
        println!("  → 中间表示：{}", p.display());
    }
}

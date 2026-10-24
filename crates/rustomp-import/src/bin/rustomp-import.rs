// SPDX-License-Identifier: GPL-3.0-or-later
//! 外部电路导入 CLI。
//!
//! 用法：
//!   rustomp-import <input.bread>                  # 只打印摘要
//!   rustomp-import <input> --spice out.cir        # 输出 SPICE 元件行
//!   rustomp-import <input> --skeleton out.rs      # 输出 Rust 电路骨架
//!   rustomp-import <input> --json out.json        # 输出中间表示
//!   rustomp-import netlist.cir --mna out.rs       # 生成 MNA 装配代码（S1a）
//!   rustomp-import netlist.cir --mna out.rs --check   # 只比对，不写入（CI 防漂移）

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
    let mut mna: Option<PathBuf> = None;
    let mut check = false;
    let mut output_node: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--check" {
            check = true;
            i += 1;
            continue;
        }
        if i + 1 >= args.len() {
            break;
        }
        match args[i].as_str() {
            "--spice" => spice = Some(PathBuf::from(&args[i + 1])),
            "--skeleton" => skeleton = Some(PathBuf::from(&args[i + 1])),
            "--json" => json = Some(PathBuf::from(&args[i + 1])),
            "--mna" => mna = Some(PathBuf::from(&args[i + 1])),
            "--output-node" => output_node = Some(args[i + 1].clone()),
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
    if let Some(p) = mna {
        let opts = rustomp_import::mna_codegen::Options {
            input: None,
            output: output_node,
            // 只写文件名：生成物的头部要稳定（不随调用时的相对路径变化）
            source: input
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("网表")
                .to_string(),
        };
        match rustomp_import::mna_codegen::generate(&circuit, &opts) {
            Ok(gen) => {
                for n in &gen.notes {
                    println!("  · {n}");
                }
                println!("  → MNA 代码（{} 个未知量）：{}", gen.unknowns, p.display());
                if check {
                    let old = std::fs::read_to_string(&p).unwrap_or_default();
                    if old != gen.code {
                        eprintln!("  ✗ 生成物与 {} 不一致（请重新生成）", p.display());
                        std::process::exit(3);
                    }
                    println!("  ✓ 生成物是最新的");
                } else {
                    std::fs::write(&p, gen.code).expect("写 MNA 代码失败");
                }
            }
            Err(e) => {
                eprintln!("生成 MNA 代码失败：{e}");
                std::process::exit(2);
            }
        }
    }
    if let Some(p) = json {
        std::fs::write(&p, circuit.to_json()).expect("写 JSON 失败");
        println!("  → 中间表示：{}", p.display());
    }
}

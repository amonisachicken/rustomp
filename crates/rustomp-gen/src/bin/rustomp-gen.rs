// SPDX-License-Identifier: GPL-3.0-or-later
//! `rustomp-gen` —— 从 `rustomp.toml` 生成插件侧代码。
//!
//! 用法：
//!   rustomp-gen <项目目录>            # 生成到 <项目>/plugin/src/generated/
//!   rustomp-gen <项目目录> --check    # 只比对，不写入（CI 防漂移）

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: {} <项目目录> [--check]", args[0]);
        std::process::exit(2);
    }
    let project = PathBuf::from(&args[1]);
    let check = args[2..].iter().any(|a| a == "--check");

    match rustomp_gen::generate(&project, check) {
        Ok(report) => {
            if check {
                if report.is_drift() {
                    eprintln!("以下生成物与 rustomp.toml 不一致：");
                    for p in &report.drifted {
                        eprintln!("  ✗ {}", p.display());
                    }
                    eprintln!(
                        "重新生成：cargo run -q -p rustomp-gen -- {}",
                        project.display()
                    );
                    std::process::exit(3);
                }
                println!("生成物是最新的（{} 个文件）", report.unchanged.len());
            } else {
                for p in &report.written {
                    println!("  → {}", p.display());
                }
                println!(
                    "已写入 {} 个文件（{} 个未变）",
                    report.written.len(),
                    report.unchanged.len()
                );
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}

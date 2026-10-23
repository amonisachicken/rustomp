// SPDX-License-Identifier: GPL-3.0-or-later
//
// 从插件项目的 rustomp.toml 生成 identity.rs（名字 / CLAP id / VST3 id / 窗口尺寸），
// 让插件身份只有**一个来源**，框架侧完全无状态。

use std::process::Command;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let toml = format!("{manifest}/../rustomp.toml");
    let tool = format!("{manifest}/../../../tools/meta/plugin_meta.py");
    println!("cargo:rerun-if-changed={toml}");
    println!("cargo:rerun-if-changed={tool}");

    // 变体由 cargo feature 决定
    let variant = if std::env::var("CARGO_FEATURE_MONO").is_ok() {
        "mono"
    } else if std::env::var("CARGO_FEATURE_STEREO").is_ok() {
        "stereo"
    } else {
        "dual"
    };

    let out = Command::new("python3")
        .arg(&tool)
        .arg(&toml)
        .arg("--rust")
        .arg("--variant")
        .arg(variant)
        .output()
        .expect("运行 tools/meta/plugin_meta.py 失败（需要 python3 ≥3.11）");
    if !out.status.success() {
        panic!(
            "plugin_meta.py 失败：\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    std::fs::write(format!("{dir}/identity.rs"), &out.stdout).expect("写入 identity.rs 失败");
}

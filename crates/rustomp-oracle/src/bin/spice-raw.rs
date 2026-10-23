// SPDX-License-Identifier: GPL-3.0-or-later
//! 查看 ngspice ASCII rawfile 的每个变量统计（替代原 tools/oracle/spice_raw.py）。
//!
//! 用法：
//!   cargo run -p rustomp-oracle --bin spice-raw -- out.raw [变量名]

use rustomp_oracle::RawFile;

fn stats(xs: &[f64]) -> (f64, f64, f64, f64, f64) {
    let n = xs.len() as f64;
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    let mut sum = 0.0;
    let mut sq = 0.0;
    for v in xs {
        mn = mn.min(*v);
        mx = mx.max(*v);
        sum += *v;
        sq += *v * *v;
    }
    let mean = sum / n;
    let rms = (sq / n).sqrt();
    (mn, mx, mx - mn, mean, rms)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: {} <rawfile> [变量名]", args[0]);
        std::process::exit(1);
    }
    let text = std::fs::read_to_string(&args[1]).expect("读不到 rawfile");
    let raw = RawFile::parse(&text).expect("解析 rawfile 失败");
    let only = args.get(2).map(|s| s.to_lowercase());

    println!(
        "变量 {} 个，点数 {}，采样率 {:.0} Hz",
        raw.vars.len(),
        raw.len(),
        raw.sample_rate()
    );
    for name in &raw.vars {
        if let Some(o) = &only {
            if name != o {
                continue;
            }
        }
        let xs = &raw.data[name];
        let (mn, mx, pp, mean, rms) = stats(xs);
        println!(
            "  {name:16} min={mn:+.5e} max={mx:+.5e} pp={pp:.5e} mean={mean:+.5e} rms={rms:.5e}"
        );
    }
}

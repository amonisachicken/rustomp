// SPDX-License-Identifier: GPL-3.0-or-later
//!
//! **谁更接近 SPICE？** 用**同一条带限方波**（奇次谐波 1…19 求和，音频带内），
//! 分别喂给 ngspice 与我们的 MNA，逐次比较偶次谐波 h2…h22。
//!
//! 为什么不用"理想方波"：理想方波（1 ns 边沿）在 ngspice 里根本采不到（步长 1 µs），
//! 而我们的电路在 192 kHz 下能看到 96 kHz 以内的全部内容 —— 两边激励不等价，
//! 比出来的是激励差异，不是模型差异。（第一版就踩了这个坑。）

include!("../tests/generated/sparklin_drive.rs");

use rustomp_oracle::Oracle;
use std::f64::consts::TAU;

const FS: f64 = 192_000.0;
const F0: f64 = 1_000.0;
const AMP: f64 = 0.2;
/// 带限方波保留的奇次谐波上限（1,3,…,19）
const MAX_ODD: usize = 19;

/// 带限方波：Σ_{奇次 k≤MAX_ODD} (4A/(πk))·sin(2πkf₀t)
fn drive(t: f64) -> f64 {
    let mut s = 0.0;
    let mut k = 1;
    while k <= MAX_ODD {
        s += (4.0 * AMP / (std::f64::consts::PI * k as f64)) * (TAU * F0 * k as f64 * t).sin();
        k += 2;
    }
    s
}

fn mag(x: &[f64], f: f64) -> f64 {
    let n = x.len();
    let mean = x.iter().sum::<f64>() / n as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (k, v) in x.iter().enumerate() {
        let a = TAU * f * k as f64 / FS;
        re += (v - mean) * a.cos();
        im -= (v - mean) * a.sin();
    }
    2.0 * (re * re + im * im).sqrt() / n as f64
}

fn main() {
    let dt = 1.0 / FS;
    let warm = (0.4 * FS) as usize;
    let n = (0.2 * FS) as usize;

    // ---- 我们的电路：不同迭代次数各跑一遍（看差异是不是"截断"造成的）----
    let run_ours = |iters: usize| -> Vec<f64> {
        let mut net = build(FS, &Params::default());
        net.solve_dc();
        let mut out = Vec::with_capacity(n);
        for i in 0..(warm + n) {
            net.set_fixed(FIXED_IN, drive(i as f64 / FS));
            let v = net.step_fixed(dt, iters);
            if i >= warm {
                out.push(v[OUT]);
            }
        }
        out
    };
    let ours = run_ours(NEWTON_ITERS);
    let ours8 = run_ours(8);
    let ours16 = run_ours(16);

    // ---- ngspice：同一波形用"串联正弦源"精确合成（不依赖边沿分辨率）----
    let Some(oracle) = Oracle::discover_or_skip() else { return };
    let fixture = include_str!("../../rustomp-import/tests/fixtures/sparklin_drive.cir");
    // 串联正弦源：0 → s1 → s3 → … → s19，最后用 0 V 源接到输入节点
    let mut sources = String::new();
    let mut prev = "0".to_string();
    let mut k = 1;
    while k <= MAX_ODD {
        let amp = 4.0 * AMP / (std::f64::consts::PI * k as f64);
        let cur = format!("s{k}");
        sources.push_str(&format!(
            "V{k} {cur} {prev} SIN(0 {amp} {} 0 0 0)\n",
            F0 * k as f64
        ));
        prev = cur;
        k += 2;
    }
    // 把原来的 VS1（输入端口）换成串联正弦源，源链末端接到 n_t21L
    let body = fixture
        .replace(
            "VS1 n_t21L 0 DC 0 AC 0.2 SIN(0 0.2 1k 0 0 0)",
            &format!("{sources}VDC n_t21L {prev} DC 0"),
        )
        .replace(".end", ".tran 1u 0.6 0.4\n.end");
    let raw = oracle.run_netlist(&body).expect("ngspice 运行失败");
    let spice = raw.resample("v(n_t27r)", dt).expect("resample 失败");
    let spice = &spice[..n.min(spice.len())];

    let f_ours = mag(&ours, F0);
    let f_spice = mag(spice, F0);
    println!("\n=== 带限方波（1 kHz，奇次 1–19）：偶次谐波（dB 相对基频）===");
    println!("{:>4}{:>8}{:>10}{:>9}{:>9}{:>9}", "h", "f/Hz", "ngspice", "4次", "8次", "16次");
    let mut worst = 0.0f64;
    let mut worst_h = 0;
    let f8 = mag(&ours8, F0);
    let f16 = mag(&ours16, F0);
    for k in (2..=22).step_by(2) {
        let d = |x: &[f64], fref: f64| 20.0 * (mag(x, F0 * k as f64) / fref).max(1e-15).log10();
        let b = 20.0 * (mag(spice, F0 * k as f64) / f_spice).max(1e-15).log10();
        let (a4, a8, a16) = (d(&ours, f_ours), d(&ours8, f8), d(&ours16, f16));
        if b > -80.0 && (a4 - b).abs() > worst {
            worst = (a4 - b).abs();
            worst_h = k;
        }
        println!("{k:>4}{:>8}{b:>10.1}{a4:>9.1}{a8:>9.1}{a16:>9.1}", (F0 * k as f64) as i32);
    }
    println!("\n偶次最大偏差 {worst:.1} dB（h{worst_h}，>−80 dB 的分量）");
    println!(
        "基频：ngspice {f_spice:.5}  我们(4/8/16 次) {f_ours:.5} / {f8:.5} / {f16:.5}（4 次差 {:+.3} dB）",
        20.0 * (f_ours / f_spice).log10()
    );
}

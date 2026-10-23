// 过采样倍数够不够？在 48 kHz 宿主下，用 1x / 2x / 4x / 8x 分别跑电路，比较
//   输出基波幅度 与 THD(2..9)
// 结论（见 DEVLOG E11）：**1x 直接跑 48k 电路会失效**（隐含求解欠解析），
// 2x 在 8 kHz 输入下已经开始失真，4x 起稳定。
use rustomp_dsp::{Engine, Fakedist, Params};
use std::f64::consts::TAU;

fn measure(fs_host: f64, os: usize, f0: f64, amp: f64) -> (f64, f64) {
    let fs = fs_host * os as f64;
    let t = 1.0 / fs;
    // os == 1 时直接跑电路；否则走 Engine（内部固定 4x）——这里为通用性手动实现
    let mut circuit = Fakedist::new(fs);
    circuit.set_params(Params::default());
    let mut up = [0.0f64; 8];
    let mut phase: f64 = 0.0;
    let mut os_state = rustomp_dsp::Oversampler4x::new();
    let _ = (&mut up, &mut os_state);
    // 为了比较不同 os，直接以 fs 跑电路（Engine 固定 4x，不适合这里）
    let mut y_last = 0.0;
    let warm = (3.0 * fs) as usize;
    for _ in 0..warm {
        y_last = circuit.process(amp * phase.sin());
        phase += TAU * f0 * t;
    }
    let n = ((fs / f0).round() as usize) * 8;
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n {
        buf.push(circuit.process(amp * phase.sin()));
        phase += TAU * f0 * t;
    }
    let mean = buf.iter().sum::<f64>() / n as f64;
    let mag = |f: f64| -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (k, v) in buf.iter().enumerate() {
            let a = TAU * f * k as f64 * t;
            re += (v - mean) * a.cos();
            im -= (v - mean) * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / n as f64
    };
    let fund = mag(f0);
    let sq: f64 = (2..=9).map(|h| mag(f0 * h as f64).powi(2)).sum();
    let _ = y_last;
    (fund, sq.sqrt() / fund * 100.0)
}

fn main() {
    let fs_host = 48_000.0;
    println!("48 kHz 宿主，输入峰值 0.2 V\n");
    println!(
        "{:<8} {:<8} {:>12} {:>12} {:>10}",
        "过采样", "电路fs", "基波幅度", "THD(2..9)%", "状态"
    );
    for os in [1usize, 2, 4, 8] {
        for f0 in [1000.0f64, 3000.0, 8000.0] {
            let (fund, thd) = measure(fs_host, os, f0, 0.2);
            let status = if fund < 0.05 {
                "失效"
            } else if (fund - 0.48).abs() > 0.06 {
                "偏低"
            } else {
                "ok"
            };
            println!(
                "{:<8} {:<8} {:>12.4} {:>12.2} {:>10}   (f0={} Hz)",
                format!("{os}x"),
                format!("{:.0}k", fs_host * os as f64 / 1000.0),
                fund,
                thd,
                status,
                f0
            );
        }
        println!();
    }
}

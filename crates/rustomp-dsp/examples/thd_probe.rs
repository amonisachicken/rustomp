// 用多采样率跑 THD，看采样率收敛性
use rustomp_dsp::{Fakedist, Params};
use std::f64::consts::TAU;

fn thd(fs: f64, amp: f64, f0: f64, nh: usize) -> (f64, f64, Vec<f64>) {
    let mut d = Fakedist::new(fs);
    d.set_params(Params::default());
    let t = 1.0 / fs;
    let mut ph: f64 = 0.0;
    for _ in 0..(3.0 * fs) as usize { d.process(amp * ph.sin()); ph += TAU * f0 * t; }
    let n = (fs / f0).round() as usize;
    let mut buf = Vec::with_capacity(n);
    for _ in 0..n { buf.push(d.process(amp * ph.sin())); ph += TAU * f0 * t; }
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
    let hs: Vec<f64> = (2..=nh).map(|h| mag(f0 * h as f64) / fund * 100.0).collect();
    let sq9: f64 = hs.iter().take(8).map(|x| x * x).sum();
    let sq: f64 = hs.iter().map(|x| x * x).sum();
    let _ = sq;
    (sq9.sqrt(), fund, hs)
}

fn main() {
    for fs in [48_000.0, 96_000.0, 192_000.0, 768_000.0] {
        for amp in [0.2, 1e-3] {
            let (thd, fund, hs) = thd(fs, amp, 1000.0, 20);
            println!("fs={:>9.0} amp={:<7} THD(2..9)={:>7.3}%  基波={:.4}V  h2={:.2}% h3={:.2}% h5={:.2}% h7={:.2}% h9={:.2}%",
                fs, amp, thd, fund, hs[0], hs[1], hs[3], hs[5], hs[7]);
        }
    }
}

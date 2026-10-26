// SPDX-License-Identifier: GPL-3.0-or-later
//!
//! **AB 台架（Rust FFT 版）**：把 1 kHz 正弦/方波喂给 `Engine<C>`，用 rustfft 出谱。
//!
//! 输出一行一个 JSON 对象（供 `scripts/abfft_compare.py` 合并对比）。
//! 参数固定：Gain = Level = 0.5，幅度 0.2。
//!
//! 采集：3 s 预热 + 1 s（48000 点 = 1000 个整周期 → 矩形窗零泄漏，频率分辨率 1 Hz）。

use rustomp_dsp::{Circuit, Engine};
use rustfft::{num_complex::Complex, FftPlanner};

// 用入库的 S1b 夹具（与 Sparklin Drive 插件里的电路同一份生成代码）
include!("../tests/generated/sparklin_drive.rs");

const FS: f64 = 48_000.0;
const F0: f64 = 1_000.0;
const AMP: f64 = 0.2;
const PLUGIN_TAG: &str = "v2-mna";

#[derive(Clone, Copy, PartialEq)]
enum Sig {
    Sine,
    Square,
}

impl Sig {
    fn name(self) -> &'static str {
        match self {
            Sig::Sine => "sine",
            Sig::Square => "square",
        }
    }
    fn value(self, t: f64) -> f64 {
        match self {
            Sig::Sine => AMP * (std::f64::consts::TAU * F0 * t).sin(),
            Sig::Square => {
                if (t * F0).fract() < 0.5 {
                    AMP
                } else {
                    -AMP
                }
            }
        }
    }
}

fn capture(sig: Sig) -> Vec<f64> {
    let mut e = Engine::<Ckt>::new(FS);
    let mut p = Params::default();
    p.gain = 0.5;
    p.level = 0.5;
    e.set_params(p);
    let warm = (3.0 * FS) as usize;
    let n = FS as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..(warm + n) {
        let y = e.process(sig.value(i as f64 / FS));
        if i >= warm {
            out.push(y);
        }
    }
    out
}

fn db(v: f64, reference: f64) -> f64 {
    20.0 * (v / reference).max(1e-12).log10()
}

fn analyse(plugin: &str, sig: Sig, out: &[f64]) {
    let n = out.len();
    let mut buf: Vec<Complex<f64>> = out.iter().map(|v| Complex::new(*v, 0.0)).collect();
    FftPlanner::new().plan_fft_forward(n).process(&mut buf);
    let mag: Vec<f64> = (0..=n / 2).map(|k| 2.0 * buf[k].norm() / n as f64).collect();
    let bin_of = |f: f64| (f / (FS / n as f64)).round() as usize;
    let fund = mag[bin_of(F0)];

    let rms = (out.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
    let dc = out.iter().sum::<f64>() / n as f64;

    // h2..h23（dB 相对基频）
    let h: Vec<String> = (2..=23)
        .map(|k| format!("{:.2}", db(mag[bin_of(F0 * k as f64)], fund)))
        .collect();
    // 极低频 20..800 Hz 的最大值
    let df = FS / n as f64;
    let (mut lf_max, mut lf_hz) = (0.0f64, 0.0f64);
    for k in (20.0 / df) as usize..=(800.0 / df) as usize {
        if mag[k] > lf_max {
            lf_max = mag[k];
            lf_hz = k as f64 * df;
        }
    }
    // 分频段能量（dB 相对总 RMS）
    let bands = [
        (0.0, 100.0),
        (100.0, 300.0),
        (300.0, 1000.0),
        (1000.0, 3000.0),
        (3000.0, 6000.0),
        (6000.0, 10000.0),
        (10000.0, 16000.0),
        (16000.0, 24000.0),
    ];
    let mut b = Vec::new();
    for (f1, f2) in bands {
        let k1 = (f1 / df).ceil() as usize;
        let k2 = ((f2 / df).floor() as usize).min(mag.len() - 1);
        let e: f64 = (k1..=k2).map(|k| mag[k] * mag[k]).sum::<f64>().sqrt();
        b.push(format!("{:.2}", db(e, rms)));
    }
    let thd = 100.0
        * (h.iter()
            .map(|s| s.parse::<f64>().unwrap())
            .map(|d| 10f64.powf(d / 10.0))
            .sum::<f64>())
        .sqrt();

    println!(
        "{{\"plugin\":\"{plugin}\",\"signal\":\"{}\",\"rms\":{:.5},\"dc\":{:.6},\"fund\":{:.5},\"thd_pct\":{:.3},\"lf_max_db\":{:.2},\"lf_max_hz\":{:.0},\"h2_23\":[{}],\"bands_db\":[{}]}}",
        sig.name(),
        rms,
        dc,
        fund,
        thd,
        db(lf_max, fund),
        lf_hz,
        h.join(","),
        b.join(",")
    );
}

fn main() {
    for sig in [Sig::Sine, Sig::Square] {
        let out = capture(sig);
        analyse("v2-mna", sig, &out);
    }

    // 可选：把采集到的 1 s 输出 dump 成 raw f64（供 `abeven` 做偶次谐波对比）
    if let Ok(dir) = std::env::var("AB_DUMP_DIR") {
        for sig in [Sig::Sine, Sig::Square] {
            let out = capture(sig);
            let name = format!(
                "{dir}/{}_{}.f64",
                PLUGIN_TAG,
                if sig == Sig::Sine { "sine" } else { "square" }
            );
            let mut bytes = Vec::with_capacity(out.len() * 8);
            for v in &out {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            std::fs::write(&name, bytes).expect("dump 失败");
            eprintln!("dumped {} ({} 点)", name, out.len());
        }
    }
}

// CPU 开销：48 kHz、立体声（2 × Engine）、4x 过采样
use rustomp_dsp::{Engine, Params};
use std::f64::consts::TAU;
use std::time::Instant;

fn main() {
    let fs = 48_000.0;
    let secs = 10.0;
    let n = (fs * secs) as usize;
    let mut e = [Engine::new(fs), Engine::new(fs)];
    for x in e.iter_mut() {
        x.set_params(Params::default());
    }
    let mut ph: f64 = 0.0;
    let step = TAU * 220.0 / fs;
    let mut acc = 0.0f64;
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let t0 = Instant::now();
        for _ in 0..n {
            let s = 0.2 * ph.sin();
            ph += step;
            acc += e[0].process(s) + e[1].process(s);
        }
        best = best.min(t0.elapsed().as_secs_f64());
    }
    let ns = best / n as f64 * 1e9;
    println!(
        "立体声 48kHz + 4x 过采样：{:.0} ns / 帧（两个声道），单核占用 ≈ {:.1} %（校验和 {:.3}）",
        ns,
        ns * fs / 1e7,
        acc
    );
}

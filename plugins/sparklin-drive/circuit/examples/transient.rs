// 瞬态/大信号压力测试：复现"突然自激"
use rustomp_dsp::Engine;
use sparklin_drive_circuit::{Fakedist, Params};
use std::f64::consts::TAU;

fn zero_cross_rate(y: &[f64]) -> f64 {
    let mut z = 0;
    for w in y.windows(2) { if (w[0] >= 0.0) != (w[1] >= 0.0) { z += 1; } }
    z as f64 / y.len() as f64
}

fn run(name: &str, fs: f64, gain: f64, gen: impl Fn(usize, f64) -> f64, secs: f64) {
    let mut e = Engine::<Fakedist>::new(fs);
    e.set_params(Params { gain, level: 0.5, ..Params::default() });
    let t = 1.0 / fs;
    let n = (secs * fs) as usize;
    let mut y = Vec::with_capacity(n);
    let mut bad = false;
    for i in 0..n {
        let out = e.process(gen(i, i as f64 * t));
        if !out.is_finite() { bad = true; break; }
        y.push(out);
    }
    let tail = &y[y.len().saturating_sub((0.2 * fs) as usize)..];
    let tail_peak = tail.iter().fold(0.0f64, |a, b| a.max(b.abs()));
    let tail_zcr = zero_cross_rate(tail);
    let peak = y.iter().fold(0.0f64, |a, b| a.max(b.abs()));
    let latch = tail_peak > 0.5 && tail_zcr > 0.15; // 尾段仍有大幅且高频 ⇒ 自激
    println!(
        "{:<28} peak={:>8.3} 尾段峰值={:>8.3} 尾段过零率={:>6.3} {}",
        name, peak, tail_peak, tail_zcr,
        if bad { "NaN!" } else if latch { "← 自激!" } else { "ok" }
    );
}

fn main() {
    let fs = 48_000.0;
    // 1) 直流阶跃（模拟插拔/大瞬态）
    run("DC 阶跃 3V @1s", fs, 0.5, |_, t| if t > 1.0 && t < 2.0 { 3.0 } else { 0.0 }, 4.0);
    // 2) 方波（瞬态极丰富）
    run("方波 220Hz 2V", fs, 0.5, |_, t| if (t * 220.0).fract() < 0.5 { 2.0 } else { -2.0 }, 4.0);
    // 3) 脉冲（拨弦冲击）
    run("脉冲 5V 1ms @1s", fs, 0.5, |_, t| if (1.0..1.001).contains(&t) { 5.0 } else { 0.0 }, 4.0);
    // 4) 静音→大声→静音
    run("噪声突发 2V", fs, 0.5, |i, t| {
        if (1.0..2.0).contains(&t) {
            let mut s = i as u64;
            s ^= s << 13; s ^= s >> 7; s ^= s << 17;
            ((s % 2000) as f64 / 1000.0 - 1.0) * 2.0
        } else { 0.0 }
    }, 4.0);
    // 5) gain 拉满 + 大信号
    run("gain=1.0 方波 2V", fs, 1.0, |_, t| if (t * 110.0).fract() < 0.3 { 2.0 } else { -2.0 }, 4.0);
    // 6) 直流 + 交流（带偏移的输入）
    run("DC 2V + 220Hz 1V", fs, 0.5, |_, t| 2.0 + (TAU * 220.0 * t).sin(), 4.0);
    // 7) 极低频方波（很多低频能量）
    run("方波 20Hz 3V", fs, 0.5, |_, t| if (t * 20.0).fract() < 0.5 { 3.0 } else { -3.0 }, 5.0);
}

// 大信号 / 极端参数 / 不同采样率 的稳定性压力测试
use rustomp_dsp::Engine;
use sparklin_drive_circuit::{Fakedist, Params};
use std::f64::consts::TAU;

fn run(fs: f64, gain: f64, level: f64, amp: f64, f0: f64, secs: f64) -> (f64, f64, bool) {
    let mut e = Engine::<Fakedist>::new(fs);
    e.set_params(Params { gain, level, ..Params::default() });
    let t = 1.0 / fs;
    let mut ph: f64 = 0.0;
    let n = (secs * fs) as usize;
    let (mut peak, mut bad) = (0.0f64, false);
    let mut tail_peak = 0.0f64;
    for i in 0..n {
        let x = amp * ph.sin();
        ph += TAU * f0 * t;
        let y = e.process(x);
        if !y.is_finite() { bad = true; break; }
        peak = peak.max(y.abs());
        if i > n * 9 / 10 { tail_peak = tail_peak.max(y.abs()); }
    }
    (peak, tail_peak, bad)
}

fn main() {
    println!("{:<8} {:<6} {:<6} {:<6} {:<8} {:>10} {:>12} {:>10}", "fs", "gain", "level", "amp", "f0", "峰值", "尾段峰值", "状态");
    let mut fails = 0;
    for &fs in &[44_100.0, 48_000.0, 96_000.0] {
        for &gain in &[0.0, 0.5, 1.0] {
            for &amp in &[0.2, 1.0, 3.0] {
                for &f0 in &[82.0, 1000.0, 3000.0] {
                    let (peak, tail, bad) = run(fs, gain, 0.5, amp, f0, 3.0);
                    // 判据：不发散、稳定（尾段峰值与整体峰值同量级）、无 NaN
                    let unstable = bad || !peak.is_finite() || peak > 20.0 || (peak > 0.01 && tail > 3.0 * peak.max(1.0));
                    if unstable { fails += 1; }
                    if unstable || amp >= 1.0 {
                        println!("{:<8.0} {:<6.1} {:<6.1} {:<6.1} {:<8.0} {:>10.4} {:>12.4} {:>10}", fs, gain, 0.5, amp, f0, peak, tail, if unstable {"发散!"} else {"ok"});
                    }
                }
            }
        }
    }
    println!("\n失败用例：{fails}");
}

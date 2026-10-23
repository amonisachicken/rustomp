// CPU 成本分解：过采样 FIR / 电路求解 / 二极管模型
use rustomp_dsp::{Diode1n4148, Engine, Fakedist, Oversampler4x, Params};
use std::f64::consts::TAU;
use std::hint::black_box;
use std::time::Instant;

const N: usize = 1_000_000;

fn bench<F: FnMut()>(name: &str, iters: usize, mut f: F) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let t0 = Instant::now();
        for _ in 0..iters {
            f();
        }
        let ns = t0.elapsed().as_secs_f64() / iters as f64 * 1e9;
        best = best.min(ns);
    }
    println!("  {name:<44} {best:>8.1} ns");
    best
}

fn main() {
    println!("=== 单项成本（单声道，单次调用）===");

    let os_ns = {
        let mut os = Oversampler4x::new();
        let mut ph: f64 = 0.0;
        let mut up = [0.0f64; 4];
        bench("过采样：1 基带样本（上采样+下采样）", N, || {
            up = [0.0; 4];
            os.upsample(black_box(ph.sin()), &mut up);
            black_box(os.downsample(&up));
            ph += 1e-4;
        })
    };

    let circ_ns = {
        let mut c = Fakedist::new(192_000.0);
        c.set_params(Params::default());
        let mut ph: f64 = 0.0;
        bench("电路：1 个 192k 样本（含牛顿求解）", N, || {
            black_box(c.process(black_box(0.2 * ph.sin())));
            ph += TAU * 220.0 / 192_000.0;
        })
    };

    let eng_ns = {
        let mut e = Engine::new(48_000.0);
        e.set_params(Params::default());
        let mut ph: f64 = 0.0;
        bench("引擎：1 个 48k 样本（4x 过采样 + 4 次电路）", N / 4, || {
            black_box(e.process(black_box(0.2 * ph.sin())));
            ph += TAU * 220.0 / 48_000.0;
        })
    };

    let d = Diode1n4148::default();
    let d1 = bench("二极管：1 次 I-V + 电导（含内层牛顿）", N / 4, || {
        black_box(d.current_and_conductance(black_box(0.6)));
    });
    let d23 = Diode1n4148 { n_vt: 2.0 * 0.0453, rs: 2.0 * 0.568, ..Diode1n4148::default() };
    let d2 = bench("二极管(串联对)：1 次 I-V + 电导", N / 4, || {
        black_box(d23.current_and_conductance(black_box(1.2)));
    });

    // 牛顿迭代次数统计
    let mut c = Fakedist::new(192_000.0);
    c.set_params(Params::default());
    let mut ph: f64 = 0.0;
    let mut iters = 0u64;
    let n = 200_000;
    for _ in 0..n {
        c.process(0.2 * ph.sin());
        ph += TAU * 220.0 / 192_000.0;
        iters += c.probe().iters as u64;
    }
    let avg = iters as f64 / n as f64;

    println!("\n=== 汇总 ===");
    println!("  外层牛顿平均迭代次数            {avg:.2} 次/样本");
    println!("  实测引擎                           {eng_ns:.1} ns/样本");
    println!("  推算：4 次电路                     {:.1} ns", 4.0 * circ_ns);
    println!("        + 过采样                     {os_ns:.1} ns");
    println!("        = 合计                       {:.1} ns（实测 {eng_ns:.1}）", 4.0 * circ_ns + os_ns);
    println!("\n  电路占引擎 {:.0}%，过采样占 {:.0}%",
        4.0 * circ_ns / eng_ns * 100.0, os_ns / eng_ns * 100.0);
    println!("  每次电路求解约含 {:.1} 次二极管模型调用（2 支路 × 外层迭代）", 2.0 * avg);
    println!("  f64 exp 成本约等于一次二极管调用的主要部分：d1={d1:.1} ns, d23={d2:.1} ns");
}

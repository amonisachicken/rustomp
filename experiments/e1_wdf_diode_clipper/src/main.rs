//! WDF 电路建模可行性验证 spike
//! 拓扑: Vin --Rs--+-- C --+-- gnd
//!                +-- D --+
//! 三个单端口 (Thevenin 源 / 电容 / 二极管) 并联 -> 3 端口并联适配器
//! 每采样: 一次标量牛顿迭代解节点电压 v
use std::hint::black_box;

const VT: f32 = 0.025852;   // kT/q @ 300K
const IS: f32 = 2.52e-9;    // 1N4148 饱和电流
const N_IDEAL: f32 = 1.752;

// ---------- 指数函数: libm vs 快速近似 ----------
#[inline(always)]
fn fast_exp(x: f32) -> f32 {
    let x = x.clamp(-80.0, 80.0);
    let y = x * 1.442_695_041;             // log2(e)
    let n = y.floor();
    let f = y - n;                          // [0,1)
    let p = 1.0 + f * (0.693_147_18 + f * (0.240_226_5 + f * (0.055_504_11
            + f * (0.009_618_129 + f * 0.001_333_355_4))));
    let scale = f32::from_bits((((n as i32) + 127) as u32) << 23);
    scale * p
}

#[inline(always)]
fn diode_model(v: f32, fast: bool) -> (f32, f32) {   // (电流, 电导)
    let x = (v / (N_IDEAL * VT)).min(60.0);
    let e = if fast { fast_exp(x) } else { x.exp() };
    (IS * (e - 1.0), IS * e / (N_IDEAL * VT))
}

// ---------- WDF 削波级 ----------
pub struct WdfClipper {
    rs: f32, rc: f32, rd: f32,
    gs: f32, gc: f32, gd: f32,
    cap_state: f32,
    v_prev: f32,
    fast: bool,
}

impl WdfClipper {
    pub fn new(sr: f32, rs: f32, c: f32, fast: bool) -> Self {
        let rc = 1.0 / (2.0 * c * sr);      // 电容端口阻抗 => b[n] = a[n-1]
        let rd = rs;                        // 二极管端口阻抗: 自由选择
        let rp = 1.0 / (1.0 / rs + 1.0 / rc + 1.0 / rd);
        Self { rs, rc, rd, gs: rp / rs, gc: rp / rc, gd: rp / rd,
               cap_state: 0.0, v_prev: 0.0, fast }
    }

    #[inline(always)]
    pub fn tick(&mut self, vin: f32) -> f32 {
        let (gs, gc, gd, rd, cs, fast) = (self.gs, self.gc, self.gd, self.rd, self.cap_state, self.fast);
        // F(v) = v - gs*Vin - 2*gc*cs - gd*(v - Rd*Id(v))
        let mut v = self.v_prev;
        for _ in 0..10 {
            let (i_d, g_d) = diode_model(v, fast);
            let f = v - gs * vin - 2.0 * gc * cs - gd * (v - rd * i_d);
            let df = 1.0 - gd * (1.0 - rd * g_d);
            let step = f / df;
            v -= step;
            if step.abs() <= 1e-6 * (1.0 + v.abs()) { break; }
        }
        self.cap_state = v - cs;
        self.v_prev = v;
        v
    }
}

fn analytic_dc(vin: f32, rs: f32) -> f32 {
    let mut v = vin * 0.5;
    for _ in 0..300 {
        let (i, g) = diode_model(v, false);
        let f = (vin - v) / rs - i;
        let df = -1.0 / rs - g;
        let s = f / df;
        v -= s;
        if s.abs() < 1e-12 { break; }
    }
    v
}

fn bench<F: FnMut() -> f64>(mut f: F) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..5 { best = best.min(f()); }
    best
}

fn main() {
    // 0. 快速 exp 精度
    let mut max_rel = 0.0f64;
    for i in -4000..=4000 {
        let x = i as f32 * 0.01;
        let (a, b) = (x.exp(), fast_exp(x));
        if a > 1e-30 { max_rel = max_rel.max(((a - b) / a).abs() as f64); }
    }
    println!("[0] fast_exp 最大相对误差 (x∈[-40,40]): {:.2e}", max_rel);

    // 1. DC 正确性 (对比解析解)
    println!("\n[1] DC 工作点校验 (Rs=1k, C=1uF)");
    for fast in [false, true] {
        let mut worst = 0.0f32;
        for vin in [0.2f32, 0.5, 1.0, 2.0, 5.0, 9.0] {
            let mut w = WdfClipper::new(192_000.0, 1000.0, 1e-6, fast);
            let mut y = 0.0;
            for _ in 0..400_000 { y = w.tick(vin); }
            worst = worst.max((y - analytic_dc(vin, 1000.0)).abs());
        }
        println!("   exp={:>4} 最大绝对误差 = {:.2e} V", if fast {"fast"} else {"libm"}, worst);
    }

    // 2. 稳定性: 10 秒 ±10V 白噪声
    {
        let mut w = WdfClipper::new(192_000.0, 10_000.0, 47e-9, true);
        let mut seed = 0x1234_5678u32;
        let (mut bad, mut peak) = (0usize, 0.0f32);
        for _ in 0..(192_000 * 10) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = ((seed >> 8) as f32 / 8_388_608.0 - 1.0) * 10.0;
            let y = w.tick(black_box(x));
            if !y.is_finite() { bad += 1; }
            peak = peak.max(y.abs());
        }
        println!("\n[2] 稳定性: 10s 白噪声 ±10V -> 非有限值 {} 个, 输出峰值 {:.4} V", bad, peak);
    }

    // 3. 吞吐量
    let n_1s = 48_000usize;
    let mut sine = vec![0.0f32; n_1s * 16];
    for (i, b) in sine.iter_mut().enumerate() { *b = ((i as f32) * 0.003).sin() * 4.0; }

    println!("\n[3] 吞吐量 (单级削波, 单声道, 每样本 1 次标量牛顿)");
    for fast in [false, true] {
        for &os in &[1u32, 4, 8, 16] {
            let sr = 48_000.0 * os as f32;
            let n = n_1s * os as usize;
            let mut w = WdfClipper::new(sr, 1e3, 47e-9, fast);
            for i in 0..8000 { w.tick(black_box(((i as f32) * 0.01).sin())); }
            let secs = bench(|| {
                let mut acc = 0.0f64;
                for &x in &sine[..n] { acc += black_box(w.tick(black_box(x))) as f64; }
                black_box(acc);
                // 计时放在外层
                0.0
            });
            let _ = secs;
            let t = {
                let mut best = f64::INFINITY;
                for _ in 0..5 {
                    let t0 = std::time::Instant::now();
                    let mut acc = 0.0f64;
                    for &x in &sine[..n] { acc += black_box(w.tick(black_box(x))) as f64; }
                    black_box(acc);
                    best = best.min(t0.elapsed().as_secs_f64());
                }
                best
            };
            let ns = t / n as f64 * 1e9;
            println!("   exp={:>4} {:>2}x OS | {:>5.1} ns/sample | {:>5.2}% 单核 (单级单声道) | 单块 @4x 估 ≈{:.1}%",
                if fast {"fast"} else {"libm"}, os, ns, ns * sr as f64 / 1e7, ns * 4.0 * 48_000.0 / 1e7 * 4.0);
        }
    }

    // 4. 多级串联
    println!("\n[4] 多级串联 @ 4x OS (单声道)");
    for fast in [false, true] {
        for &stages in &[1usize, 2, 4, 6] {
            let sr = 192_000.0;
            let n = n_1s * 4;
            let mut chain: Vec<WdfClipper> = (0..stages)
                .map(|i| WdfClipper::new(sr, 1e3 + i as f32 * 470.0, 47e-9 / (1 + i) as f32, fast))
                .collect();
            let mut best = f64::INFINITY;
            for _ in 0..5 {
                let t0 = std::time::Instant::now();
                let mut acc = 0.0f64;
                for &x in &sine[..n] {
                    let mut y = x;
                    for s in chain.iter_mut() { y = s.tick(black_box(y)); }
                    acc += y as f64;
                }
                black_box(acc);
                best = best.min(t0.elapsed().as_secs_f64());
            }
            let ns = best / n as f64 * 1e9;
            println!("   exp={:>4} {} 级 | {:>6.1} ns/sample | {:>5.2}% 单核", 
                if fast {"fast"} else {"libm"}, stages, ns, ns * 192_000.0 / 1e7);
        }
    }
}

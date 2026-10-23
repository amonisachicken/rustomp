use rustomp_dsp::Diode1n4148;
use std::hint::black_box;
use std::time::Instant;
const N: usize = 2_000_000;
fn t(name: &str, mut f: impl FnMut() -> f64) {
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let t0 = Instant::now();
        for _ in 0..N { black_box(f()); }
        best = best.min(t0.elapsed().as_secs_f64() / N as f64 * 1e9);
    }
    println!("  {name:<38} {best:>7.2} ns");
}
fn main() {
    let d = Diode1n4148::default();
    println!("=== exp 成本 ===");
    t("f64::exp", || black_box(0.7f64).exp());
    t("f32::exp", || (black_box(0.7f32).exp()) as f64);
    println!("\n=== 二极管 I-V+电导（含内层牛顿）在不同工作点 ===");
    for v in [0.0, 0.2, 0.5, 0.65, 0.8, 1.2, 2.0] {
        t(&format!("v={v:.2} V"), || { let (i,g)=d.current_and_conductance(black_box(v)); i+g });
    }
}

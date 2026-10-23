use rustomp_dsp::{Fakedist, Params};
fn main() {
    let fs = 192_000.0; // 4x48k
    let mut c = Fakedist::new(fs);
    c.set_params(Params::default());
    let mut s: u64 = 12345;
    // 1 秒静音，然后 1 秒 ±2V 噪声，然后静音
    for i in 0..(4.0 * fs) as usize {
        let t = i as f64 / fs;
        let x = if (1.0..2.0).contains(&t) {
            s ^= s << 13; s ^= s >> 7; s ^= s << 17;
            ((s % 2000) as f64 / 1000.0 - 1.0) * 2.0
        } else { 0.0 };
        c.process(x);
        if i % (fs as usize / 2) == 0 {
            let p = c.probe();
            println!("t={:.1}s v_out={:.4} v_inm={:.4} vc2={:.4} iters={}", t, p.v_out, p.v_inm, p.vc2, p.iters);
        }
    }
    println!("\n最后若干采样（输入已是 0）：");
    for _ in 0..12 {
        c.process(0.0);
        let p = c.probe();
        println!("  v_out={:>9.4} v_inm={:>9.4} vc2={:>9.4} iters={}", p.v_out, p.v_inm, p.vc2, p.iters);
    }
}

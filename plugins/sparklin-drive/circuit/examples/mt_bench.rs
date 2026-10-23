// 多线程可行性原型：立体声 = 左右声道各一个线程
//
// 约束：电路是**逐样本递推**的，单声道内部无法并行；唯一的并行维度是左右声道。
// 这里测：单线程 vs 双线程（自旋握手）处理同样多的音频，看真实加速比。
use rustomp_dsp::Engine;
use sparklin_drive_circuit::{Fakedist, Params};
use std::hint::black_box;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

const BLOCK: usize = 256;
const BLOCKS: usize = 4000; // 约 21 秒 @48k

struct Shared {
    // 0 = 空闲，1 = 有新块，2 = 完成
    go: AtomicU32,
    done: AtomicU64,
    gen: AtomicU64,
    inbuf: [f64; BLOCK],
}

fn main() {
    let fs = 48_000.0;

    // ---------- 单线程基线 ----------
    let mut e0 = Engine::<Fakedist>::new(fs);
    let mut e1 = Engine::<Fakedist>::new(fs);
    e0.set_params(Params::default());
    e1.set_params(Params::default());
    let input: Vec<f64> = (0..BLOCK)
        .map(|i| 0.2 * (std::f64::consts::TAU * 220.0 * i as f64 / fs).sin())
        .collect();

    let mut best_st = f64::INFINITY;
    for _ in 0..3 {
        let t0 = Instant::now();
        for _ in 0..BLOCKS {
            for &x in &input {
                black_box(e0.process(x));
                black_box(e1.process(x));
            }
        }
        best_st = best_st.min(t0.elapsed().as_secs_f64());
    }
    let st_ns = best_st / (BLOCKS * BLOCK) as f64 * 1e9;

    // ---------- 双线程 ----------
    let shared = Arc::new(Shared {
        go: AtomicU32::new(0),
        done: AtomicU64::new(0),
        gen: AtomicU64::new(0),
        inbuf: [0.0; BLOCK],
    });
    let mut e1_worker = Engine::<Fakedist>::new(fs);
    e1_worker.set_params(Params::default());
    let sh = shared.clone();
    let handle = std::thread::spawn(move || {
        let mut local = [0.0f64; BLOCK];
        let mut last_gen = 0u64;
        loop {
            // 自旋等待新块
            while sh.gen.load(Ordering::Acquire) == last_gen {
                std::hint::spin_loop();
            }
            last_gen = sh.gen.load(Ordering::Acquire);
            if last_gen == u64::MAX {
                return;
            }
            local.copy_from_slice(&sh.inbuf);
            for &x in local.iter() {
                black_box(e1_worker.process(x));
            }
            sh.done.store(last_gen, Ordering::Release);
        }
    });

    let mut best_mt = f64::INFINITY;
    for _ in 0..3 {
        let t0 = Instant::now();
        for b in 0..BLOCKS {
            unsafe {
                // 原型里用 unsafe 直接写共享缓冲（生产实现会用无锁环形队列）
                let p = shared.inbuf.as_ptr() as *mut f64;
                for (i, &x) in input.iter().enumerate() {
                    *p.add(i) = x;
                }
            }
            let gen = b as u64 + 1;
            shared.gen.store(gen, Ordering::Release);
            for &x in &input {
                black_box(e0.process(x));
            }
            while shared.done.load(Ordering::Acquire) != gen {
                std::hint::spin_loop();
            }
        }
        best_mt = best_mt.min(t0.elapsed().as_secs_f64());
    }
    shared.gen.store(u64::MAX, Ordering::Release);
    handle.join().unwrap();
    let mt_ns = best_mt / (BLOCKS * BLOCK) as f64 * 1e9;

    println!("立体声（左右各一 Engine，48k + 4x）");
    println!("  单线程：{st_ns:>7.1} ns/帧   单核占用 {:>5.1}%", st_ns * fs / 1e7);
    println!("  双线程：{mt_ns:>7.1} ns/帧   合计 {:>5.1}% 单核（两块核心各一半）", mt_ns * fs / 1e7);
    println!("  加速比：{:.2}x", st_ns / mt_ns);
    println!("\n注意：块大小 {BLOCK}，自旋握手；真实插件里每块只有一次握手开销。");
}

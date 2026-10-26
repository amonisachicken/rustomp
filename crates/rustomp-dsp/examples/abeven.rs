// SPDX-License-Identifier: GPL-3.0-or-later
//!
//! **偶次谐波对比台架**：把两个实现的输出按"只保留偶次谐波"重建，写成可听的 WAV。
//!
//! 用法：
//!
//! ```bash
//! # 1) 两边各自 dump 采集结果（1 kHz 方波，0.2 V，Gain/Level 50%）
//! AB_DUMP_DIR=/tmp/abdump cargo run -p rustomp-dsp --release --example abfft        # v2
//! (cd ../sparklin-drive-legacy && AB_DUMP_DIR=/tmp/abdump \
//!    cargo run -p sparklin-drive-circuit --release --example abfft)                 # legacy
//!
//! # 2) 重建 + 出 WAV
//! cargo run -p rustomp-dsp --release --example abeven -- /tmp/abdump /tmp/abwav
//! ```
//!
//! 产出（48 kHz / 16-bit / 单声道）：
//!
//! | 文件 | 内容 | 增益 |
//! |---|---|---|
//! | `even_v2` / `even_legacy` | 各自**只含偶次谐波**（h2…h22）的重建 | **同一增益** |
//! | `even_diff` | `even_v2 − even_legacy` | **同一增益**（因此比上面低约 12 dB，这才是真实关系） |
//! | `abex` | 三段各 1 s：v2 偶次 → legacy 偶次 → 差值 | **同一增益**（三段可直接比音量） |
//! | `even_diff_boost` | 差值（单独归一化到 −1 dBFS） | 诊断用：**专门放大**才听得清内容 |
//! | `full_diff` | 全带差值 `v2 − legacy`（单独归一化） | 诊断用 |
//! | `swap_a_carrier_v2` / `swap_b_v2_with_legacy_even` | **判断可听性的关键实验**：两条都是 v2 的原始电平输出，
//!   唯一的区别是"偶次谐波用谁的" —— 两者之差恰好等于 `even_diff` | 原始电平（不放大） |
//!
//! ⚠️ 教训（DEVLOG E55）：**不要对"要对比的两段"各自做峰值归一化** ——
//! 那会把真实的电平差抹掉，让 12 dB 的差异听成"一样响"。

use rustfft::{num_complex::Complex, FftPlanner};
use std::path::{Path, PathBuf};

const FS: f64 = 48_000.0;
const F0: f64 = 1_000.0;

fn read_raw(path: &Path) -> Vec<f64> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("读不到 {}: {e}", path.display()));
    assert!(bytes.len() % 8 == 0, "{} 不是 f64 原始数据", path.display());
    bytes
        .chunks_exact(8)
        .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// 只保留 k·F0（k 为偶数）的频点，其余置零，再 IFFT 回时域。
fn even_harmonics_only(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let mut buf: Vec<Complex<f64>> = x.iter().map(|v| Complex::new(*v, 0.0)).collect();
    FftPlanner::new().plan_fft_forward(n).process(&mut buf);

    let bin_of = |f: f64| (f / (FS / n as f64)).round() as usize;
    let keep: Vec<bool> = (0..n)
        .map(|k| {
            let f = if k <= n / 2 {
                k as f64 * FS / n as f64
            } else {
                (n - k) as f64 * FS / n as f64
            };
            let h = f / F0;
            h >= 1.5 && (h.round() - h).abs() < 1e-6 && (h.round() as i64) % 2 == 0
        })
        .collect();
    let _ = bin_of;

    for (k, c) in buf.iter_mut().enumerate() {
        if !keep[k] {
            *c = Complex::new(0.0, 0.0);
        }
    }
    FftPlanner::new().plan_fft_inverse(n).process(&mut buf);
    buf.iter().map(|c| c.re / n as f64).collect()
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

fn peak(x: &[f64]) -> f64 {
    x.iter().fold(0.0f64, |a, v| a.max(v.abs()))
}

/// 最小 16-bit PCM WAV 写入（无依赖）。
fn write_wav(path: &Path, samples: &[f64]) {
    let n = samples.len() as u32;
    let data_len = n * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&(FS as u32).to_le_bytes());
    out.extend_from_slice(&((FS as u32) * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in samples {
        let s = (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out).unwrap_or_else(|e| panic!("写 {} 失败: {e}", path.display()));
}

fn db(v: f64, reference: f64) -> f64 {
    20.0 * (v / reference).max(1e-15).log10()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).map(String::as_str).unwrap_or("/tmp/abdump"));
    let out_dir = PathBuf::from(args.get(2).map(String::as_str).unwrap_or("/tmp/abwav"));
    std::fs::create_dir_all(&out_dir).expect("建输出目录失败");

    let v2 = read_raw(&dir.join("v2-mna_square.f64"));
    let legacy = read_raw(&dir.join("legacy-wdf_square.f64"));
    assert_eq!(v2.len(), legacy.len(), "两边采集长度不一致");

    let ev2 = even_harmonics_only(&v2);
    let eleg = even_harmonics_only(&legacy);
    let ediff: Vec<f64> = ev2.iter().zip(&eleg).map(|(a, b)| a - b).collect();
    let fdiff: Vec<f64> = v2.iter().zip(&legacy).map(|(a, b)| a - b).collect();

    let db_full = db(rms(&v2), 1.0);
    println!("原始输出 RMS: v2 {:.1} dBFS", db_full);
    println!(
        "  偶次谐波分量 RMS: v2 {:.1} dB  ·  legacy {:.1} dB（相对满刻度）",
        db(rms(&ev2), 1.0),
        db(rms(&eleg), 1.0)
    );
    println!(
        "  偶次差值 RMS: {:.1} dB（比偶次本身低 {:.1} dB）  ·  全带差值 RMS: {:.1} dB",
        db(rms(&ediff), 1.0),
        db(rms(&ev2), 1.0) - db(rms(&ediff), 1.0),
        db(rms(&fdiff), 1.0)
    );

    // **统一增益**：三段（v2 偶次 / legacy 偶次 / 差值）共用同一个增益，
    // 这样它们的相对音量就是真实关系（差值低约 12 dB）。
    let g_common = 0.891 / peak(&ev2).max(peak(&eleg)).max(peak(&ediff)).max(1e-12);
    let scale = |x: &[f64], g: f64| -> Vec<f64> { x.iter().map(|v| v * g).collect() };

    // 诊断用：差值单独归一化（专门放大才听得清"差在哪"）
    let g_boost = 0.891 / peak(&ediff).max(1e-12);

    println!(
        "  统一增益 {:+.1} dB（三段共用）；差值诊断版 {:+.1} dB",
        db(g_common, 1.0),
        db(g_boost, 1.0)
    );

    write_wav(&out_dir.join("even_v2.wav"), &scale(&ev2, g_common));
    write_wav(&out_dir.join("even_legacy.wav"), &scale(&eleg, g_common));
    write_wav(&out_dir.join("even_diff.wav"), &scale(&ediff, g_common));
    write_wav(
        &out_dir.join("even_diff_boost.wav"),
        &scale(&ediff, g_boost),
    );
    write_wav(&out_dir.join("full_diff.wav"), &scale(&fdiff, g_boost));

    // 三段拼接（统一增益 → 可听出真实电平关系）
    let mut abex = Vec::with_capacity(v2.len() * 3);
    abex.extend_from_slice(&scale(&ev2, g_common));
    abex.extend_from_slice(&scale(&eleg, g_common));
    abex.extend_from_slice(&scale(&ediff, g_common));
    write_wav(&out_dir.join("abex.wav"), &abex);

    // **可听性实验**：两条都是原始电平的全信号，只有"偶次谐波用谁的"不同。
    // A = v2 原样；B = (v2 − 它的偶次) + legacy 的偶次。
    // A − B 恰好等于 even_diff → 若正常音量下听不出 A/B 差别，偶次差异就不可听。
    let carrier_v2: Vec<f64> = v2.iter().zip(&ev2).map(|(a, b)| a - b).collect();
    let swap_b: Vec<f64> = carrier_v2.iter().zip(&eleg).map(|(a, b)| a + b).collect();
    write_wav(&out_dir.join("swap_a_carrier_v2.wav"), &v2);
    write_wav(&out_dir.join("swap_b_v2_with_legacy_even.wav"), &swap_b);
    let swap_diff: Vec<f64> = v2.iter().zip(&swap_b).map(|(a, b)| a - b).collect();
    println!(
        "  swap 实验：A RMS {:.1} dBFS · B RMS {:.1} dBFS · A−B RMS {:.1} dBFS（应等于偶次差值）",
        db(rms(&v2), 1.0),
        db(rms(&swap_b), 1.0),
        db(rms(&swap_diff), 1.0)
    );

    println!("\n已写入 {}:", out_dir.display());
    for f in [
        "even_v2.wav",
        "even_legacy.wav",
        "even_diff.wav",
        "abex.wav",
        "even_diff_boost.wav",
        "full_diff.wav",
        "swap_a_carrier_v2.wav",
        "swap_b_v2_with_legacy_even.wav",
    ] {
        println!("  {}", out_dir.join(f).display());
    }
}

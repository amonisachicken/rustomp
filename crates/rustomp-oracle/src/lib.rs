// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 框架的 **ngspice 对拍接口**。
//!
//! 用途：把"电路级 DSP 实现"和"真实 SPICE 仿真"放在**同一把尺子**上比较：
//!
//! 1. [`Oracle::discover`] 找到本机 ngspice（环境变量 `NGSPICE` 或 `PATH`）；
//! 2. [`Oracle::run_netlist`] 在临时目录里跑批处理仿真，读回 ASCII rawfile；
//! 3. [`RawFile`] 解析成 `变量名 → 采样序列`；
//! 4. [`harmonics`] / [`thd_percent`] / [`gain_db`] 对**任意波形**做同样的测量 ——
//!    对 ngspice 的输出和 DSP 的输出都用这几个函数，避免"两把尺子"。
//!
//! 没有 ngspice 时 `discover()` 返回 `None`，测试应跳过而不是失败。
//!
//! ```no_run
//! use rustomp_oracle::{thd_percent, Oracle};
//! let oracle = Oracle::discover().expect("需要 ngspice");
//! let netlist = "V1 in 0 SIN(0 1 1k)\nR1 in out 1k\nC1 out 0 100n\n.tran 5u 30m 20m\n.end\n";
//! let raw = oracle.run_netlist(netlist).unwrap();
//! let out = raw.get("v(out)").unwrap();
//! let thd = thd_percent(out, raw.sample_rate(), 1000.0, 9);
//! println!("THD = {thd:.3} %");
//! ```

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 一次仿真的 rawfile 内容。
pub struct RawFile {
    /// 变量名（小写，例如 `v(out)`、`time`）
    pub vars: Vec<String>,
    /// 每个变量的采样序列
    pub data: HashMap<String, Vec<f64>>,
}

impl RawFile {
    /// 解析 ngspice 的 **ASCII rawfile**（`ngspice -b -r out.raw` +
    /// 工作目录 `.spiceinit` 里 `set filetype=ascii`）。
    ///
    /// 格式要点：`Values:` 之后**只有每个数据点的第一行带序号**，后续变量行只有值；
    /// 点序号是 0,1,2… 递增（不是每点都写 0）。
    pub fn parse(text: &str) -> Result<Self, String> {
        let lines: Vec<&str> = text.lines().collect();
        let mut nvar = 0usize;
        let mut vars_start = None;
        let mut vals_start = None;
        for (i, line) in lines.iter().enumerate() {
            if let Some(rest) = line.strip_prefix("No. Variables:") {
                nvar = rest.trim().parse().map_err(|_| "No. Variables 解析失败")?;
            } else if line.starts_with("Variables:") {
                vars_start = Some(i + 1);
            } else if line.starts_with("Values:") {
                vals_start = Some(i + 1);
                break;
            }
        }
        let (vars_start, vals_start) = match (vars_start, vals_start) {
            (Some(a), Some(b)) => (a, b),
            _ => return Err("不是可解析的 ASCII rawfile".into()),
        };

        let mut vars = Vec::with_capacity(nvar);
        for line in lines[vars_start..vals_start.saturating_sub(1)].iter().take(nvar) {
            let tok: Vec<&str> = line.split_whitespace().collect();
            if tok.len() >= 2 {
                vars.push(tok[1].to_lowercase());
            }
        }
        if vars.is_empty() {
            return Err("rawfile 里没有变量".into());
        }

        let mut data: HashMap<String, Vec<f64>> = vars.iter().map(|v| (v.clone(), Vec::new())).collect();
        let mut cur: Vec<f64> = Vec::with_capacity(vars.len());
        let flush = |cur: &mut Vec<f64>, data: &mut HashMap<String, Vec<f64>>| {
            if cur.len() == vars.len() {
                for (name, v) in vars.iter().zip(cur.iter()) {
                    data.get_mut(name).unwrap().push(*v);
                }
            }
            cur.clear();
        };
        for line in &lines[vals_start..] {
            if line.trim().is_empty() {
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                // 后续变量：只有值
                if let Some(tok) = line.split_whitespace().next() {
                    if let Ok(v) = tok.parse::<f64>() {
                        cur.push(v);
                    }
                }
            } else {
                // 新数据点：`<序号>\t\t<var0 的值>`
                let tok: Vec<&str> = line.split_whitespace().collect();
                if tok.len() >= 2 {
                    if let Ok(v) = tok[1].parse::<f64>() {
                        flush(&mut cur, &mut data);
                        cur.push(v);
                    }
                }
            }
        }
        flush(&mut cur, &mut data);

        Ok(RawFile { vars, data })
    }

    pub fn get(&self, name: &str) -> Option<&Vec<f64>> {
        self.data.get(&name.to_lowercase())
    }

    /// 采样率（由 `time` 向量推断）；没有 time 时返回 0。
    pub fn sample_rate(&self) -> f64 {
        let t = match self.get("time") {
            Some(t) if t.len() > 1 => t,
            _ => return 0.0,
        };
        let dt = t[1] - t[0];
        if dt > 0.0 {
            1.0 / dt
        } else {
            0.0
        }
    }

    /// 名义步长（相邻时间间隔的**中位数**）。
    ///
    /// ngspice 的 `.tran` 是**变步长**的：`.tran 5u` 只是上限，
    /// 削波/开关处会自动缩小步长。所以不能按"样本个数"切周期，
    /// 必须先按名义步长重采样（见 [`RawFile::resample`]）。
    pub fn nominal_dt(&self) -> Option<f64> {
        let t = self.get("time")?;
        if t.len() < 2 {
            return None;
        }
        let mut dts: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
        dts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(dts[dts.len() / 2])
    }

    /// 把某个变量按固定步长 `dt` 线性插值重采样（从第一个时间点开始）。
    pub fn resample(&self, name: &str, dt: f64) -> Option<Vec<f64>> {
        let t = self.get("time")?;
        let y = self.get(name)?;
        if t.len() < 2 || dt <= 0.0 {
            return None;
        }
        let t0 = t[0];
        let t_end = *t.last()?;
        let n = (((t_end - t0) / dt) as usize).max(1) + 1;
        let mut out = Vec::with_capacity(n);
        let mut j = 0usize;
        for i in 0..n {
            let tt = t0 + i as f64 * dt;
            while j + 2 < t.len().min(y.len()) && t[j + 1] < tt {
                j += 1;
            }
            let (ta, tb) = (t[j], t[j + 1]);
            let frac = if tb > ta { (tt - ta) / (tb - ta) } else { 0.0 };
            let frac = frac.clamp(0.0, 1.0);
            let (ya, yb) = (y[j], y[j + 1]);
            out.push(ya + (yb - ya) * frac);
        }
        Some(out)
    }

    pub fn len(&self) -> usize {
        self.data.values().next().map(|v| v.len()).unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 本机 ngspice。
pub struct Oracle {
    binary: PathBuf,
}

impl Oracle {
    /// **测试用**：找到 ngspice 就用它；没有时按环境变量决定"跳过"还是"失败"。
    ///
    /// - 未设置 `RUSTOMP_REQUIRE_NGSPICE`：打印跳过原因并返回 `None`（本地开发友好）
    /// - 设为 `1`/`true`/`yes`：**直接 panic** —— CI 用这个，避免"忘了装 ngspice，
    ///   测试静默跳过，CI 却是绿的"这种最坏的假阳性。
    pub fn discover_or_skip() -> Option<Oracle> {
        match Self::discover() {
            Some(o) => Some(o),
            None => {
                let required = std::env::var("RUSTOMP_REQUIRE_NGSPICE")
                    .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
                    .unwrap_or(false);
                if required {
                    panic!(
                        "RUSTOMP_REQUIRE_NGSPICE 已设置，但找不到 ngspice。\n\
                         请安装（macOS: `brew install ngspice`），或用 `NGSPICE=/path/to/ngspice` 指定。"
                    );
                }
                eprintln!("跳过：本机没有 ngspice（设 RUSTOMP_REQUIRE_NGSPICE=1 可强制要求）");
                None
            }
        }
    }

    /// 找 ngspice：`NGSPICE` 环境变量 → `PATH`。
    pub fn discover() -> Option<Oracle> {
        if let Ok(p) = std::env::var("NGSPICE") {
            let p = PathBuf::from(p);
            if p.exists() {
                return Some(Oracle { binary: p });
            }
        }
        for dir in std::env::var("PATH").unwrap_or_default().split(':') {
            let cand = Path::new(dir).join("ngspice");
            if cand.is_file() {
                return Some(Oracle { binary: cand });
            }
        }
        None
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// 跑一段网表（需要自带 `.tran`/`.ac` 等分析语句和 `.end`），返回 rawfile。
    ///
    /// 在临时目录里执行，并写一个 `.spiceinit` 让 rawfile 是 ASCII。
    pub fn run_netlist(&self, netlist: &str) -> Result<RawFile, String> {
        let dir = unique_temp_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("建临时目录失败: {e}"))?;
        let cleanup = Cleanup(dir.clone());

        std::fs::write(dir.join(".spiceinit"), "set filetype=ascii\n")
            .map_err(|e| format!("写 .spiceinit 失败: {e}"))?;
        let cir = dir.join("circuit.cir");
        let raw = dir.join("out.raw");
        {
            let mut f = std::fs::File::create(&cir).map_err(|e| format!("写网表失败: {e}"))?;
            f.write_all(netlist.as_bytes())
                .map_err(|e| format!("写网表失败: {e}"))?;
        }

        let out = Command::new(&self.binary)
            .arg("-b")
            .arg("-r")
            .arg(&raw)
            .arg(&cir)
            .current_dir(&dir)
            .output()
            .map_err(|e| format!("启动 ngspice 失败: {e}"))?;

        if !raw.exists() {
            let _ = cleanup;
            return Err(format!(
                "ngspice 没有产出 rawfile（退出码 {:?}）\nstdout:\n{}\nstderr:\n{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let text = std::fs::read_to_string(&raw).map_err(|e| format!("读 rawfile 失败: {e}"))?;
        let parsed = RawFile::parse(&text);
        let _ = cleanup;
        parsed
    }
}

/// 临时目录的 RAII 清理。
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unique_temp_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("rustomp-oracle-{}-{}", std::process::id(), n))
}

// ---------------------------------------------------------------- 测量

/// 单频正弦的谐波幅度（峰值）。返回 `h[0] = 基波, h[1] = 2 次, …`，共 `n_harm` 个。
///
/// 用矩形窗 + 整数周期求和；调用方应保证窗口内是整数个周期。
pub fn harmonics(samples: &[f64], fs: f64, f0: f64, n_harm: usize) -> Vec<f64> {
    let n = samples.len();
    if n == 0 || fs <= 0.0 {
        return vec![0.0; n_harm];
    }
    let mean = samples.iter().sum::<f64>() / n as f64;
    (1..=n_harm)
        .map(|h| {
            let f = f0 * h as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (k, v) in samples.iter().enumerate() {
                let a = std::f64::consts::TAU * f * k as f64 / fs;
                re += (v - mean) * a.cos();
                im -= (v - mean) * a.sin();
            }
            2.0 * (re * re + im * im).sqrt() / n as f64
        })
        .collect()
}

/// 各次谐波相对基波的百分比（不含基波）。
pub fn harmonics_percent(h: &[f64]) -> Vec<f64> {
    let fund = h.first().copied().unwrap_or(0.0);
    h.iter()
        .skip(1)
        .map(|v| if fund > 0.0 { v / fund * 100.0 } else { 0.0 })
        .collect()
}

/// THD（%），用 `harmonics_percent` 的结果。
pub fn thd_percent_from_percent(hp: &[f64]) -> f64 {
    hp.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// 便捷：直接由样本算 THD（%）。
pub fn thd_percent(samples: &[f64], fs: f64, f0: f64, n_harm: usize) -> f64 {
    let h = harmonics(samples, fs, f0, n_harm);
    thd_percent_from_percent(&harmonics_percent(&h))
}

/// 某频点的增益（dB），以两个波形各自的基波幅度之比计算。
pub fn gain_db(input: &[f64], output: &[f64], fs: f64, f0: f64) -> f64 {
    let hi = harmonics(input, fs, f0, 1)[0];
    let ho = harmonics(output, fs, f0, 1)[0];
    if hi <= 0.0 || ho <= 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * (ho / hi).log10()
}

/// 取波形中「整数个周期」的最后一段，供 `harmonics` 使用。
pub fn last_integer_periods(samples: &[f64], fs: f64, f0: f64, periods: usize) -> &[f64] {
    let per = (fs / f0).round() as usize;
    let n = (per * periods).min(samples.len());
    &samples[samples.len() - n..]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手工构造的 ASCII rawfile（含"只有第一行带序号"的格式细节）
    const SAMPLE: &str = "Title: test\n\
Plotname: Transient Analysis\n\
Flags: real\n\
No. Variables: 2\n\
No. Points: 3\n\
Variables:\n\
\t0\ttime\ttime\n\
\t1\tv(out)\tvoltage\n\
Values:\n\
0\t0.000000000000000e+00\n\
\t1.000000000000000e+00\n\
1\t1.000000000000000e-06\n\
\t2.000000000000000e+00\n\
2\t2.000000000000000e-06\n\
\t3.000000000000000e+00\n";

    #[test]
    fn parses_ascii_rawfile() {
        let raw = RawFile::parse(SAMPLE).unwrap();
        assert_eq!(raw.vars, vec!["time", "v(out)"]);
        assert_eq!(raw.get("v(out)").unwrap(), &vec![1.0, 2.0, 3.0]);
        assert!((raw.sample_rate() - 1e6).abs() < 1.0);
        assert_eq!(raw.len(), 3);
    }

    #[test]
    fn harmonics_of_pure_sine() {
        // 1 kHz、幅度 1 的正弦，采样 200 kHz，取 10 个整数周期
        let fs = 200_000.0;
        let f0 = 1000.0;
        let n = (fs / f0) as usize * 10;
        let samples: Vec<f64> = (0..n)
            .map(|i| (std::f64::consts::TAU * f0 * i as f64 / fs).sin())
            .collect();
        let h = harmonics(&samples, fs, f0, 5);
        assert!((h[0] - 1.0).abs() < 1e-9, "基波 = {}", h[0]);
        assert!(thd_percent_from_percent(&harmonics_percent(&h)) < 1e-9);
    }

    #[test]
    fn detects_known_thd() {
        // 基波 + 10% 三次谐波 → THD = 10%
        let fs = 200_000.0;
        let f0 = 1000.0;
        let n = (fs / f0) as usize * 10;
        let samples: Vec<f64> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * f0 * i as f64 / fs;
                a.sin() + 0.1 * (3.0 * a).sin()
            })
            .collect();
        let thd = thd_percent(&samples, fs, f0, 9);
        assert!((thd - 10.0).abs() < 1e-6, "THD = {thd}");
    }

    #[test]
    fn last_integer_periods_works() {
        let v: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let slice = last_integer_periods(&v, 48_000.0, 1000.0, 4);
        assert_eq!(slice.len(), 192);
        assert_eq!(slice[slice.len() - 1], 999.0);
    }

    #[test]
    fn resamples_variable_step_data() {
        // 人为构造变步长数据：1 kHz 正弦，步长在 5µs/2.5µs 之间跳
        let f0 = 1000.0;
        let mut t = Vec::new();
        let mut y = Vec::new();
        let mut tt = 0.0;
        let mut i = 0u32;
        while tt < 0.02 {
            t.push(tt);
            y.push((std::f64::consts::TAU * f0 * tt).sin());
            tt += if i % 2 == 0 { 5e-6 } else { 2.5e-6 };
            i += 1;
        }
        let mut raw = RawFile {
            vars: vec!["time".into(), "v(x)".into()],
            data: HashMap::new(),
        };
        raw.data.insert("time".into(), t);
        raw.data.insert("v(x)".into(), y);
        let dt = raw.nominal_dt().unwrap();
        assert!((dt - 2.5e-6).abs() < 1e-9 || (dt - 5e-6).abs() < 1e-9);
        let uni = raw.resample("v(x)", 5e-6).unwrap();
        let thd = thd_percent(&uni, 200_000.0, f0, 5);
        assert!(thd < 1e-6, "重采样后仍是纯正弦，THD={thd}");
    }

    #[test]
    fn discover_does_not_panic() {
        // 有没有 ngspice 都应正常返回
        let _ = Oracle::discover();
    }
}

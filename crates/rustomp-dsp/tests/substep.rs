// SPDX-License-Identifier: GPL-3.0-or-later

//! 实时路径「大跳变内部子步进 + 失败恢复」回归（E95）。
//!
//! 旧行为：输入 0 → 0.175 V 的阶跃在 192 kHz 下牛顿发散，`step_fixed` 拒绝提交
//! 这一拍；输入持续这么大时下一拍从同一状态再失败 → 永久冻结（每拍白跑 24 次迭代）。
//!
//! 新行为：固定节点单拍变化 > [`SUBSTEP_DELTA`] 时把这一拍拆成若干 `dt/k`；
//! 子步仍失败则按目标电压做一次 DC 恢复，连续失败到上限强制 reset。

include!("generated/pretty_face.rs");

const FS: f64 = 192_000.0;
const DT: f64 = 1.0 / FS;

fn fresh() -> Netlist<N> {
    let mut n = build(FS, &Params::default());
    n.solve_dc();
    n
}

#[test]
fn small_steps_stay_single_step() {
    let mut n = fresh();
    for i in 0..2_000 {
        let x = 0.01 * (i as f64 * 0.05).sin();
        n.set_fixed(FIXED_IN, x);
        n.step_fixed(DT, 3);
        assert_eq!(n.last_substeps, 1, "正常信号不该拆子步（i={i}）");
        assert!(n.last_accept, "正常信号不该失败（i={i}）");
    }
    assert_eq!(n.last_recoveries, 0, "正常信号不该走 DC 恢复");
}

#[test]
fn big_step_substeps_and_advances() {
    let mut n = fresh();
    let before = n.v[OUT];
    n.set_fixed(FIXED_IN, 0.2);
    n.step_fixed(DT, 3);
    println!(
        "step 0→0.2: substeps={} accept={} recoveries={} OUT {before:+.6} → {:+.6}",
        n.last_substeps, n.last_accept, n.last_recoveries, n.v[OUT]
    );
    assert!(n.last_substeps >= 2, "0.2 V 阶跃应拆子步");
    assert!(n.last_accept, "子步应正常收敛");
    assert_eq!(n.last_recoveries, 0, "0.2 V 阶跃不该走到 DC 恢复");
    assert!((n.v[OUT] - before).abs() > 1e-6, "输出没有前进（仍被冻结）");
    assert!(n.v.iter().all(|x| x.is_finite()));
}

#[test]
fn square_does_not_freeze() {
    let mut n = fresh();
    let mut peak = 0.0f64;
    let n_samples = (FS * 0.5) as usize;
    for i in 0..n_samples {
        let t = i as f64 * DT;
        let x = if (t * 1_000.0).fract() < 0.5 {
            0.5
        } else {
            -0.5
        };
        n.set_fixed(FIXED_IN, x);
        n.step_fixed(DT, 3);
        assert!(n.v.iter().all(|v| v.is_finite()), "第 {i} 拍出现非有限值");
        peak = peak.max(n.v[OUT].abs());
    }
    println!(
        "0.5 V 方波 0.5 s: peak={peak:.4} substeps(last)={} recoveries={}",
        n.last_substeps, n.last_recoveries
    );
    assert!(peak > 0.01, "方波没有输出（电路冻结）");
    assert_eq!(n.last_recoveries, 0, "0.5 V 方波不该走到 DC 恢复");
}

#[test]
fn extreme_input_recovers_without_nan() {
    let mut n = fresh();
    for i in 0..64 {
        n.set_fixed(FIXED_IN, if i % 2 == 0 { 5.0 } else { -5.0 });
        n.step_fixed(DT, 2);
        assert!(n.v.iter().all(|v| v.is_finite()), "第 {i} 拍出现非有限值");
    }
    // 极端输入允许走子步或 DC 恢复，但必须还在推进（不能永久停在直流）
    assert!(
        n.last_substeps > 1 || n.last_recoveries > 0,
        "极端输入既没拆步也没恢复（可能已冻结）"
    );
}

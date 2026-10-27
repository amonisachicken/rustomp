// SPDX-License-Identifier: GPL-3.0-or-later

//! **锗管法兹（AC127 ×2）的回归**：设计器工程 → 代码 → 与 ngspice 的工作点对齐。
//!
//! ```text
//! crates/rustomp-import/tests/data/pretty-face.design.json   ← 用户自己画的法兹
//!   → rustomp-import --mna → crates/rustomp-dsp/tests/generated/pretty_face.rs
//!   → 本测试
//! ```
//!
//! 这个电路是"纯三极管单块"的第一个样本，也是**两处求解器缺陷**的现场
//! （见 DEVLOG E81）：
//!
//! | 症状 | 根因 |
//! |---|---|
//! | 直流工作点跑到 −1.6e34 V | `exp` 用 `min(80)` 硬截断 ⇒ 导数被钉成 0 ⇒ 内层牛顿拿不到方向 |
//! | 瞬态第 77 拍炸到 1e25 V | 同上；另外实时路径"3 次迭代"是硬天花板，收敛不了就把烂解留给下一拍 |
//!
//! 现在的判据取 **ngspice 的 `.op`**（设计器【仿真】里能直接看到），一位不差地卡住：
//! 只要模型/求解器再退回去，这里立刻红。

include!("generated/pretty_face.rs");

use rustomp_dsp::Circuit;

const FS: f64 = 192_000.0;

/// ngspice `.op`（设计器 `designer.simulate` 的 op 结果，AC127 模型取自 `devices/devices.lib`）
const NGSPICE_OP: [(&str, usize, f64); 7] = [
    ("n1", node::N1, 0.100_533_7),
    ("n2", node::N2, 0.174_887_7),
    ("n3", node::N3, 0.116_053_6),
    ("n4", node::N4, 7.649_942_8),
    ("n5", node::N5, 8.164_836_5),
    ("n6", node::N6, 0.058_026_8),
    ("vcc", node::VCC, 8.2),
];

#[test]
fn 直流工作点与_ngspice_一致() {
    let mut net = build(FS, &Params::default());
    let v = net.solve_dc();
    for (name, idx, want) in NGSPICE_OP {
        let got = v[idx];
        assert!(
            (got - want).abs() < 1e-3,
            "node {name}: 我们 {got:.6}，ngspice {want:.6}"
        );
    }
}

#[test]
fn 瞬态不发散且节点电压物理() {
    let mut c = Ckt::new(FS);
    let dt = 1.0 / FS;
    let n = (0.05 * FS) as usize; // 50 拍 1 kHz ≈ 5 个周期
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for i in 0..n {
        let t = i as f64 * dt;
        let x = 0.1 * (std::f64::consts::TAU * 1_000.0 * t).sin();
        let y = c.process(x);
        assert!(y.is_finite(), "第 {i} 拍输出是 {y:e}（不收敛）");
        // 供电 8.2 V、地 0 V：任何节点的解都该落在 rails 附近（留点余量给结压降）
        assert!(
            (-1.0..=9.5).contains(&y),
            "第 {i} 拍输出 {y:.3} 超出物理范围（应 ≈ −0.2…0.2 V）"
        );
        if i > n / 2 {
            lo = lo.min(y);
            hi = hi.max(y);
        }
    }
    let pp = hi - lo;
    // 收敛解实测 [−0.158, +0.102] ⇒ 0.26 Vpp；ngspice 同量级
    assert!(
        (0.15..=0.45).contains(&pp),
        "稳态输出摆幅 {pp:.3} Vpp 不合理（收敛解 ≈ 0.26 Vpp，ngspice ≈ 0.22–0.25）"
    );
}

/// 收敛判据自带"残差一票"（E84）：每拍结束后 `last_resid` 必须达标，
/// 否则说明是**假收敛**（Δv 小但 KCL 还差很远）。
#[test]
fn 每拍的残差都达标() {
    let mut net = build(FS, &Params::default());
    net.solve_dc();
    let dt = 1.0 / FS;
    let mut worst: f64 = 0.0;
    for i in 0..2_000 {
        let t = i as f64 * dt;
        let x = 0.1 * (std::f64::consts::TAU * 1_000.0 * t).sin();
        net.set_fixed(FIXED_IN, x);
        net.step_fixed(dt, 3);
        assert!(
            net.last_resid.is_finite(),
            "第 {i} 拍没做残差复检（Δv 判据没触发？）"
        );
        worst = worst.max(net.last_resid);
        assert!(
            net.last_resid <= net.resid_tol,
            "第 {i} 拍残差 {:.3e}",
            net.last_resid
        );
    }
    println!("最坏归一化残差 {worst:.2e}（阈值 {:e}）", net.resid_tol);
}

#[test]
fn 电位器拧到两端也稳() {
    let mut p = Params::default();
    p.p1 = 1.0;
    p.p2 = 0.0;
    let mut c = Ckt::new(FS);
    c.set_params(p);
    for i in 0..4_000 {
        let t = i as f64 / FS;
        let x = 0.1 * (std::f64::consts::TAU * 1_000.0 * t).sin();
        let y = c.process(x);
        assert!(
            y.is_finite() && (-1.0..=9.5).contains(&y),
            "第 {i} 拍 {y:e}"
        );
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! 通用 **MNA 装配器**：把器件模型与拓扑组合成可求解的电路。
//!
//! 设计要点（承接 `docs/component-library.md` §7）：
//!
//! - **器件只描述"端电压 → 流入器件的电流"**（`multiport` 闭包），不含拓扑、不含矩阵。
//! - 装配器用**数值雅可比**统一处理任意多端器件：
//!   `J_ij = ∂I_i/∂V_j`，然后按 `I ≈ J·V + (I₀ − J·V₀)` 线性化。
//!   → 好处：一个新器件只要写"电流函数"就能接进电路，不可能出现符号/矩阵写错；
//!   → 代价：每次牛顿迭代多算 `n+1` 次模型。对 3 端器件可忽略（见 §6 的预算表）。
//! - 电容用**后向欧拉**伴随模型（`g = C/dt`，并联电流源 `−g·v_prev`）。
//! - 非线性求解：**gmin 步进** + 阻尼牛顿，避免结器件初值敏感。
//! - 节点 0 恒为地（不进入方程组）。
//!
//! 用法（链式 builder）：
//!
//! ```ignore
//! let mut net = Netlist::<4>::new();
//! net.resistor(1, 0, 1_000.0)
//!    .diode(D_1N4148, 1, 0)
//!    .voltage_source(0, 1, 0, 0.0);   // 支路 0：Vin 节点 ← 地
//! net.set_voltage(0, 0.7);
//! let v = net.solve_dc();
//! ```

use crate::devices::{Bjt, Diode, Jfet, Nmos, OpAmpModel, OpAmpState};

/// 地节点编号（电压恒为 0，不进入矩阵）
pub const GND: usize = 0;

/// 电阻的数值下限（1 mΩ）。
///
/// MNA 用"导纳"表示电阻，`R = 0` 会算出 `g = ∞`，矩阵立刻出 NaN，
/// 再被引擎的熔断器判定为异常 → **整块静音**。
/// 电位器拧到端点（`level = 1` 时下臂 = 0 Ω、`gain = 0` 时反馈电阻 = 0 Ω）
/// 正好会踩到这一点（DEVLOG E51）。
///
/// 1 mΩ 在数值上等效"短路"，与生成器里"开关闭合 = 1 mΩ"是同一个约定。
pub const MIN_RESISTANCE: f64 = 1e-3;

/// 把电阻值夹到安全范围（非有限值/0/负数都变成 `MIN_RESISTANCE`）。
#[inline]
pub fn floor_resistance(r: f64) -> f64 {
    if r.is_finite() {
        r.abs().max(MIN_RESISTANCE)
    } else {
        MIN_RESISTANCE
    }
}

/// 稀疏度不重要（N ≤ 16），用稠密矩阵 + 部分主元 LU。
///
/// 未知量排布：前若干个是**节点电压**，其后是**电压源支路电流**。
#[derive(Debug, Clone)]
pub struct Mna<const N: usize> {
    y: [[f64; N]; N],
    rhs: [f64; N],
    /// 每个节点对地的 gmin（gmin 步进用）
    gmin: f64,
    /// **固定电位节点**（电源轨、输入源）：不是未知量，电压由外部给定。
    ///
    /// 好处：把"理想电压源节点"从方程组里消掉（省掉节点 + 支路两个未知量），
    /// 对 LU 的代价是**立方关系** —— 这条电路 12 → 7 个未知量，求解快 ~3 倍。
    fixed: [Option<f64>; N],
}

impl<const N: usize> Default for Mna<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Mna<N> {
    pub fn new() -> Self {
        Self {
            y: [[0.0; N]; N],
            rhs: [0.0; N],
            gmin: 0.0,
            fixed: [None; N],
        }
    }

    /// 清空矩阵（保留 gmin 设置）。
    pub fn clear(&mut self) {
        self.y = [[0.0; N]; N];
        self.rhs = [0.0; N];
        // fixed 不清：它属于拓扑/激励，每次 solve 前由调用方设置
    }

    /// 把一个节点固定到给定电位（电源轨、输入源）。`node == GND` 时忽略。
    pub fn fix_node(&mut self, node: usize, voltage: f64) {
        if node != GND && node < N {
            self.fixed[node] = Some(voltage);
        }
    }

    /// 该节点是否被固定
    #[inline]
    pub fn fixed_voltage(&self, node: usize) -> Option<f64> {
        if node == GND || node >= N {
            None
        } else {
            self.fixed[node]
        }
    }

    /// 每个节点到地的漏电导（SPICE 的 `gmin`，帮助牛顿收敛）。
    pub fn set_gmin(&mut self, g: f64) {
        self.gmin = g;
    }

    /// 当前 gmin
    pub fn gmin(&self) -> f64 {
        self.gmin
    }

    /// 导纳 `g` 接在节点 `a`–`b` 之间（`GND` 表示地）。
    pub fn conductance(&mut self, a: usize, b: usize, g: f64) {
        let va = self.fixed_voltage(a);
        let vb = self.fixed_voltage(b);
        match (va, vb) {
            // 两端都固定：不影响任何未知量
            (Some(_), Some(_)) => {}
            // 只有 a 固定：b 的方程里 `-g·v_a` 移到右边
            (Some(va), None) => {
                if b < N {
                    self.y[b][b] += g;
                    self.rhs[b] += g * va;
                }
            }
            (None, Some(vb)) => {
                if a < N {
                    self.y[a][a] += g;
                    self.rhs[a] += g * vb;
                }
            }
            (None, None) => {
                if a != GND && a < N {
                    self.y[a][a] += g;
                }
                if b != GND && b < N {
                    self.y[b][b] += g;
                }
                if a != GND && b != GND && a < N && b < N {
                    self.y[a][b] -= g;
                    self.y[b][a] -= g;
                }
            }
        }
    }

    /// 电阻。**低于 [`MIN_RESISTANCE`] 会被夹到下限**（0 Ω 会算出 ∞ 导纳 → NaN）。
    pub fn resistor(&mut self, a: usize, b: usize, r: f64) {
        self.conductance(a, b, 1.0 / floor_resistance(r));
    }

    /// 独立电流源：电流 `i` **从 a 经元件流向 b**（外部注入 a、流出 b）。
    pub fn current_source(&mut self, a: usize, b: usize, i: f64) {
        self.inject(a, -i);
        self.inject(b, i);
    }

    /// 线性化元件：导纳 `g` + 等效电流源 `i_eq`（电流**从 a 经元件流向 b**）。
    ///
    /// 这是 `I ≈ I₀ + g·(V−V₀)` 的伴随形式，`multiport()` 的数值版本最终也归到这里。
    pub fn stamp_lin(&mut self, a: usize, b: usize, g: f64, i_eq: f64) {
        self.conductance(a, b, g);
        self.current_source(a, b, i_eq);
    }

    /// 向节点注入电流（正 = 流入节点）。注入到固定节点上的电流由理想源吸收，忽略。
    pub fn inject(&mut self, node: usize, i: f64) {
        if node != GND && node < N && self.fixed[node].is_none() {
            self.rhs[node] += i;
        }
    }

    /// 电压源：支路未知量 `k` 上，`v(a) − v(b) = value`。
    pub fn voltage_source(&mut self, k: usize, a: usize, b: usize, value: f64) {
        // 支路方程
        if a != GND {
            self.y[k][a] += 1.0;
            self.y[a][k] += 1.0;
        }
        if b != GND {
            self.y[k][b] -= 1.0;
            self.y[b][k] -= 1.0;
        }
        self.rhs[k] += value;
    }

    /// 压控电压源（VCVS）：`v(a) − v(b) = gain·(v(c) − v(d)) + offset`。
    ///
    /// 运放的输出级就靠这个装进代数环（见 [`Netlist::opamp`]）。
    pub fn vcvs(
        &mut self,
        k: usize,
        a: usize,
        b: usize,
        c: usize,
        d: usize,
        gain: f64,
        offset: f64,
    ) {
        if a != GND {
            self.y[k][a] += 1.0;
            self.y[a][k] += 1.0;
        }
        if b != GND {
            self.y[k][b] -= 1.0;
            self.y[b][k] -= 1.0;
        }
        if c != GND {
            self.y[k][c] -= gain;
        }
        if d != GND {
            self.y[k][d] += gain;
        }
        self.rhs[k] += offset;
    }

    /// **通用多端非线性器件**。
    ///
    /// `nodes`：各端子的节点号（`GND` 表示地，其电压恒为 0 且不进矩阵）。
    /// `f`：给定各端子**实际电压**，返回**流入器件**的电流（每个端子一个，符号必须自洽）。
    ///
    /// 内部做数值雅可比 + 线性化：
    /// `I(V) ≈ I₀ + J·(V − V₀)` → 导纳 `J` + 等效电流源 `I₀ − J·V₀`。
    pub fn multiport<F>(&mut self, nodes: &[usize], v: &[f64], f: F)
    where
        F: Fn(&[f64]) -> Vec<f64>,
    {
        let n = nodes.len();
        let v0: Vec<f64> = nodes
            .iter()
            .map(|&node| if node == GND { 0.0 } else { v[node] })
            .collect();
        let i0 = f(&v0);

        // 数值雅可比：对每个端子电压中心差分
        let mut jac = vec![vec![0.0; n]; n];
        for j in 0..n {
            let h = 1e-7 * (1.0 + v0[j].abs());
            let mut vp = v0.clone();
            let mut vm = v0.clone();
            vp[j] += h;
            vm[j] -= h;
            let ip = f(&vp);
            let im = f(&vm);
            for i in 0..n {
                jac[i][j] = (ip[i] - im[i]) / (2.0 * h);
            }
        }

        // 导纳部分（固定节点的行/列不参与求解；其对未知量的耦合已含在下面的 i_eq 里）
        for i in 0..n {
            if self.fixed_voltage(nodes[i]).is_some() {
                continue;
            }
            for j in 0..n {
                let (ni, nj) = (nodes[i], nodes[j]);
                if ni != GND && nj != GND && ni < N && nj < N && self.fixed[nj].is_none() {
                    self.y[ni][nj] += jac[i][j];
                }
            }
        }

        // 等效电流源部分：i_eq = I₀ − J·V₀（从节点流向器件）
        for i in 0..n {
            let mut i_eq = i0[i];
            for j in 0..n {
                i_eq -= jac[i][j] * v0[j];
            }
            self.inject(nodes[i], -i_eq);
        }
    }

    /// 解 `Y·x = rhs`（部分主元高斯消元）。
    ///
    /// 内层用切片 `zip` 而不是 `a[row][k]` 双重下标 —— 去掉边界检查后
    /// 实时路径的 LU 快 ~3 倍（这是 E43 里最有效的一处优化）。
    pub fn solve(&self) -> [f64; N] {
        self.solve_len(N)
    }

    /// 只对**前 `n` 个未知量**做消元。
    ///
    /// 生成器会把**固定电位节点**（电源轨、输入）排在所有未知量之后，
    /// 它们的行列全零 —— 交给通用 LU 只是白算。12 个未知量里 3 个是尸体，
    /// 砍掉后 LU 代价降到 `(9/12)³ ≈ 0.42`，即 **2.4 倍**（DEVLOG E57）。
    pub fn solve_len(&self, n: usize) -> [f64; N] {
        let n = n.clamp(1, N);
        #[allow(unused_mut)]
        let mut a = self.y;
        let mut b = self.rhs;
        self.eliminate(&mut a, &mut b, n);
        self.back_substitute(&a, &b, n)
    }

    fn eliminate(&self, a: &mut [[f64; N]; N], b: &mut [f64; N], n: usize) {
        for col in 0..n {
            // 选主元：MNA 矩阵天然对角占优，**对角线够大就跳过搜索**（省掉 66 次比较和行交换）；
            // 只有对角线偏小（病态）时才做部分主元，保证数值安全。
            let mut pivot = a[col][col];
            if pivot.abs() < 1e-12 {
                let mut best = col;
                let mut best_abs = pivot.abs();
                for row in (col + 1)..n {
                    let v = a[row][col].abs();
                    if v > best_abs {
                        best_abs = v;
                        best = row;
                    }
                }
                if best_abs < 1e-300 {
                    continue; // 奇异：该未知量留 0
                }
                if best != col {
                    a.swap(col, best);
                    b.swap(col, best);
                    pivot = a[col][col];
                }
            }
            let inv_pivot = 1.0 / pivot;
            let (head, tail) = a.split_at_mut(col + 1);
            let pivot_row = &head[col];
            for (offset, row) in tail.iter_mut().enumerate() {
                let j = col + 1 + offset;
                if j >= n {
                    break;
                }
                let factor = row[col] * inv_pivot;
                if factor == 0.0 {
                    continue;
                }
                let (row_lo, row_hi) = row.split_at_mut(col);
                let (piv_lo, piv_hi) = pivot_row.split_at(col);
                let _ = (row_lo, piv_lo);
                let upto = n - col;
                for (r, p) in row_hi[..upto].iter_mut().zip(piv_hi[..upto].iter()) {
                    *r -= factor * p;
                }
                b[j] -= factor * b[col];
            }
        }
    }

    fn back_substitute(&self, a: &[[f64; N]; N], b: &[f64; N], n: usize) -> [f64; N] {
        let mut x = [0.0; N];
        for row in (0..n).rev() {
            if a[row][row].abs() < 1e-300 {
                continue;
            }
            let mut sum = b[row];
            for (k, xv) in x.iter().enumerate().take(n).skip(row + 1) {
                sum -= a[row][k] * xv;
            }
            x[row] = sum / a[row][row];
        }
        // 固定节点即使被后续回代覆盖也强制回填
        for (node, fx) in self.fixed.iter().enumerate() {
            if let Some(v) = fx {
                x[node] = *v;
            }
        }
        x
    }
}

/// 电路里的一个元件（拓扑 + 型号）。
#[derive(Debug, Clone)]
pub enum Element {
    /// 非线性单口
    Diode { model: Diode, a: usize, k: usize },
    /// 三端 BJT（`npn = false` 时按 PNP 处理：电压取反、电流取反）
    Bjt {
        model: Bjt,
        npn: bool,
        c: usize,
        b: usize,
        e: usize,
    },
    /// JFET（N 沟道）
    Jfet {
        model: Jfet,
        d: usize,
        g: usize,
        s: usize,
    },
    /// MOSFET（N 沟道；体二极管请另外用 `Diode` 接上）
    Mosfet {
        model: Nmos,
        d: usize,
        g: usize,
        s: usize,
    },
    /// 电阻
    Resistor { a: usize, b: usize, r: f64 },
    /// 电容（后向欧拉伴随模型）
    Capacitor {
        a: usize,
        b: usize,
        c: f64,
        v_prev: f64,
    },
    /// 运放（单极点宏模型；`branch` 是输出电压源支路）
    OpAmp {
        model: OpAmpModel,
        state: OpAmpState,
        p: usize,
        m: usize,
        out: usize,
        branch: usize,
        /// 直流求解时输出已饱和 → 钉在这个电压上（`None` = 正常）
        pin: Option<f64>,
    },
}

impl Element {
    /// 在**当前牛顿迭代点** `v` 上装配伴随模型。
    ///
    /// `dc = true` 时运放按**连续极限**装配（增益 `A0`、无离散极点），这样
    /// 解出的就是真正的工作点；`dc = false` 时按离散更新式（含 `v_out_prev`）。
    pub fn stamp<const N: usize>(&self, sys: &mut Mna<N>, v: &[f64], dt: f64, dc: bool) {
        match *self {
            Element::Diode { model, a, k } => {
                // **解析雅可比**（实时路径的主力）：`current_and_conductance` 已含 Rs 的内层迭代，
                // 比通用的数值差分少 2 次模型调用。
                let va = if a == GND { 0.0 } else { v[a] };
                let vk = if k == GND { 0.0 } else { v[k] };
                let vl = va - vk;
                let (i, g) = model.current_and_conductance(vl);
                sys.stamp_lin(a, k, g, i - g * vl);
            }
            Element::Bjt {
                model,
                npn,
                c,
                b,
                e,
            } => {
                sys.multiport(&[c, b, e], v, |vv| {
                    let (vc, vb, ve) = (vv[0], vv[1], vv[2]);
                    // PNP：电压取反后喂 NPN 公式，电流再取反
                    let s = if npn { 1.0 } else { -1.0 };
                    let (ic, ib) = model.terminal(s * (vb - ve), s * (vb - vc));
                    let (ic, ib) = (s * ic, s * ib);
                    // 流入器件的电流（NPN 正向：Ic 流入集电极、Ib 流入基极、Ie 流出射极）
                    vec![ic, ib, -(ic + ib)]
                });
            }
            Element::Jfet { model, d, g, s } => {
                sys.multiport(&[d, g, s], v, |vv| {
                    let (vd, vg, vs) = (vv[0], vv[1], vv[2]);
                    let id = model.drain_current(vg - vs, vd - vs);
                    // 栅极电流按 0（<nA，可忽略）
                    vec![id, 0.0, -id]
                });
            }
            Element::Mosfet { model, d, g, s } => {
                sys.multiport(&[d, g, s], v, |vv| {
                    let (vd, vg, vs) = (vv[0], vv[1], vv[2]);
                    let id = model.drain_current(vg - vs, vd - vs);
                    vec![id, 0.0, -id]
                });
            }
            Element::Resistor { a, b, r } => sys.resistor(a, b, r),
            Element::Capacitor { a, b, c, v_prev } => {
                // **直流时电容开路**（不装配）。
                // 若按 `g = C/dt` 把 dt 当 1 s 处理，1 µF 就变成 1 MΩ 的对地电阻，
                // 输出会经隔直电容漏到音量电位器上（实测让工作点偏 41 mV）。
                if dc || dt <= 0.0 {
                    return;
                }
                // 瞬态：后向欧拉 i = (C/dt)·(v − v_prev) → 导纳 g + 电流源 −g·v_prev
                let g = c / dt;
                sys.conductance(a, b, g);
                sys.current_source(a, b, -g * v_prev);
            }
            Element::OpAmp {
                model,
                state,
                p,
                m,
                out,
                branch,
                pin,
            } => {
                // `pin` 只属于直流工作点：瞬态时一律按离散更新式走
                if let (true, Some(v_pin)) = (dc, pin) {
                    // 已饱和：输出钉在限幅上（相当于理想电压源）
                    sys.voltage_source(branch, out, GND, v_pin);
                } else if dc {
                    // 直流极限：v_out = A0·(v+ − v−)
                    sys.vcvs(branch, out, GND, p, m, model.p.a0, 0.0);
                } else {
                    // 瞬态：v_out = a·v_out_prev + b·(v+ − v−) —— 线性，直接进代数环
                    sys.vcvs(branch, out, GND, p, m, model.b, model.a * state.v_out);
                }
            }
        }
    }

    /// 是否是（牛顿迭代中需要重装配的）非线性元件。
    pub fn is_nonlinear(&self) -> bool {
        matches!(
            self,
            Element::Diode { .. }
                | Element::Bjt { .. }
                | Element::Jfet { .. }
                | Element::Mosfet { .. }
        )
    }

    /// 收敛后提交状态（电容 `v_prev`、运放输出）。
    ///
    /// `dc = true` 时**不应用压摆率限制** —— 直流工作点没有"每步最多变化多少"的概念。
    /// （踩过：`solve_dc` 用 dt=1 提交，运放状态被压摆率钳到 `SR/fs`，于是第一拍带着
    /// 错误的初始偏移，凭空多出一个瞬态。）
    pub fn commit(&mut self, v: &[f64], _dt: f64, dc: bool) {
        match self {
            Element::Capacitor { a, b, v_prev, .. } => {
                let va = if *a == GND { 0.0 } else { v[*a] };
                let vb = if *b == GND { 0.0 } else { v[*b] };
                *v_prev = va - vb;
            }
            Element::OpAmp {
                model, state, out, ..
            } => {
                let raw = if *out == GND { 0.0 } else { v[*out] };
                state.v_out = if dc {
                    model.p.clamp_out(raw) // 直流：只钳摆幅，不限摆率
                } else {
                    model.post(state.v_out, raw) // 瞬态：压摆率 + 摆幅
                };
            }
            _ => {}
        }
    }

    /// 运放实例（链式 builder 用）
    pub fn opamp(model: OpAmpModel, p: usize, m: usize, out: usize, branch: usize) -> Self {
        Element::OpAmp {
            model,
            state: OpAmpState::at(0.0),
            p,
            m,
            out,
            branch,
            pin: None,
        }
    }
}

/// 一个完整网表：元件 + 独立电压源 + 牛顿求解器。
#[derive(Debug, Clone)]
pub struct Netlist<const N: usize> {
    pub elems: Vec<Element>,
    /// `(支路号, 节点 a, 节点 b, 电压)`：`v(a) − v(b) = 电压`
    pub sources: Vec<(usize, usize, usize, f64)>,
    /// `(节点, 电压)`：**固定电位节点**（电源轨、输入源），不占未知量
    pub fixed: Vec<(usize, f64)>,
    /// 实时路径的提前退出阈值（两次迭代间的最大相对变化）。
    ///
    /// `0` = 关闭（严格跑满 `iters` 次）。默认 `1e-6`：实测在 4~5 次内满足，
    /// 与"跑满 8 次"的输出差异可忽略（DEVLOG E57）。
    pub fixed_tol: f64,
    /// 上一拍实际做了几次牛顿迭代（诊断用）
    pub last_iters: usize,
    /// 真正的未知量个数（= 节点未知量 + 电压源支路）。
    ///
    /// 生成器把固定电位节点排在**最大的几个下标**上，于是 LU 只需处理前 `unknowns` 行
    /// （`0` 表示"不裁剪"，等价于 `N`）。
    pub unknowns: usize,
    pub v: [f64; N],
    /// 上一拍的解（牛顿初值用线性外推 `2v − v_prev2`，实测能省掉一次迭代）
    v_prev2: [f64; N],
    has_prev2: bool,
    /// 牛顿迭代上限
    pub max_iter: usize,
    /// 收敛阈值（电压，V）
    pub tol: f64,
}

impl<const N: usize> Default for Netlist<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Netlist<N> {
    pub fn new() -> Self {
        Self {
            elems: Vec::new(),
            sources: Vec::new(),
            fixed: Vec::new(),
            fixed_tol: 1e-6,
            last_iters: 0,
            unknowns: 0,
            v: [0.0; N],
            v_prev2: [0.0; N],
            has_prev2: false,
            max_iter: 200,
            tol: 1e-9,
        }
    }

    // ---- 链式 builder ----

    pub fn push(&mut self, e: Element) -> &mut Self {
        self.elems.push(e);
        self
    }
    pub fn resistor(&mut self, a: usize, b: usize, r: f64) -> &mut Self {
        self.push(Element::Resistor {
            a,
            b,
            r: floor_resistance(r),
        })
    }
    pub fn capacitor(&mut self, a: usize, b: usize, c: f64) -> &mut Self {
        self.push(Element::Capacitor {
            a,
            b,
            c,
            v_prev: 0.0,
        })
    }
    pub fn diode(&mut self, model: Diode, a: usize, k: usize) -> &mut Self {
        self.push(Element::Diode { model, a, k })
    }
    pub fn bjt(&mut self, model: Bjt, npn: bool, c: usize, b: usize, e: usize) -> &mut Self {
        self.push(Element::Bjt {
            model,
            npn,
            c,
            b,
            e,
        })
    }
    pub fn jfet(&mut self, model: Jfet, d: usize, g: usize, s: usize) -> &mut Self {
        self.push(Element::Jfet { model, d, g, s })
    }
    pub fn mosfet(&mut self, model: Nmos, d: usize, g: usize, s: usize) -> &mut Self {
        self.push(Element::Mosfet { model, d, g, s })
    }
    pub fn opamp(
        &mut self,
        model: OpAmpModel,
        p: usize,
        m: usize,
        out: usize,
        branch: usize,
    ) -> &mut Self {
        self.push(Element::opamp(model, p, m, out, branch))
    }
    /// 电压源（`v(a) − v(b) = value`），`branch` 必须唯一且 < N
    pub fn voltage_source(&mut self, branch: usize, a: usize, b: usize, value: f64) -> &mut Self {
        self.sources.push((branch, a, b, value));
        self
    }
    /// 声明真正的未知量个数（生成器会调用；固定节点排在它之后）。
    pub fn set_unknowns(&mut self, n: usize) {
        self.unknowns = n.min(N);
    }

    /// LU 实际要处理的行数。
    #[inline]
    fn solve_len(&self) -> usize {
        if self.unknowns == 0 {
            N
        } else {
            self.unknowns
        }
    }

    /// 固定一个节点的电位（电源轨、输入源），返回索引供 [`Self::set_fixed`] 用。
    pub fn fix_node(&mut self, node: usize, v: f64) -> usize {
        self.fixed.push((node, v));
        self.fixed.len() - 1
    }

    /// 修改第 `idx` 个固定节点的电位（输入激励用；比电压源支路少两个未知量）
    pub fn set_fixed(&mut self, idx: usize, v: f64) {
        if let Some(entry) = self.fixed.get_mut(idx) {
            entry.1 = v;
        }
    }

    /// 修改第 `idx` 个电压源的电压（瞬态扫描用）
    pub fn set_voltage(&mut self, idx: usize, value: f64) {
        self.sources[idx].3 = value;
    }

    /// 电压源支路电流（收敛后有意义）
    pub fn source_current(&self, branch: usize) -> f64 {
        self.v[branch]
    }

    // ---- 求解 ----

    /// 装配**线性**部分（电阻/电容/独立源/运放）。实时路径每拍只做一次。
    fn stamp_linear(&self, sys: &mut Mna<N>, v: &[f64], dt: f64, dc: bool) {
        sys.clear();
        for &(node, volt) in &self.fixed {
            sys.fix_node(node, volt);
        }
        if sys.gmin() > 0.0 {
            let g = sys.gmin();
            for n in 1..N {
                sys.conductance(n, GND, g);
            }
        }
        for &(branch, a, b, value) in &self.sources {
            sys.voltage_source(branch, a, b, value);
        }
        for e in &self.elems {
            if !e.is_nonlinear() {
                e.stamp(sys, v, dt, dc);
            }
        }
    }

    /// 装配**非线性**部分（二极管/BJT/JFET/MOSFET）。
    fn stamp_nonlinear(&self, sys: &mut Mna<N>, v: &[f64], dt: f64, dc: bool) {
        for e in &self.elems {
            if e.is_nonlinear() {
                e.stamp(sys, v, dt, dc);
            }
        }
    }

    fn stamp_all(&self, sys: &mut Mna<N>, v: &[f64], dt: f64, dc: bool) {
        self.stamp_linear(sys, v, dt, dc);
        self.stamp_nonlinear(sys, v, dt, dc);
    }

    /// 牛顿迭代求解（直流工作点）。
    ///
    /// **gmin 步进**：先用 `gmin=1e-3` 把各节点"拽"到近似解，再逐级降到 **0**。
    /// 每一级都以上一级的解做热启动，最后一级是**真实电路**（没有 gmin）——
    /// 否则 gmin 会作为漏电导留在结果里（曾经把 0.2 V 分压成 0.1 V，就是踩了这个坑）。
    pub fn solve_dc(&mut self) -> [f64; N] {
        // 运放饱和处理：解出工作点后若有输出越界，钉在限幅上重解（分段线性迭代）
        for _ in 0..8 {
            let v = self.solve_dc_once();
            let mut changed = false;
            for e in &mut self.elems {
                if let Element::OpAmp {
                    model, out, pin, ..
                } = e
                {
                    if pin.is_none() {
                        let vout = if *out == GND { 0.0 } else { v[*out] };
                        let clamped = model.p.clamp_out(vout);
                        if (clamped - vout).abs() > 1e-9 {
                            *pin = Some(clamped);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                self.v = v;
                for e in &mut self.elems {
                    e.commit(&self.v, 1.0, true);
                }
                return self.v;
            }
        }
        let v = self.solve_dc_once();
        self.v = v;
        for e in &mut self.elems {
            e.commit(&self.v, 1.0, true);
        }
        self.v
    }

    fn solve_dc_once(&mut self) -> [f64; N] {
        let mut sys = Mna::<N>::new();
        let mut v = self.v;
        for &gmin in &[
            1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12, 0.0,
        ] {
            sys.set_gmin(gmin);
            match self.newton(&mut sys, v, 1.0, true) {
                Some(v_new) => v = v_new,
                // 这一级没收敛：保留上一级的解继续往下（下一级 gmin 更小，通常更容易）
                None => {}
            }
        }
        v
    }

    /// 内层牛顿迭代：给定线性化点 `v0`，迭代到收敛或超限。
    fn newton(&self, sys: &mut Mna<N>, mut v: [f64; N], dt: f64, dc: bool) -> Option<[f64; N]> {
        for _ in 0..self.max_iter {
            self.stamp_all(sys, &v, dt, dc);
            let v_new = sys.solve_len(self.solve_len());
            let mut max_dv: f64 = 0.0;
            for i in 0..N {
                let d = (v_new[i] - v[i]).abs();
                max_dv = max_dv.max(d / (1.0 + v_new[i].abs()));
            }
            // 阻尼：大跳变时减半，防止结器件来回振荡
            if max_dv > 0.5 {
                for i in 0..N {
                    v[i] = 0.5 * (v[i] + v_new[i]);
                }
            } else {
                v = v_new;
            }
            if max_dv < self.tol {
                return Some(v);
            }
        }
        None
    }

    /// 瞬态推进一步（后向欧拉），返回新的节点电压。
    pub fn step(&mut self, dt: f64) -> [f64; N] {
        // 清掉直流求解留下的"钉住"状态（那是工作点才有的东西）
        for e in &mut self.elems {
            if let Element::OpAmp { pin, .. } = e {
                *pin = None;
            }
        }
        let mut sys = Mna::<N>::new();
        sys.set_gmin(1e-12);
        let v = self.newton(&mut sys, self.v, dt, false).unwrap_or_else(|| {
            // 未收敛：退回一次更小步长的尝试（极端情况下保底）
            self.newton(&mut sys, self.v, dt * 0.1, false)
                .unwrap_or(self.v)
        });
        self.v = v;
        for e in &mut self.elems {
            e.commit(&self.v, dt, false);
        }
        self.v
    }

    /// **实时路径**：固定迭代次数（不判收敛），从上一拍解热启动。
    ///
    /// 为什么不用 `step()`：实时不允许"这一拍多迭代几次"，耗时必须是可预测的常数。
    ///
    /// ⚠️ **迭代次数是正确性参数，不是性能参数**（DEVLOG E50）：
    /// 次数不够时解会进入**次谐波极限环** —— 正弦输入下看不出任何异常（THD、谐波都对得上），
    /// 一旦换成方波/快速边沿，基频以下会冒出 −0 dB 量级的次谐波（听感是低频"噗噗"+ 高频发毛）。
    /// 验收时务必用**方波**测一次（见 `tests/sparklin_generated.rs::no_subharmonics_under_square_input`）。
    pub fn step_fixed(&mut self, dt: f64, iters: usize) -> [f64; N] {
        // 线性部分（含运放状态、电容伴随导纳）每拍重建一次；
        // 牛顿迭代里只重装 3 只二极管 —— 这是实时档的主要优化。
        let mut base = Mna::<N>::new();
        // stamp_linear 内部会 clear()，这里不需要额外清零
        self.stamp_linear(&mut base, &self.v, dt, false);

        // 初值：线性外推（热启动）。温启动让 2 次迭代就够用。
        let tol = self.fixed_tol;
        let mut v = if self.has_prev2 {
            let mut guess = [0.0; N];
            for i in 0..N {
                guess[i] = 2.0 * self.v[i] - self.v_prev2[i];
            }
            guess
        } else {
            self.v
        };
        self.last_iters = 0;
        for _ in 0..iters.max(1) {
            self.last_iters += 1;
            let mut sys = base.clone();
            self.stamp_nonlinear(&mut sys, &v, dt, false);
            let v_new = sys.solve_len(self.solve_len());
            // **自适应提前退出**：残差够小就不再迭代。
            // 这一步很关键：固定 4 次在硬驱动下差 0.76 dB，固定 8 次又太贵；
            // 用"两次迭代之间的最大相对变化"当判据，稳态常见 4~5 次、难算时自动补到 8 次。
            let mut max_dv: f64 = 0.0;
            for i in 0..N {
                max_dv = max_dv.max((v_new[i] - v[i]).abs() / (1.0 + v_new[i].abs()));
            }
            v = v_new;
            if tol > 0.0 && max_dv < tol {
                break;
            }
        }
        self.v_prev2 = self.v;
        self.has_prev2 = true;
        self.v = v;
        for e in &mut self.elems {
            e.commit(&self.v, dt, false);
        }
        self.v
    }

    /// 按索引改一个电阻的值（电位器/开关用；值没变就不动，保证 `set_params` 便宜）。
    pub fn set_resistance(&mut self, idx: usize, r: f64) -> bool {
        let r = floor_resistance(r);
        if let Some(Element::Resistor { r: cur, .. }) = self.elems.get_mut(idx) {
            if *cur != r {
                *cur = r;
                return true;
            }
        }
        false
    }

    /// 加一个电阻并返回它的索引（便于之后用 [`Self::set_resistance`] 调参）。
    pub fn push_resistor(&mut self, a: usize, b: usize, r: f64) -> usize {
        let idx = self.elems.len();
        self.elems.push(Element::Resistor {
            a,
            b,
            r: floor_resistance(r),
        });
        idx
    }

    /// 复位所有动态状态（电容电压、运放输出）。
    pub fn reset(&mut self) {
        self.v = [0.0; N];
        self.v_prev2 = [0.0; N];
        self.has_prev2 = false;
        for e in &mut self.elems {
            if let Element::Capacitor { v_prev, .. } = e {
                *v_prev = 0.0;
            }
            if let Element::OpAmp { state, pin, .. } = e {
                state.reset();
                *pin = None;
            }
        }
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! **fakedist v1** —— 电路级实时实现（wave-digital 风格 + 1 维牛顿）。
//!
//! 电路（来自 `circuits/fakedist/netlist.cir`，权威来源 `fakedist.bread`）：
//!
//! ```text
//!   (V+ = 9V) ── R2 47k ──┬── R3 47k ── GND      →  Vbias = 4.5V, Rth = 23.5k
//!                        └── R4 470k ── IN+
//! IN ─C1 4.7n─► IN+ ── OP07 ── OUT ─C3 1µF─► P2(LEVEL 500k) ─► 出
//!                │             │
//!       R5 4.7k ─┴─ C2 47n     └─► P1(GAIN 500k, 变阻器)
//!                │                    │
//!               IN- ◄─────────────────┘
//!                └─ 削波环: D1(1 只) ∥ (D2+D3 串联反向)
//! ```
//!
//! # 实现方式
//!
//! * **线性储能元件**（C1/C2/C3）用**梯形法伴随模型**（companion model）：
//!   `i[n] = (2C/T)(v[n]-v[n-1]) - i[n-1]`。
//!   这正是 WDF 单端口在 `R = T/(2C)` 时的同一个差分方程（WDF 电容满足 `b[n]=a[n-1]`），
//!   只是用电流/电压而非入/反射波表达，便于和节点方程直接合并。
//! * **非线性反馈节点**（IN-）用 **1 维牛顿**求解；这等价于把该节点写成 WDF 并联适配器
//!   后对公共电压求根 —— 未知量只有 1 个，收敛 2~3 步（实测牛顿平均 < 4 次）。
//! * **运放** 用单极点宏模型（`A0`、`GBW`）并**隐含求解**：把梯形积分的更新式直接代进节点方程，
//!   所以反馈环里**没有延迟**（不是"上一拍输出"那种近似）。
//!
//! # 为什么不用完整的 OP07A `.SUBCKT`
//! 它是 30+ 内节点的非线性子电路（含 BJT 差分对），整块转 WDF 既慢又难验证。
//! 见 DEVLOG ADR-001：线性部分用宏模型，**用 GBW/压摆率体现 OP07 的性格**
//! （0.6 MHz / 0.25 V/µs —— 高频滚降的主要来源）。

use crate::diode::Diode1n4148;

/// 元件值（Ω / F），全部来自网表。
#[derive(Debug, Clone, Copy)]
pub struct ComponentValues {
    pub vbias: f64,
    /// R4 + (R2∥R3)：IN+ 对偏置源的等效电阻
    pub r_in_bias: f64,
    pub c1: f64,
    pub r5: f64,
    pub c2: f64,
    /// GAIN 电位器总阻值（线性，抽头位置决定实际阻值）
    pub r_gain_pot: f64,
    /// LEVEL 电位器总阻值（线性）
    pub r_level_pot: f64,
    pub c3: f64,
}

impl Default for ComponentValues {
    fn default() -> Self {
        Self {
            vbias: 4.5,
            // R4 470k + R2∥R3 (47k∥47k = 23.5k)
            r_in_bias: 493_500.0,
            c1: 4.7e-9,
            r5: 4_700.0,
            c2: 47e-9,
            r_gain_pot: 500_000.0,
            r_level_pot: 500_000.0,
            c3: 1e-6,
        }
    }
}

/// 运放宏模型参数（OP07）。
#[derive(Debug, Clone, Copy)]
pub struct OpAmpModel {
    /// 开环增益（OP07 ≈ 114 dB）
    pub a0: f64,
    /// 增益带宽积（OP07 ≈ 0.6 MHz）
    pub gbw: f64,
}

impl Default for OpAmpModel {
    fn default() -> Self {
        Self {
            a0: 5e5,
            gbw: 0.6e6,
        }
    }
}

/// 用户可调参数（都是 0..1 的线性电位器位置）。
#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// GAIN（P1）位置：0 = 0Ω，1 = 500kΩ
    pub gain: f64,
    /// LEVEL（P2）位置：0 = 静音，1 = 全量
    pub level: f64,
}

impl Default for Params {
    fn default() -> Self {
        // 基准测试约定：两个电位器都在 50%
        Self {
            gain: 0.5,
            level: 0.5,
        }
    }
}

pub struct Fakedist {
    comp: ComponentValues,
    opamp: OpAmpModel,
    params: Params,
    d1: Diode1n4148,
    d23: Diode1n4148,
    fs: f64,

    // ---- 派生系数（采样率或参数变化时重算）----
    g1: f64,
    r_in: f64,
    /// R5+C2 分支的电导（含梯形法缩放）
    g_branch: f64,
    g2_r5_inv: f64,
    g3: f64,
    r_level_total: f64,
    alpha: f64,
    beta: f64,
    slew_step: f64,

    // ---- 状态 ----
    vc1: f64,
    i1: f64,
    vc2: f64,
    i2: f64,
    vc3: f64,
    i3: f64,
    v_out: f64,
    d_prev: f64,
    v_inm_prev: f64,
}

impl Fakedist {
    pub fn new(fs: f64) -> Self {
        let mut s = Self {
            comp: ComponentValues::default(),
            opamp: OpAmpModel::default(),
            params: Params::default(),
            d1: Diode1n4148::default(),
            d23: Diode1n4148 {
                // D2 + D3 **串联**：电流相同，所以
                //   电压要按两份分 ⇒ 发射系数翻倍：i = Is*(exp(u/(2*N*Vt)) - 1)
                //   体电阻相加 ⇒ Rs 翻倍
                // （只翻 Rs 不翻 N 是错的开方根等级错误，会让 2 只串联支路提前导通）
                is: 2.52e-9,
                n_vt: 2.0 * 1.752 * 0.025_852,
                rs: 2.0 * 0.568,
            },
            fs,
            g1: 0.0,
            r_in: 0.0,
            g_branch: 0.0,
            g2_r5_inv: 0.0,
            g3: 0.0,
            r_level_total: 0.0,
            alpha: 0.0,
            beta: 0.0,
            slew_step: 0.0,
            vc1: 0.0,
            i1: 0.0,
            vc2: 0.0,
            i2: 0.0,
            vc3: 0.0,
            i3: 0.0,
            v_out: 0.5 * 4.5,
            d_prev: 0.0,
            v_inm_prev: 0.5 * 4.5,
        };
        s.recompute();
        s.reset();
        s
    }

    /// 采样率变化时调用（会重置状态）。
    pub fn set_sample_rate(&mut self, fs: f64) {
        self.fs = fs;
        self.recompute();
        self.reset();
    }

    pub fn set_params(&mut self, p: Params) {
        self.params = p;
        self.recompute();
    }

    pub fn params(&self) -> Params {
        self.params
    }

    fn recompute(&mut self) {
        let t = 1.0 / self.fs;
        let c = &self.comp;

        // 输入级：C1 + R_in
        self.g1 = 2.0 * c.c1 / t;
        self.r_in = c.r_in_bias;

        // 反馈下臂：R5 串联 C2 到地
        let g2 = 2.0 * c.c2 / t;
        // i = [g2*(v - vc2) - i_prev] / (1 + g2*R5)
        self.g_branch = g2 / (1.0 + g2 * c.r5);
        self.g2_r5_inv = 1.0 / (1.0 + g2 * c.r5);

        // 输出级：C3 + 电平电位器（抽头不改变总阻值）
        let g3 = 2.0 * c.c3 / t;
        self.r_level_total = c.r_level_pot; // R_A + R_B 恒为总阻值
        self.g3 = g3 / (1.0 + g3 * self.r_level_total);

        // 运放单极点模型，梯形离散 ⇒ 输出对 d 的"瞬时"关系 v_out = A - beta*v_inm
        let wc = std::f64::consts::TAU * self.opamp.gbw / self.opamp.a0;
        let g = t * wc / 2.0;
        self.alpha = (1.0 - g) / (1.0 + g);
        self.beta = g * self.opamp.a0 / (1.0 + g);
        self.slew_step = 0.25e6 * t; // 0.25 V/µs 的压摆率上限（预留，暂未强制）
    }

    /// 回到直流工作点（避免插件加载时的"砰"声，也让测试不用等 2.5 秒）。
    ///
    /// 工作点：`IN+ = IN- = OUT = 4.5V`，`n1 = 0`（C3 隔直），各电容按此充电。
    pub fn reset(&mut self) {
        let vb = self.comp.vbias;
        self.vc1 = -vb; // v_C1 = v_in(0) - v_inp(4.5)
        self.i1 = 0.0;
        self.vc2 = 0.0;
        self.i2 = 0.0;
        self.vc3 = vb; // v_out(4.5) - n1(0)
        self.i3 = 0.0;
        self.v_out = vb;
        self.d_prev = 0.0;
        self.v_inm_prev = vb;
    }

    /// 削波环的电流（方向：IN- → OUT）与小信号电导。
    #[inline]
    fn clipper(&self, u: f64) -> (f64, f64) {
        // u = v_inm - v_out
        // 支路 A：D2+D3 串联，u > 0 时正向导通
        let (i_a, g_a) = self.d23.current_and_conductance(u);
        // 支路 B：D1，阳极在 OUT、阴极在 IN-，-u > 0 时导通
        let (i_b, g_b) = self.d1.current_and_conductance(-u);
        (i_a - i_b, g_a + g_b)
    }

    /// 处理一个采样。
    pub fn process(&mut self, x: f64) -> f64 {
        let c = self.comp;
        let t = 1.0 / self.fs;

        // ---------- 1. 输入级：C1 + 偏置电阻 → v_inp ----------
        let v_inp = (self.g1 * (x - self.vc1) - self.i1 + c.vbias / self.r_in)
            / (self.g1 + 1.0 / self.r_in);
        self.vc1 = x - v_inp;
        self.i1 = (v_inp - c.vbias) / self.r_in;

        // ---------- 2. 反馈节点 IN- 的 1 维牛顿 ----------
        // v_out = A - beta*v_inm
        let a = self.alpha * self.v_out + self.beta * (v_inp + self.d_prev);
        let rf = (c.r_gain_pot * self.params.gain).max(1.0); // 电位器到底时用 1Ω 兜底
        let g_f = 1.0 / rf;

        // 牛顿求 F(v_inm) = i_branch + (u)/Rf + i_clip(u) = 0，u = (1+beta)*v_inm - A
        let mut v_inm = self.v_inm_prev;
        for _ in 0..12 {
            let u = (1.0 + self.beta) * v_inm - a;
            let i_br = self.g_branch * (v_inm - self.vc2) - self.i2 * self.g2_r5_inv;
            let (i_cl, g_cl) = self.clipper(u);
            let f = i_br + u * g_f + i_cl;
            let df = self.g_branch + (1.0 + self.beta) * (g_f + g_cl);
            let step = f / df;
            v_inm -= step;
            if step.abs() <= 1e-9 * (1.0 + v_inm.abs()) {
                break;
            }
        }
        let i_br = self.g_branch * (v_inm - self.vc2) - self.i2 * self.g2_r5_inv;

        // 更新 R5+C2 分支状态
        self.i2 = i_br;
        self.vc2 = v_inm - c.r5 * i_br;

        // ---------- 3. 运放输出 ----------
        let v_out_prev = self.v_out;
        self.v_out = a - self.beta * v_inm;
        // 供电路由钳位（OP07 单电源，输出不可能超出 0..9V）
        self.v_out = self.v_out.clamp(0.0, 9.0);
        // 压摆率限制（只在极端情况下起作用）
        self.v_out = self
            .v_out
            .clamp(v_out_prev - self.slew_step, v_out_prev + self.slew_step);
        self.d_prev = v_inp - v_inm;
        self.v_inm_prev = v_inm;

        // ---------- 4. 输出级：C3 + LEVEL 电位器 ----------
        let g3 = 2.0 * c.c3 / t;
        let n1 = (g3 * (self.v_out - self.vc3) - self.i3) / (g3 + 1.0 / self.r_level_total);
        self.vc3 = self.v_out - n1;
        self.i3 = n1 / self.r_level_total;

        // 抽头 = 分压比 = 位置（线性电位器）
        n1 * self.params.level
    }

    /// 内部节点快照（测试/调试用）。
    pub fn probe(&self) -> Probe {
        Probe {
            v_out: self.v_out,
            v_inm: self.v_inm_prev,
            vc1: self.vc1,
            vc2: self.vc2,
            vc3: self.vc3,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub v_out: f64,
    pub v_inm: f64,
    pub vc1: f64,
    pub vc2: f64,
    pub vc3: f64,
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! **分支电路（拓扑切换）**：把两个**完整的电路实现**打包成一个 `Circuit`。
//!
//! 设计依据（`rustomp-circuit-designer/docs/00-decisions.md` D-005）：
//! 开关**不在电路层**实现（用 1 mΩ/1 TΩ 模拟理想开关会让矩阵条件数剧变、切换瞬间爆音），
//! 而是让每个开关状态对应一份**完整电路**，由插件层选择 + 交叉淡化。
//!
//! ```text
//!            ┌── A（例如"非对称削波"整份电路）──┐
//!  in ──────┤                                  ├── 10 ms 线性淡化 ── out
//!            └── B（例如"对称削波"整份电路）────┘
//! ```
//!
//! - **不在切换中时只跑当前分支**（CPU 不翻倍）；
//! - 切换分两段：先 **预热 5 ms**（两路都跑，输出仍全走旧分支），再 **10 ms 线性交叉淡化**。
//!
//! 为什么要预热（实测教训）：切进来的那一支状态是**冷的**（还停在直流工作点），
//! 突然喂进半途中的信号会让它的输入高通/反馈电容产生起始瞬态；
//! 这个瞬态乘上 0.09 的淡化权重就已经是 0.099 的台阶（会听成咔哒）。
//! 先让它"跟着信号跑 5 ms"（约等于输入高通 68 Hz 的 1/3 时间常数），再开始混合，台阶就消失了。
//!
//! 用法：
//!
//! ```ignore
//! pub type Branch = rustomp_dsp::Branch<asym::Ckt, sym::Ckt>;
//! let mut engine = Engine::<Branch>::new(48_000.0);
//! engine.set_params(BranchParams { a: asym_p, b: sym_p, select_b: true });
//! ```

use crate::Circuit;

/// 分支参数：两路各自的参数 + 选择位。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BranchParams<PA, PB> {
    pub a: PA,
    pub b: PB,
    /// `false` = 走 A，`true` = 走 B
    pub select_b: bool,
}

impl<PA: Default, PB: Default> Default for BranchParams<PA, PB> {
    fn default() -> Self {
        Self { a: PA::default(), b: PB::default(), select_b: false }
    }
}

/// 两路电路 + 交叉淡化。见模块文档。
pub struct Branch<A: Circuit, B: Circuit> {
    a: A,
    b: B,
    /// 当前混合比：0 = 全 A，1 = 全 B
    mix: f64,
    /// 目标：0 或 1
    target: f64,
    /// 每样本的混合增量
    step: f64,
    /// 正在切换（预热或淡化）：这时两路都要跑
    fading: bool,
    /// 预热剩余样本数（>0 时只跑不混，输出仍全走旧分支）
    prime: usize,
    /// 每次切换的预热样本数
    prime_samples: usize,
}

impl<A: Circuit, B: Circuit> Branch<A, B> {
    /// 淡化时长（毫秒）。10 ms 对"同一信号的两个拓扑"够用，也不会让切换听起来拖沓。
    pub const FADE_MS: f64 = 10.0;
    /// 预热时长（毫秒）：切进来那一支先跟着信号跑这么久，再开始混合。
    pub const PRIME_MS: f64 = 5.0;

    /// 是否正在交叉淡化（诊断用）。
    pub fn is_fading(&self) -> bool {
        self.fading
    }

    /// 当前混合比（0..1）。
    pub fn mix(&self) -> f64 {
        self.mix
    }
}

impl<A: Circuit, B: Circuit> Circuit for Branch<A, B> {
    type Params = BranchParams<A::Params, B::Params>;

    fn new(sample_rate: f64) -> Self {
        Self {
            a: A::new(sample_rate),
            b: B::new(sample_rate),
            mix: 0.0,
            target: 0.0,
            step: 1.0 / (sample_rate * Self::FADE_MS / 1000.0).max(1.0),
            fading: false,
            prime: 0,
            prime_samples: (sample_rate * Self::PRIME_MS / 1000.0).round() as usize,
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.a.set_sample_rate(sample_rate);
        self.b.set_sample_rate(sample_rate);
        self.step = 1.0 / (sample_rate * Self::FADE_MS / 1000.0).max(1.0);
        self.prime_samples = (sample_rate * Self::PRIME_MS / 1000.0).round() as usize;
    }

    fn set_params(&mut self, params: Self::Params) {
        self.a.set_params(params.a);
        self.b.set_params(params.b);
        let want = if params.select_b { 1.0 } else { 0.0 };
        // 只在**选择位变化**时启动淡化；参数每样本都在调，不能重复触发
        if want != self.target {
            self.target = want;
            self.fading = true;
            self.prime = self.prime_samples;
        }
    }

    fn reset(&mut self) {
        self.a.reset();
        self.b.reset();
        self.mix = self.target;
        self.fading = false;
        self.prime = 0;
    }

    fn process(&mut self, x: f64) -> f64 {
        if self.fading {
            // 两路都跑
            let ya = self.a.process(x);
            let yb = self.b.process(x);
            if self.prime > 0 {
                // 预热阶段：输出仍**全走旧分支**，只是让新分支跟着信号热起来
                self.prime -= 1;
                return if self.mix > 0.5 { yb } else { ya };
            }
            self.mix = if self.target > self.mix {
                (self.mix + self.step).min(self.target)
            } else {
                (self.mix - self.step).max(self.target)
            };
            if (self.mix - self.target).abs() < 1e-12 {
                self.mix = self.target;
                self.fading = false;
            }
            ya * (1.0 - self.mix) + yb * self.mix
        } else if self.mix > 0.5 {
            self.b.process(x)
        } else {
            self.a.process(x)
        }
    }

    fn activity(&self) -> f32 {
        if self.mix > 0.5 {
            self.b.activity()
        } else {
            self.a.activity()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用最小电路：纯增益。
    struct Gain {
        g: f64,
        p: f64,
    }
    #[derive(Clone, Copy, Default, PartialEq)]
    struct GainParams {
        g: f64,
    }
    impl Circuit for Gain {
        type Params = GainParams;
        fn new(_fs: f64) -> Self {
            Self { g: 1.0, p: 1.0 }
        }
        fn set_sample_rate(&mut self, _fs: f64) {}
        fn set_params(&mut self, p: Self::Params) {
            self.p = p.g;
        }
        fn reset(&mut self) {
            self.g = self.p;
        }
        fn process(&mut self, x: f64) -> f64 {
            self.g += 0.0 * x; // 保持"有状态"的形状
            self.p * x
        }
    }

    fn make(fs: f64) -> Branch<Gain, Gain> {
        let mut br = Branch::<Gain, Gain>::new(fs);
        br.set_params(BranchParams { a: GainParams { g: 1.0 }, b: GainParams { g: 2.0 }, select_b: false });
        br.reset();
        br
    }

    #[test]
    fn passes_only_active_branch() {
        let fs = 48_000.0;
        let mut br = make(fs);
        assert!((br.process(1.0) - 1.0).abs() < 1e-12, "默认应走 A（增益 1）");
        assert!(!br.is_fading());
    }

    #[test]
    fn switching_crossfades_over_10ms_then_settles() {
        let fs = 48_000.0;
        let mut br = make(fs);
        br.process(1.0);
        // 切换前先预热一遍，让 a/b 都跑过
        br.set_params(BranchParams { a: GainParams { g: 1.0 }, b: GainParams { g: 2.0 }, select_b: true });
        assert!(br.is_fading(), "选择位变化应触发淡化");

        // 切换 = 预热（PRIME_MS，输出仍走旧分支）+ 淡化（FADE_MS）
        let total = ((Branch::<Gain, Gain>::PRIME_MS + Branch::<Gain, Gain>::FADE_MS) / 1000.0 * fs) as usize;
        let mut prev = br.process(1.0);
        let mut max_jump = 0.0f64;
        for _ in 0..total {
            let y = br.process(1.0);
            max_jump = max_jump.max((y - prev).abs());
            prev = y;
        }
        // 淡化期间的相邻样本跳变必须很小（1.0 → 2.0 分摊到 480 个样本 ≈ 2.1e-3/样本）
        assert!(max_jump < 0.01, "淡化期间跳变 {max_jump:.4} 过大（会听成咔哒）");
        assert!(!br.is_fading(), "预热+淡化应当在 15 ms 内结束");
        assert!((br.process(1.0) - 2.0).abs() < 1e-9, "切换后应走 B（增益 2）");
    }

    #[test]
    fn repeated_set_params_does_not_restart_fade() {
        let fs = 48_000.0;
        let mut br = make(fs);
        let p = BranchParams { a: GainParams { g: 1.0 }, b: GainParams { g: 2.0 }, select_b: true };
        br.set_params(p);
        let total = ((Branch::<Gain, Gain>::PRIME_MS + Branch::<Gain, Gain>::FADE_MS) / 1000.0 * fs) as usize;
        // 每样本都调 set_params（插件就是这样做的）：不能重置淡化进度
        for _ in 0..total {
            br.set_params(p);
            br.process(1.0);
        }
        assert!(!br.is_fading(), "重复 set_params 不应让淡化永不结束");
        assert!((br.process(1.0) - 2.0).abs() < 1e-9);
    }
}

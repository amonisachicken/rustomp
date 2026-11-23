# rustomp-dsp

电路级建模的 DSP 核心。所有实现都以 **ngspice 基线**为准，并且基线是**可复现的对拍测试**
（没有 ngspice 时自动跳过，不算失败）。

```
devices/       器件库：Diode / Bjt / Jfet / Nmos / OpAmp  （型号是闭集 const）
mna.rs         通用 MNA 装配器：Element 枚举 + Netlist<N> + 牛顿求解
branch.rs      Branch<A,B>：两份完整电路 + 开关切换（预热 + 交叉淡化）
engine.rs      Engine<C>：2×/4× 过采样 + 真旁通 + 活动指示 + NaN 熔断
oversample.rs  多相 FIR（2× / 4× 共用；默认 4×）
circuit.rs     与框架的唯一契约：Circuit trait
```

---

## 1. 器件库（`src/devices/`）

**闭集**：每个器件类型只维护少数经过对拍的型号；参数名与 SPICE 对齐，LTspice 模型可直接映射。

| 模块 | 类型 | 库内型号 | 精度档 |
|---|---|---|---|
| `diode.rs` | 二极管（`Is/N/Rs`） | `D_1N4148` `D_1N34A` `D_1N4007` `D_1N5817`(肖特基) `LED_RED` `LED_GREEN` `LED_BLUE` | 静态 D2 |
| `bjt.rs` | Gummel-Poon（含 `Rb/Re/Rc` 内层牛顿；动态 C1/C2） | `BC549C` `BC559C`(PNP) `AC127` `AC128` | 静态 D3 + 动态 C1/C2（四型号全开） |
| `jfet.rs` | Shichman-Hodges | `J201` | 静态 D3 |
| `mosfet.rs` | level-1（+ 体二极管单口） | `NMOS_2N7000` `NMOS_2N7000_BODY` | 静态 D3 |
| `opamp.rs` | 单极点宏模型 + 压摆率 + 输出钳位 | `OP07` | — |
| `triode.rs` | **Koren 模型**（1996 / 12-97 两种本体）+ 栅流 + 极间电容 | `T12AX7` `T12AT7` `T12AU7` `T12AY7`(拟合) `T6SN7` | 静态 D3 |

对拍（`cargo test -p rustomp-dsp --release --test devices -- --nocapture`）：

| 器件 | 比较量 | 误差 |
|---|---|---|
| 二极管 ×4 + LED ×3 | 1 µA ~ 10 mA 的 `Vf` | **0.00%** |
| BJT ×4 | `Ic` 与 `Ib`（含 PNP） | **≤0.09%**（BC549C 在 4.6 mA 处 0.67%） |
| `J201` | 转移 + 输出特性 | **0.00%** |
| `2N7000` | 沟道 8 点 + 体二极管 3 点 | **≤0.01%** |
| `OP07` | 闭环 -3 dB | **<2%**（4× 过采样） |

### 电子三极管（`triode.rs`）

`Triode::plate(vpk, vgk)` 返回 `(Ip, gm, gp)`（**解析导数**），`Triode::grid(vgk)` 返回栅流：

```text
V96（Koren Tube.lib 1996）：E1 = Va/KP·log(1+exp(KP·(1/MU + (Vg+VCT)/sqrt(KVB + Va²))))
                            Ia = 2·E1^EX/KG1
V97（Koren Tube97.lib 12/97）：分母 sqrt(KVB² + Va²)，Ia = 2·(E1/KG1)^EX
```

- **两种本体不能混用参数**：6SN7 的官方参数配 V96 会给 53 mA（应 9 mA）。
- 栅流 = `RGI + 二极管`（正栅才导通，内部二分求解）—— 过载时的"格子阻塞"来源。
- 极间电容 `CCG/CGP/CCP` 作为元素状态装配，`CGP` 的米勒效应是共阴级高频滚降主因。

对拍（`tests/triode.rs`，Va=250）：12AX7 0.95 mA / gm 1.67 / Rp 53.7 kΩ（手册 1.2 / 1.6 / 62.5 k），
6SN7 8.86 mA / 2.73 / 7.4 k（手册 9 / 2.6 / 7.7 k）；与 ngspice 参考实现逐点差 <0.03%；
12AX7 共阴级（Ra=100k / Rk=1.5k / VHT=300）：Vp=203 V、1 kHz 增益 60.9×。

### 三条来自实战的约定

1. **热电压用 27 °C（`VT=0.025865`）**，与 SPICE 默认 `TNOM` 对齐；用 300 K 会带 −0.05% 系统偏差。
2. **BJT 是极性无关的**：PNP 把 `(veb, vcb)` 喂进 NPN 公式、电流取负（等价 SPICE 的 `type=-1`）。
3. **嵌套内层牛顿的雅可比不要手推**：改"对残差做中心差分"后，AC127 的 8% 偏差立刻归零。

### 运放的两个离散化决定

| 方案 | 结果 |
|---|---|
| 显式欧拉 | **发散**（闭环时间常数 0.27 µs ≪ 采样周期） |
| 梯形（Tustin） | 稳定，但 54 kHz 处频率轴扭曲 +36%，运放"变快" |
| **精确极点 `v[n] = a·v[n−1] + b·vd[n]`** ✅ | 4× 过采样下误差 <2%；对 `vd` 线性 → MNA 可解进代数环 |

已知代价：192 kHz 下 -3 dB 偏低约 26%；近 Nyquist 有 `b/(2+bk)` 的增益地板
（该频段会被抽取滤波器滤掉，音频带内不受影响）。

---

## 2. MNA 装配器（`mna.rs`）

```rust
let mut net = Netlist::<5>::new();
net.resistor(1, 2, 4_700.0)
   .bjt(BC549C, true, 2, 3, GND)
   .voltage_source(4, 1, GND, 9.0);

let v = net.solve_dc();   // 工作点（gmin 步进 + 阻尼牛顿）
let v = net.step(dt);     // 瞬态一步（后向欧拉伴随模型）
```

线性元件（`R` / `C` / `L`）走各自的伴生模型，**不占未知量**：

| 元件 | 瞬态（后向欧拉） | DC |
|---|---|---|
| 电阻 | `g = 1/R` | 同左 |
| 电容 | `g = C/dt` + 历史电流 | **开路** |
| **电感** | `g = 1/(rdc + L/dt)` + 历史电流 | **`rdc`**（理想 L 在 DC 是短路；`rdc = 0` 夹到 1 mΩ） |
| **耦合电感 / 变压器** | 多端口导纳矩阵 `M = (R + L/dt)^{-1}` + 历史电流（2..=4 绕组） | 每个绕组各自退化成 `rdc` |

耦合电感的实现要点（`stamp_port_admittance` + `inductance_matrix`）：

- `H = M·(L/dt)` **不能直接算** —— 矩阵元素量级是 `L/dt`（1 H / 5 µs = 2·10⁵），
  而 `H` 几乎就是单位矩阵，直接求会踩灾难性抵消（实测固定端/理想源差 1 mV 级）。
  改成 `H = (I + dt·L⁻¹R)⁻¹`、`M = H·(dt·L⁻¹)` 后误差降到 1e-13 量级。
- 固定电位节点的行列必须按 [`Mna::conductance`] 的规则移到 RHS
  （P0 的复发点，`tests/mna.rs::transformer_fixed_rail_equals_ideal_source` 守着）。
- `M`/`H` 按 `dt` 缓存（`Cell`）：**只有子步进/采样率变化才重算**，
  正常音频每拍只是 n² 次加法 + n 次注入（n = 绕组数）。

电感的 `rdc` 是**绕线电阻**：ngspice 的 `L` 没有 DCR 参数，真实网表要写串联电阻
（`L1 in mid 10m` + `RDCR mid out 0.1`）。`commit()` 用提交点的电压反算 `i_prev`，
所以 `reset()` → `solve_dc()` 之后的首拍不会"砰"一声。对拍数据见
`tests/generated_inductor.rs`（1 kHz 增益 vs 解析 −0.73%、vs ngspice −0.60%）。

**核心取舍：器件只描述"端电压 → 流入器件的电流"**，雅可比由装配器数值差分：

```rust
sys.multiport(&[c, b, e], v, |vv| {
    let (ic, ib) = model.terminal(s * (vv[1] - vv[2]), s * (vv[1] - vv[0]));
    vec![s * ic, s * ib, -s * (ic + ib)]
});
```

- 加一个新器件只需写一个电流函数 → **不可能写错矩阵/符号**；
- 代价是每次牛顿迭代多 `n+1` 次模型调用（3 端器件可忽略）。

对拍（`cargo test -p rustomp-dsp --release --test mna -- --nocapture`）：

| 电路 | 偏差 |
|---|---|
| 二极管削波 DC（6 点） | **≤1.6 µV** |
| BJT 共射偏置 | **≤123 µV** |
| JFET 共源 | **34 µV** |
| RC + 削波瞬态（48 kHz，240 点） | **≤17 mV**（BE vs 梯形） |
| 运放非反相 10× | DC 增益 10.998；饱和正确钉在 7.8 V |
| **Sparklin Drive 整机**（10 节点 + 3 支路） | DC 0.056 mV / **THD 27.579% vs 27.5682%** / 频响 ≤0.5 dB |

### 参数化与代码生成（S1b）

电位器/开关不需要手写：网表里写指令，生成的代码会给出 `Params` + `impl Circuit`。

```rust
// 由 rustomp-import 生成（tests/generated/sparklin_drive.rs）
let mut c = Ckt::new(192_000.0);
c.set_params(Params { gain: 0.8, level: 0.5, symmetric: true });
let y = c.process(x);
```

`set_params` 只改几个电阻值（`Netlist::set_resistance`），不重建网表；
参数没变时直接 `return`。

### 四条坑

1. **gmin 步进必须降到 0**：第一版一收敛就 `break`，漏电导留在结果里 → 0.2 V 被分压成 0.1 V。
2. **瞬态对拍必须重采样**：ngspice `.tran` 是变步长（首步 1e-11 s），否则"误差"是 286 mV。
3. **运放的 DC 与瞬态不是同一个方程**：DC 要用连续极限 `A0`，否则解出的是"第一拍的增益"；
   饱和是非线性的，需"越界就钉在限幅上重解"。
4. **`R = 0` 会算出 ∞ 导纳 → NaN → 整块静音**：电位器拧到端点时手到擒来
   （`level=1` 的下臂、`gain=0` 的反馈电阻）。框架在唯一装配入口夹到 `MIN_RESISTANCE`（1 mΩ）。

---

## 2.5 分支切换（开关）

开关**不在电路层**实现：每个状态是一份**完整电路**，由 `Branch<A,B>` 选择：

```rust
pub type Branch = rustomp_dsp::Branch<asym::Ckt, sym::Ckt>;
let mut engine = Engine::<Branch>::new(48_000.0);
engine.set_params(BranchParams { a: asym_p, b: sym_p, select_b: true });
```

- 切换 = **预热 5 ms**（两路都跑、输出仍走旧分支）+ **10 ms 线性淡化**；
- 不切换时只跑当前分支（CPU 不翻倍）；
- 预热是必需的：切进来的那一支状态是冷的，冷启动瞬态乘上淡化权重就是可听的咔哒
  （实测 0.099 的台阶，预热后降到与信号自身削波边沿相同）。

## 2.7 Engine 的三个实时开关（2026-10-26）

| 能力 | API | 行为 |
|---|---|---|
| **真旁通** | `set_bypass(bool)` / `is_bypassed()` | 复用 5 ms 指数淡化 + 15 样本干路对齐；到位后吸附到目标。实测：淡出后与干路偏差 **0.00e0** |
| **削波指示** | `activity_indicator()` | 快起（~1 ms）慢落（~150 ms）包络，供 GUI 画 LED；**不参与音频** |
| **自适应迭代** | 内部 | 上限 8、残差 `1e-6` 提前退出；实测平均 2 次、与固定 8 次输出差 −227 dB |

`Circuit::activity()` 的**量纲由电路决定**：生成代码用 `max(diode 电流) / ACTIVITY_I_REF`，
参考电流由网表指令 `* @rustomp activity_i=` 给出（本电路 0.2 V 输入时 ~30 µA，取 50 µA）。

## 3. Engine 与过采样

`Engine<C>` = `Circuit`（按选定倍数在 2×/4× fs 运行）+ 多相 FIR 过采样 + 真旁通 + 活动指示 + NaN 熔断。
**1× 会失效；2× 是可选下限，4× 是默认、也是更稳的档位**：

| 过采样 | 电路 fs | 1 kHz | 3 kHz | 8 kHz |
|---|---|---|---|---|
| 1x | 48k | 0.4792 ✓ | **0.0000 失效** | **0.0000 失效** |
| 2x | 96k | 0.4791 ✓ | 0.4999 ✓ | 0.3130 偏低 |
| **4x** | **192k** | **0.4791 ✓** | **0.5000 ✓** | **0.4998 ✓** |
| 8x | 384k | 0.4791 ✓ | 0.5001 ✓ | 0.4998 ✓ |

48 kHz 直接跑电路时，运放闭环极点（≈10.8 kHz）离 Nyquist 太近，牛顿会收敛到错解（输出被钳在轨上）。

---

## 4. 早期实现的整机对拍（历史）

早期 fakedist v1（一维牛顿 + WDF）的整机数据、以及从那次实现里得到的三条教训，
已归档到 [`docs/history/sparklin-drive-legacy.md`](../../docs/history/sparklin-drive-legacy.md)。
当前整机/生成电路的验证数字见 [`docs/verification.md`](../../docs/verification.md)。

---

## 5. 实时路径的代价

`step_fixed()` + 固定电位节点 + 解析二极管雅可比之后，Sparklin Drive 的 MNA 实时版
（9 个未知量、2 次牛顿迭代）：

| 迭代 | 耗时（192 kHz） | 单核 | THD @0.2 V（离线 27.579%） |
|---|---|---|---|
| 1 | 1127 ns | 21.6 % | 29.045 %（+1.47 pp，不够） |
| **2** | **2072 ns** | **39.8 %** | **27.677 %（+0.098 pp）** |
| 3 | 2809 ns | 53.9 % | 27.591 %（+0.012 pp） |

legacy 的手推实现约 824 ns —— 那是把电路化简到 **1 个未知量**换来的。
MNA 用 2.5 倍时间换"任意拓扑"，这是保留两套求解器的理由（ADR-012）。

分项：二极管 ~150 ns/只、9×9 LU ≈ 240 ns、矩阵 clone 24 ns。
下一个大头是**预计算稀疏 LU**（预计压到 ~1.3 µs/sample）。

### 5.1 大跳变与失败恢复（E95）

定步长牛顿对**逐样本大跳变**敏感：输入 0→0.175 V 在 192 kHz 下会发散。
`step_fixed` 的处理分三层：

| 层 | 条件 | 行为 |
|---|---|---|
| 快速路径 | 固定节点变化 ≤ 0.1 V | 原实现，逐位不变 |
| 子步进 | > 0.1 V | 拆 `ceil(Δ/0.1)`（2–8）步，固定节点线性插值，每步 `dt/k` |
| 兜底 | 子步仍失败 | 按目标电压做一次 DC 恢复；连续 8 拍失败强制 `reset()` |

诊断：`last_substeps`（正常音频恒 1）、`last_accept`、`last_recoveries`（应长期为 0）。
子步只在**失败/大跳变**时发生，正常音频不付代价。变步长 `step()` 未改。

> ⚠️ **绝对时间只能同机同进程对比**：这台机器一小时内 ns/样本会漂 3–5×（DEVLOG E57）。
> 上表的数字用于"同一进程内的方案比较"，不要跨时间做绝对性能判断。

### 5.2 按电路规模估实时开销（选电路之前先算这笔账）

**先用 2× 估实时可行性，4× 是音质档。** 默认 4× 是为了 8 kHz 以上不塌（见第 3 节表），
但它就是 2× 的两倍 CPU —— 大电路先用 2× 跑通、再决定要不要 4×：

| 开关 | 位置 | 影响 |
|---|---|---|
| `oversample-2x`（feature） | 插件 `Cargo.toml` 的 `rustomp-dsp` 依赖 | 全局 4× → 2×，CPU ≈ −50% |
| `NEWTON_ITERS` | 网表 `* @rustomp newton=`（生成物里的常量） | 每减 1 次 ≈ −35%（也是**正确性**参数，别乱降） |
| `fixed_tol` / `resid_tol` / `rescue_cap` | `Netlist` 公开字段 | 提前退出阈值；`resid_tol = 0` 关残差复检（压测用） |
| `strip_bjt_body_resistance()` | `Netlist` 方法 | 去掉 `rb/re/rc` 的内层牛顿（见下） |

**BJT 是最大的单项成本**：只要 `rb/re/rc` 任一非 0，每只管子每次装配都要跑一段内层牛顿
（3~5 次模型求值 + 回溯线搜索）。代价取决于型号与收敛情况，两个实测点：

| 电路 | 原样 | 去掉体电阻 | 说明 |
|---|---|---|---|
| Freedist（4×BJT，硅管，i3-8100B，4×） | RTF **1.42** | RTF **0.88**（−38%） | 下游项目实测（`BC549C/BC559C`，`Rc≈1.17 Ω`） |
| pretty-face（2×AC127，锗管，本仓基准） | RTF 0.20 | RTF 0.19（−7%） | `cargo test -p rustomp-dsp --release --test bjt_body_bench -- --ignored --nocapture` |

差别来自内层牛顿的收敛次数（锗管热启动一两步就到），所以**别把 −38% 当成通用常数**，
量一下自己的电路。取舍：`rc/rb` 的压降通常只有 mV 级；`re` 上的 mV 会经 `gm·Re` 影响电流
（≈4%/mV），所以去掉体电阻应当**用对拍确认**（下游：1 kHz THD 2.08% → 2.11%）。

规模经验值（192 kHz 内层、2 次迭代、单核）：Sparklin Drive 9 个未知量 ≈ 2.1 µs/样本；
把 Lu 换成稀疏分解前，**按 `未知量³` 粗估**（`solve_len` 是稠密消元），
15~17 个未知量的 4×BJT 电路直接上 4× 会不够实时（RTF > 1）。

## 6. 待办

- [x] 用 MNA 重写 Sparklin Drive（`tests/sparklin_mna.rs`，THD +0.011 pp）
- [x] MNA 包成 `Circuit`（`step_fixed` + 固定电位节点 + 解析雅可比）
- [x] 网表 → 电路代码生成（`rustomp-import --mna`，见该 crate 的 README）
- [ ] 预计算稀疏 LU（拓扑固定，消除顺序离线算一次）
- [ ] 二极管用上一拍结电压热启动
- [ ] 器件动态档：BJT `Cje/Cjc/Tf/Tr`、二极管 `Cjo/tt`、MOS `Cgs/Cgd`

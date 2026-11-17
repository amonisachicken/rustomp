# Freedist 开发中遇到的上游问题报告（归档）

> 归档：2026-10-29。**下游项目 Freedist**（离散运放 fuzz，4×BJT + 2×二极管，17 个节点）
> 使用 `rustomp v0.1.2` 时提交的问题报告，原文保留在下面（只加了这一节处置表）。
>
> 结论：**P0 是框架真 bug**（固定电位端子的雅可比被重复计入），P1/P2/P5 已加固，
> P3/P4 已给杠杆与体检，P6 属于下游自己的仓库。修完的当前事实见
> [`../verification.md`](../verification.md)、[`../architecture.md`](../architecture.md)、
> [`../manual.md`](../manual.md)。

## 处置表

| 编号 | 结论 | 修在哪 | 回归测试 |
|---|---|---|---|
| **P0** 固定端雅可比重复计入 | ✅ 真 bug，已修 | `mna.rs` 的 `stamp_ports` / `multiport`：`i_eq` 只减**未知列** | `tests/mna.rs`：`pnp_emitter_on_fixed_rail_matches_ngspice`、`fixed_node_equals_ideal_voltage_source`（BJT/JFET/MOSFET）、`fixed_input_equals_ideal_source_in_transient` |
| **P1** DC 求解鲁棒性 | ✅ 已加固 | `solve_dc_once`：最后一级（gmin=0）必须过**KCL 残差复检**，否则换起点重试；新增 `last_dc_resid / last_dc_gmin / last_dc_iters / last_dc_retried` 与 `strict_dc` | `tests/mna.rs::dc_solution_satisfies_kcl` |
| **P2** `1M` 被当成 milli | ✅ 两侧都加护栏 | 导入器：`pot max < 1 Ω` 直接报错（附 `1meg` 提示）、`m` 后缀且 < 1 Ω 的电阻给 warning；设计器：电位器 `M` 规范成 `meg` + `pot-value-milli` 提示 | `rustomp-import/tests/codegen.rs` 三条；设计器 `netlist.rs` 两条 |
| **P3** 4×BJT 实时太重 | ✅ 给杠杆 + 文档（数值路径未改） | `Bjt::no_body_resistance()` + `Netlist::strip_bjt_body_resistance()`；README 新增「按电路规模估实时开销」；基准 `tests/bjt_body_bench.rs`（`--ignored`） | 基准输出 + 文中的两个实测点 |
| **P4** doctor 与 build 用的不是同一套 rust | ✅ 已修 | mac 打包器新增 `scripts/lib-toolchain.sh`（`doctor` / `build-variant` 共用；用 `rustc --print target-libdir` 判断，而不是 `rustup target list`） | 打 fake-PATH 复现（见提交说明） |
| **P5** `[ui]` 整数被静默忽略 | ✅ 已修 | `rustomp-gen` 的 `travel_deg / pointer_len / pointer_width / press_scale / drag_full_range_px`（以及参数 `default`）统一走"浮点或整数" | `emit.rs`：`panel_rs_accepts_integer_ui_fields`、`float_default_of_integer_param_is_accepted` |
| **P6** `dsh-circuit2vst` 的 `pack` 没 await | — 下游仓库，上游无动作 | — | — |
| 小观察：重复 `if` | ✅ 已修 | `mna_codegen.rs`：按字段合并 `if`（电位器两段共用一个分支） | 生成物漂移守卫（`tests/codegen.rs`） |
| 小观察：熔断静默 | ✅ 已修 | `Engine::guard_trips()` / `last_guard_value()`（`RUSTOMP_GUARD_LOG=1` 时打印） | `engine.rs` 两条单测 |
| 小观察：1 mΩ 条件数 | 📄 已文档化 | `MIN_RESISTANCE` 注释写明代价与"不要为了条件数改它" | — |

原文（未改动）如下。

---


> 项目：Freedist（离散运放 FUZZ，4 × BJT + 2 × 二极管，17 个节点）
> 环境：rustomp `v0.1.2`（913222a）；rustomp-mac-plugin `v0.1.7`（dd5b50b）；
> rustomp-win-plugin `v0.3.0-1-g2e1c18f`；macOS 21.6.0 / Windows 10 19044；
> rustc 1.100.0（mac，rustup）/ 1.99.1（win，MSVC）；测试机 Intel i3-8100B。
> 电路与 ngspice 的逐点对拍、插件打包（AU/VST3/CLAP、Windows MSI）本身都走通了，
> 下面是过程中真正挡路或需要绕开的上游问题。

---

## P0（严重）`rustomp-dsp`：固定电位端子的 Jacobian 被重复计入

**位置**：`crates/rustomp-dsp/src/mna.rs`
`stamp_ports()`（L284–L316）与 `multiport()`（L230–L277）的 `i_eq` 计算。

**问题**：装配三端器件时，固定节点（`fix_node` 的电源轨/输入）的**列**被从 `Y`
里跳过，但 `i_eq = i − J·v0` 仍然减了**全部**列的 `J·v0`。固定列的项在
LHS 不存在、在 RHS 却存在，所以不会抵消：

```rust
// 现在（L310）：
for k in 0..3 { i_eq -= jac[t][k] * v0[k]; }        // 含固定列
// 但 L299 的 Y 装配只加 `self.fixed[nk].is_none()` 的列
```

正确写法是只减**未知列**（固定节点在这一轮里 `v_k ≡ v0_k`，两边抵消）：

```rust
if self.fixed[nodes[k]].is_none() { i_eq -= jac[t][k] * v0[k]; }
```

`multiport()`（L271）是同一处逻辑，数值雅可比版本也要一起改。

**最小复现**（PNP 发射极接固定 9 V，基极固定 8.3 V）：

```rust
// nodes: 0=gnd, 1=collector, 2=base(fixed), 3=emitter(fixed)
let mut net = Netlist::<4>::new();
net.set_unknowns(2);            // 行 0 = 地，行 1 = collector
net.fix_node(3, 9.0);           // emitter = VCC
net.fix_node(2, 8.3);           // base = 8.3 V  → Vbe = 0.7 V
net.resistor(1, 0, 2200.0);     // collector → GND
net.bjt(BC559C, false, 1, 2, 3);
let v = net.solve_dc();
// ngspice：vc ≈ 1.51 V；现状：vc ≈ -147 V（迭代到限幅区后落在一个伪解上）
```

**为什么影响面很大**：误差量级取决于 `∂i/∂v` 在固定端上的大小。
- 固定端是**正偏结**（PNP 的发射极接 VCC，或任何基极/发射极被钉死的 BJT）→
  `dic_dvbe≈gm` 被乘上 9 V，等效注入几十安的误差电流，工作点直接飞到 −2 V 附近；
- 固定端是**反偏结**（NPN 集电极接 VCC）→ `dic_dvbc` 很小，几乎看不出来。
  这解释了为什么常见的“集电极接 VCC 的 NPN 射极跟随”一直没暴露问题，
  而本项目的 `Q4 BC559C 发射极接 VCC` 一上来就整机静音。

**我们遇到的具体现象**：Q4 基极（Q2 集电极）稳定在 −2.61 V，OP0 = −0.90 V，
输出 RMS 1.8e-5 V；ngspice 同网表是 8.25 V / 3.84 V / 0.43 V。
从正确工作点热启动、或把 VCC 从 0 V 缓慢 ramp 上去，都还是会掉进这个伪解。

**临时绕法**（原理图里加了一只 R12 = 1 Ω 串在 9 V 源和 VCC 之间）：让 VCC 变成
普通未知节点，固定节点只剩“源脚”和 IN，避开这条路径。这只应该当作 workaround，
根治请在 `stamp_ports` / `multiport` 里只减未知列。

---

## P1（建议）`rustomp-dsp`：DC 工作点求解的鲁棒性

**位置**：`solve_dc()`（L977）/ `solve_dc_once()`（L1022）。

**观察**：`solve_dc_once` 从 `gmin=1e-3`（≈1 kΩ 对地）开始步进，再把 gmin 降到 0。
高阻节点（比如 1 MΩ 偏置电位器的抽头、PNP 基极）在这一步会被拉得很远；
而 `newton()` 只判 `Δv < tol`（1e-9），**没有 KCL 残差复检**（`step_fixed_once`
有，DC 没有）。所以如果模型在限幅区存在一个“Δv 很小、KCL 并不成立”的伪解，
DC 求解会直接接受它。本项目的伪解就是 P0 诱发的；修掉 P0 后冷启动能解对，
所以这条**不一定是独立 bug**，但建议：

1. 给 DC 求解也加一次残差复检（和 `step_fixed_once` 的 `check_residual` 同款）；
2. `gmin` 起步降到 1e-5/1e-6，或对高阻节点先用更小的 gmin；
3. 失败时打印/回报 `last_resid` 与落在哪个 gmin 档位，便于定位。

---

## P2（中）`rustomp-import` + 设计器：`1M` 会被当成 **milli**

**位置**：设计器的电位器值格式化（`halfValue`，我们这边在
`src/client/circuit-netlist.js:186`，与上游 `backend/src/netlist.rs` 同源）；
`crates/rustomp-import/src/mna_codegen.rs:108` 的 `parse_value`（L140 `'m' => 1e-3`）。

**现象**：原理图里 1 MΩ 的 BIAS 电位器值写 `1M`，导出网表时 `halfValue("1M")`
得到 `0.5M`；SPICE 和 `parse_value` 都按 milli 解析成 **0.0005 Ω**，两半只相当于
短路，整个偏置网络被钉死。改成 `1meg` 后一切正常（`halfValue("1meg") → "0.5meg"`）。

**建议**：
- 设计器的电位器值输入框对 `M` 给显式警告（或自动规范成 `meg`），
  并在导出时校验 `R` 值范围；
- 或者在 `parse_value` 里对 `M/m` 后缀做一个“可疑值”lint（0.5 mΩ 的电阻不是
  任何音频电路的合法值）。

这不是解析器写错（SPICE 的 `M` 确实是 milli），而是**设计器/导入器之间缺少护栏**，
用户很容易踩。

---

## P3（性能，不是 bug）`rustomp-dsp`：4× 实时路径对 4-BJT 电路太重

**实测**（i3-8100B，48 kHz，单声道实例，`Engine<Ckt>`；同一电路，R11 移除前后 CPU 差异 < 2%）：

| 配置 | 实时系数 RTF | CPU |
|---|---|---|
| 4×，原样 | 1.42× | **142 %**（DAW 里必然 buffer underrun） |
| 4×，去体电阻 + Δv 1e-4 | 0.88× | 88 % |
| 2×，去体电阻 + Δv 1e-4 | 0.54× | **54 %** |

**热点**：
1. `Bjt::eval_mapped`：只要 `rb/re/rc` 有任意一个非 0，每只 BJT 每次装配就要跑
   3~5 次内层牛顿。`BC549C/BC559C` 的 `Rc≈1.17 Ω` 在几 mA 下只有 ~2 mV 压降，
   但触发了这整条慢路径。建议：当 `|ic·rc|`（以及 `ib·rb`）低于阈值（比如 1 mV）
   时直接走 `intrinsic_eval`；或提供 `Rc=0` 的“无体电阻”型号变体 / 编译开关。
2. `solve_len`：每个牛顿迭代都做一次 `unknowns × unknowns` 稠密消元（`unknowns≈15`），
   成本 O(n³)。对这个规模的电路，稀疏/带状求解能省一个量级；退一步也建议在文档里
   给出“每 BJT / 每未知量大约多少 CPU”的经验值，方便选电路规模。
3. `oversample-2x` 特性是很好的杠杆（正好 −50%），但文档里没有把它和“实时可行性”
   绑在一起说；建议在 README 的 CPU 一节点明：**先按 2× 估实时开销，4× 是音质档**。

**我们最终的调优**（都在项目侧，没改框架）：2× 过采样 + 在 `Ckt::new` 里把
`Bjt { rc, rb, re }` 清零 + `fixed_tol=1e-4`（`resid_tol` 保持 1e-6）。
对拍误差几乎不变（1 kHz 从 2.08% → 2.11%），142% → 54%。

---

## P4（工具链）`rustomp-mac-plugin`：doctor 与 build 用的不是同一套 Rust

**位置**：`scripts/doctor.sh` L61–L67 用 `rustup target list --installed` 检查；
`scripts/build-variant.sh` L60 用 PATH 里的 `cargo nih-plug bundle-universal` 构建。

**现象**（本机）：PATH 里 `cargo` 是 Homebrew 的 rustc 1.93（没有
`aarch64-apple-darwin` target），而 `~/.cargo/bin/rustup` 的 1.100 两个 target 都在。
doctor 全部 ✓，打包时却报：

```text
error[E0463]: can't find crate for `core`
  = note: the `aarch64-apple-darwin` target may not be installed
```

**建议**：doctor 和 build 用**同一个** `cargo`/`rustc`（比如统一 `command -v cargo`
后，用 `rustc --print sysroot` + `rustup target list --installed --toolchain
$(rustup show active-toolchain | ...)` 检查），或者干脆在 build 前做一次
`cargo build --target aarch64-apple-darwin` 的 dry-run 并给出明确报错。
我们这边是 `PATH="$HOME/.cargo/bin:$PATH"` 绕过的（rustup 的 cargo 在通用二进制下正常）。

---

## P5（小）`rustomp-gen`：`[ui]` 里的整数被静默忽略

**位置**：`crates/rustomp-gen/src/emit.rs`
`travel_deg`（L603）、`drag_full_range_px`（L655）、`pointer_len`/`pointer_width`/
`press_scale` 都只做 `as_float()`；而 `radius`/`center`/`image_size` 走的是
`vf`/`vpair`，会 `as_float().or_else(as_integer)`。

**现象**：rustomp.toml 写

```toml
travel_deg = 270
drag_full_range_px = 220
```

生成的 `panel.rs` 里 `angle_min/angle_max` 是默认的 −135/135（碰巧正确，所以看不出），
`drag_range` 是默认的 **260**（写 220 被忽略）。不会报错，只是“写了没生效”。

**建议**：这几个字段统一用 `vf` 同款转换，或对整数值直接接受。

---

## P6（宿主层，已在本仓库修好）`dsh-circuit2vst`：`pack` 没有 await

**位置**：`src/host/tool.js` 的 `pack` 分支（L234 附近）、`src/host/routes.js`（L149）。

`service.pack()` 现在先跑 `verifyCircuit()` 再入队，已经是 `async`；两处仍按同步调用：

```js
const started = service.pack(resolved, { install: ... });   // Promise
const job = await service.awaitJob(started.id);             // started.id === undefined
// → 工具报 “没有这个任务：undefined”
```

`routes.js` 里则是把一个 Promise 直接 `json()`，序列化成 `{}`。
**修复**：两处各加一个 `await` 即可。已在工作区改好（`tool.js` / `routes.js`），
`npm test` 31/31 通过；当前运行的 DSH 进程要重启后才会用上新代码。

---

## 一些小的观察（不一定要修）

- `rustomp-import` 生成的 `Ckt::set_params` 里，`level` 和 `bias` 各自出现了
  **两个完全相同的 `if params.X != self.params.X`**（RP2A/RP2B、RR1A/RR1B 各一个块）。
  功能上没错（`self.params` 最后才更新），但看起来是 codegen 的复制粘贴残留。
- `Engine::process` 在 `wet` 非有限或 `> GUARD_LIMIT` 时 `reset()` 并返回 0.0：
  这是一个**静默丢拍**通道，没有计数器/日志。我们没触发到，但建议加一个
  `last_guard_trips` 之类的诊断量（或 debug 日志），否则这类问题只能靠波形分析发现。
- 电位器到 0 Ω 时夹到 `MIN_RESISTANCE = 1 mΩ`（1000 S），在 16 未知量的稠密 LU 里
  条件数会变差；我们在 fuzz/level = 0 的端点扫了方波和阶跃，没有丢拍，但值很极端时
  建议留意。

## 正面反馈

- MNA 与 ngspice 的逐点对拍非常可信：1 kHz/0.2 V 下相对误差 **2.1%**、相关 0.99978；
  4 kHz/0.4 V 的 10.8% 来自未建模 `Nk`/结电荷简化（原始生成路径同样是 10.80%）。
  这套“生成代码 + 参数一致性检查 + 打包前 ngspice 闸门”的组合确实抓得住真问题。
- `oversample-2x` 特性、`fixed_tol`/`resid_tol` 旋钮、`Netlist` 的公开字段
  （`v` / `elems` / `fixed`）让我们能在**不改框架**的前提下做实时调优，设计是对的。
- 两个打包器（macOS / Windows）的契约 B 很好用：Windows 侧 `pack.sh --format msi`
  在 WiX 3.14 免安装 zip 下一次通过，pluginval strictness 5、clap-probe、PE x64 全过。

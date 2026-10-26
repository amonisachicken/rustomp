# 进 GUI 之前的清单（2026-10-25）

> 目的：确认「从网表到可安装插件」这条链在**没有界面**的情况下已经闭合，
> 并把 GUI 阶段会依赖的接口逐条点名。
>
> 每条都给了**可复核的证据**（测试名 / 命令 / 文件），不是"我记得做过"。
> 复核总入口：`./scripts/verify.sh`（8 层，见 §6）。
>
> **状态（2026-10-26）：清单全部通过，GUI 已开工并出第一个带界面的插件（S1d）。**
> 本文保留为**基线**：以后改动 DSP / 求解器 / 生成器，仍要回跑这 9 节。
> GUI 侧的 bug 与修复见 DEVLOG E61/E62；界面契约见 `crates/rustomp-gui/README.md`。

---

## 1. 器件库（闭集）

| 项 | 状态 | 证据 |
|---|---|---|
| 二极管 4 种 + LED 3 种（`Is/N/Rs`） | ✅ | `tests/devices.rs::diodes_match_ngspice_dc`：1 µA~10 mA 逐点 **0.00 %** |
| BJT 4 种（`BC549C/BC559C/AC127/AC128`，Gummel-Poon + `Rb/Re/Rc`） | ✅ | 同文件：**Ic 与 Ib 均测**，AC127/AC128/BC559C ≤0.01 %，BC549C ≤0.09 % |
| JFET `J201` | ✅ | 转移 + 输出特性 **0.00 %**（含三极管区） |
| MOSFET `2N7000`（level-1 + 体二极管） | ✅ | 沟道 8 点 + 体二极管 3 点 ≤0.01 % |
| 通用运放 `OP07`（单极点 + 压摆率 + 输出钳位） | ✅ | 闭环 −3 dB 误差 <2 %（4× 过采样） |
| **动态档**（`Cje/Cjc/Tf`、`Cjo/tt`、`Cgs/Cgd`） | ⬜ 未做 | 不影响 GUI；属音色精度提升，见 §8 |

判据：`docs/component-library.md` §5（|I|≥1 µA 时相对误差 <2 %、绝对电压差 <5 mV）。

---

## 2. MNA 装配器与实时路径

| 项 | 状态 | 证据 |
|---|---|---|
| 任意拓扑装配 + 牛顿求解 | ✅ | `tests/mna.rs`：削波 / BJT / JFET / RC 瞬态 / 运放 五项 |
| DC 精度 | ✅ | 对 ngspice **≤123 µV**（削波 1.6 µV、JFET 34 µV） |
| 瞬态精度 | ✅ | RC + 削波 240 点 **≤17 mV**（后向欧拉 vs 梯形） |
| 实时档：固定迭代 → **自适应迭代** | ✅ | `step_fixed(dt, max)` + 残差 `1e-6` 提前退出；实测平均 **2 次**（E57） |
| LU 只算真正的未知量 | ✅ | 生成器把固定电位节点排到最后（`UNKNOWNS`）；收益仅 1.09×，保留（E57） |
| 数值兜底 | ✅ | 引擎级 NaN 熔断 + 状态钳位 + `MIN_RESISTANCE`（0 Ω → 1 mΩ，E51） |

---

## 3. 代码生成（S1a / S1b / S1c）

| 项 | 状态 | 证据 |
|---|---|---|
| **S1a** 网表 → MNA 装配代码 | ✅ | `rustomp-import --mna`；`tests/codegen.rs`（漂移守卫 + 未知型号报错 + 导线合并） |
| **S1b** 参数化（电位器/开关）+ `Q/J/M` 卡片 | ✅ | 生成 `Params` + `impl Circuit`；BJT 前级 Vb 差 **2.2 µV** |
| **S1c** `rustomp.toml` → 插件外壳 | ✅ | `rustomp-gen`（11 个单测）；生成的项目 `cargo build` 通过、导出 `clap_entry` + `GetPluginFactory` |
| 多分支（开关 = 两份完整电路） | ✅ | 框架 `Branch<A,B>`（3 个单测）+ 生成器 `params_builder`（E58） |
| 生成物漂移守卫 | ✅ | `verify.sh` 的 drift 两层 + `all_generated_files_are_up_to_date` |

---

## 4. 插件外壳（无 GUI）

| 项 | 状态 | 证据 |
|---|---|---|
| 身份 / 参数表 / `Plugin` 实现全部生成 | ✅ | `plugins/sparklin-drive/plugin/src/lib.rs` 只有 `mod generated;` |
| 单声道 / 立体声独立身份（Waves 式） | ✅ | AU `Sp02`/`Sp03`（legacy 为 `SLm1`/`SLs1`） |
| 三种格式 | ✅ | `./scripts/build-all.sh` 产出 AU + VST3 + CLAP |
| AU 能力自检 | ✅ | `./scripts/check-au.sh [mono\|stereo\|dual]`（au-smoke 绕过系统注册表） |
| 参数：Gain / Clip Mode（开关）/ Level | ✅ | 生成自 `rustomp.toml` 的 `[[params]]` |

---

## 5. 整机与 AB 结论

| 项 | 状态 | 证据 |
|---|---|---|
| 生成电路 = 手写网表 = ngspice | ✅ | THD 27.579 % vs 27.5682 %；DC 0.06 mV |
| 与 legacy（手推 WDF）AB | ✅ 无可感差别 | `docs/ab-test-sparklin-v2.md`（含 FFT 复核与最终收敛数据） |
| 与 ngspice 逐次谐波（带限方波） | ✅ | 8 次迭代：基频差 **0.0004 dB**、偶次 ≤2 dB（`examples/abcmp_ngspice.rs`） |
| 开关两态 | ✅ | 非对称 THD 27.58 % / 对称 **h2 0.000 %**；切换跳变比值 0.99 |
| 实时无爆音 | ✅ | 用户实机确认（单声道 + 立体声） |

---

## 6. 验证与 CI

| 项 | 状态 | 证据 |
|---|---|---|
| 一键验证 | ✅ | `./scripts/verify.sh [--only fast\|oracle] [--report md] [--logs dir]` |
| 分层对拍 | ✅ | 器件级 / 装配器级 / 整机级 / 生成电路 —— 4 层 |
| 缺 ngspice 必须失败（CI） | ✅ | `Oracle::discover_or_skip()` + `RUSTOMP_REQUIRE_NGSPICE=1` |
| CI 两层 job + job summary | ✅ | `.github/workflows/ci.yml`（fast / oracle / fmt） |
| 用例总数 | ✅ | **84** 个（`cargo test --workspace --release`） |

---

## 7. GUI 阶段会依赖的接口（都已经就位）

| 接口 | 位置 | GUI 阶段怎么用 |
|---|---|---|
| `ParamAccess { get(id) / set(id, v) }` | `crates/rustomp-gui` | 面板控件与 nih-plug 参数之间的唯一桥；**参数 id 就是 `rustomp.toml` 里的 `id`** |
| `PanelSpec` + `Assets` | `crates/rustomp-gui` | 面板声明（画布/旋钮/LED/拨杆）由 `[ui]` 生成 |
| `gui` feature + `editor::make(...)` | 生成器预留的钩子 | 打开 feature 后 `plugin_impl.rs` 自动调用手写的 `src/editor.rs` |
| `Params` 的 id 列表 | `plugin/src/generated/params.rs` | 面板控件按 id 绑定；**开关是 bool 参数**，控件用 `ToggleSpec` |
| 延迟对齐 | `Engine::latency_samples()` = 15 | GUI 无需关心，bypass 与淡化的对齐已由引擎处理 |

> 结论：**GUI 只加"显示与交互"，不改 DSP、不改生成器、不改契约。**

---

## 8. 未完成项（都不阻塞 GUI，按优先级）

| 优先 | 项 | 影响 |
|---|---|---|
| P1 | 电位器 A/C taper | 旋钮手感（现在只有线性 B）；GUI 阶段一起做更自然 |
| P1 | 器件动态档（BJT `Cje/Cjc/Tf`、`Cjo/tt`、`Cgs/Cgd`） | 高频细节；AB 已证明静态档够用 |
| P2 | 映射分层 2/3/4/5（别名 / 项目 `mapping.toml` / 类别兜底 / 参数直通） | 只有导入"外来网表"时才需要 |
| P2 | 预计算稀疏 LU | E57 证明 LU 已不是瓶颈，**降级为可选** |
| P3 | CI 矩阵（Windows/Linux、三平台打包 S2–S3） | 发布相关 |
| P3 | 示例电路（BJT 前级 / 光电压缩 / BBD） | 内容扩展 |

---

## 9. 一条命令复核全部

```bash
./scripts/verify.sh                 # 8 层：fmt + 工作区测试 + 2×漂移 + 4×对拍
./scripts/verify.sh --only fast     # 不需要 ngspice 的部分
./scripts/verify.sh --only oracle   # 只跑对拍（缺 ngspice 直接失败）
```

最近一次结果：**8/8 通过**（2026-10-26：91 个用例 + 4 层对拍）。

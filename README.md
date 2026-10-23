# rustomp

**用 Rust 做电路级建模的音频效果器框架。**

> 目标：让"把一块模拟效果器数字化"变成可复用、可验证的工程流程 ——
> 从面包板/原理图 → SPICE 网表 → 与 ngspice 对拍的 DSP 实现 → VST3 / CLAP / AUv2 插件。

当前状态（2026-10-23）：

| 层 | 状态 |
|---|---|
| **元件库** | ✅ 二极管×3 / BJT×3 / JFET / MOSFET / 通用运放 —— 全部与 ngspice 逐点对拍 |
| **MNA 装配器** | ✅ 任意拓扑 + 5 类器件 + 牛顿求解（DC 对拍 µV 级、瞬态 mV 级） |
| 示例插件 | ✅ `Sparklin Drive` 已完整交付并拆为独立仓库（`sparklin-drive-legacy`） |
| 下一步 | 用 MNA 路径重写 Sparklin Drive 做回归验证 → 声明式代码生成（S1） |

---

## 目录

- [这是什么](#这是什么)
- [核心特性](#核心特性)
- [仓库结构](#仓库结构)
- [快速开始（macOS）](#快速开始macos)
- [新建一个插件](#新建一个插件)
- [电路建模的工作流](#电路建模的工作流)
- [质量把关：与 ngspice 对拍](#质量把关与-ngspice-对拍)
- [持续集成](#持续集成)
- [宿主兼容性（Logic）](#宿主兼容性logic)
- [示例插件 Sparklin Drive](#示例插件-sparklin-drive)
- [已知限制与路线图](#已知限制与路线图)
- [文档索引](#文档索引)
- [许可证](#许可证)

---

## 这是什么

传统做法里，"把硬件效果器做成插件"要么是黑箱神经网络（TONEX 一类），要么是各家自研的
封闭电路仿真代码。**rustomp 想提供第三条路：一个开源、可验证、以电路为骨架的 Rust 框架。**

它由三层组成：

| 层 | 位置 | 作用 |
|---|---|---|
| **器件库** | `crates/rustomp-dsp/src/devices/` | 闭集器件模型（二极管/BJT/JFET/MOSFET/运放），参数与 SPICE 对齐 |
| **电路装配** | `crates/rustomp-dsp/src/mna.rs` | 通用 MNA：器件 + 拓扑 → 可求解电路（牛顿、gmin 步进、伴随模型） |
| **框架机制** | `crates/rustomp-dsp/src/{engine,oversample,circuit}.rs` | `Circuit` 契约、`Engine<C>`、4× 过采样、真旁通、NaN 熔断 |
| **SPICE 模型库** | `devices/` | 清洗后的 `devices.lib` + 手写子电路 + 对拍报告 |
| **插件外壳** | `template/` + `scripts/new-plugin.sh` | nih-plug + egui + 单声道/立体声变体（下游项目生成到 `plugins/`） |

`Sparklin Drive` 是第一个用这套框架做出来的插件，也是"怎么用这个框架"的活样板。

---

## 核心特性

- **元件库是闭集、且每个型号都对拍过**：`1N4148` `1N34A` `1N4007` `BC549C` `AC127` `AC128`
  `J201` `2N7000` + 通用运放 `OP07`（NPN/PNP/锗管/增强型 MOS 都在内）。
  参数名与 SPICE 一致（`Is/Bf/Vaf/Ikf/Ise/Ne/Rb/Re/Rc/...`），LTspice 模型可直接映射。
- **两套求解器共用器件模型**：
  - **MNA 装配器**（`mna.rs`）—— 任意拓扑、任意多管电路。器件只写"端电压 → 流入器件的电流"，
    雅可比由装配器数值差分得到：加新器件不会写错矩阵。
  - **一维牛顿 / WDF** —— 简单电路（如单级削波）更快，用于性能敏感的级。
- **精度分档明确**（ADR-012）：静态 D1/D2/D3、动态 C0/C1/C2，逐器件给出档位；
  「静态给足、动态按需」，元件精度**不影响延迟**（固定 15 样本）。
- **运放宏模型**：单极点（A0 / GBW）+ 压摆率 + 输出钳位，
  **更新式代进节点方程，反馈环内没有一采样延迟**。
- **数值鲁棒**：阻尼牛顿、残差判据、失败兜底、状态限幅、引擎级 NaN 熔断。
  曾经在瞬态后锁死成 ~10 kHz 自激，修复过程与证据完整记录在 `DEVLOG.md` E12。
- **可验证到器件级**：二极管 0.00%、BJT ≤0.09%、JFET 0.00%、MOSFET ≤0.01%；
  MNA 装配后的整机 DC 工作点对拍 µV 级、瞬态 mV 级。
- **三个插件格式**：VST3 / CLAP 由 nih-plug 直接导出；**AUv2** 由 `clap-wrapper` 包装
  （含三处必要补丁，见 `patches/`）。
- **单声道 / 立体声独立变体**（Waves 式）：宿主按通道条声道数过滤 AU，
  拆成两个单一布局的身份最稳（ADR-008）。
- **CPU**：立体声 48 kHz + 4x 过采样 ≈ **7.9% 单核**，延迟 15 样本。

---

## 仓库结构

```
devices/                器件库（SPICE 侧）：models.add 清洗→ devices.lib，手写子电路，对拍报告
  build.sh              models.add → parsed.lib + handwritten.lib + reference.lib → devices.lib
  REPORT.md             每个型号的实测数据与"丢弃了哪些字段、为什么"
crates/
  rustomp-dsp/          框架：器件库（devices/）、MNA 装配器（mna.rs）、Engine、过采样
    src/devices/        Diode / Bjt / Jfet / Nmos / OpAmp（型号是闭集 const）
    src/mna.rs          Element 枚举 + Netlist<N> + 牛顿求解
  rustomp-gui/          框架：声明式面板 + 交互 + 纹理生命周期（不含素材）
  rustomp-oracle/       框架：ngspice 对拍（跑网表 / 解析 rawfile / 统一测量函数）
  rustomp-import/       框架：外部电路导入（IR + 可插拔 Importer，内置 breadspice）
plugins/                下游插件项目的输出目录（new-plugin.sh 生成，不入库）
                        ↑ 参考实现见独立仓库 sparklin-drive-legacy
template/               脚手架模板（new-plugin.sh 用；与下游插件结构同构）
tools/
  meta/                 rustomp.toml 读取器（身份/界面声明生成）
  oracle/               ngspice 冒烟测试用的最小电路
  au-smoke/             AU 进程内自检（绕过系统注册表）
  clap-probe/           最小 CLAP 宿主（查端口 / feature / 参数）
patches/                第三方补丁（clap-wrapper 的宿主兼容修复）
scripts/                构建、网表生成、基线仿真、自检、缓存清理
experiments/            归档的一次性验证代码（不参与构建，见其 README）
.github/workflows/      CI：cargo test --workspace（macOS + 可选 ngspice）
DEVLOG.md               开发日志：每个决策、实验数据、踩过的坑
```

---

## 快速开始（macOS）

前置：Rust 工具链、CMake、Homebrew 的 `bison` / `readline`（AU 包装与 ngspice 需要）。

```bash
# 1) 拉取第三方源码（clap-wrapper / clap / AudioUnitSDK，固定 commit 并自动打补丁）
./scripts/fetch-third-party.sh

# 2) 构建 + 安装（Mono + Stereo 两个 AU，同时产出 VST3 / CLAP）
./scripts/build-all.sh
```

产物安装到 `~/Library/Audio/Plug-Ins/{Components,VST3,CLAP}/`。

```bash
./scripts/check-au.sh          # 自检：通道能力 / 渲染 / Logic 缓存记录
./scripts/reset-logic-cache.sh # Logic 里看不到插件时：退出 Logic 后清缓存
```

---

## 新建一个插件

```bash
./scripts/new-plugin.sh "My Drive"        # 生成 plugins/my-drive/（与示例同构）
cd plugins/my-drive
# 1) 改 rustomp.toml：名字 / AU 四字符码 / CLAP id / 素材 / 控件坐标
# 2) 写电路：circuit/src/（从纯增益示例 Gain 换起）
# 3) 生成网表与基线，做对拍：
#      ./scripts/make-netlist.sh plugins/my-drive/circuit/netlist
#      ./scripts/sim-baseline.sh plugins/my-drive/circuit/netlist
# 4) 构建安装：
PLUGIN_DIR=$PWD/plugins/my-drive ./scripts/build-all.sh
```

## 电路建模的工作流

以生成的项目 `plugins/my-drive` 为例：

```bash
# 1) 面包板工程（.bread） → 中间表示 → SPICE 网表
./scripts/make-netlist.sh plugins/my-drive/circuit/netlist

# 2) ngspice 跑 op / ac / tran+THD / 波形统计，产出 baseline.txt
./scripts/sim-baseline.sh plugins/my-drive/circuit/netlist

# 3) 与 DSP 实现对拍（不需要 ngspice 的快速回归）
cargo test -p my-drive-circuit --release

# 4) 活体对拍（每次真的跑 ngspice；没装则自动跳过）
cargo test -p my-drive-circuit --release --test oracle
```

要接入**你自己的电路**：

1. `./scripts/new-plugin.sh "My Circuit"` 生成同构项目；
2. 用 [breadspice](https://github.com/amonisachicken/breadspice) 画面包板，把 `.bread`
   放进 `<项目>/circuit/netlist/`，跑上面第 1、2 步拿到基线；
3. 在 `<项目>/circuit/src/` 里实现 `rustomp_dsp::Circuit`
   （线性用梯形伴随模型 / 非线性用一维牛顿；参考
   `sparklin-drive-legacy` 仓库的 `circuit/src/fakedist.rs`）；
4. 写对拍测试把 h2–h9 锁住（参考示例的 `baseline.rs` 与 `oracle.rs`）；
5. 在 `<项目>/plugin/src/lib.rs` 里挂参数，在 `rustomp.toml` 的 `[ui]` 里挂控件。

> 好习惯：先用 `rustomp-import --spice` 把导入结果的元件行与"手写网表"对一遍，
> 确认拓扑和数值没读错，再开始写 DSP。

---

## 质量把关：与 ngspice 对拍

对拍分三层，越往下越接近"整机"。

### 1) 器件级：`tests/devices.rs`

每个型号都扫一段 V-I 曲线（覆盖多个数量级），与 `devices/devices.lib` 里的 SPICE 模型逐点比较：

| 器件 | 比较量 | 误差 |
|---|---|---|
| `1N4148` / `1N34A` / `1N4007` | 1 µA ~ 10 mA 的 `Vf` | **0.00%** |
| `BC549C` / `AC127` / `AC128` | `Ic` 与 `Ib`（PNP 也测） | **≤0.09%**（BC549C 在 4.6 mA 处 0.67%） |
| `J201` | 转移特性 + 输出特性（含三极管区） | **0.00%** |
| `2N7000` | 沟道 8 点 + 体二极管 3 点 | **≤0.01%** |
| `OP07` | 闭环 -3 dB（4× 过采样） | **<2%** |

### 2) 装配器级：`tests/mna.rs`

同一电路分别用 `Netlist` 和 ngspice 求解：

| 电路 | 量 | 偏差 |
|---|---|---|
| 二极管削波（6 个 DC 点） | `v(out)` | **≤1.6 µV** |
| BJT 共射偏置 | `v(b)` / `v(c)` | **≤123 µV** |
| JFET 共源 | `v(d)` | **34 µV** |
| RC + 削波瞬态（48 kHz，240 点） | 波形 | **≤17 mV**（后向欧拉 vs 梯形积分） |
| 运放非反相 10× | DC 增益 / 饱和 / -3 dB | 10.998 / 钉在 7.8 V / 40.5 kHz |

### 3) 整机：`Sparklin Drive`（fakedist v1，已交付）

实测（电位器 50%）：

| 项目 | WDF 实现 | ngspice | 误差 |
|---|---|---|---|
| 直流工作点 | 4.49999 V | 4.50000 V | 0.01 mV |
| 频响 50 Hz – 10 kHz | — | — | **≤ 0.31 dB** |
| THD @0.2 V（2–9 次） | 27.650 % | 27.616 % | +0.034 pp |
| 对称模式 THD | 25.595 % | 25.516 % | +0.08 pp |
| h2–h9（非对称） | 3.915 / 22.488 / 2.428 / 11.981 / 1.836 / 7.700 / 1.484 / 5.400 % | 3.920 / 22.470 / 2.430 / 11.960 / 1.837 / 7.674 / 1.483 / 5.374 % | ≤0.03 pp |
| **活体对拍**（每次重跑 ngspice） | 同上 | 3.921 / 22.457 / … / 5.344 % | **≤0.056 pp** |

两套测试互补：

| 测试 | 是否跑 ngspice | 用途 |
|---|---|---|
| 插件项目里 `tests/baseline.rs` | 否（期望值是抄下来的常量） | 日常回归，快 |
| 插件项目里 `tests/oracle.rs` | **是**（每次重跑） | 抓网表/电路改动，无 ngspice 时自动跳过 |

> 基准一律从 **ngspice 原始波形**自己算 DFT，不用 `.four`
> （它只取 1 个周期 + 线性插值，高次谐波偏低）。
> 另外 ngspice 的 `.tran` 是**变步长**，对拍前必须先重采样（见 `RawFile::resample`）。

---

## 持续集成

`.github/workflows/ci.yml`：macOS runner 上跑 `cargo test --workspace --release`，
并尝试 `brew install ngspice` 让活体对拍生效（装不上时自动跳过，不算失败）。
格式检查先设为"不阻塞"（仓库还没统一跑过一次 `cargo fmt`）。

## 宿主兼容性（Logic）

AUv2 由 `clap-wrapper` 包装，为让 Logic 正确识别，本项目带三个必要补丁：

| 问题 | 症状 | 补丁 |
|---|---|---|
| 无条件宣称 `kMusicDeviceProperty_*` | 被当乐器，只出现在软件乐器插槽 | 仅 `aumu` 时宣称 |
| 无条件宣称 MIDI 属性 | 被当 MIDI 控制效果器 | 仅真有 note port 时宣称 |
| 只认 `configurable-audio-ports`（nih-plug 只有 `audio-ports-config`） | AU 只报立体声，单声道轨菜单里消失 | 用旧扩展枚举布局并 `select()` |

另外：**插件分开发布单声道版与立体声版**（ADR-008），避免宿主按声道数过滤时出错。

---

## 示例插件 Sparklin Drive

> 完整插件（电路 + 基线 + 界面 + 打包）已拆到独立仓库 **`sparklin-drive-legacy`**，
> 本仓库只保留框架。下一次迭代会用 MNA 装配器重写同一电路做回归验证。

单电源 9V、OP07、**反馈环内非对称二极管削波**的失真/法兹：

```
IN ─C1─► IN+ ─ OP07 ─ OUT ─C3─► LEVEL ─► 出
          │      │
   R5+C2 ─┴      └─► GAIN（可变电阻）
          │
         IN- ◄── 削波环：D1(1 只) ∥ (D2+D3 串联反向)
```

| 控件 | 功能 |
|---|---|
| GAIN / LEVEL | 线性电位器，0–100% |
| Clip Mode | 上 = 3 只二极管非对称（原机）；下 = 2 只对称（短接 D3） |
| 大开关 | true-bypass（15 样本延迟对齐 + 5 ms 交叉淡化，电路持续运行） |
| LED | 削波指示（`|u|` 阈值 + 150 ms 释放） |

AU 码：`aufx / SPm1 / SPs1 / HpEf`（Mono / Stereo）。

---

## 已知限制与路线图

**当前限制**

- **动态（容性）部分大多未建模**：BJT 的 `Cje/Cjc/Tf/Tr`、二极管 `Cjo/tt`（1N4007 除外）、
  MOSFET 的 `Cgs/Cgd` 都还是 C0 档；高频细节会偏硬。静态部分（D3）已完成并逐点对拍。
- BC549C 在 `Ic≈4.6 mA` 处偏 +0.67%：疑似未建模的 `Nk`。**没有为拟合去猜 SPICE 语义**。
- 运放在 192 kHz 下 -3 dB 偏低约 26%（54.5 → 40.5 kHz）：`vd` 无延迟进代数环的代价，
  近 Nyquist 存在增益地板（该频段会被抽取滤波器滤掉）。4× 以上过采样可改善。
- 运放只到单极点 + 压摆率 + 输出钳位；未建模失调、输入偏置电流、共模范围、噪声。
- 单线程；多线程原型实测立体声 1.97x，但因单声道变体零收益 + 实时调度风险暂未启用（ADR-010）。
- 界面是按像素贴图 + 程序化指针，未做矢量/多主题。

**路线图**

- [ ] **用 MNA 装配器重写 Sparklin Drive**（同一电路、两条独立实现，比失真谱与输出电平）
- [ ] S1：声明式代码生成（网表 → `Element` 列表）；之后再把 oracle 对拍铺进 CI 矩阵
- [ ] 动态档补齐：BJT `Cje/Cjc/Tf`、二极管 `Cjo/tt`、MOS `Cgs/Cgd`
- [ ] 电位器 A/C taper；预设与状态管理；安装包（pkg）+ 签名公证
- [ ] 更多示例电路（BJT 前级 / 光电压缩 / BBD）

---

## 文档索引

| 文档 | 内容 |
|---|---|
| **`DEVLOG.md`** | 开发日志（**最重要**）：每个决策、实测数据、踩坑与修复 |
| `sparklin-drive-legacy` 仓库 | 参考实现（完整插件 + 电路解读 + 基线数据） |
| `docs/component-library.md` | **元件库精度规范**：档位定义、逐器件推荐、验收实验模板 |
| `docs/circuit-import.md` | **电路导入与元件映射规范**：可插拔导入、映射流水线、引脚绑定、mapping.toml |
| `docs/declarative-build.md` | **声明式构建规范**：代码生成 vs 打包两个平面、后端契约、多平台矩阵、S1–S5 路线 |
| `crates/rustomp-dsp/README.md` | DSP 核心：器件库、MNA 装配器、Engine、对拍结果 |
| `devices/REPORT.md` | 器件模型的清洗报告（保留/丢弃了哪些参数、为什么） |
| `patches/README.md` | 三个宿主兼容补丁的原因与验证方法 |
| `tools/oracle/smoke/README.md` | ngspice 冒烟测试的用法与基线数字 |

---

## 许可证

**GNU GPL v3.0**（见 `LICENSE`）。第三方依赖与许可证登记见 `DEVLOG.md` §ADR-005。

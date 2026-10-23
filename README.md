# rustomp

**用 Rust 做电路级建模的音频效果器框架**，并附带一个完整的示例插件 **Sparklin Drive**
（HONEYPIE EFFECTS）。

> 目标：让"把一块模拟效果器数字化"变成可复用、可验证的工程流程 ——
> 从面包板/原理图 → SPICE 网表 → 与 ngspice 对拍的 DSP 实现 → VST3 / CLAP / AUv2 插件。

---

## 目录

- [这是什么](#这是什么)
- [核心特性](#核心特性)
- [仓库结构](#仓库结构)
- [快速开始（macOS）](#快速开始macos)
- [电路建模的工作流](#电路建模的工作流)
- [质量把关：与 ngspice 对拍](#质量把关与-ngspice-对拍)
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

| 层 | crate | 作用 |
|---|---|---|
| **DSP 核心** | `rustomp-dsp` | 电路元件、牛顿求解、过采样、削波/旁通等通用机制 |
| **电路资产** | `circuits/` | 面包板工程 / 网表 / 基线数据 / 电路解读文档 |
| **插件外壳** | `rustomp-plugin` | nih-plug 外壳 + egui 界面 + 单声道/立体声变体 |

`Sparklin Drive` 是第一个用这套框架做出来的插件，也是"怎么用这个框架"的活样板。

---

## 核心特性

- **电路级建模**：非线性用一维牛顿法逐样本求解；线性储能元件用梯形伴随模型
  （与 WDF 单端口等价：`i[n] = (2C/T)(v[n]−v[n−1]) − i[n−1]`）。
- **运放宏模型**：单极点（A0 / GBW）+ 压摆率，**反馈环内没有一采样延迟**。
- **数值鲁棒**：阻尼牛顿、残差判据、失败兜底、状态限幅、引擎级 NaN 熔断。
  曾经在瞬态后锁死成 ~10 kHz 自激，修复过程与证据完整记录在 `DEVLOG.md` E12。
- **可验证**：每个电路都有 **ngspice 基线**，逐次谐波（h2–h9）对拍；
  `Sparklin Drive` 的误差 ≤0.03 个百分点。
- **三个插件格式**：VST3 / CLAP 由 nih-plug 直接导出；**AUv2** 由 `clap-wrapper` 包装
  （含三处必要补丁，见 `patches/`）。
- **单声道 / 立体声独立变体**（Waves 式）：宿主按通道条声道数过滤 AU，
  拆成两个单一布局的身份最稳（ADR-008）。
- **CPU**：立体声 48 kHz + 4x 过采样 ≈ **7.9% 单核**，延迟 15 样本。

---

## 仓库结构

```
crates/
  rustomp-dsp/          框架：Circuit 契约、Engine<C>、元件库、过采样（与电路无关）
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

`Sparklin Drive`（fakedist v1）的实测（电位器 50%）：

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

- 二极管模型含 `Is / N / Rs`，尚未含结电容 `Cjo` 与渡越时间 `tt`（高频细节略偏硬）。
- 运放只建模到单极点 + 压摆率，未用完整 OP07 `.SUBCKT`（30+ 内部节点）。
- 单线程；多线程原型实测立体声 1.97x，但因单声道变体零收益 + 实时调度风险暂未启用（ADR-010）。
- 界面是按像素贴图 + 程序化指针，未做矢量/多主题。

**路线图**

- [ ] M1：把"网表 → 电路代码"再自动化一层；把 oracle 对拍做成 CI
- [ ] 二极管 `Cjo` / `tt`；电位器 A/C taper
- [ ] 预设与状态管理、安装包（pkg）+ 签名公证
- [ ] 更多示例电路（BJT 前级 / 光电压缩 / BBD）

---

## 文档索引

| 文档 | 内容 |
|---|---|
| **`DEVLOG.md`** | 开发日志（**最重要**）：每个决策、实测数据、踩坑与修复 |
| `sparklin-drive-legacy` 仓库 | 参考实现（完整插件 + 电路解读 + 基线数据） |
| `crates/rustomp-dsp/README.md` | DSP 核心设计、对拍结果、过采样必要性 |
| `patches/README.md` | 三个宿主兼容补丁的原因与验证方法 |
| `tools/oracle/smoke/README.md` | ngspice 冒烟测试的用法与基线数字 |

---

## 许可证

**GNU GPL v3.0**（见 `LICENSE`）。第三方依赖与许可证登记见 `DEVLOG.md` §ADR-005。

# rustomp

**用 Rust 做电路级建模的音频效果器框架。**

> 目标：让"把一块模拟效果器数字化"变成可复用、可验证的工程流程 ——
> 从面包板/原理图 → SPICE 网表 → 与 ngspice 对拍的 DSP 实现 → VST3 / CLAP / AUv2 插件。

当前状态（2026-10-29）：

| 层 | 状态 |
|---|---|
| **元件库** | ✅ 二极管×4 + LED×3 / BJT×4 / JFET / MOSFET / 通用运放 —— 全部与 ngspice 逐点对拍 |
| **MNA 装配器** | ✅ 任意拓扑 + 5 类器件 + 牛顿求解（DC 对拍 µV 级、瞬态 mV 级） |
| **实时路径** | ✅ `step_fixed` + 固定电位节点 + 解析雅可比（2 次迭代 2.07 µs/样本） |
| **代码生成** | ✅ **网表 → 可运行电路**（S1a/S1b）+ **`rustomp.toml` → 插件外壳**（S1c，含参数表/身份/导出宏） |
| 示例插件 | ✅ `Sparklin Drive` 已完整交付并拆为独立仓库（`sparklin-drive-legacy`） |
| **进 GUI 之前** | ✅ 清单全过（`docs/pre-gui-checklist.md`）：网表 → 电路 → 插件 → 安装 → AB 无可感差别 |
| **平台策略** | **macOS / Windows / Linux 三平台可用**（ADR-015）：macOS（AU/VST3/CLAP + DMG）、Windows（VST3/CLAP + MSI）、Linux（VST3/CLAP + 可复现 tar.gz），各走独立打包仓 |
| 下一步 | **GUI 设计器**（`rustomp-box-designer`）；器件动态档（`Cje/Cjc`、`Cjo/tt`） |

> **闭环已打通**：面包板/原理图 → SPICE 网表 → 与 ngspice 对拍的 DSP → 插件。
> 其中「原理图设计器」与「插件 GUI 设计器」将作为独立项目开发，本项目只保证契约（[接口说明](#与外部设计器的接口)）。

---

## 目录

- [这是什么](#这是什么)
- [核心特性](#核心特性)
- [仓库结构](#仓库结构)
- [快速开始](#快速开始)
- [新建一个插件](#新建一个插件)
- [电路建模的工作流](#电路建模的工作流)
- [质量把关：与 ngspice 对拍](#质量把关与-ngspice-对拍)
- [平台策略](#平台策略)
- [与外部设计器的接口](#与外部设计器的接口)
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
| **框架机制** | `crates/rustomp-dsp/src/{engine,oversample,circuit,branch}.rs` | `Circuit` 契约、`Engine<C>`、4× 过采样、真旁通（`set_bypass`）、NaN 熔断、分支切换 |
| **界面层** | `crates/rustomp-gui` | 声明式面板（`PanelSpec`）、参数桥（`ParamAccess`）、贴图生命周期、面板校验 |
| **SPICE 模型库** | `devices/` | 清洗后的 `devices.lib` + 手写子电路 + 对拍报告 |
| **插件外壳** | `template/` + `scripts/new-plugin.sh` + `rustomp-gen` | nih-plug 外壳 + 单声道/立体声变体（下游项目生成到框架**同级**目录）；`gui = true` 时含面板模块与窗口尺寸记忆 |

`Sparklin Drive` 是第一个用这套框架做出来的插件，也是"怎么用这个框架"的活样板。

---

## 核心特性

- **元件库是闭集、且每个型号都对拍过**：`1N4148` `1N34A` `1N4007` `1N5817`(肖特基)
  红/绿/蓝 LED、`BC549C` `BC559C`(PNP) `AC127` `AC128`(锗)、`J201` `2N7000` + 通用运放 `OP07`。
  参数名与 SPICE 一致（`Is/Bf/Vaf/Ikf/Ise/Ne/Rb/Re/Rc/...`），LTspice 模型可直接映射。
- **网表即源码**：写几行 `* @rustomp param/switch` 指令，
  `rustomp-import --mna` 就能把网表变成 **能实时跑的 `Circuit`**（电位器/开关直接映射成 `Params`）。
- **两套求解器共用器件模型**：
  - **MNA 装配器**（`mna.rs`）—— 任意拓扑、任意多管电路。器件只写"端电压 → 流入器件的电流"，
    雅可比由装配器数值差分得到：加新器件不会写错矩阵。
  - **一维牛顿 / WDF** —— 简单电路（如单级削波）更快，用于性能敏感的级。
- **精度分档明确**（ADR-012）：静态 D1/D2/D3、动态 C0/C1/C2，逐器件给出档位；
  「静态给足、动态按需」，元件精度**不影响延迟**（固定 15 样本）。
- **界面也是声明式的**：`rustomp.toml` 的 `[ui]` → `PanelSpec`（毫米坐标、12 px/mm 素材）→
  旋钮整图旋转 / 拨杆 / LED / 脚踏，控件数量不限；**开关 = 两份完整电路 + 交叉淡化**。
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
                        ⚠️ 插件项目**不在本仓**（cargo-nih-plug 要求），用 RUSTOMP_GUEST 指路
  build.sh              models.add → parsed.lib + handwritten.lib + reference.lib → devices.lib
  REPORT.md             每个型号的实测数据与"丢弃了哪些字段、为什么"
crates/
  rustomp-dsp/          框架：器件库（devices/）、MNA 装配器（mna.rs）、Engine、过采样
    src/devices/        Diode / Bjt / Jfet / Nmos / OpAmp（型号是闭集 const）
    src/mna.rs          Element 枚举 + Netlist<N> + 牛顿求解（含实时档 step_fixed）
    tests/sparklin_*    手写网表 / 生成电路 / 实时路径的整机回归
    tests/generated/    由网表自动生成的电路（入库，有漂移守卫）
  rustomp-gui/          框架：声明式面板 + 交互 + 纹理生命周期（不含素材）
  rustomp-oracle/       框架：ngspice 对拍（跑网表 / 解析 rawfile / 统一测量函数）
  rustomp-gen/          框架：插件侧代码生成（rustomp.toml → identity/params/plugin_impl/panel）
  rustomp-import/       框架：外部电路导入（IR + 可插拔 Importer）+ **网表 → 装配代码生成**
    src/spice.rs        SPICE 网表导入器（.cir/.net/.sp）
    src/mna_codegen.rs  网表 → Netlist/Params/impl Circuit（参数化、器件卡片、导线合并）
plugins/                历史位置（现在默认生成到框架**同级**目录；此目录仅作兼容）
                        ↑ 参考实现见独立仓库 sparklin-drive-legacy
template/               脚手架模板（new-plugin.sh 用；与下游插件结构同构）
tools/
  meta/                 rustomp.toml 读取器（身份/界面声明 panel.rs 生成）
  oracle/               ngspice 冒烟测试用的最小电路
  au-smoke/             AU 进程内自检（绕过系统注册表）
  clap-probe/           最小 CLAP 宿主（查端口 / feature / 参数）
patches/                第三方补丁（clap-wrapper 的宿主兼容修复）
scripts/                构建、网表生成、基线仿真、自检、缓存清理
experiments/            归档的一次性验证代码（不参与构建，见其 README）
.github/workflows/      CI：macOS 矩阵（cargo test --workspace + verify.sh 的 ngspice 对拍）
                        ↑ 各平台打包在独立仓库（mac / win / linux，见「平台策略」）
DEVLOG.md               开发日志：每个决策、实验数据、踩过的坑（追加时请"读全文→写临时文件→替换"）
```

---

## 快速开始

本仓**只做框架**：器件 / MNA / 实时路径 / 代码生成 / 面板契约。
**打包在独立仓库**：[rustomp-mac-plugin](https://github.com/amonisachicken/rustomp-mac-plugin)（macOS）、
[rustomp-win-plugin](https://github.com/amonisachicken/rustomp-win-plugin)（Windows）、
[rustomp-linux-plugin](https://github.com/amonisachicken/rustomp-linux-plugin)（Linux），
契约见 `docs/ecosystem.md`。

```bash
# 0) 让框架能编译（插件项目不在本仓，见 DEVLOG E70）
cargo test --workspace            # 不需要 ngspice；不需要任何插件
```

### macOS

```bash
# 目录约定：框架、打包工具、插件项目都是**同级**
#   some-dir/{rustomp, rustomp-mac-plugin, my-effect}
# 1) 一次性：拉打包工具与它的第三方源码
git clone git@github.com:amonisachicken/rustomp-mac-plugin.git
./rustomp-mac-plugin/scripts/fetch-third-party.sh
./rustomp-mac-plugin/scripts/doctor.sh        # 体检：cargo / Xcode CLT / rust targets / 主题

# 2) 构建 + 安装一个插件项目（插件住在**框架之外**）
./scripts/build-all.sh ../my-effect            # → AU/VST3/CLAP，装到 ~/Library/Audio/Plug-Ins/
./scripts/build-all.sh ../my-effect --format dmg --no-install   # 额外产出 <Name> Installer.dmg
```

`./scripts/build-all.sh` 是**统一入口**：按当前平台把插件目录交给对应的打包工具
（macOS / Windows / Linux 三个仓），只认 `ARTIFACT: <路径>`
这一行输出（契约 B）。找不到打包工具时它会打印 clone 命令，绝不会自己编；
也可用 `RUSTOMP_PACKER=/path/to/pack.sh` 显式指定。

```bash
./rustomp-mac-plugin/scripts/check-au.sh          # AU 自检
./rustomp-mac-plugin/scripts/reset-logic-cache.sh # Logic 看不到插件时清缓存
```

### Windows

前置：Windows 10/11 x64、Git Bash 或 MSYS2、rustup（**`x86_64-pc-windows-msvc`**，`-gnu` 编不了 VST3）、
VS Build Tools 2022 的 C++ 工作负载、Python ≥ 3.11。逐项安装命令与体积见
[rustomp-win-plugin README](https://github.com/amonisachicken/rustomp-win-plugin) §4。

```bash
# 目录约定：框架与打包工具都是插件项目的**同级**
#   some-dir/{rustomp, rustomp-win-plugin, my-effect}
./rustomp-win-plugin/scripts/doctor.sh          # 缺什么、怎么装，逐项给命令
./scripts/build-all.sh ../my-effect             # 自动分派到 Windows 打包工具 → VST3 + CLAP + 安装

# 打包工具的全量参数也可以经 build-all 透传（或直接调用它）
./scripts/build-all.sh ../my-effect --arch both --no-install
./rustomp-win-plugin/pack.sh ../my-effect --format msi --no-install   # 系统级 / 用户级两个 MSI
```

平台事实：**Windows 没有 AU**（`--format au` 会明确报错）；不做 standalone；
MSI 按安装范围固定为**两个包**（单包双范围在 Windows Installer 里不可靠，见该仓 W-010），
同一台机器只装其中一种。

### Linux

前置：x86_64 Linux、bash、rustup（**`*-unknown-linux-gnu`**）、g++ / build-essential、
Python ≥ 3.11、binutils + tar + gzip；带界面的插件另需 X11 开发库。
逐项安装命令见 [rustomp-linux-plugin README](https://github.com/amonisachicken/rustomp-linux-plugin) §4。

```bash
# 目录约定：框架、打包工具、插件项目都是**同级**
#   some-dir/{rustomp, rustomp-linux-plugin, my-effect}
./rustomp-linux-plugin/scripts/doctor.sh          # 缺什么、怎么装
./rustomp-linux-plugin/scripts/fetch-third-party.sh --all   # CLAP SDK（探针）+ pluginval
./scripts/build-all.sh ../my-effect               # → VST3 + CLAP，装到 ~/.vst3、~/.clap
./scripts/build-all.sh ../my-effect --format targz --no-install   # 可复现 tar.gz 分发包
./scripts/build-all.sh ../my-effect --format targz --container    # debian:12 基线（需要 docker）
```

平台事实：**Linux 没有 AU**；没有 universal（x64 / arm64 各自独立产物）；分发包是 **tar.gz**
（`install.sh` 支持 user / system / prefix / uninstall / dry-run，`--system-install` 需要 root）；
产物的 **glibc 基线 = 构建机**，要兼容老发行版就用 `--container`（默认 debian:12 / glibc 2.36）；
`--format` 写 `targz`（`tar.gz` 会明确提示正确写法）。

> **插件项目必须住在本仓之外**：`cargo-nih-plug` 会向上找**最顶层**的 `Cargo.toml` 当 workspace 根，
> 插件嵌在 `rustomp/plugins/` 里会被拉到框架根 → `cannot specify features for packages outside of workspace`。
> 三个平台同理，插件必须与 `rustomp/` 和对应打包仓同级。

---

## 新建一个插件

```bash
./scripts/new-plugin.sh "My Drive"        # 生成 ../my-drive/（框架同级，与示例同构）
# 1) 改 ../my-drive/rustomp.toml：名字 / AU 四字符码 / CLAP id / 素材 / 控件坐标
# 2) 写电路：../my-drive/circuit/src/（从纯增益示例 Gain 换起）
# 3) 生成网表与基线，做对拍：
./scripts/make-netlist.sh ../my-drive/circuit/netlist
./scripts/sim-baseline.sh ../my-drive/circuit/netlist
# 4) 构建安装（按当前平台自动分派到 mac / win 打包工具）
./scripts/build-all.sh ../my-drive
```

## 电路建模的工作流

以生成的项目 `../my-drive` 为例：

```bash
# 1) 面包板工程（.bread） → 中间表示 → SPICE 网表
./scripts/make-netlist.sh ../my-drive/circuit/netlist

# 2) ngspice 跑 op / ac / tran+THD / 波形统计，产出 baseline.txt
./scripts/sim-baseline.sh ../my-drive/circuit/netlist

# 3) 与 DSP 实现对拍（不需要 ngspice 的快速回归；在插件自己的 workspace 里跑）
(cd ../my-drive && cargo test -p my-drive-circuit --release)

# 4) 活体对拍（每次真的跑 ngspice；没装则自动跳过）
(cd ../my-drive && cargo test -p my-drive-circuit --release --test oracle)
```

要接入**你自己的电路**：

1. `./scripts/new-plugin.sh "My Circuit"` 生成同构项目；
2. 用 **[rustomp-circuit-designer](https://github.com/amonisachicken/rustomp-circuit-designer)**
   （**首选**）画图，把工程文件 `.design.json` 直接交给生成器：

   ```bash
   RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
     cargo run -q -p rustomp-import -- <项目>/circuit/<设计>.design.json \
     --mna <项目>/src/generated/circuit.rs
   ```

   也可以先导出 `.cir` 再喂（同一条链），或继续用
   [breadspice](https://github.com/amonisachicken/breadspice) 的 `.bread`（次选）；
3. 在 `<项目>/circuit/src/` 里实现 `rustomp_dsp::Circuit`
   （线性用梯形伴随模型 / 非线性用一维牛顿；参考
   `sparklin-drive-legacy` 仓库的 `circuit/src/fakedist.rs`）；
   **或者完全不手写**：走上面第 2 步的 `--mna`，直接得到
   `Netlist` + `Params` + `impl Circuit`（见 `crates/rustomp-import/README.md`）；
4. 写对拍测试把 h2–h9 锁住（参考示例的 `baseline.rs` 与 `oracle.rs`）；
5. 在 `<项目>/plugin/src/lib.rs` 里挂参数，在 `rustomp.toml` 的 `[ui]` 里挂控件。

> 好习惯：先用 `rustomp-import --spice` 把导入结果的元件行与"手写网表"对一遍，
> 确认拓扑和数值没读错，再开始写 DSP。

---

## 质量把关：与 ngspice 对拍

**一条命令判断"能不能发"：**

```bash
./scripts/verify.sh                 # 全部：测试 + 漂移守卫 + 四层对拍
./scripts/verify.sh --only fast     # 不需要 ngspice 的部分（改代码时用这个）
./scripts/verify.sh --only oracle   # 只跑对拍（缺 ngspice 直接失败，不静默跳过）
./scripts/verify.sh --report out.md # 额外产出 markdown 报告（CI 写进 job summary）
./scripts/verify.sh --logs vlogs    # 保留每层日志（排查失败用）
```

对拍分三层，越往下越接近"整机"。

### 1) 器件级：`tests/devices.rs`

每个型号都扫一段 V-I 曲线（覆盖多个数量级），与 `devices/devices.lib` 里的 SPICE 模型逐点比较：

| 器件 | 比较量 | 误差 |
|---|---|---|
| `1N4148` / `1N34A` / `1N4007` / `1N5817` | 1 µA ~ 10 mA 的 `Vf` | **0.00%** |
| 红 / 绿 / 蓝 LED | 1 µA ~ 10 mA 的 `Vf` | **0.00%** |
| `BC549C` / `BC559C` / `AC127` / `AC128` | `Ic` 与 `Ib`（PNP 也测） | **≤0.09%**（BC549C 在 4.6 mA 处 0.67%） |
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

### 2.5) 生成电路：`tests/sparklin_generated.rs` / `tests/generated_bjt.rs`

由**网表自动生成**的电路（不是手写装配）也要过同样的尺子：

| 项目 | 生成电路 | 手写/ngspice | 差 |
|---|---|---|---|
| Sparklin Drive THD @0.2 V | **27.677 %** | 27.579 % / 27.5682 % | +0.10 pp |
| 对称开关（短接 D3） | h2 **0.000 %** | 非对称 4.002 % | ✓ 开关真的接对了 |
| GAIN 0.15 / 0.5 / 1.0 | THD 14.20 / 25.87 / 28.62 % | — | 单调 ✓ |
| BJT 前级 Vb / Ic | 0.679100 V / 2.0432 mA | 0.679098 V / 2.0379 mA | 2.2 µV / 0.26 %（未建模 `Nk`） |
| BJT 前级小信号增益 | 143× | — | 共射反相 ✓ |

生成物还有**漂移守卫**（重新生成必须逐字节一致），配套 `--check` 可进 CI。

### 3) 整机：`Sparklin Drive`

同一电路有**两条独立实现**，都与 ngspice 基线对拍：

| 实现 | THD @0.2 V | 说明 |
|---|---|---|
| ngspice（完整 OP07A 子电路） | 27.5682 % | 基线 |
| **MNA 装配器**（本仓库，E42） | **27.579 %** | +0.011 pp |
| legacy WDF（一维牛顿，已归档） | 27.650 % | +0.034 pp |

legacy 实现的其余实测（电位器 50%）：

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

## 平台策略

**框架跨平台，打包按平台分仓**（ADR-015）。不在本仓做条件编译或交叉编译：

```text
rustomp（框架 + 平台契约）
   ├── rustomp-mac-plugin     macOS：AU / VST3 / CLAP（universal）        ✅ v0.1.7
   ├── rustomp-win-plugin     Windows：VST3 / CLAP（x64/arm64）+ MSI     ✅ v0.3.0
   └── rustomp-linux-plugin   Linux：VST3 / CLAP + tar.gz                ✅ v0.1.0
```

| 层 | 谁提供 | 状态 |
|---|---|---|
| DSP / 电路 / 器件 / 代码生成 | **本仓库** | ✅ 纯 Rust，三平台都能编 |
| macOS 打包 | **`rustomp-mac-plugin`**（`scripts/build-all.sh` 按平台分派） | ✅ AU/VST3/CLAP，已在 Logic 验证 |
| Windows 打包 | **`rustomp-win-plugin`** | ✅ VST3/CLAP + MSI，已在本机 DAW 出声 |
| Linux 打包 | **`rustomp-linux-plugin`** | ✅ VST3/CLAP + tar.gz，x64 实测（arm64/容器/真实宿主待验） |

三个已落地的打包工具都实现**同一个契约 B**：吃 `<plugin_dir>/rustomp.toml` + 已生成的
`src/generated/`，只读不改；成功时 stdout 逐行 `ARTIFACT: <绝对路径>`，其余日志走 stderr。
所以接一个新平台不需要改框架、不需要改插件源码。

框架侧只有一个入口 `scripts/build-all.sh`：按平台分派到对应的 `pack.sh`，参数原样透传，
`RUSTOMP_PACKER` 可指定任意实现契约 B 的脚本。新平台 = 实现一个 `pack.sh` + 在分派里加一行。

各自的平台事实（都写进了工具仓的 README / `capabilities.toml`）：

| | macOS | Windows | Linux |
|---|---|---|---|
| 格式 | AUv2 / VST3 / CLAP | VST3 / CLAP（**无 AU**） | VST3 / CLAP（**无 AU**） |
| 架构 | universal（x86_64 + arm64） | x64 / arm64（各自独立产物） | x64 / arm64（各自独立产物） |
| 安装包 | **DMG**（`--format dmg`）；`pkg` 未做 | MSI，**系统级 / 用户级两个包** | **tar.gz**（`--format targz`，含 `install.sh` + `SHA256SUMS`）；deb/flatpak 未做 |
| 签名 | ad-hoc（对外分发待做） | 未做（Authenticode） | 无（发行版包管理器的事） |
| 宿主验证 | `auval` + `au_smoke` + Logic | `clap-probe` + `pluginval` + PE 校验 | `clap-probe` + `pluginval` + ELF/ldd（arm64 仅静态校验） |
| 分发基线 | 最低系统 11.0 | — | **glibc 基线**（原生=构建机；`--container` = debian:12 / 2.36） |

理由：难点不在编译而在**打包与宿主兼容**（clap-wrapper/CMake/MSVC/签名/宿主缓存），
每个平台都必须在本机验证；先写没环境验证的脚本等于猜。详见 `docs/declarative-build.md` §4.0。

## 与外部设计器的接口

原理图由 **[rustomp-circuit-designer](https://github.com/amonisachicken/rustomp-circuit-designer)**
（**首选设计器**）画；面板由单独的 GUI 设计器出。本项目不内置它们，只保证**契约**：

```text
 rustomp-circuit-designer ──(.design.json 工程文件)──┐
                        └─（或导出 .cir 网表）───────┤
 breadspice（次选）──────(.bread)───────────────────┴─► rustomp-import --mna ──► src/generated/circuit.rs
                                                                                        │
 面板设计器 ──(rustomp.toml 的 [ui] + PNG)──► plugin_meta.py ──► panel.rs ───────────────┴──► 插件 crate
```

> 首选设计器的工程文件**可以直接导入**（`CircuitDesigner` 导入器，2026-10-27）：
> 它运行时读设计器的 `symbols/symbols.json` 拿引脚几何，
> 并用设计器导出的 `.cir` 做**逐行对拍**（`crates/rustomp-import/tests/designer.rs`）。
> 面包板工程 `.bread` 仍支持，但不再推荐作为画原理图的默认路径。
>
> 面板链路：`[ui]`（毫米坐标）→ `PanelSpec`（`const`，漏素材在编译期就报错）→ `rustomp_gui::draw`。
> 契约见 `crates/rustomp-gui/README.md`。

对设计器的三条硬要求（`docs/circuit-import.md` §10）：

| 要求 | 原因 |
|---|---|
| 端口**显式标注**（`in=` / `out=`） | 绝不猜"哪个节点是输出"；`in=` 写节点名时表示"纯输入端口" |
| 可调元件**显式声明**（`param`） | 不从 `250k + 250k` 反推电位器；元素名必须写网表元件名（`RP1A`） |
| 型号落在**闭集表**里 | 未命中在导入阶段就报错，而不是上线后才发现换了器件 |

> 为什么设计器不直接生成 Rust：**网表是可被 ngspice 验证的中间格式**，
> 对拍全靠它。设计器直接出代码，对拍链就断了。

## 持续集成

`.github/workflows/ci.yml` 分两层 job，都用 `scripts/verify.sh`（本地与 CI 同一入口）：

| job | 内容 | ngspice |
|---|---|---|
| **快速层** `fast` | `cargo test --workspace --release` + 两份生成物的漂移守卫 | 不需要（对拍测试自动跳过） |
| **对拍层** `oracle` | 器件级 / 装配器级 / 整机级 / 生成电路 四层活体对拍；结果写进 job summary | **硬依赖**（装不上就让 CI 红） |
| 格式检查 `fmt` | `cargo fmt --all --check` | 不阻塞（仓库还没统一跑过一次 fmt） |

两个关键设计：

1. **缺 ngspice 必须是失败，不能是跳过。** 本地用 `Oracle::discover_or_skip()`
   （没装就打印跳过原因，方便开发）；CI 里设 `RUSTOMP_REQUIRE_NGSPICE=1`，
   同一函数变成 `panic!` —— 避免"忘了装 ngspice、测试静默跳过、CI 却是绿的"这种
   最坏的假阳性。
2. **失败时日志自动上传。** `--logs ./verify-logs` 保留每层日志，失败时作为 artifact 上传，
   不用靠猜"哪一层挂了"。
3. **对拍数字沉淀在 job summary 里。** `--report "$GITHUB_STEP_SUMMARY"` 会把每层状态
   与关键数字（V-I 误差、THD、频响偏差）渲染成 markdown 表格，翻历史构建就能看到
   "这次的精度是多少"，而不是只有一句“passed”。

> 三平台分发由参考插件仓的 `.github/workflows/release.yml` 覆盖：tag `v*` 触发
> macOS / Windows / Linux 构建 → GitHub Release（技术预览 / 未签名）；`workflow_dispatch`
> 可只构建（dry-run）。见 DEVLOG E100。

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

> 完整插件（电路 + 基线 + 界面 + 打包）在独立仓库 **`sparklin-drive`**；
> **预编译包见 [Releases v0.1.9](https://github.com/amonisachicken/sparklin-drive/releases/tag/v0.1.9)**
> （macOS DMG / Windows MSI ×2 / Linux tar.gz；未签名技术预览）。
> 更早的手推实现冻结在 `sparklin-drive-legacy`。

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
- **代码生成器的边界**：型号映射只做了「精确命中 / 报错」两级（别名、项目 `mapping.toml`、
  类别兜底尚未实现）；不支持 `.asc` 原理图、多电源/浮地源、非线性电容。
- 实时 MNA 路径 2.07 µs/样本（192 kHz 单核 40%），比手推实现慢 2.5×；
  下一步是预计算稀疏 LU。
- 单线程；多线程原型实测立体声 1.97x，但因单声道变体零收益 + 实时调度风险暂未启用（ADR-010）。
- 界面：**声明式贴图面板**已可用（毫米坐标、12 px/mm、旋钮整图旋转、拨杆/LED/脚踏），
  但还没有数值显示、素材热重载、多主题；`template/` + `new-plugin.sh` 尚未支持 `--gui`
  （现在要手工加 `gui` feature 并写 `editor.rs`）。
- `panel.rs` 目前由 `tools/meta/plugin_meta.py` 生成；搬进 `rustomp-gen`（单一发射器）还是 TODO。
- **Windows 侧**：无代码签名；arm64 产物只能静态校验（x64 机器跑不了 arm64 DLL）；
  不做 standalone；MSI 是两个固定范围的包，没有"向导里选范围"的单入口；打包器仓自身没有 CI
  （由参考插件的三平台 release workflow 覆盖）。
- **Linux 侧**：分发包的 glibc 基线随构建机（CI 用 Ubuntu 22.04 / glibc 2.35）；arm64 /
  容器 / GUI 插件 / 真实宿主仍未实测；不做 deb/flatpak/AppImage、standalone；打包器仓自身没有 CI。

**路线图**

- [x] **用 MNA 装配器重写 Sparklin Drive** ✅ 整机 THD 27.579% vs ngspice 27.5682%（+0.011 pp）、
      DC ≤0.27 mV、频响 ≤0.5 dB（`crates/rustomp-dsp/tests/sparklin_mna.rs`）
- [x] 把 MNA 包成 `Circuit` 供实时使用 ✅ 2 次迭代 2072 ns/样本（192 kHz 单核 39.8%），THD +0.098 pp
- [x] 实时路径优化 ✅ **自适应牛顿迭代**（上限 8、残差 1e-6 提前退出）：实测平均 **2 次**，
      比固定 8 次快 3.0×、输出差 −227 dB。~~预计算稀疏 LU~~ 已证明不是瓶颈（E57）
- [x] **S1a：网表 → MNA 装配代码** ✅ `rustomp-import --mna`（生成电路 THD 与手写版逐位一致）
- [x] **S1b：参数化 + 器件卡片** ✅ `param`/`switch` 指令 → 生成 `Params` + `Ckt`（`impl Circuit`）；
      `Q/J/M` 卡片映射；BJT 前级 Vb 差 2.2 µV
- [x] **S1c：插件侧代码生成** ✅ `rustomp-gen`（参数表 / 身份 / `Plugin` 实现 + 导出宏），
      生成的项目能 `cargo build` 并导出 CLAP/VST3 入口；模板改为无 GUI 外壳
- [x] **多分支（开关）** ✅ 框架 `Branch<A,B>`：两份完整电路 + 预热/交叉淡化；
      Sparklin Drive 已有 Clip Mode（非对称 THD 27.58 % / 对称 h2 **0.000 %**）
- [x] **S1d：界面** ✅ `[ui]` → `panel.rs`、`gui` feature + 手写 `editor.rs`（~100 行）；
      Sparklin Drive v2 已带面板（毫米坐标、12 px/mm、旋钮整图旋转、3PDT 踩钉、LED 活动量）
- [x] 把 oracle 对拍铺进 CI（macOS，缺 ngspice 即失败）
- [ ] **图形设计器**（`rustomp-box-designer`，独立仓库）：面板 → `[ui]` + 素材，本仓库只维护契约
- [x] **原理图设计器** ✅ `rustomp-circuit-designer`（独立仓库，**首选**）：原理图 → 网表；
      工程文件 `.design.json` 已可直接导入（2026-10-27），取代 breadspice 成为默认路径
- [x] **Windows 打包工具** ✅ `rustomp-win-plugin` v0.3.0（独立仓库，ADR-015）：
      VST3/CLAP（x64/arm64）+ 系统级/用户级 MSI，clap-probe + pluginval 自检，本机 DAW 实测出声
- [x] **Linux 打包工具** ✅ `rustomp-linux-plugin` v0.1.0（独立仓库，ADR-015）：
      VST3/CLAP（x64/arm64）+ 可复现 tar.gz（install.sh / SHA256SUMS），clap-probe + pluginval 自检
- [x] **三平台 GitHub Release** ✅ `sparklin-drive v0.1.9`：tag 触发三平台构建，产出
      macOS DMG / Windows MSI×2 / Linux tar.gz（未签名 prerelease，见 DEVLOG E100）
- [ ] 动态档补齐：BJT `Cje/Cjc/Tf`、二极管 `Cjo/tt`、MOS `Cgs/Cgd`
- [x] **macOS DMG 安装包** ✅ `rustomp-mac-plugin --format dmg`（用户级：三种插件 + 双击安装脚本）
- [ ] 电位器 A/C taper；预设与状态管理；`pkg` 安装包 + 签名公证
- [ ] 更多示例电路（BJT 前级 / 光电压缩 / BBD）

---

## 文档索引

| 文档 | 内容 |
|---|---|
| **`DEVLOG.md`** | 开发日志（**最重要**）：每个决策、实测数据、踩坑与修复 |
| `sparklin-drive-legacy` 仓库 | 参考实现（完整插件 + 电路解读 + 基线数据） |
| `rustomp-mac-plugin` / `rustomp-win-plugin` / `rustomp-linux-plugin` 仓库 | 三个已落地的打包工具（契约 B 的实现）：macOS AU/VST3/CLAP + DMG；Windows VST3/CLAP + MSI；Linux VST3/CLAP + tar.gz |
| **`docs/split-plan.md`** | **拆仓执行清单**：每个文件从哪来/到哪去、验证命令、明天的操作顺序 |
| **`docs/ecosystem.md`** | **生态与仓库边界**：8 个仓谁负责什么、3 个跨仓契约、版本约束、拆分顺序与风险 |
| `docs/component-library.md` | **元件库精度规范**：档位定义、逐器件推荐、验收实验模板 |
| `docs/circuit-import.md` | **电路导入与元件映射规范**：可插拔导入、映射流水线、引脚绑定、mapping.toml |
| `docs/declarative-build.md` | **声明式构建规范**：代码生成 vs 打包两个平面、后端契约、**平台策略（macOS 默认 + 外部打包工具）**、平台矩阵、S1–S5 路线 |
| `docs/ab-test-sparklin-v2.md` | **AB 测试复盘**：v2 生成路径 vs legacy 手推实现；两个 bug 的定位过程、13 条经验、最终收敛数据 |
| `docs/pre-gui-checklist.md` | **进 GUI 之前的清单**：器件/求解器/生成器/外壳/验证逐项状态 + GUI 阶段依赖的接口 |
| `crates/rustomp-gui/README.md` | 界面层：面板声明 `PanelSpec`、绘制顺序、贴图与生命周期、校验 |
| `crates/rustomp-dsp/README.md` | DSP 核心：器件库、MNA 装配器、实时路径、对拍结果 |
| `crates/rustomp-import/README.md` | 导入器 + 网表 → 装配代码生成（指令、产物、漂移守卫、更新流程） |
| `crates/rustomp-gen/README.md` | `rustomp.toml` → 插件外壳（身份/参数/Plugin 实现/面板模块、三条硬规则） |
| `devices/REPORT.md` | 器件模型的清洗报告（保留/丢弃了哪些参数、为什么） |
| `patches/README.md` | 三个宿主兼容补丁的原因与验证方法 |
| `tools/oracle/smoke/README.md` | ngspice 冒烟测试的用法与基线数字 |

---

## 许可证

**GNU GPL v3.0**（见 `LICENSE`）。第三方依赖与许可证登记见 `DEVLOG.md` §ADR-005。

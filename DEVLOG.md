# rustomp 开发日志

> 项目：把自研硬件吉他效果器数字化为 VST3 / CLAP / AU 插件，后端用 Rust，路线为**电路建模（元件级）**。
>
> 本文件是**唯一事实来源（single source of truth）**的开发记录，目的是让每一步决策都能被复盘。
> `prepare.md` 是早期的调研问答，**含事实错误，已在本文件 §2 记录勘误，不要直接采信**。

## 维护约定

### 本项目的两条硬性约定（务必遵守）

1. **每个插件必须分别构建「单声道」和「立体声」两个独立身份**
   （`./scripts/build-all.sh`，见 ADR-008 / E14）。
   不要依赖"一个插件同时支持两种布局"。
2. 新增第三方封装层（clap-wrapper 等）后，**必须用 `tools/au-smoke` + `tools/clap-probe`
   核对能力字段**，并与正常商业插件（如 BYOD / Waves）的宿主缓存对照
   （见 E13：这类问题不报错、能出声，只让插件出现在错误的地方或消失）。

每次会话/每个里程碑追加一节，按下面的固定结构写：

```
## <日期> / <里程碑或实验编号> <标题>
### 做了什么      （可复现的命令、代码位置）
### 结果          （数字，不要写"大概"、"感觉"）
### 结论          （影响到什么决策）
### 踩坑 / 风险    （后来者最需要看的部分）
```

---

# 1. 项目状态快照

| 项 | 值 |
|---|---|
| 当前阶段 | **M1 进行中**：器件库（5 类，全部对拍）+ 通用 MNA 装配器已完成；下一步用 MNA 重写 Sparklin Drive 做回归 |
| 已交付 | fakedist v1 / Sparklin Drive（已拆为独立仓库 `sparklin-drive-legacy`） |
| 已确定路线 | 电路建模；WDF 为主 + 自研 MNA 作对拍 oracle（ngspice 后置交叉验证） |
| 许可证 | **GPL-3.0-or-later**（ADR-005） |
| 首款产品 | **二极管削波类失真**（ADR-007） |
| 首要宿主 | **macOS / Logic Pro / AUv2**（ADR-006，不涉及 Pro Tools/AAX） |
| 构建链 | nih-plug（CLAP）→ clap-wrapper（AUv2 + VST3） |
| 代码资产 | `crates/rustomp-plugin`（nih-plug 空插件 / M0-a）、`cmake/`（AU 包装）、`scripts/`（构建链）、`tools/au-smoke`（AU 进程内测试）、`experiments/e1_wdf_diode_clipper`（spike） |
| 外部参考 | `amonisachicken/breadspice`（GPL-3，可复用 oracle/器件模型） |
| 一键构建 | **`./scripts/build-all.sh`**（→ Mono + Stereo 两个 AU，通用二进制，安装并自测） |
| 产物身份 | `Rustomp Fuzz Mono` = `aufx/RFm1/Amch`；`Rustomp Fuzz Stereo` = `aufx/RFs1/Amch` |
| 自检 / 排障 | `./scripts/check-au.sh`、`./scripts/reset-logic-cache.sh` |
| 未决事项 | 原理图获取、硬件 A/B 条件、Logic 版本（见 §7） |

### 2026-10-23 环境快照

| 项 | 值 |
|---|---|
| OS | macOS 13.7.2 (22H313) |
| CPU | Intel Core i9-10910 @ 3.60GHz，20 逻辑核（x86_64） |
| Rust | rustc / cargo 1.99.1 |
| 编译器工具链 | 仅有 Command Line Tools（`/Library/Developer/CommandLineTools`），**未安装完整 Xcode** |
| SDK | MacOSX13.3.sdk 等可用 |
| Cargo 源 | `~/.cargo/config.toml` 走 USTC 稀疏镜像 |
| 其他 | clang/cmake/pkg-config/python3 均在（Homebrew） |
| 已安装 | `ffmpeg 8.1.1` ✓、git ✓、cmake ✓、**`ngspice-47` ✓（已链接到 `/usr/local/bin/ngspice`，见 E8）**、Homebrew `readline 8.3`、`bison` |
| **未安装** | 完整 Xcode ✗（只有 CLT，不影响本次全部构建） |
| 固定第三方 | `nih-plug` @ `de42101`（ISC）、`clap-wrapper` @ `806bba0`（MIT）、`clap` @ `1.2.9`（MIT）、`AudioUnitSDK` @ `1.1.0`（Apache-2.0） |
| 网络 | 直连受限；需 `source ~/vpn.env` 后可用 github/raw.githubusercontent；Homebrew Tier 3 无 bottle |

---

# 2. `prepare.md` 勘误表（重要）

`prepare.md` 是 AI 问答整理稿，其中若干关键事实不成立。以下均为 2026-10-23 实际核查结果：

| # | 原文档说法 | 核查结果 | 影响 |
|---|---|---|---|
| 2.1 | 存在 `wdf-rs`、`scalar_wdf`、`vector_wdf`、`audio-graph` 等 Rust WDF 库 | ❌ **不存在**。crates.io 上 `wdf` 仅 0.0.0 空占位（419 次下载）、`wdf-macros` 同为占位 | **WDF 核心必须自研**：既是最大的工作量，也是本项目的核心 IP |
| 2.2 | `nih-plug` 可以直接作为 crates.io 依赖 | ⚠️ 主 crate 不在 crates.io，只有 `nih_plug_egui` / `nih_plug_iced` / `nih_log` / `nih_plug_core` 等子 crate。必须用 **git 依赖**（github 可直连，USTC 镜像索引不到它） | 依赖要用 `git = "https://github.com/robbert-vdh/nih-plug.git"` |
| 2.3 | FAUST 的 `circuit.faust` 提供 `resistor` / `capacitor` / `diode` 原语 | ❌ 不存在该库 | 不要基于 FAUST 规划 |
| 2.4 | 已有成熟的 "SPICE 网表 → WDF 自动转换工具"（2019 论文） | ❌ 无成熟可用实现；学术上有路径（Holters/Zölzer、Kurt Werner） | 不要指望自动转换，手写电路 + 逐级对拍是主路径 |
| 2.5 | 可基于 Guitarix 源码二次开发 / 用 ngspice 做实时引擎 | ⚠️ **半错，已修正**。ngspice **核心是 New BSD-3-Clause**（Spice3f5），只有可选组件（numparam/adms/tclspice）是 LGPL，见 `breadspice/LICENSES/ngspice-COPYING.txt`；真正 GPL 的是 **Guitarix**。另：ngspice 是离线变步长仿真器，仍然**不能**做实时引擎 | ngspice 可自由链接/分发（保留版权声明），法律上无障碍；但它不适合实时音频，只能做 oracle |
| 2.7 | nih-plug 支持 VST3 / CLAP / **AU** | ❌ **错的**。官方 README 原文：*"NIH-plug can currently export VST3 and CLAP plugins"*，源码中不存在 `nih_export_au`，`nih_plug_xtask` 也没有 AU 目标 | **AU 必须靠外层包装**：CLAP → `clap-wrapper`(MIT) → AUv2。见 ADR-006 |
| 2.6 | WDF 的实践文档"需要自己拼凑" | ✅ 属实，但基础比文档描述的更薄：连库都没有，一切从适配器公式开始 | 见 §8 真实可用参考资料 |

**结论：`prepare.md` 可用作背景阅读，但技术选型、依赖清单、许可证判断一律以本文件为准。**

---

# 3. 实验记录

## E1：WDF 二极管削波级 spike（2026-10-23）

代码：`experiments/e1_wdf_diode_clipper/`（`cargo run --release` 可复现全部数字）

### 3.1 目的

在投入架构设计前，用最小可运行代码回答三个问题：

1. WDF 能不能算**对**（对比解析解）？
2. 长时间运行**稳不稳**（NaN / 发散）？
3. 实时性能**够不够**（每级每样本成本）？

### 3.2 被测拓扑

```
Vin ──Rs──┬── C ──┬── gnd
          └── D ──┘
```

三个单端口（Thevenin 源 / 电容 / 二极管）并联 → **3 端口并联适配器**，每采样做一次**标量**牛顿迭代求解节点电压 `v`。

关键公式（已验证）：

```
A_src = Vin/2                     Thevenin 源的反射波是常量
A_cap = cap_state (= a_cap[n-1])  电容端口阻抗取 R=T/(2C) ⇒ b[n]=a[n-1]
A_d   = (v - Rd*Id(v))/2          二极管
F(v)  = v - γs·Vin - 2·γc·A_cap - γd·(v - Rd·Id(v)) = 0
γ_k   = Rp/R_k,  Rp = 1/Σ(1/R_k)
cap_state[n] = v - cap_state[n-1]
```

### 3.3 结果

**正确性 —— DC 工作点 vs 解析解**（Rs=1k, C=1µF, 1N4148, Vin 0.2→9V）

| 实现 | 最大绝对误差 |
|---|---|
| libm `exp` | 2.35e-6 V |
| 快速 exp 近似 | 2.35e-6 V |

**稳定性**：10 秒 ±10V 白噪声，192kHz → 非有限值 **0** 个，输出峰值 1.94V（正常钳位，无发散/无死锁）

**吞吐量**（48kHz 单声道，f32，libm exp，取 5 次最小值）

| 配置 | ns/样本 | 单核占用 |
|---|---|---|
| 1 级 @ 1x | 78.9 | 0.38% |
| 1 级 @ 4x | 78.2 | 1.50% |
| 1 级 @ 8x | 78.4 | 3.01% |
| 1 级 @ 16x | 78.0 | 5.99% |
| 4 级串联 @ 4x | 246.7 | 4.74% |
| 6 级串联 @ 4x | 368.3 | 7.07% |

牛顿迭代平均 **2.3 次**收敛（热启动 = 上一采样解）。

### 3.4 结论

- ✅ **正确性成立**：与解析解误差在 f32 噪声量级（1e-6 V）。
- ✅ **实时性完全不是瓶颈**：多级失真踏板 @4x 过采样 + 立体声，预估 10~15% 单核。**性能不构成路线风险。**
- ✅ **每级 1 个未知数、2~3 次牛顿** 是可靠的工作点 —— 这是 WDF 相对整块 MNA 的核心优势。
- 📌 成本杠杆是 **过采样倍数 × 级联级数**，不是单次 exp 的微优化（见踩坑 3.5.2）。

### 3.5 踩坑（最有价值的部分）

**3.5.1 端口波方向写反 → 静默 NaN**
第一版把 Thevenin 源的入射/反射波方向搞反了。表现：**牛顿迭代照常收敛、代码没有任何报错、DC 测试输出 `NaN` 或错的低电平**。修正后才发现真值。

> 教训：WDF 的错误会以"收敛到错误的解"的形式出现，听起来甚至可能不刺耳。
> **必须有数值 oracle 对拍**（解析解 / ngspice 离线渲染 / 实测录音），绝不能靠耳朵验收。
> → 这条直接决定了 M1「对拍基础设施」必须排在写电路之前。

**3.5.2 自研快速 exp 反而更慢**
手写多项式 2^x 近似（相对误差 8.6e-5）实测 **93 ns vs libm 78 ns**，慢 18%。
推测 LLVM 已把 `f32::exp` 内联成高效指令序列。**结论：不要过早微优化 exp。** 若将来确实需要，应先 profile 再动手。

**3.5.3 基准测量本身的坑**
初版基准把 `sin()` 生成输入信号算进了计时循环，且 4 级用例被编译器整体 DCE 掉（输出 0.0 ns）。修正：**输入缓冲区预生成 + `std::hint::black_box` + 5 次取最小值**。
→ 后续所有 benchmark 必须遵守这三条，否则数字不可信。

---

## E2：breadspice 侦察（2026-10-23）

来源：`github.com/amonisachicken/breadspice`，GPL-3.0，已克隆到 `/tmp/x/breadspice` 研读（2743 行 Rust + TS 前端）。

### 3.6 一句话结论

> breadspice 是 **ngspice 前端 + 面包板 UI**，**不含任何 WDF / 实时 DSP 代码**。
> 对本项目的价值 **100% 在「oracle 与器件模型」，0% 在实时音频引擎**。

不要抱"复用它的仿真内核"的期待；**但它的 oracle 基础设施正好是 M1 最贵的部分**，而 M1 本来就是必须先做的。

### 3.7 可复用清单（同协议 GPL-3，可直接拷贝，保留 SPDX 头与版权）

| 资产 | 原路径 | 行数 | 在本项目的用途 | 改造量 |
|---|---|---|---|---|
| ngspice 驱动 | `backend/src/ngspice.rs` | 641 | 离线 oracle：`ngspice -b -r` 出 ASCII rawfile → 解析成 traced | **中**：`tran` 步长需改到音频级 + 必须强制 max step |
| 器件模型库 | `backend/assets/models.lib` | — | 1N4148 / 1N5817 / LED×3 / BC549C / BC559C / J201 / 2N7000 / OP07A | **中**：需补 2N3904 / 2N5088 / TL072 / JRC4558 等常用型号 |
| 网表生成 | `backend/src/netlist.rs` | 927 | 元件→SPICE 行映射；**电位器=两段串联电阻**；导线=1mΩ | 中：本项目拓扑固定，可大幅简化 |
| 领域模型 | `backend/src/domain.rs` | 133 | Circuit / Component / Pin / Net | 小 |
| 音频→PWL | `backend/src/audio.rs` + `presets.rs` | 223 | 把测试信号做成 PWL 源喂给 tran | 小 |

已确认有用的小细节：

- rawfile ASCII 输出靠工作目录的 `.spiceinit` 设 `filetype=ascii`，再 `ngspice -b -r result.raw`。
- 电位器映射：`R<ref>A` = 总阻值×百分比（引脚1-2），`R<ref>B` = 剩下部分（引脚2-3），两端到底时用 0.001Ω 兜底。**这就是 ADR-003 参数化的第一个真实样本**，可直接演进成「taper + 抽头阻抗」。
- ac 分析在 `-r` 模式下 frequency 变量带未初始化虚部垃圾值 → 取实部。

### 3.8 不可复用 / 差异

| 差异 | 说明 |
|---|---|
| 无 WDF / 无实时 DSP / 无固定步长求解器 | ngspice 每样本耗时是毫秒级，**绝对不能进音频线程** |
| 面向「用户拖拽面包板」 | 本项目是「固定拓扑 + 参数化」，数据结构可以简单一个数量级 |
| `tran` 默认 step=1e-5（≈100kHz） | 对 48kHz 音频 oracle **太粗**；且 ngspice tran 默认**变步长**，必须显式限制 max step，否则对拍无意义 |
| 前端 DIYLC SVG 资产（GPL-3） | 本项目用不到 |

### 3.9 关键发现：E1 的二极管模型过于简化（必须修）

`models.lib` 里的 1N4148：

```
.model 1N4148 D(Is=2.52n Rs=.568 N=1.752 Cjo=4p M=.4 tt=20n)
```

而 E1 spike 只用了 `Is` 和 `N`，**漏了 `Rs=0.568Ω`、`Cjo=4p`、`tt=20n`、`M=.4`**。

- `Rs` 是**串联体电阻**：几十 mA 时压降 = 0.568Ω × 0.05A ≈ 28mV，会**明显软化削波拐点**（真实踏板比 E1 的曲线更圆、更“管味”）。这是听感差异，不是数值细节。
- `Cjo/tt` 决定高频行为（尤其强过载时的边沿）。

→ **M2 验收项：二极管必须用完整模型（Rs、Cjo、M、tt），不能用 E1 的二参数简化版。**
→ 在 WDF 里 `Rs` 很好处理：它就是一个串联电阻，多一个 series adaptor 即可。

### 3.10 后续值得从 breadspice 引入的东西

- 新增器件模型：2N3904 / 2N5088 / 2N2222（失真踏板常用 BJT）、TL072 / JRC4558 / LM308（RAT 用）——模型参数可从厂商 SPICE 模型库获取。
- 电位器 `R_A/R_B` 拆分思路 → 直接迁移到「真实电位器 taper + 抽头阻抗」实现。

---

## E3：ngspice CLI 安装（2026-10-23）✅ 已解决

### 3.11 过程

```bash
brew install ngspice   # 失败
```

- 本机 Intel + macOS 13 在 Homebrew 属 **Tier 3** 配置 → **无预编译 bottle**，需源码编译整棵依赖树（libomp、libpng…）。代理下卡在 sourceforge 下载 > 15 分钟，已终止。
- 已安装：`ffmpeg` ✓；未安装：`ngspice` ✗。

### 3.12 备选方案

| 方案 | 评价 |
|---|---|
| A. `brew install` 继续等 | ✗ 放弃：Intel macOS 13 属 Homebrew **Tier 3**，无 bottle，要编整棵依赖树（libomp/libpng…），代理下卡在 sourceforge > 15 分钟 |
| **B. 源码手编** | ✅ **已采用并成功**：`./configure --without-x --disable-openmp --disable-debug --disable-tcl` |
| C. 不用 ngspice | 保留：仍然以自研 MNA 作一等 oracle，ngspice 作独立交叉验证 |

### 3.13 决策与最终结果

**已装好**：`~/.local/ngspice/bin/ngspice` — ngspice-47，带 KLU 直接法求解器。

```
** ngspice-47 : Circuit level simulation program
** Compiled with KLU Direct Linear Solver
```

构建要点（复现时必须）：

| 问题 | 解决 |
|---|---|
| 系统 `bison 2.3` 太旧 | `PATH=/usr/local/opt/bison/bin:$PATH`（Homebrew bison） |
| 第一次 make 失败：`signal_handler.c:66 error: undeclared function 'rl_reset_after_signal'` | 系统 readline 是 **4.2**（libedit 模拟），ngspice-47 需 readline ≥ 8.1 → 用 Homebrew `readline 8.3`：`CFLAGS=-I/usr/local/opt/readline/include LDFLAGS=-L/usr/local/opt/readline/lib` |
| 不想拉 X11 / OpenMP 依赖 | `--without-x --disable-openmp --disable-tcl` |

冒烟测试（RC 低通 tran）输出正常。

仍以自研 MNA+牛顿 作**一等** oracle（可控步长、可注入状态、可对比中间节点）；
ngspice 的价值是**独立第三方**校验，避免我们自证自明。

---

## E4：M0-a 构建链打通（2026-10-23）✅

目标：**不写任何 DSP**，先证明「Rust 插件 → Logic 可加载的 AU」这条链在本机成立。

### 4.1 结果

| 步骤 | 产物 | 验证 |
|---|---|---|
| Rust 插件 (nih-plug) | `crates/rustomp-plugin` | `cargo build --release` 通过 |
| 通用二进制打包 | `Rustomp Skeleton.clap` / `.vst3` | `lipo -info` → **x86_64 + arm64** ✓ |
| clap-wrapper → AUv2 | `Rustomp Skeleton.component` | `lipo -info` → x86_64 + arm64 ✓；`.clap` 已嵌入 `Contents/PlugIns/` ✓ |
| AU 进程内冒烟测试 | `tools/au-smoke` | plist 声明 + dlopen / 工厂 / Initialize / 渲染全部通过，**直通电平 0.250000** ✓ |
| 安装 | `~/Library/Audio/Plug-Ins/{Components,VST3,CLAP}` | ✓ |
| `auval` | — | ✗ 受本机环境阻塞（见 E6.2，与插件无关）；且曾因 E6.1 的 plist bug 而必然失败，**已修复** |

一条命令：`./scripts/build-au.sh`

### 4.2 已固定的身份参数（一旦发布不可改）

| 项 | 值 |
|---|---|
| AU type / subtype / manufacturer | `aufx` / `Rstp` / `Amch` |
| bundle id | `com.amonisachicken.rustomp.skeleton` |
| VST3 class id | `b"RustompSkeletonM"` |
| CLAP id | `com.amonisachicken.rustomp.skeleton` |

查询方式：`auval -v aufx Rstp Amch`

### 4.3 踩坑

**4.3.1 AudioUnitSDK 1.3.0 编不过**
`AUUtility.h` 用了 `std::span` 与 `requires`（C++20），而 clap-wrapper 默认 `CLAP_WRAPPER_CXX_STANDARD=17`。
→ **用 clap-wrapper 自己 CPM 钉的 `AudioUnitSDK-1.1.0`**（已写进 `fetch-third-party.sh`），不要用 latest。

**4.3.2 AU 制造商码必须含大写字母**
`auval` 直接报 `ERROR: Manufacturer OSType should have at least one non-lower case character: 'amch'`。
→ 用 `Amch`。（同理 subtype 建议大写开头。）

**4.3.3 nih-plug 把 CFBundleIdentifier 硬编码成 `com.nih-plug.<package>`**
源码 `nih_plug_xtask/src/lib.rs:772` 写死。→ 构建脚本里用 `plutil -replace` 修正，完成后才能签名。

**4.3.4 AUv2 的参数名是空的，不是 bug**
`AudioUnitParameterInfo.name` (char[52]) 被 clap-wrapper **故意清零**（`src/wrapasauv2.cpp:767`），名字走 `cfNameString`。
宿主（Logic / AULab）优先读 `cfNameString`，实测拿到 `"Output"` ✓。→ 以后看到 name 为空不要当 bug 查。

**4.3.5 冒烟测试里不能给输出总线挂 render callback**
初版给 output scope 挂了回调，回调里又调 `AudioUnitRender(inst, ..., bus 0, ...)` → 递归自身，输出恒为 0。
→ 效果器只需在 **input** 总线挂回调，宿主直接 `AudioUnitRender` 拉 output bus。

**4.3.6 只给 input bus 设 mono 格式会失败（-10868 FormatNotSupported）**
CLAP 声明的是双声道端口 → AU 只接受匹配的声道数。

---

## E5：本机 AudioComponentRegistrar 缓存损坏（2026-10-23）⚠️ 需用户处理

### 现象

```
auval -v aufx Rstp Amch
ERROR: Cannot get Component's Name strings
FATAL ERROR: didn't find the component
```

### 定位（对照组实验）

系统里已安装的第三方 AU（`/Library/Audio/Plug-Ins/Components/BYOD.component`，chowdsp）跑同一命令：

```
auval -v aufx Dqu0 Chow   →   完全相同的失败
auval -a                  →   列表中没有任何第三方 AU
```

→ **确认是环境问题，不是我们的 bundle 有问题。**

### 根因与修复

排查过程中我把 `~/Library/Caches/AudioUnitCache` 删了，但 `AudioComponentRegistrar` (PID 252) 是受保护的系统 daemon，`killall -9` 返回 `Operation not permitted`，它无法重建缓存。

修复（任一，需用户操作）：

```bash
sudo killall -9 AudioComponentRegistrar     # 无需重启
# 或直接重启系统
```

修复前在 Logic 里**看不到插件**，与插件无关。

### 绕过方案（已实现，且比 auval 更适合 CI）

`tools/au-smoke/au_smoke.c`：用 `AudioComponentRegister()` 把 bundle 的工厂函数
（`wrapAsAUV2_inst0Factory`）直接注册到本进程，完全不依赖系统注册表。
这样能完整验证 dlopen → 工厂 → 实例化 → 格式设定 → Initialize（真正加载内嵌 CLAP）→ 渲染。

这也意味着：**以后可以把 AU 行为写进 CI，不需要图形界面，也不需要 auval。**

---

## E6：Logic 看不到插件的真因（2026-10-23）🔴 已定位

用户重启服务后 Logic 里仍找不到插件。排查出 **两个独立问题**，第一个是我们自己的 bug。

### 6.1 问题 A（我们的 bug）：AU 包的 Info.plist 里没有 `AudioComponents`

**直接看已安装的 plist：**

```
CFBundlePackageType => APPL        ← 不对，应该是 BNDL
CFBundleIdentifier  => com.amonisachicken.rustomp.skeleton.component
（完全没有 AudioComponents 键）
```

没有 `AudioComponents` 声明 = 这个 bundle **根本不是个 AU** → 系统注册表不会收录它，
`auval` 报 `didn't find the component`，Logic 自然也看不到。

**机制（重要，会重复发生）：**

1. clap-wrapper 的正确 plist 是 **build 时**生成到 `build/au/rustomp_as_auv2-build-helper-output/auv2_Info.plist`；
2. 再用一条 **POST_BUILD** 命令拷进 bundle（`cmake/wrap_auv2.cmake:283`）；
3. 但 target 不需重新链接时（**增量构建**），POST_BUILD **不执行**；
4. 而 CMake 的 `MACOSX_BUNDLE` 默认模板每次都会把 `Info.plist` 写成 `APPL` 且无 `AudioComponents`。

→ 所以：**第一次完整构建正确，之后的增量构建静默损坏。** 这是最阴的一类回归：
产物存在、能 dlopen、能渲染，只是系统看不到。

**修复（`scripts/build-au.sh`）：**

- 无条件把 `auv2_Info.plist` 拷回 `Contents/Info.plist`；
- 补齐 `CFBundlePackageType=BNDL`、`LSMinimumSystemVersion=10.13`、`CFBundleSupportedPlatforms=[MacOSX]`；
- `AudioComponents.0.sandboxSafe = NO`（让 Logic 在进程内托管，不走 AUHostingService，ad-hoc 签名下更稳）；
- **断言** type/subtype/manufacturer/factoryFunction/bundleId 全部符合预期，不符直接构建失败；
- 把 **plist 校验加进 `au-smoke`**：从 Mach-O 路径反推 bundle，用 `CFBundleCreate` + `CFBundleGetValueForInfoDictionaryKey("AudioComponents")` 逐项比对。

> **教训（写给以后的自己）**：上一轮 E4 里 au-smoke “通过”给人一种虚假安全感 ——
> 它直接 `dlopen` 了 Mach-O，**完全绕过了 Info.plist**。
> 能加载 ≠ 能被系统发现。现在工具已补上这一环。

### 6.2 问题 B（本机系统状态）：AudioComponentRegistrar 客户端路径全线失效

修复 plist 后 `auval` 仍失败。做三路对照（全部看不到任何第三方 AU）：

| 探测方式 | 结果 |
|---|---|
| `auval -a` | 只有 56 个 Apple 内置 AU |
| `AudioComponentFindNext`（aufx/lpas/appl） | FOUND ✓（说明我的调用代码没问题） |
| `AudioComponentFindNext`（aufx/Dqu0/Chow = 已装的 BYOD） | NULL ✗ |
| `AVAudioUnitComponentManager.components`（Swift/AVFoundation） | **56 个，非 Apple = 0** ✗ |

但注册表日志显示：重启后 11:13:35–11:15:07 扫描了 **88 个第三方 AU 全部 success**，
且缓存文件里确实有 `Chow`/`BYOD` 条目。

→ 结论：**扫描与写缓存正常，但客户端查不到**。

**机制推断**：`launchctl print system` 显示

```
43081  -9  com.apple.audio.AudioComponentRegistrar   ← 被 SIGKILL，未重启
```

system 域的 registrar 已死（用户执行 killall 后 launchd 没拉起来），而客户端只拿到 Apple 内置表。
GUI 域那个（PID 42801）负责扫描，但它不提供客户端查询端点。

**修复（需要 root，我做不了）：**

```bash
sudo launchctl kickstart -k system/com.apple.audio.AudioComponentRegistrar
# 或直接重启整机（最可靠）
```

验证命令（BYOD 是现成的对照组）：

```bash
auval -v aufx Dqu0 Chow     # 修好后应通过 → 证明环境好了
auval -v aufx Rstp Amch     # 我们的插件
```

### 6.3 本次得到的可复用诊断手段

| 手段 | 用途 |
|---|---|
| `plutil -p <AU>.component/Contents/Info.plist` | 5 秒判断“它到底有没有被声明成 AU” |
| `auval -v aufx Dqu0 Chow`（BYOD） | **对照组**：区分“我们的问题”和“环境问题” |
| `launchctl print system \| grep -A1 AudioComponentRegistrar` | 看 system 域 registrar 是否活着（`-9` = 死了） |
| `log show --predicate 'process == "AudioComponentRegistrar"'` + `~/Library/Caches/AudioUnitCache/Logs/AUScan*.plist` | 看扫描/验证结果 |
| `tools/au-smoke` | 绕开系统注册表验证 AU 本体 + plist 声明 |

---

## E7：导入 v1 电路网表（2026-10-23）

用户提供了 `Netlist_Schematic1_2026-10-23.asc`（已归档为 `circuits/v1-fuzz/source.asc`）。

### 7.1 格式判定：能用，但不是 SPICE 网表

| 项 | 结论 |
|---|---|
| 实际格式 | **EasyEDA Pro 导出的 PADS-PowerPCB ASCII（PCB 版）**，不是 LTspice `.asc`、也不是 SPICE |
| 是否含拓扑 | ✅ 有：`*NET*` 段里 `*SIGNAL* 网名` + `REF.PIN` 列表，共 **21 个网** |
| 是否含数值 | ✅ 大部分有：`ATTRIBUTE VALUES` 段里有 `Name`/`Value` 属性 |
| 是否含引脚语义 | ❌ 没有：引脚是 **PCB 焊盘号**（`Q1.1/2/3`），不是 B/C/E 名字 |
| 是否含模型绑定 | ❌ 没有：没有 SPICE 模型、没有器件方向（阳极/阴极靠 pin 号约定） |

**结论：够用来提取「拓扑 + 数值」，作为唯一真相来源不够 —— 必须配原理图消除歧义。**

写了 `tools/netlist/easyeda_asc.py`：解析 asc → 生成 `netlist.json` + `NETLIST.md`
（BOM、网表、器件视角、自动值冲突检测）。已入库。

### 7.2 读出的电路（详见 `circuits/v1-fuzz/CIRCUIT.md`）

**4 只分立三极管搭的「离散运放」失真 + 光控输出衰减**：

```
IR ─SWI─► C1 ─► Q1(射极跟随器) ─R5─► Q2/Q3(差分对) ─► Q4(Sziklai) ─► OPO
                                    ▲                                  │
                                    └── H1(FUZZ) ◄── 反馈 ─────────────┘
                                        OPI- ─C2─R4─ GND
OPO ─C3─► R6 ─┬─ D2(1N4007)─ GND      ← 硬削波
              ├─ LED2 ─────── GND      ← 疑点，见 7.3
              ├─ C4 ───────── GND
              └─ H2(LEVEL) ─► FXOUT ─SWO─► OUT
                                └─ R1(LDR) ─ GND   ← 光控衰减
```

- 反馈拓扑：`H1`（2/3 脚短接当可变电阻）从 `OPO` 接回 `OPI-`，`OPI-` 经 `C2`+`R4` 到地
  → **非反相放大，AC 增益 ≈ 1 + H1/(R4 + 1/sC2)，DC 增益 = 1**。经典踏板增益级。
- 电源：`V+ → D1(1N4007) → VCC` 反接保护；`V-` 接输入插座的环（电池开关）。
- 电源指示：`VCC → R10(4.7k) → [C9 到地] → R9(4.7k) → LED1 → GND`，电流仅 ~0.7mA。

### 7.3 发现的三个硬问题（已写进 CIRCUIT.md 等用户确认）

| # | 问题 | 影响 |
|---|---|---|
| 1 | **网表里没有 OP07 也没有 1N4148**，只有 1N4007 + 2 只 LED + 4 只分立三极管 | 与用户此前所说「4148 + OP07」矛盾 → **建模对象未定** |
| 2 | **LED2 与 1N4007 同向直接并联** → 1N4007 的 Vf(0.7V) 钳位，LED2(1.8V) **永远不导通** → LDR 没有音频光源 | 光控压缩根本无法工作。怀疑 EasyEDA 里 LED 与 Diode 符号的 pin1/pin2 约定相反，网表"看起来同向"实际"反向" |
| 3 | **9 处元件值冲突**（库名 vs 属性名）：C1 4n7/10nF、C3 4n7/1µF、Q1 4n7... Q1-Q3 BC547B/BC549C、C9 10u/100µF | 取错值音色就完全不同。暂定以属性 `Name` 为准 |

另缺：H1/H2 电位器阻值与 taper、LDR 型号（亮/暗阻）、LED 颜色。

### 7.4 结论

- `.asc` **可用于提取拓扑与数值**，不能作为唯一真相 —— 仍需原理图。
- 我们**不需要** SPICE 网表作为产物输入：最终要手写 WDF 代码，网表只是拓扑/数值来源。
- 但「二极管/LED 方向」和「到底是不是 OP07 版本」必须由原理图或用户确认。

---

## E8：ngspice 可用性验证 + 装进 PATH（2026-10-23）

### 8.1 结果：可用 ✓

| 项 | 值 |
|---|---|
| 可执行文件 | `~/.local/ngspice/bin/ngspice`（7.4 MB 安装） |
| 版本 | **ngspice-47**，带 **KLU 直接法求解器** |
| 系统级入口 | `/usr/local/bin/ngspice` → 上面那个的符号链接（zsh / bash 登录 shell 均可见） |
| 编译选项 | `--without-x --disable-openmp --disable-debug --disable-tcl` |
| 运行时依赖 | `/usr/local/opt/readline/lib/libreadline.8.dylib`（Homebrew readline 8.3）—— **`brew uninstall readline` 会让 ngspice 失效** |

### 8.2 实测（不是"能启动"，是真跑出结果）

电路：`Vin(1kHz, 1V) — Rs 10k — [C 47nF ∥ D 1N4148] — GND`，
1N4148 模型取自 breadspice 的 `models.lib`。

| 能力 | 状态 |
|---|---|
| `.tran` 瞬态 | ✓ 20008 点（`.tran 1u 20m`），耗时 0.06 s |
| `.four` 傅里叶 / THD | ✓ **THD = 0.353802 %**，基波 0.319904 V，2 次 0.314%、3 次 0.145% |
| `-r` **ASCII rawfile** | ✓（靠工作目录 `.spiceinit` 的 `set filetype=ascii`）—— 这正是 oracle 驱动要消费的格式 |
| `.op` / `.dc` / `.ac` | ✓（breadspice 驱动已覆盖，未逐个复测） |
| 交互式绘图（nutmeg `plot`） | ✗ 不可用（`--without-x`）。要看图请把 rawfile 导出后用 Python/matplotlib |

### 8.3 已入库

| 文件 | 用途 |
|---|---|
| `scripts/build-ngspice.sh` | 从源码到安装的一键脚本（含两处历史坑的注释：系统 readline 4.2、系统 bison 2.3） |
| `tools/oracle/smoke/clipper_1n4148.cir` | 冒烟测试电路 |
| `tools/oracle/smoke/README.md` | 两条命令 + 基线数据（THD 0.353802%、基波 0.319904 V），以后 WDF 对拍就拿这组数字 |

### 8.4 对 M1/M2 的意义

oracle 链路已经不存在"工具缺失"风险了：ngspice 能出 THD/频响/波形，能出可解析的 ASCII rawfile。
剩下的工作是把 `rustomp-oracle` 写出来（喂网表 → 拿波形 → 与 WDF 逐样本/频谱比对）。

---

## E9：用 breadspice 生成权威网表并跑通基线（2026-10-23）✅

用户提供了 `fakedist.bread`（breadspice 面包板工程）并指定**以它为准**。

### 9.1 管线跑通（一条命令）

```bash
./scripts/make-netlist.sh circuits/fakedist    # .bread → circuit.json → netlist.cir
./scripts/sim-baseline.sh circuits/fakedist    # op / ac / tran+THD / 波形统计
```

| 步骤 | 工具 | 产物 |
|---|---|---|
| `.bread` → `Circuit` JSON | `tools/netlist/bread_to_circuit.py`（复刻前端的 node→net 规则） | `circuit.json` |
| `Circuit` → ngspice 网表 | `tools/netlist/nlgen`（**直接调用 breadspice 后端 `build_netlist`**，git 依赖锁 rev `eda7ef8`） | `netlist.cir`（21 器件） |
| 网表 → 基线 | `scripts/sim-baseline.sh` + ngspice-47 | `baseline.txt` |

复用面包板后端的 `build_netlist` 而不是自己重写：电位器展开成两段电阻、运放按引脚名排序、
模型库注入这些细节由上游保证。

### 9.2 电路识别（详见 `circuits/fakedist/CIRCUIT.md`）

单电源 9V · OP07 单运放 · **反馈环内非对称二极管削波**：

```
IN ─C1 4.7n─► IN+ ─ OP07 ─ OUT ─C3 1µF─► P2 500k(LEVEL) ─► 出
               │            │
      R5 4.7k ─┴─ C2 47n    └─► P1 500k（可变电阻 = 增益）
               │                  │
              IN- ◄───────────────┘
               └─ 削波环：D1(1 只) ∥ (D2+D3 串联反向)
```

三个**必须建模**的要点：

1. **削波在反馈环里**（TS/RAT 式软削波），而非对地硬削波；增益本身被压缩。
2. **1 只 vs 2 只二极管的非对称环** → 偶次谐波 + 直流偏移。
   仿真里 `v(t5L)`（串联对的中点）在 3.88–4.91 V 摆动 → 证明该支路真在导通。
3. **C3(1µF) × P2(500k) ≈ 0.5 s 的直流恢复** → "blocking distortion"，是音色的组成部分。

### 9.3 基线数据（已入 `baseline.txt`）

| 项 | 值 |
|---|---|
| DC 增益 | **1.0000**（C2 隔直，验证拓扑没错） |
| 频响峰值 | **+28.42 dB @ 2.72 kHz**（P1=250k 时的理论值 1+250k/4.7k≈54×，再乘音量电位器 1/2 分压 ✓） |
| −3 dB 带宽 | 682 Hz ~ 10.8 kHz（高频滚降来自 OP07 的 GBW 0.6MHz / 压摆率 0.25V/µs） |
| THD @0.2 V 峰值 | **27.57 %** |
| THD @1 mV 峰值 | **0.283 %**（≈线性 → 削波确实来自二极管） |
| 输出摆幅（相对 4.5 V） | −1.04 / +0.62 V（**不对称**，1 vs 2 二极管 ✓） |

### 9.4 踩坑

**9.4.1 测量窗口太短会得到错的基线**
非对称削波给 C3 充电，而放电时间常数 ≈ 0.5 s。只跑到 40 ms 时输出均值是 **−0.089 V**（还在充），
跑到 3 s 才是 **+0.00001 V**。→ 基线脚本默认 `.tran 5u 3 2.98`（只保存最后 20 ms）。

**9.4.2 保存全程 rawfile 会爆**
`.tran 1u 3 0` 在 1 µs 步长下产生 300 万点、**2.38 GB** rawfile、77 秒。
用 `start` 参数只保存尾窗 → 3.1 MB / 5.9 秒。**以后再也不要 `-r` 保存全程。**

**9.4.3 ngspice ASCII rawfile 的格式细节**
`Values:` 之后**只有每个数据点的第一行带序号**（`0<TAB><TAB>v0`），后续变量行只有值；
且点序号是 0,1,2… 递增（不是每点都写 0）。我第一版解析器按"每行都有序号"写，结果解析出 0 个点。
→ `tools/oracle/spice_raw.py` 已按正确规则实现，是后续所有对拍的基础。

**9.4.4 节点名大小写**
网表里写 `n_t27R`，ngspice 输出里一律小写 `n_t27r`。比对时要统一转小写
（`sim-baseline.sh` 已处理，否则 grep 不到输出节点）。

### 9.5 已确认（2026-10-23，用户答复）✅

| # | 问题 | 确认结果 |
|---|---|---|
| 1 | 三只二极管的环是否非对称削波 | **是，非对称削波**（1 只 ∥ 2 只反向） |
| 2 | 电位器 taper | **P1、P2 均为线性（B）** |
| 3 | 基准测试的电位器位置 | **均为 50%**（P1 → 250k，P2 → 0.5 分压） |
| 4 | 电源 | **单电源 9V，V− 接地** |

→ 电路定义完全确定，可以开始写 DSP。

---

## E10：fakedist v1 的 WDF 实现（2026-10-23）✅ 与 ngspice 对拍通过

代码：`crates/rustomp-dsp`（`diode.rs` / `fakedist.rs` / `tests/baseline.rs`）

### 10.1 实现方式

| 部分 | 做法 |
|---|---|
| 线性储能元件 C1/C2/C3 | **梯形法伴随模型** `i[n]=(2C/T)(v[n]−v[n−1])−i[n−1]`。这与 WDF 单端口在 `R=T/2C` 时（`b[n]=a[n−1]`）是**同一个差分方程**，只是用电压/电流表达，便于和节点方程合并 |
| 非线性反馈节点 IN− | **1 维牛顿**（未知量只有 `v_IN−`），2~4 步收敛 |
| 运放 | 单极点宏模型（A0=5e5、GBW=0.6MHz）。梯形离散后的更新式 `v_out = A − β·v_inm` **直接代入节点方程**解，所以**反馈环内没有一采样延迟** |
| 非线性 | 1N4148 的 `Is / N / Rs`，Rs 用内层牛顿解结电压（`v = vj + Rs·I(vj)`） |
| 电位器 | 线性 taper；GAIN=变阻器 0–500k，LEVEL=分压比 |

`reset()` 直接置到直流工作点（`IN+=IN−=OUT=4.5V`），避免插件加载时的"砰"声。

### 10.2 对拍结果（`cargo test -p rustomp-dsp --release --test baseline`）

| 项目 | WDF | ngspice | 误差 |
|---|---|---|---|
| 直流工作点 | 4.49999 V | 4.50000 V | 0.01 mV |
| 频响 50 Hz – 10 kHz | — | — | **≤ 0.31 dB** |
| 峰值增益 | +28.30 dB @ 2 kHz | +28.29 dB | +0.01 dB |
| THD @0.2 V (2–9 次) | 27.650 % | 27.616 % | +0.034 pp |
| THD @1 mV | 0.283 % | 0.283 % | < 0.001 pp |
| h2/h3/h5/h7/h9 | 3.92/22.49/11.98/7.70/5.40 | 3.92/22.47/11.96/7.67/5.37 | ≤0.03 pp |

**非线性形状逐次谐波都对上了** —— 这是比 THD 单一数字强得多的证据。

### 10.3 踩坑

**10.3.1 两只二极管串联：发射系数也要翻倍**
串联后电流相同、电压按两份分，所以 `i = Is·(exp(u/(2·N·Vt))−1)`。
我第一版只把 `Rs` 翻倍、忘了 `N·Vt`，结果串联支路在 0.7V 就导通（应该在 1.4V），
波形变得过于对称：**h2 从 3.92% 掉到 0.00%**、THD 从 27.6% 掉到 26.4%。
→ 这个错误只有"逐次谐波对拍"能立刻抓出来，只看 THD 数字会以为是"差不多"。

**10.3.2 ngspice 的 `.four` 不能当基准**
`.four` 只用 1 个周期 + 线性插值（Gridsize 200），报 THD 27.57%；
用 1 µs 步长的原始波形 + 自家 DFT 算是 **27.616%**。
→ 基准必须从 rawfile 自己算（`tools/oracle/spice_raw.py` 就是为此写的）。

### 10.4 已知不足（下一步）

| 项 | 状态 |
|---|---|
| **4x 过采样** | 未做。核心可跑 48 kHz（THD 27.49%，实测只差 0.2%），但 h10+ 会折叠，需要抗混叠 + "混叠能量"指标 |
| 二极管 `Cjo=4p` / `tt=20n` | 未做（高频细节、开关软化） |
| 压摆率限制 | 已预留 `slew_step`，当前幅度下不触发 |
| 电位器 A/C taper | 只实现线性 B |
| 插件接线 | `crates/rustomp-plugin` 仍是直通 |

---

## E11：4x 过采样 + 插件接线（2026-10-23）✅

### 11.1 做了什么

| 项 | 内容 |
|---|---|
| `oversample.rs` | 4x 多相 FIR（64 抽头 Kaiser-windowed sinc，β=8，截止 0.1125），插值每相 16 抽头；群延迟 15 个基带样本 |
| `engine.rs` | `Engine` = `Fakedist`(4×fs) + `Oversampler4x`；`latency_samples()` = 15 |
| `rustomp-plugin` | 接上 `Engine`，立体声各一实例；参数 **Gain / Level**（0–100%，线性 B taper）；`initialize()` 里 `set_latency_samples(15)`；`reset()` 到直流工作点 |
| 命名 | 插件改名 **Rustomp Fuzz**，AU 码改为 `aufx / RFz1 / Amch`，bundle id `...rustomp.fuzz` |
| `tools/au-smoke` | 输入从直流改为 **1 kHz 正弦**（电路有隔直电容，喂直流测不出东西），判据改为 RMS |

### 11.2 一个重要的发现：过采样是**正确性**要求，不是音质优化

`cargo run -p rustomp-dsp --release --example os_probe`（48 kHz 宿主，输入 0.2 V）：

| 过采样 | 电路 fs | 1 kHz | 3 kHz | 8 kHz |
|---|---|---|---|---|
| 1x | 48k | 0.4792 ✓ | **0.0000 输出归零** | **0.0000 输出归零** |
| 2x | 96k | 0.4791 ✓ | 0.4999 ✓ | 0.3130 偏低 |
| **4x** | **192k** | 0.4791 ✓ | 0.5000 ✓ | 0.4998 ✓ |
| 8x | 384k | 0.4791 ✓ | 0.5001 ✓ | 0.4998 ✓ |

48 kHz 直接跑电路时，**3 kHz 以上输入会让输出完全消失**。
原因：运放的梯形离散把闭环极点（≈10.8 kHz）放到离 Nyquist 太近，
隐含方程欠解析 → 牛顿收敛到错误解（输出被钳在 0V 轨）。

→ **4x 是正确性下限**；8x 与 4x 相差 <0.1%，暂时不需要。
→ 这条已固化成测试 `works_at_all_host_rates`（44.1k/48k/96k/192k 都必须正常）。

### 11.3 验证

```
cargo test -p rustomp-dsp --release        # 11 个测试全绿
./scripts/build-au.sh --skip-auval         # 通用二进制 CLAP/VST3/AUv2
```

AU 冒烟测试（1 kHz 正弦，512 帧）：
```
[info] 参数数量: 2
        [0] cfName="Gain"  range=0..1 default=0.5
        [1] cfName="Level" range=0..1 default=0.5
[info] 渲染 512 帧: 非有限值=0, 输出均值=-0.059, RMS=0.354
```
输出 RMS 0.354 与 ngspice 基线（0.351）一致 ✓；均值 −0.059 是 0.5s 直流恢复
在该窗口内尚未走完（正常物理行为）。

48 kHz + 4x 过采样的 THD = **27.648 %**（基线 27.616 %）✓。

### 11.4 CPU

`cargo run -p rustomp-dsp --release --example bench`
（Intel i9-10910 @3.6GHz，立体声，48 kHz 宿主，4x 过采样）

| 指标 | 值 |
|---|---|
| 每帧（两声道） | **3564 ns** |
| 单核占用 | **≈ 17 %** |

偏高但可用。主要开销是每输入样本 4 次电路求解 × 每次 2~3 轮外层牛顿 ×
每轮 2 处二极管（各带内层牛顿 + exp）。优化方向（按收益排序）：

1. `f64 → f32`（预计 1.3~1.5×）
2. 二极管结电压用上一拍热启动，内层牛顿固定 3 次
3. FIR 抽头数 64 → 48
4. 8x 反而更贵，暂不做

---

## E12：修复「弹一会儿突然自激成刺耳高频」（2026-10-23）🔴 已修复

用户实测反馈：**起初音色正确，之后突然变成与输入无关的持续刺耳高频，且无法恢复。**

### 12.1 复现

写了 `examples/transient.rs`（直流阶跃 / 脉冲 / 噪声突发 / 方波 / 低频方波）：

```
DC 阶跃 3V @1s       peak=2.545 尾段峰值=1.465 尾段过零率=0.366  ← 自激!
噪声突发 2V          peak=2.545 尾段峰值=1.492 尾段过零率=0.411  ← 自激!
```

尾段（输入已经停了）仍有 1.49 V、过零率 0.41 → **≈9.8 kHz 的持续振荡**，正是用户描述的"刺耳高频"。

### 12.2 根因（三个问题叠加）

用 `examples/limit_cycle.rs` 打出内部状态：

```
t=1.5s v_out=7.6979  v_inm=7921262780648306.0000  vc2=-12594046760029626.0000  iters=12
```

`v_inm` 跑到 **7.9e15**、`vc2` 跑到 **−1.3e16**，且每次牛顿都用满 12 次迭代（没收敛）。

1. **牛顿无阻尼**：大信号瞬态时 `f` 被二极管指数项放大，`f/df` 一步就可能飞出几个数量级；
2. **`v_out` 硬钳位破坏了前后一致性**：求解时用的是未钳位的 `v_out`，存下来的却是 `clamp(0,9)` 的结果，
   于是 `d_prev` 与实际状态不符 —— 一旦被推到轨外就锁死成极限环；
3. **状态不设限**：`vc2`/`i2` 被污染到 1e16 后，没有任何机制能把电路拉回来。

### 12.3 修复

| 措施 | 说明 |
|---|---|
| **阻尼牛顿** | 单步限制 `MAX_STEP = 0.10 V` |
| **残差判据** | 收敛判据改成 `|f| < 1e-10 A`，不再只看步长 |
| **失败兜底** | 迭代到上限仍未收敛 → 判定运放饱和，取 `u = 0`（`v_inm = v_out`），而不是把发散值写进状态 |
| **状态限幅** | `vc2/vc3` ∈ ±60 V，`i2/i3` ∈ ±10 A |
| **删掉 `v_out` 硬钳位** | 物理上限由反馈环里的二极管负责；钳位反而制造不一致 |
| **引擎级兜底** | 输出非有限或 `|y| > 100` → 立刻 `reset()` 到工作点并输出静音（一个坏样本绝不能变永久自激） |

### 12.4 验证

```
$ cargo run -p rustomp-dsp --release --example transient
DC 阶跃 3V @1s        尾段峰值=0.000 过零率=0.000  ok
脉冲 5V 1ms @1s       尾段峰值=0.000 过零率=0.034  ok
噪声突发 2V           尾段峰值=0.003 过零率=0.000  ok
方波 220Hz 2V         尾段峰值=2.026 过零率=0.009  ok   （一直在响，正常）
gain=1.0 方波 2V      尾段峰值=2.286 过零率=0.014  ok
方波 20Hz 3V          尾段峰值=3.697 过零率=0.006  ok
```

**精度完全没变**：h2 3.915 / h3 22.488 / h5 11.981 / h7 7.700（与修复前一字不差），
THD 27.648 %（基线 27.616 %），频响误差仍 ≤0.31 dB。
→ 说明修的是**数值鲁棒性**，不是靠改模型"绕过"问题。

新增回归测试 `no_self_oscillation_after_transients`：4 种波形 × 3 个 gain 档位，
要求「输入停止后尾段必须 < 0.05 V」/「持续信号过零率正常」。CPU 15.0 %（略降）。

### 12.5 同时排查的第 2 个问题（Logic 只在软件乐器插槽里出现）

用新写的 CLAP 探针（`tools/clap-probe`）+ AU 探针（`tools/au-smoke` 增强）取证：

| 检查项 | 结果 |
|---|---|
| CLAP 端口 | 输入 1 个 stereo、输出 1 个 stereo，feature = `audio-effect` ✓ |
| AU 组件类型 | **`aufx`** ✓（不是 `aumu`） |
| AU 总线 | 输入 1 条、输出 1 条 ✓ |
| `SupportedNumChannels` | `in=2 out=2` ✓ |
| 参数 | Gain / Level ✓ |

→ **插件本身是一个完全正常的效果器 AU**，没有任何"乐器"特征。
怀疑与本机 `AudioComponentRegistrar` 损坏（见 E5/E6）导致 Logic 的 AU 数据库分类降级有关，
需用户做对照实验（见 §12.6）。备用实验：把 `sandboxSafe` 从 0 改回 clap-wrapper 默认的 1 再试。

### 12.6 待用户确认

1. Logic 里**已安装的其他第三方效果器**（如 BYOD / CLA-76）在音频轨的 Audio FX 菜单里能看到吗？
   - 如果**也看不到** → 是 Logic/AU 注册表的问题，不是本插件；
   - 如果**能看到** → 是本插件被误分类，我改 `sandboxSafe` / 换 facade 再试。
2. 跑过 `sudo launchctl kickstart -k system/com.apple.audio.AudioComponentRegistrar`（或重启）了吗？
   `auval` 若能通过，Logic 的插件管理器就能正常分类。

---

## E13：修复「Logic 只在软件乐器插槽里出现」（2026-10-23）✅

用户反馈：BYOD / CLA-76 等第三方效果器在音频轨的 Audio FX 里都能找到，
**只有我们的找不到**；而且「从软件乐器槽拖到音频轨后就能找到了，但重启 Logic 又没了」。

### 13.1 决定性证据：Logic 的 AU 缓存

`~/Library/Preferences/com.apple.logic10.plist` 里有一份 **AU 能力缓存**，
每个插件一个条目（`aufx-RFz1-Amch`）。对比：

| 插件 | MIDIProtocol |
|---|---|
| **Rustomp Fuzz（我们）** | **1** ← 被当成 MIDI 插件 |
| BYOD / Waves 等原生 AUv2 效果器 | **0** |

Logic 把「类型=aufx 且支持 MIDI」的插件归类为 **"MIDI 控制效果器"**，
而 MIDI 控制效果器在 Logic 里**只出现在软件乐器轨道的插槽**中。
这解释了两个现象：音频轨找不到；拖过去之后 Logic 的缓存被就地改写于是短暂能用了，
重启后重新读缓存又变回去。

### 13.2 根因：clap-wrapper 无条件宣称支持 MIDI

`third_party/clap-wrapper/src/wrapasauv2.cpp` 的 `GetPropertyInfo()` 里，
`kAudioUnitProperty_AudioUnitMIDIProtocol` / `HostMIDIProtocol` /
`MIDIOutputCallback(Info)` 都在 `#if AUSDK_MIDI2_AVAILABLE` 下**无条件**返回 `noErr`，
完全不管被托管的 CLAP 有没有 note port。我们的插件是 `MidiConfig::None`，一个 note port 都没有。

### 13.3 修复：给 clap-wrapper 打本地补丁

`patches/0001-auv2-advertise-midi-only-when-present.patch`
（`scripts/fetch-third-party.sh` 自动应用，幂等）：

- 新增 `hasMIDI() = _midi_wants_midi_input || !_midi_outports.empty()`
- 只有 `hasMIDI()` 为真时才返回 MIDI 相关属性，否则 `kAudioUnitErr_InvalidProperty`
- `MIDIOutputCallback(Info)` / `MIDIOutputEventListCallback` 只在有 MIDI 输出时宣称

`tools/au-smoke` 增加了这项检查：

```
[ ok ] 未宣称支持 MIDI（纯音频效果器）→ Logic 会归到 Audio FX
```

### 13.3b 第一版补丁不够：还有 MusicDevice 专属属性

只把 MIDI 属性门控住之后，Logic 仍然把插件归到乐器槽（用户复测失败），
而且缓存里 `MIDIProtocol` 依旧是 **1**。说明触发点不止 MIDI 属性。

继续读 `GetPropertyInfo()`，发现还有三个**乐器专属**属性是无条件宣称的：

| 属性 | 用途 |
|---|---|
| **`kMusicDeviceProperty_InstrumentCount`** | MusicDevice（乐器）专属；**宿主正是靠它判断"是不是乐器"** |
| `kMusicDeviceProperty_SupportsStartStopNote` | 同上 |
| `kMusicDeviceProperty_DualSchedulingMode` | 同上 |

→ 补丁扩展为「`kMusicDeviceProperty_*` 只在 `_autype == aumu_musicdevice` 时宣称」。

验证（`tools/au-smoke` 新增检查）：

```
[ ok ] 不支持 kMusicDeviceProperty_InstrumentCount（效果器应有行为）
[ ok ] 不支持 kMusicDeviceProperty_SupportsStartStopNote（效果器应有行为）
[ ok ] 未宣称支持 MIDI（纯音频效果器）→ Logic 会归到 Audio FX
```

### 13.3c 还没解决：真正的原因是**声道布局**

清缓存重扫后，用户反馈：**插件管理器里能看到、且正确归类为 effect**，
但音频轨（单声道）的插入菜单里依然没有。

→ 说明分类已经正确，问题在别处。继续查：

| 检查 | 结果 |
|---|---|
| CLAP 的 `clap.audio-ports-config` | **2 个**：Stereo(2/2)、**Mono(1/1)** |
| AU 的 `SupportedNumChannels` | **只有 1 组：{2,2}** ← 差异在这里 |

**根因**：clap-wrapper 只通过 `clap.configurable-audio-ports`（新扩展）枚举可选布局，
而 **nih-plug 只实现较旧的 `clap.audio-ports-config`**。于是 `_channelCapsCache` 为空，
AU 退回按端口数推导 → 只报 `{2,2}`。Logic 的插入菜单按通道条声道数过滤：
**单声道音轨需要 in=1 的配置**，我们没有 → 插件从菜单消失；
插件管理器读静态信息，所以照常显示。也解释了"拖过去能用"（Logic 强行适配）。

**修复**（同一补丁的第二部分）：

- `PostConstructor()`：`_channelCapsCache` 为空且插件有 `clap.audio-ports-config` 时，
  用它枚举布局（仅限「单主输入 + 单主输出」，多总线不臆测）；
- `applyConfigurationFromBusFormats()`：宿主选定布局与某个 config 匹配时调用
  `apc->select(plugin, cfg.id)` 真正切换（在 `activateCLAP` 之前，符合规范）。

**验证**：

```
SupportedNumChannels: 2 组 → in=2 out=2 / in=1 out=1
单声道渲染 RMS=0.354240 ✓    立体声渲染 RMS=0.354240 ✓
```

### 13.4 用户侧还需做一步：清掉 Logic 里的旧缓存

Logic 缓存里已经有错误的 `MIDIProtocol=1`，需要让它重新扫。两种办法（选一）：

```bash
# A. 退出 Logic 后，删掉这两个缓存条目（旧的 Rstp 也在里面，一并清掉）
plutil -remove "aufx-RFz1-Amch" ~/Library/Preferences/com.apple.logic10.plist
plutil -remove "aufx-Rstp-Amch" ~/Library/Preferences/com.apple.logic10.plist
killall cfprefsd
```

或 B. 打开 Logic → 设置 → 插件管理器 → 重置并重新扫描所选。

### 13.5 经验

> 第三方封装层「无条件宣称支持某个可选能力」，会让宿主做出错误的**分类**决定。
> 这类问题不会报错、不影响出声，只会让插件"出现在错误的地方"，
> 极难从插件自身代码看出来 —— 必须去看**宿主的缓存文件**。

---

## E14：Waves 式声道变体（2026-10-23）

用户建议：像 Waves 那样分别出「只支持单声道」和「只支持立体声」的独立插件。
这个思路正确，而且有个额外收益：**新插件身份 = 新缓存条目 = 宿主强制全新扫描**，
天然绕开任何陈旧缓存。

### 14.1 先确认：缓存确实一直没刷新

`./scripts/check-au.sh` 显示 Logic 的缓存里仍是：

```
MIDIProtocol: 1              ← 这是**两次修复之前**的值
ChannelConfigurations: ['2/2']
```

如果 Logic 真的重扫过，`MIDIProtocol` 必然是 0（MusicDevice 属性已不再宣称）。
→ 说明 `plutil -remove` 没生效：**Logic 运行时会把内存里的 pref 写回**。
新增 `scripts/reset-logic-cache.sh`：先检测 Logic 是否在运行，退出后才删除并 `killall cfprefsd`。

### 14.2 实现：三个独立身份

插件 crate 增加 `mono` / `stereo` feature，各自切换 `AUDIO_IO_LAYOUTS`、
插件名、CLAP id、VST3 class id（AU subtype 由构建脚本传入）：

| 插件 | AU 码 | 通道能力 | CLAP id |
|---|---|---|---|
| `Rustomp Fuzz`（默认，双布局） | `aufx / RFz1 / Amch` | `{1,1}` + `{2,2}` | `...fuzz` |
| **`Rustomp Fuzz Mono`** | **`aufx / RFm1 / Amch`** | **`{1,1}`** | `...fuzz.mono` |
| **`Rustomp Fuzz Stereo`** | **`aufx / RFs1 / Amch`** | **`{2,2}`** | `...fuzz.stereo` |

新脚本 `scripts/build-variant.sh mono|stereo`：构建 → 重命名 bundle 并修正 plist →
clap-wrapper 生成 AU → 签名 → 安装 → 自检。

### 14.3 验证（三个都装好，都能出声）

```
Rustomp Fuzz Mono    → in=1 out=1              RMS=0.348448 ✓
Rustomp Fuzz Stereo  → in=2 out=2              RMS=0.348448 ✓
Rustomp Fuzz         → in=2 out=2 / in=1 out=1 RMS=0.348448 ✓
```

### 14.4 踩坑

`build-variant.sh` 第一版报 `NAME: unbound variable`：
macOS 自带 bash 3.2 会把**紧跟变量的多字节字符**（如 `$NAME（` 里的全角括号）
当成变量名的一部分。→ 变量后面跟中文时必须写 `${NAME}`。

### 14.5 结论：有效 ✅

用户实测：**单声道音轨的 Audio FX 里终于出现了 `Rustomp Fuzz Mono`**。
立体声轨用 `Rustomp Fuzz Stereo`。至此 M0 全部完成。

**从这件事得到的通用规律（以后所有插件都照此办理）：**

> 宿主在插入菜单里**按通道条的声道数过滤 AU**。
> 一个"同时支持 mono + stereo"的插件能否被正确识别，
> 取决于宿主是否可靠地探测到多套布局 —— 实测（Logic 11 / macOS 13）不可靠。
> **拆成「单声道版 + 立体声版」两个只声明单一布局的独立身份最稳**，
> 且各自拥有独立的 AU 码与缓存条目，互不干扰。

标准入口已固化为 `./scripts/build-all.sh`（默认只出 Mono + Stereo；
`--with-dual` 才会额外出双布局版本）。

### 14.6 量产时的取舍

三个身份共存只是过渡方案。**最终发布只发 `Mono` + `Stereo` 两个**
（`build-all.sh` 默认行为），双布局版本仅作调试用，不进发行包。

---

## E15：技术评估 —— CPU 优化与 GUI（2026-10-23，先评估后动手）

### 15.1 CPU 实测分解（`cargo run --example profile` / `--example diode_bench`）

单声道、48 kHz 宿主、4x 过采样，Intel i9-10910：

| 项目 | 成本 |
|---|---|
| **整个引擎（1 个 48k 样本）** | **1592 ns** |
| 电路求解 1 次（192k 样本） | 396 ns → ×4 = 1585 ns |
| 过采样 FIR（上+下采样） | **72.7 ns（≈4.6%）** |
| 外层牛顿平均迭代 | 3.48 次/样本 |
| 每次电路求解的二极管调用 | 2 支路 × 3.48 ≈ **7 次** |

**二极管模型的成本随工作点剧烈变化**（这是关键发现）：

| 端电压 | 单次 `current_and_conductance` |
|---|---|
| 0.0 V | 10.6 ns |
| 0.5 V | 50.1 ns |
| 0.65 V | 78.9 ns |
| 0.80 V | 129.6 ns |
| **1.20 V** | **315.6 ns** |

而 `exp` 本身只要 **3.9 ns**（f64）/ 3.76 ns（f32）。

→ **瓶颈是二极管里的内层牛顿**：高电流时它达到 12 次迭代上限还没收敛
（因为初值 `vj = min(v,1.2)` 离真解很远，且容差 1e-9 过严）。
这也解释了为什么"大信号段落"比小信号更吃 CPU。

### 15.2 结论：哪些优化有效、哪些是伪优化

| 优化 | 预期收益 | 是否改变输出 |
|---|---|---|
| **二极管内层牛顿：热启动 + 容差放宽到 1e-6 V + 上限 4 次** | **最大（2 倍以上）** | 否（1e-6 V 误差 ≈ −100 dB） |
| 外层牛顿更好初值 | ~15% | 否（收敛解相同） |
| 两声道合并处理（ILP） | 10~30% | 否（逐位相同） |
| 预计算常数 / 减少重复除法 | 5% | 否 |
| **f64 → f32** | **≈0**（exp 只快 3.6%） | 会（舍入不同） |
| 减少 FIR 抽头 | ~2%（只占 4.6%） | 会（滤波变差） |
| 快速 exp 近似（查表/多项式） | 有，但比第 1 项小得多 | **会**（需 A/B 量化） |
| 降过采样到 2x | 省 2 倍电路 | **会**，且实测 8 kHz 输入已失真（0.313 vs 0.5）→ 不接受 |

**保守估计**：只做「不改变输出」那几项，1592 ns → **600~800 ns**，
CPU 15% → **6~8%**，且 h2…h9、THD、频响必须与现在**完全一致**（作为硬性验收）。

### 15.3 音色影响的判定标准（写死，避免"优化着优化着声音变了"）

每次优化只允许以下两种结果之一：

- **A 类（数值等价类）**：`cargo test -p rustomp-dsp --release --test baseline` 的
  h2–h9 与当前值（3.915 / 22.488 / 2.428 / 11.981 / 1.836 / 7.700 / 1.484 / 5.400）
  误差 **≤0.01 pp**，频响误差 ≤0.05 dB，THD 差 ≤0.02 pp；
- **B 类（近似类）**：允许更大偏差，但必须**先量化**并记录到 DEVLOG，且不与 A 类混在同一次提交里。

### 15.4 GUI 的技术栈（三层）

```
Logic / 宿主
   │  AUv2: kAudioUnitProperty_CocoaUI → AudioUnitCocoaViewInfo（NSView 工厂）
   │  VST3: IPlugView      CLAP: clap.gui + CLAP_WINDOW_API_COCOA
   ▼
clap-wrapper 的 Cocoa 桥   ← 我们的 AU 二进制里已有符号 fillAUCV_wrapAsAUV2_...（通路是通的）
   ▼
nih-plug 的 baseview（Cocoa 窗口/事件/缩放）
   ▼
UI 框架：vizia / egui / iced（或自带 wgpu、softbuffer 渲染）
```

要点：

1. clap-wrapper **只有**在 CLAP 的 GUI 声明支持 `CLAP_WINDOW_API_COCOA` 时，
   才对宿主暴露 `kAudioUnitProperty_CocoaUI`（见 `wrapasauv2.cpp` 里那段判断）。
   nih-plug 的 baseview 走的就是 Cocoa，所以接上 `Editor` 后 AU 会自动有界面。
2. 我们现在的插件 **没有 GUI**（Logic 缓存里 `HasCustomView = false`），
   所以只能看到 Logic 的通用参数面板。
3. 线程模型：GUI 在**主线程**，音频在音频线程，两者只通过无锁原子（`FloatParam`）通信；
   GUI 里绝不允许加锁或分配影响到音频线程。
4. 还要处理：HiDPI/缩放、窗口尺寸持久化、打开/关闭不能阻塞（宿主的 GUI 超时有上限）。
5. 两个声道变体共用同一套 GUI 代码，只是 bundle 名不同。

### 15.5 GUI 框架选型（建议，待定）

| 方案 | 优点 | 缺点 | 适用 |
|---|---|---|---|
| **egui**（`nih_plug_egui`） | 即时模式，画旋钮/贴图最直接；代码量最小；迭代快 | 每帧重绘；精细动效要自己写 | **快速做出可用的踏板 UI** |
| **vizia**（`nih_plug_vizia`） | 声明式 + 类 CSS 样式，nih-plug 官方示例（Diopser/Crisp）在用 | 学习曲线，自定义绘制不如 egui 直接 | 追求精致/动画的成品界面 |
| 自绘（baseview + wgpu/softbuffer） | 完全控制（可做拟物踏板、金属质感） | 工作量最大 | 品牌级外观 |

三个后端都在同一 nih-plug rev 内（ISC 许可，与 GPL-3 兼容）。
**倾向**：先用 **egui** 做一个「踏板面板图 + 2 个旋钮」的可用界面（约 1 天），
外观定型后再决定是否换 vizia 或自绘。

---

## E16：CPU 优化（A 类，数值等价）— 1592 → 824 ns，1.93x（2026-10-23）

只做「不改变输出」的优化。全程用 `cargo test -p rustomp-dsp --release` 对拍，
最终 **h2–h9 逐次谐波与优化前逐位一致**。

### 16.1 结果

| 指标 | 优化前 | 优化后 | 变化 |
|---|---|---|---|
| 电路（1 个 192k 样本） | 396 ns | **206 ns** | −48% |
| 引擎（1 个 48k 样本，4x） | 1592 ns | **824 ns** | **−48%** |
| 立体声 48k CPU | 15.0% 单核 | **7.8% 单核** | −48% |
| 外层牛顿迭代 | 3.48 次 | **2.00 次** | −43% |
| h2 / h3 / h5 | 3.915 / 22.488 / 11.981 | 同左 | 0 |
| THD@0.2V / @1mV | 27.650 / 0.283 | 同左 | 0 |
| 频响最大误差 | 0.31 dB | 0.31 dB | 0 |

### 16.2 真正有效的三项

| 优化 | 收益 | 原理 |
|---|---|---|
| **外层牛顿用线性外推初值**（`2*v[n-1] − v[n-2]`，偏离>0.2V 时退回） | **1.7x**（3.48→2.00 次迭代） | 音频是平滑信号，外推能贴着解走；少一次迭代就少两处二极管求值 |
| 二极管 `junction()` 里消掉除法（预计算 `1/(N·Vt)`） | ~11% | 浮点除法比乘法贵 4~8 倍，热路径里每样本有十几次 |
| `process()` 里所有 `/fs`、`/r_in`、`/2C·T` 预计算成常数 | ~3% | 同上 |

### 16.3 试过但**没用**的（都是被实测否掉的）

| 想法 | 实测 | 原因 |
|---|---|---|
| **二极管内层牛顿热启动** | **更慢**（1592→1785 ns） | 外层牛顿每步让 `u` 跳 0.1V，热启动的猜测常落在 0.5V 之外而失效，还多了一次分支 |
| **f64 → f32** | ≈0 | `exp` 在 f32 只快 3.6%（3.76 vs 3.90 ns），而 exp 才是主要开销 |
| 把外层容差放宽到 1e-8 | 「更快」但**失真** | 小信号下（1mV→nA 电流）绝对容差比信号还大，50Hz 增益偏 0.33 dB |
| 改为相对容差 `f ≤ 1e-14 + 1e-9·scale` | 更慢（4.16 次迭代） | 大信号时比原来的 1e-10 更严 |
| 减少过采样 FIR 抽头 | ≈0 | FIR 只占 4.6~9% |
| **把 `clipper` 改成 `&mut self`** | **−30%**（1592→2048） | 借用检查器迫使热循环里每个字段每轮从内存重载，寄存器被挤掉 |

### 16.4 多线程：能到 1.97x，但**建议暂不做**

原型 `examples/mt_bench.rs`（左右声道各一线程，256 样本一块，自旋握手）：

| 方式 | ns/帧 | 合计单核占用 |
|---|---|---|
| 单线程 | 1758 | 8.4% |
| 双线程 | **892** | 4.3%（两块核各一半） |
| **加速比** | **1.97x** | 握手开销仅 0.06 µs/块 |

为什么不建议现在上：

1. **只有立体声变体受益**（1.97x）；**单声道变体一点都拿不到**（电路逐样本递推，单声道内无并行度）。
   而我们发布的两个变体里，吉他轨用的恰恰是 Mono。
2. 宿主本来就会把**不同插件实例 / 不同轨道**并行调度；同一条轨上的单个实例再并行，收益有限。
3. 代价：自旋会让工作线程空转烧一个核；改用 park/unpark 又引入 5~20 µs 唤醒抖动。
   两者都要长期承担实时线程调度的风险（优先级反转、宿主线程预算）。
4. 现在 7.8% 已经不构成瓶颈。

**结论**：先把单线程保持在 8% 以内；如果将来上 8x 过采样或更大的电路，
再把这个原型产品化（parked worker + 块级派发，可用开关控制）。

---

## E17：无 GUI 阶段冻结 + 面板素材评估（2026-10-23）

### 17.1 冻结

用户确认音色已达预期（"很有机的声音"、动态最好）。加入 GUI 之前先冻结一版：

- git tag **`v0.1.0-nogui`**
- 二进制存档：`dist/v0.1.0-nogui/`（**不入库**，`.gitignore` 已加 `/dist/`）
  含 3 个变体 × 3 种格式（AUv2 / VST3 / CLAP），`SHA256SUMS` 校验通过，
  `README.md` 记录了当时的技术状态（精度 / CPU / 延迟 / 参数）。

| 项 | 值 |
|---|---|
| 精度 | h2–h9 ≤0.03 pp（对 ngspice）；THD 27.650% vs 27.616%；频响 ≤0.31 dB |
| CPU | 立体声 48 kHz + 4x ≈ **7.8%** 单核 |
| 延迟 | 15 样本 |
| 参数 | Gain / Level |

### 17.2 面板素材 `assets/sparklindrive.svg` 的评估

竖版踏板面板，`viewBox 0 0 170.08 317.48`（宽高比 **1 : 1.867**），62 条路径 + 6 个圆
+ 2 个 rect + 2 个 text，**没有背景矩形（透明底）**。

| 元素 | 几何 | 用途 |
|---|---|---|
| LED | 圆 `(85.04, 33.83) r=8.5`，`fill:#e60012` | 可做亮灭/呼吸 |
| **LEVEL 旋钮** | 圆 `(43.41, 71.59) r=27.86`，`fill:#231815` | 可旋转 |
| **GAIN 旋钮** | 圆 `(126.51, 70.43) r=27.86`，同色 | 可旋转 |
| 指针 ×2 | `rect w=0.98 h=24.59`，从圆心竖直指向上方（12 点） | **需要单独旋转** |
| 小开关符号 | 中心约 `(85, 122)`，两条短线 | 用途待确认 |
| 大六角螺母 | `(85.33, 236.17) r=19.84` | 装饰（插孔螺母？） |
| 文字 | `GAIN` / `LEVEL`，**字体 Maple Mono 未嵌入**（无 `@font-face`） | 会回退成系统字体 |

**三个需要处理的技术点**（已向用户提出）：

1. **指针**：目前是画死在 12 点方向的两个 rect。GUI 里指针要随参数旋转，
   所以要么用户删掉这两个 rect（我也可以程序化删除），要么我在代码里画指针、用户给无指针版本。
2. **字体未嵌入**：`GAIN`/`LEVEL` 用 Maple Mono（无 `@font-face`），
   resvg 渲染会回退 → 外观会变。建议**转曲**（文字转路径），或提供字体文件。
3. **透明背景**：面板没有底色，窗口底色需要决定（白 / 米白 / 深色）。

### 17.3 GUI 实现方案（拟定）

- 渲染：`resvg`（纯 Rust，MIT）把 SVG 光栅化成纹理，按窗口像素尺寸 × 缩放因子生成，
  缓存；窗口缩放/换屏时重建。
- 框架：`nih_plug_egui`（即时模式，画纹理 + 叠加可拖拽的旋钮热区最直接）。
- 交互：旋钮区域按圆心/半径定义热区，鼠标上下拖动改参数（Logic 里也可用宿主自动化）；
  指针角度 = `-135° + 270° × value`；双击回默认值。
- LED：可按"有信号"或"削波"点亮度（待定，需要用户决定语义）。
- 两个声道变体共用同一套 GUI 代码。

---

## E18：GUI（egui）+ 新功能（对称削波 / bypass / 削波指示）+ 改名（2026-10-23）

### 18.1 面板素材

用户提供了两版：`assets/sparklindrive.svg`（初版）与
`sparklindrive_画板 1.svg`（**GAIN/LEVEL 已转曲，无 text 元素、不依赖字体**）。

加工流程（已固化）：

1. 用脚本删掉两个指针 `rect`（GUI 里程序化绘制可旋转指针）；
2. 插入 `#faf1e0` 背景矩形 → `assets/panel.svg`；
3. `inkscape --export-width=680` 光栅化成 **`assets/panel@2x.png`**（680×1269）；
4. 插件 `include_bytes!` 内嵌这张 PNG，运行时解码并缩放到窗口像素尺寸；
   不引入 SVG/字体解析依赖。

面板几何（SVG 坐标，`viewBox 170.08 × 317.48`，窗口 **340×635**）：

| 元素 | 坐标 | 交互 |
|---|---|---|
| LED | `(85.04, 33.83) r=8.5` | 削波指示；bypass 时变灰 |
| LEVEL 旋钮 | `(43.41, 71.59) r=27.86` | 上下拖动 / 滚轮 / 双击回 50% |
| GAIN 旋钮 | `(126.51, 70.43) r=27.86` | 同上 |
| 削波模式开关 | `(85, 124.5)` | 点击切换：上=非对称（3 只），下=对称（2 只） |
| bypass 开关 | `(85.33, 236.17)` | 点击切换；按下时压暗 |

指针角度：`−135° + 270° × value`（0.5 = 正上方，与素材一致）。
用 Inkscape 合成预期画面核对过角度（LEVEL 72% / GAIN 30% 的指向正确）。

### 18.2 新增功能

**对称削波模式**（开关向下）：物理上等于**短接 D3**，剩 D1 ∥ D2（正反各一只）。
先用 ngspice 建参考（把 `DD3` 换成 0.001Ω 短接）：

| 指标 | 非对称（原状） | 对称 |
|---|---|---|
| THD(2..9) @0.2V | 27.616% | **25.516%** |
| h2 | 3.92% | **0.049%**（偶次几乎为零 ← 对称的判据） |
| h3 / h5 / h7 | 22.47 / 11.96 / 7.67 | 20.91 / 11.36 / 7.47 |
| 基波 | 0.4791 V | 0.3509 V |

WDF 实现与参考逐次谐波误差 ≤0.05 pp（`symmetric_mode_matches_ngspice`）。

**True bypass**：干路做了与过采样链路等长的 **15 样本延迟对齐**，
切换用 **5 ms 交叉淡化**，因此不会产生梳状滤波和爆音；
bypass 时电路**继续运行**（避免接回时状态陈旧）。LED 变灰由 GUI 处理。

**削波指示**：`Fakedist` 暴露削波环两端电压 `|u|`，
`Engine` 做 `(|u|−0.55)/0.8` 映射 + 150 ms 慢释放包络，供 LED 使用。
原子（`AtomicU32` 存 f32 位）传给 GUI，音频线程不加锁。

### 18.3 改名与身份

| 项 | 旧 | 新 |
|---|---|---|
| 插件名 | Rustomp Fuzz Mono/Stereo | **Sparklin Drive Mono/Stereo** |
| 厂商 | amonisachicken | **HONEYPIE EFFECTS** |
| AU 码 | `aufx/RFm1/RFs1/Amch` | **`aufx/SPm1/SPs1/HpEf`** |
| bundle id | com.amonisachicken... | **com.honeypieeffects.sparklindrive[.mono/.stereo]** |
| CLAP/VST3 id | 同步更新 | 同步更新 |

旧的 `Rustomp Fuzz*` 已从 `~/Library/Audio/Plug-Ins/*` 清除。

### 18.4 验证（au-smoke）

```
[info] 注册的组件类型: aufx / SPm1 / HpEf
        in=1 out=1
[info] 参数数量: 4
        [0] "Gain"  [1] "Clip Mode SYM"  [2] "Level"  [3] "Bypass"
[ ok ] 不支持 kMusicDeviceProperty_InstrumentCount
[ ok ] 不支持 kMusicDeviceProperty_SupportsStartStopNote
[ ok ] 提供自定义界面（CocoaUI）        ← GUI 已通过 clap-wrapper 接通
[ ok ] 未宣称支持 MIDI
[ ok ] 音频通路正常
```

CPU 仍为 **7.8%**（立体声 48k + 4x），DSP 测试 14 项全绿。

### 18.5 待打磨

- 削波模式开关目前用**小圆点**表示拨杆方向（上/下）；素材里那个拨杆是画死的，
  要做得更像实物需要单独出一张"拨杆"图层。
- bypass 按下状态用压暗表示；同样可以出一张"按下"图层。
- LED 亮度曲线、指针粗细可微调。

---

## E19：界面细节打磨（2026-10-23）

用户试弹后的四条反馈，全部落地：

| 反馈 | 处理 |
|---|---|
| 旋钮很跟手 | 保持（260 逻辑像素 = 全行程） |
| **LED 亮暗对比不够** | 灭 = 暗红 `(58,2,8)`；亮 = 接近纯红 `(255,72,66)` + **外发光圈**（半径 1+1.1t 倍，透明度随电平）。对比从 ~1.6× 拉到 ~4× |
| **要真拨杆** | 用户补了两张状态图 `tuggle_{up,down}_*.svg`（viewBox 23.68×25.21），光栅化成 `assets/switch_{up,down}@4x.png`；切换时替换贴图 |
| **bypass 不应改变外形** | 去掉压暗/内圈；改为**只有按住时**把开关区域缩到 **0.9 倍**，松开恢复 |
| **厂商名要 HONEYPIE EFFECTS** | AU plist 的 `AudioComponents.name` 与 CLAP 描述符都改为 `HONEYPIE EFFECTS`（cmake 的 `RUSTOMP_AU_MANUFACTURER_NAME` + 构建脚本传参） |

### 19.1 拨杆贴图的对位（这一步最容易出错）

独立开关 SVG 的 viewBox 是 `23.68 × 25.21`，而面板上那个开关的 bbox 是
`(73.4894, 109.989) 23.6813 × 23.2638`（比它矮 1.95 —— 因为独立文件要同时容纳
"朝上"和"朝下"两种拨杆位置）。

用**六角形**做基准对齐：

- 面板里 `polyline36` 的 bbox 中心 = `(85.33, 121.825)`
- 独立文件里那条 polyline 的顶点中心 = `(11.84, 13.37)`
- ⇒ 贴图偏移 = `(85.33 − 11.84, 121.825 − 13.37) = (73.49, 108.455)`

贴图前先用底色矩形盖住面板里画死的那个开关（否则会与旧拨杆重影）。
**用 Inkscape 合成 mock 图核对过**：放大看无重影、六角形严丝合缝。

### 19.2 bypass 按压反馈的实现

不能直接缩放纹理的一部分，做法是：

1. 从**面板纹理**用 `painter.image(tex, dest, uv, ...)` 取开关区域的子矩形（uv 子区域）；
2. 先用底色填充原位置（抹掉）；
3. 再把子矩形贴到 `0.9×` 的目标矩形里。

`Response::is_pointer_button_down_on()` 判定按住状态，松开后下一帧不再重绘即恢复。

### 19.3 验证

```
AU:  aufx / SPm1 / HpEf    name = "HONEYPIE EFFECTS: Sparklin Drive Mono"
CLAP: vendor = HONEYPIE EFFECTS
参数: Gain / Clip Mode SYM / Level / Bypass
[ ok ] 提供自定义界面（CocoaUI）
[ ok ] 音频通路正常
```

---

## E20：开源发布 —— 定位为「框架 + 示例插件」（2026-10-23）

项目已推送到 <https://github.com/amonisachicken/rustomp>（GPL-3.0，master + tag `v0.1.0-nogui`）。

### 20.1 定位调整

用户的意图明确后，仓库定位从「一个插件」调整为：

> **rustomp = 用 Rust 做电路级建模的音频效果器框架**
> **Sparklin Drive = 框架自带的示例插件**

据此新增根 `README.md`：三层结构（`rustomp-dsp` / `circuits` / `rustomp-plugin`）、
快速开始、**「接入你自己的电路」的 5 步工作流**、对拍数据表、Logic 兼容的三个补丁说明、
示例插件介绍、已知限制与路线图、文档索引。

### 20.2 界面收尾（本轮）

| 反馈 | 处理 |
|---|---|
| LED 颜色 | 暗 `#a00000` → 亮 `#ff0000` 线性插值 |
| **拨杆看起来糙** | 真因是**解码时忽略了 PNG 的 alpha**，透明像素被当成黑色，边缘发脏。改用 `--export-background="#faf1e0"` 导出**不透明**贴图 |
| **删除画死的拨杆** | 在 `assets/panel.svg` 里用底色矩形盖掉原拨杆（含下方那段残留弧，覆盖范围 `72.3,108.5,26.0,28.8`），烘焙进底图；运行时不再需要盖布，无重影 |
| 点击区域 | 取两种状态贴图的**并集**（两者 viewBox 相同，即同一个矩形） |
| 按住反馈 | 以开关中心**缩放到 0.9 倍**，松开恢复 |

贴图对齐仍用六角形基准：偏移 `(73.49, 108.455)`；已用 Inkscape 合成两态 mock 放大核对，
确认无残留、无重影。

### 20.3 仓库整理

- 素材分层：`assets/` 只放**构建产物**（`panel@2x.png`、`switch_*@4x.png`），
  原始设计稿移到 `assets/source/`；
- `dist/`、`target/`、`build/` 不入库；`third_party/` 由
  `scripts/fetch-third-party.sh` 按固定 commit 拉取并自动打补丁。

---

## E21：拨杆对齐与"按下缩小"失效（2026-10-23）

两个 UI bug，根因都很具体：

### 21.1 两种状态高度不对齐

两张开关贴图**在各自 viewBox 里六边形的高度不同**：

| 贴图 | 六边形水平两顶点 | 基线 y |
|---|---|---|
| up | (2.12, 13.37) / (21.56, 13.37) | **13.37** |
| down | (2.12, 11.84) / (21.56, 11.84) | **11.84** |

相差 **1.53**，所以用同一个偏移贴上去，切换时整体会上下跳。

**修正**：以「六边形水平方向两顶点连线」为对齐基准（面板原图里这条线的中点是
`(85.33, 121.825)`），分别计算两态偏移：

- up → `(73.49, 108.455)`
- down → `(73.49, 109.985)`

### 21.2 按下不缩小

之前用的是 `ctx.is_pointer_over_area() && pointer.primary_down() && rect.contains()`，
在 `CentralPanel` 里判定并不可靠，而且绘制发生在 `interact()` **之前**，拿不到本帧状态。

**修正**：改成「先 `ui.interact()`，再用 `Response::is_pointer_button_down_on()`」；
缩放以六边形基线中点为原点（`p + (origin − p) × 0.9`，尺寸 `× 0.9`），
而不是简单地把矩形居中缩放（否则贴图位置会漂）。

点击区域 = 两态贴图的**并集**矩形 `(73.49, 108.455, 23.68, 26.74)`。

---

## E22：窗口重开后界面消失（2026-10-23）

**症状**：关闭插件窗口再打开，贴图全没了（面板、拨杆），但点击对应位置仍能调参数，
只剩 LED 红点还在。

**根因**：`TextureHandle` 绑定的是**创建它的那个 egui Context**。
宿主每次打开编辑器都会走 `Editor::spawn()` → `EguiWindow::open_parented(...)`，
即创建一个**全新的 egui Context**；而我们的 `UiState`（含纹理句柄）挂在
`Arc<RwLock<_>>` 里跨窗口存活 → 旧句柄在新 Context 里是未知 id，`painter.image()`
静默不画。LED 用 `circle_filled` 纯几何绘制，所以还在。

**修复**：`create_egui_editor` 的 `build` 回调**每次 spawn 都会执行**，
在那里把纹理缓存清空，下一帧自动重建（解码 + 上传）：

```rust
|ctx, state| {
    ctx.set_visuals(Visuals::light());
    state.texture = None;
    state.sw_up = None;
    state.sw_down = None;
}
```

**教训**：GUI 里任何"跨帧缓存"的资源，只要它绑定的是上下文（纹理、字体、着色器），
就必须在 `build`（每次新建上下文）时失效重建。纯数据缓存（比如解码后的像素）才可以长期持有。

---

## E23：架构调整 第 1 步 —— 身份抽离（2026-10-23）

### 23.1 已确认的方向（ADR-011）

| 决策 | 结论 |
|---|---|
| 框架是否无状态 | ✅ 是：框架里不含任何插件专属字符串/坐标/素材/参数 |
| 分发模式 | **C 混合**：目录同构 + 脚手架生成 + 依赖指向框架 tag/rev |
| 电路描述规范 | **A 声明式宏**；**拓扑暂由人给出**，逐步自动化 |
| 许可证 | **保持 GPL-3**（只做开源） |
| 仓库形态 | **单 repo**：`plugins/sparklin-drive` 作为示例插件子项目 |

目标结构（9 个模块 → crate）：

```
rustomp-core      ① DSP 算法（牛顿 / 伴随模型 / 求解器框架）
rustomp-elements  ② 元件库
rustomp-circuit   ③ 电路描述规范（声明式宏 + 拓扑校验）
rustomp-platform  ④ 插件格式适配（薄 trait，nih-plug 为首个实现）
rustomp-gui       ⑤ 通用界面层（贴图 + 控件坐标表 + 交互 + 资源管线）
rustomp-import    ⑥ 外部电路导入（trait + breadspice 实现）
rustomp-oracle    ⑦ ngspice 对拍（Rust crate + 测试宏）
scripts/ tools/   ⑧⑨ 参数化构建脚本、自检工具、第三方补丁
template/         脚手架
plugins/sparklin-drive/   示例插件（= 下游项目的样板）
```

### 23.2 第 1 步：身份抽离（已完成）

新增 **`plugins/sparklin-drive/rustomp.toml`** —— 插件身份、素材路径、界面控件坐标的唯一来源：
`[plugin]` 名字/厂商/CLAP id/VST3 id、`[au]` 四字符码与 bundle id、
`[build]` 变体列表、`[assets]` 素材、`[ui]` 旋钮/拨杆/LED/按钮的坐标与参数。

新增工具与脚本：

| 文件 | 作用 |
|---|---|
| `tools/meta/plugin_meta.py` | 读 `rustomp.toml`，输出 `--shell` / `--json` / `--get` / `--rust`（只用标准库 `tomllib`） |
| `scripts/plugin-env.sh` | `source` 后把身份/素材/界面配置载入为环境变量 |
| `scripts/gen-bundler-toml.sh` | 由 `rustomp.toml` 生成 nih-plug 的 `bundler.toml`（该文件改为生成物，已加入 `.gitignore`） |

`build-variant.sh` / `build-au.sh` / `check-au.sh` / `reset-logic-cache.sh` 全部改为**从 TOML 读取身份**，
不再硬编码插件名与 AU 码。验证（未构建，仅解析）：

```
mono    → Sparklin Drive Mono    SPm1 / HpEf / ...sparklindrive.mono
stereo  → Sparklin Drive Stereo  SPs1 / HpEf / ...sparklindrive.stereo
dual    → Sparklin Drive         SPd1 / HpEf / ...sparklindrive
厂商显示名: HONEYPIE EFFECTS      变体列表: mono stereo (dual=0)
```

本轮**没有重新构建**（Rust 侧尚未改动，已安装的插件不受影响），把 I/O 压到最低。

### 23.3 后续步骤

| 步 | 内容 | 状态 |
|---|---|---|
| 1 | 抽离身份 → `rustomp.toml` | ✅ |
| 2 | 抽离电路：`fakedist.rs` 从 `rustomp-dsp` 移到插件；Rust 侧身份改由 `build.rs` 从 TOML 生成 | ⬜ 下一步 |
| 3 | 拆 GUI：通用绘制/交互/纹理生命周期 → `rustomp-gui`；坐标表与素材留在插件 | ⬜ |
| 4 | 抽 oracle → `rustomp-oracle` crate + 对拍测试宏 | ⬜ |
| 5 | 抽导入 → `rustomp-import` trait + breadspice 实现 | ⬜ |
| 6 | 建 `template/` 脚手架，验证"一键新建空插件" | ⬜ |

---

## E24：架构调整 第 2 步 —— 框架与电路分离（2026-10-23）

### 24.1 结果：框架里已经看不到任何插件痕迹

```
$ grep -rn "Fakedist\|sparklin" crates/rustomp-dsp/src/
（无输出）
```

框架现在只有：`Circuit` trait、`Engine<C>`、`Diode1n4148`、`Oversampler4x`。

### 24.2 引入电路契约 `Circuit`

```rust
pub trait Circuit {
    type Params: Copy + Default;
    fn new(sample_rate: f64) -> Self;
    fn set_sample_rate(&mut self, sample_rate: f64);
    fn set_params(&mut self, params: Self::Params);
    fn reset(&mut self);
    fn process(&mut self, x: f64) -> f64;
    fn activity(&self) -> f32 { 0.0 }   // 削波/活动指示（0..1）
}
```

`Engine<C: Circuit>` 负责过采样、true-bypass（干路延迟对齐 + 5 ms 淡化）、
活动指示包络、NaN/失控熔断 —— **对电路一无所知**。
`Fakedist` 在插件侧实现 `Circuit`，`activity()` 里做 `(|u|−0.55)/0.8` 的阈值映射。

### 24.3 目录重组

```
plugins/sparklin-drive/
  rustomp.toml            身份 / 素材 / 界面坐标（唯一来源）
  circuit/                crate: sparklin-drive-circuit
    src/fakedist.rs       电路实现（原在框架里）
    netlist/              .bread、netlist.cir、CIRCUIT.md、baseline.txt
    tests/{baseline,engine}.rs   对拍 + 引擎级测试
    examples/             profile / os_probe / transient / stress / bench / mt_bench ...
  plugin/                 crate: sparklin-drive（cdylib）
    src/lib.rs            nih-plug 外壳 + egui 界面
    build.rs              从 rustomp.toml 生成 identity.rs
  assets/                 panel@2x.png、switch_*@4x.png、source/
```

框架侧保留 `crates/rustomp-dsp`（含 `examples/diode_bench.rs` 这类与电路无关的基准）。

### 24.4 身份只有一个来源

`plugin/build.rs` 调 `tools/meta/plugin_meta.py --rust --variant <mono|stereo|dual>`
生成 `OUT_DIR/identity.rs`：显示名（带变体后缀）、CLAP id（带 `.mono/.stereo`）、
VST3 class id（三套）、窗口与画布尺寸。插件 `lib.rs` 里不再有任何硬编码身份。

VST3 三个 class id 必须各 16 字节且互不相同，写在 TOML 里：

```toml
vst3_class_id = "SparklinDriveV01"
vst3_class_id_mono = "SparklinDriveM01"
vst3_class_id_stereo = "SparklinDriveS01"
```

### 24.5 踩坑

1. **`include_bytes!` 是相对源文件**：从 `plugin/src/lib.rs` 引用
   `plugins/sparklin-drive/assets/` 要写 `../../assets/`（不是 `../assets/`）。
2. **`source` 脚本的位置参数陷阱**：`plugin-env.sh` 里用 `$1` 会读到**调用者**的参数
   （`build-variant.sh mono` 时 `$1` = "mono" → 去找 `mono/rustomp.toml`）。
   改用环境变量 `PLUGIN_DIR`。
3. `build.rs` 里相对路径要按 crate 根算：`plugin/` → 仓库根是 `../../../`。
4. `[expr; N]` 数组初始化要求 `Copy`；`Engine` 不实现 `Copy` → 用 `std::array::from_fn`。
5. TOML 里的 VST3 id 用 `*b"..."` 而不是 `*"...".as_bytes()`（后者长度不是常量）。

### 24.6 验证

```
框架 + 电路测试：  6 + 4 + 4 项全绿（含 baseline 对拍、引擎瞬态回归、多宿主采样率）
AU 自检：          aufx / SPm1 / HpEf，4 参数（Gain / Clip Mode SYM / Level / Bypass），
                   CocoaUI ✓，音频通路 ✓
已安装：           Sparklin Drive Mono / Stereo
```

---

## E25：架构调整 第 3 步 —— 通用界面层 `rustomp-gui`（2026-10-23）

### 25.1 新增框架 crate `crates/rustomp-gui`

**只提供"怎么做"**：贴图面板、声明式控件、交互、纹理生命周期。**不含任何素材、坐标、参数名**。

| 类型 | 作用 |
|---|---|
| `PanelSpec` | 面板声明（画布尺寸、窗口尺寸、背景色、旋钮/LED/拨杆/按钮） |
| `KnobSpec` / `LedSpec` / `ToggleSpec` / `ButtonSpec` | 单个控件的几何与行为 |
| `Assets` | 插件提供的素材（面板 PNG + 具名贴图），框架不持有素材 |
| `ParamAccess` trait | 框架读写参数的唯一入口（布尔用 0/1），**因此不知道参数类型** |
| `UiState` + `reset_textures()` | 纹理缓存与其生命周期（必须每次新建 Context 时清空） |
| `draw()` | 绘制 + 命中测试 + 拖动/滚轮/双击/按住缩放 |

关键设计：**界面声明由 `build.rs` 从 `rustomp.toml` 生成 Rust 字面量**
（`plugin_meta.py --panel-rust` → `OUT_DIR/panel.rs`），
所以运行时不解析 TOML、不引入额外依赖，且坐标只有一份来源。

### 25.2 插件侧瘦身

`plugins/sparklin-drive/plugin/src/lib.rs` 从 **559 行降到 296 行**，
只剩：参数定义、`GuiAccess`（把 `ParamAccess` 接到 nih-plug 参数）、
`process`、`editor` 的十行胶水、导出宏。

界面相关代码全部搬到框架（394 行）。

验证框架无状态：

```
$ grep -rn "sparklin\|Sparklin\|Fakedist" crates/rustomp-gui/src/
（无输出）
```

### 25.3 踩坑

1. `&PANEL`（const）传给 `&'static PanelSpec` 参数能成立，靠的是 **rvalue static promotion**；
   一旦 `PanelSpec` 里出现 `String`/`Vec` 之类就不是 `'static` 了，必须保持全 `&'static` 字段。
2. 拨杆点击区曾写成 `(y_down - y_up).max(0.0)`；两态顺序颠倒时会得到 0 高度 → 改成 `.abs()`。
3. 滚轮灵敏度原本写死 `1000.0 * scale`，通用化后不能再用"旋钮自身的 drag_range"推导
   （会得到 3.846 这种魔法数），直接固定 1000。

### 25.4 验证

```
AU：aufx / SPm1 / HpEf，4 参数，CocoaUI ✓，音频通路 ✓
已安装：Sparklin Drive Mono / Stereo
本轮构建：2 次（check + 正式构建），无重复基准/渲染
```

---

## E26：架构调整 第 4 步 —— ngspice 对拍接口 `rustomp-oracle`（2026-10-23）

### 26.1 新 crate `crates/rustomp-oracle`

| API | 作用 |
|---|---|
| `Oracle::discover()` | 找 ngspice：`NGSPICE` 环境变量 → `PATH`；找不到返回 `None`（测试应**跳过**而非失败） |
| `Oracle::run_netlist(&str)` | 在临时目录写 `.spiceinit`（`set filetype=ascii`）+ 网表，跑 `ngspice -b -r`，读回 rawfile |
| `RawFile::parse` | ASCII rawfile 解析（含"只有每点第一行带序号"这个格式细节） |
| `RawFile::nominal_dt` / `resample` | **变步长重采样**（见 26.3） |
| `harmonics` / `harmonics_percent` / `thd_percent` / `gain_db` | 统一的测量函数 |
| `last_integer_periods` | 取稳态的整数个周期，避免频谱泄漏 |
| `bin/spice-raw` | 命令行查看 rawfile（替代原 `tools/oracle/spice_raw.py`，已删除） |

**关键设计**：这些测量函数对 **ngspice 输出** 和 **DSP 输出** 用的是**同一份代码** ——
不会出现"两把尺子"（这是早期最大的隐患）。

### 26.2 插件侧：活体对拍测试

`plugins/sparklin-drive/circuit/tests/oracle.rs`：直接让 ngspice 跑本插件的网表，
与 WDF 实现用同一套测量函数比较。没装 ngspice 时自动跳过。

实测（0.2 V 峰值、1 kHz、电位器 50%）：

| 谐波 | WDF % | ngspice % | 差 (pp) |
|---|---|---|---|
| h2 | 3.914 | 3.921 | −0.007 |
| h3 | 22.487 | 22.457 | +0.030 |
| h4 | 2.427 | 2.430 | −0.003 |
| h5 | 11.981 | 11.939 | +0.042 |
| h6 | 1.835 | 1.833 | +0.001 |
| h7 | 7.699 | 7.649 | +0.050 |
| h8 | 1.483 | 1.477 | +0.006 |
| h9 | 5.400 | 5.344 | +0.056 |

基波 0.4791 vs 0.4794 V；THD 27.648 % vs 27.581 %。**最大谐波偏差 0.056 pp。**

与 `baseline.rs` 的分工：那边是"抄死的期望值"（快、无需 ngspice，适合日常）；
这边是"每次重跑 ngspice"（慢 ~6 s，能抓出网表/电路改动带来的偏差）。

### 26.3 关键坑：ngspice 的 `.tran` 是**变步长**

第一版对拍差得离谱（h5：WDF 11.98 vs ngspice 4.37）。
根因：`.tran 5u` 只是**步长上限**，削波处会自动缩小步长。按"样本个数"切整数周期
是错的，DFT 直接失真。

修复：`nominal_dt()`（相邻间隔的中位数）+ `resample(dt)`（线性插值到均匀网格），
再取整数周期。修完后所有谐波一次对上。

> 这条同样适用于任何用 ngspice 做音频对拍的场合 —— 必须显式重采样。

### 26.4 其它

`scripts/sim-baseline.sh` 改用 `target/release/spice-raw`（不再依赖 Python 解析器），
`tools/oracle/spice_raw.py` 删除。

---

## E27：架构调整 第 5 步 —— 可插拔电路导入 `rustomp-import`（2026-10-23）

### 27.1 设计：先归一化到中间表示，再由 IR 派生产物

```
 .bread / EasyEDA / KiCad / SPICE ...
        │  Importer（可插拔，注册在 registry()）
        ▼
   ImportedCircuit   ← 中间表示：Element{名字, 类别, 节点(已排序), 值, 模型} + 接地网 + 提示
        ├── to_spice()     → ngspice 网表元件行（喂给 rustomp-oracle）
        ├── to_skeleton()  → Rust 电路骨架（写 Circuit 实现的起点）
        └── to_json()      → 供其它工具消费
```

新增格式只需实现 [`Importer`]（`id` / `matches` / `import`）并在 `registry()` 里加一行。

### 27.2 首个实现：breadspice `.bread`

按面包板几何规则把 node id 归并成 net（`t{row}{a..e}`→`t{row}L`、
`t{row}{f..j}`→`t{row}R`、`rail_XX_{i}`→`rail_XX`），再翻译成元件：

| breadspice | 中间表示 |
|---|---|
| resistor/capacitor/inductor | `R/C/L`，值 = `数值+单位`（`4.7`+`kΩ` → `4.7k`） |
| potentiometer | **展开成两段串联电阻** `R{ref}A`/`R{ref}B`，按 `params.percent` 分配 |
| diode / led | `D`，按引脚的 `a`/`k` 定方向；模型名 = 器件值（LED 红/绿/蓝→`LedRed/…`） |
| opamp | `X`，引脚按 `IN+ IN- V+ V- OUT` 排序；`OP07`→`OP07A` |
| npn/pnp、jfet、nmos/pmos | `Q`（C/B/E）、`J`（D/G/S）、`M`（D/G/S） |
| power / vsine | `V`（含 `SIN(...)` 参数） |
| wire / jumper | 0.001 Ω |
| gnd | 把所在 net 记入 `ground_nets`，输出时映射为节点 `0` |
| oscilloscope 等 | 跳过并写进 `notes` |

`notes` 会把"电位器按 50% 展开""跳过了示波器"这类**需要人确认**的事写进网表注释里。

### 27.3 验证：与旧管线**逐行一致**

用新导入器重新生成的元件行，与之前由 breadspice 后端（`nlgen`）生成的网表
**完全相同**（22 个顶层元件，节点名/值/顺序一一对应）；差异只剩 `.SUBCKT` 内部行，
而那些属于模型库，现在单独放在 `circuit/netlist/models.lib` 里。

于是**删掉了两处旧依赖**：

- `tools/netlist/nlgen`（Rust 小工具，依赖 breadspice 的 git 仓库）
- `tools/netlist/bread_to_circuit.py`（Python 转换器）

**框架现在不再依赖任何外部项目的 git 仓库。** `scripts/make-netlist.sh` 改为：
`rustomp-import --spice _elements.cir` → `cat _elements.cir models.lib` → `netlist.cir`。

活体对拍重跑通过（h2–h9 与之前完全一致，最大偏差 0.056 pp）。

### 27.4 测试

`rustomp-import` 单测 + 集成测试 3 项：SPICE 值换算、节点名净化、**导入真实
`fakedist.bread`**（22 元件、接地网 `rail_Lm`、`XU1 … OP07A`、`RP1A … 250k`、
`DD1 n_t1L n_t10L 1N4148`）。

---

## E28：架构调整 第 6 步 —— 脚手架 `template/` + `new-plugin.sh`（2026-10-23）

### 28.1 生成物

```bash
./scripts/new-plugin.sh "My Drive" [--vendor "厂商名"]
# → plugins/my-drive/（与 plugins/sparklin-drive/ 结构同构）
```

生成的目录：

```
plugins/my-drive/
  rustomp.toml            身份/素材/界面坐标（已填好名字与派生码）
  circuit/                crate my-drive-circuit
    src/gain.rs           纯增益示例电路（实现 Circuit）
    tests/engine.rs       引擎级冒烟测试
    netlist/              空目录，等你的网表
  plugin/                 crate my-drive
    src/lib.rs            插件外壳（参数 + GuiAccess + process + editor）
    build.rs              从 rustomp.toml 生成 identity.rs / panel.rs
  assets/panel.svg + @2x.png   占位面板（矢量 + 光栅化）
```

脚本会自动：派生包名/AU 四字符码/VST3 class id/CLAP id、替换模板占位符、
把两个 crate 注册进 workspace，最后打印"还需人工做什么"。

### 28.2 验收（实际跑过）

```
./scripts/new-plugin.sh "Demo Drive"
cargo check -p demo-drive -p demo-drive-circuit      → 通过
cargo test  -p demo-drive-circuit --release          → 2 项通过
```

验收后删除了 demo 目录（模板本身即示例；不需要在仓库里留一个会持续腐化的副本）。

### 28.3 模板的"占位面板"

`template/assets/panel.svg` 是一个极简面板（米白底 + 边框 + 一个旋钮圆），
用 `inkscape --export-width=680` 光栅化成 `panel@2x.png`。
将来换皮时替换这两份即可，**代码不动**（坐标来自 `rustomp.toml` 的 `[ui]`）。

### 28.4 踩坑（又是同一个）

`new-plugin.sh` 里 `echo "==> 生成 $DEST（..."` → bash 3.2 把全角括号当成变量名的一部分，
报 `DEST: unbound variable`。**第 3 次踩同一个坑**（E24 已记录）。

已在脚本里统一处理：`$VAR` 后面紧跟非 ASCII 字符时一律写 `${VAR}`。
以后写脚本时直接遵守这条。

---

## E29：收尾 1/2/3 —— 归档整理、README 校对、CI（2026-10-23）

### 29.1 归档历史残留

| 原来 | 现在 | 说明 |
|---|---|---|
| `circuits/v1-fuzz/` + `tools/netlist/easyeda_asc.py` | `experiments/v1-easyeda-pcb/` | 早期从 EasyEDA **PCB 导出**反推电路的脚本与结果，已被 `plugins/sparklin-drive` 取代 |
| `experiments/e1_wdf_diode_clipper/` | 不动 | 最早的可行性 spike |
| —— | 新增 `experiments/README.md` | 说明这些目录**不参与构建**，只是历史留档 |

`circuits/` 与 `tools/netlist/` 目录随之消失（后者的功能已被 `rustomp-import` 接管）。

### 29.2 README 校对

改掉了所有失效路径（用脚本扫描 markdown 里的反引号路径逐个 `test -e`）：

- 目录结构块：去掉 `circuits/`，补 `template/`、`experiments/`、`.github/workflows/`、`tools/meta/`
- 「电路建模的工作流」整节重写：路径改成插件项目内的实际位置
  （`plugins/sparklin-drive/circuit/netlist`），并补上"先对一遍导入结果再写 DSP"的习惯建议
- 质量表补一行**活体对拍**（≤0.056 pp），并说明两套测试的分工
- 新增「持续集成」一节

### 29.3 CI

`.github/workflows/ci.yml`：

- **test**（macOS runner）：`cargo test --workspace --release`，
  先 `brew install ngspice`（`continue-on-error`），装上了活体对拍就生效、装不上自动跳过；
  另打印 ngspice 版本便于排查。
- **fmt**：`cargo fmt --all --check`，但标记为 `continue-on-error` ——
  仓库还没统一跑过一次 rustfmt（当前 59 个文件有格式差异），
  先作提示，等一次性 `cargo fmt` 后再改成阻塞。

本地先跑过一遍 `cargo test --workspace --release`：**17 个测试目标全绿**，
其中含 `tests/oracle.rs`（活体对拍）。

### 29.4 其它

`plugin_meta.py` 在缺少 `tomllib`（Python <3.11）时给出明确错误提示，
避免 CI 上只看到一个 ImportError。

---

## E30：Sparklin Drive 拆分为独立仓库（2026-10-23）

鉴于框架接下来会转向「闭集器件模型 + 通用 MNA 装配器」，先把现有插件**冻结**出去：

### 30.1 新仓库 `sparklin-drive-legacy`

位置：`~/develop/sparklin-drive-legacy`（**独立 git 仓库**，首提交 `af2eaee`，1.2 MB 源码）。

内容 = rustomp 在 `c960b47` 时的**完整快照**（用 `git archive HEAD` 导出，只含跟踪文件）：

| 内容 | 说明 |
|---|---|
| `plugins/sparklin-drive/` | 插件项目本体（电路 / 外壳 / 素材 / 网表 / 对拍测试） |
| `crates/rustomp-{dsp,gui,oracle,import}/` | 它依赖的框架 crate 快照 |
| `cmake/ patches/ scripts/ tools/` | 构建链、第三方补丁、自检工具 |
| `README.md` | **新写**：冻结说明、构建命令、冻结时的指标、目录说明 |
| `DEVLOG.md` | 保留，并在开头标注"这是主仓库 DEVLOG 的快照，此后独立维护" |

移除了与本仓库无关的 `template/`、`experiments/`、`.github/`。
已验证 `cargo check --workspace` 通过 —— **可以脱离 rustomp 独立构建**。

### 30.2 主仓库变为"纯框架"

| 变化 | 说明 |
|---|---|
| `plugins/sparklin-drive/` | 已删除（`git rm`） |
| workspace members | 只剩 `crates/rustomp-{dsp,gui,oracle,import}` |
| `/plugins/` | 加入 `.gitignore` —— 现在是**下游项目的输出目录**（`new-plugin.sh` 生成） |
| `scripts/plugin-env.sh` | 没有默认插件了：未指定 `PLUGIN_DIR` 时，若 `plugins/` 下恰好一个项目则自动使用，否则报错并给出提示 |
| `make-netlist.sh` / `sim-baseline.sh` | 目录参数改为**必填** |
| `rustomp-import` 测试 | 原来读插件的 `.bread`；改为自带 fixture `tests/data/fakedist.bread` |
| README | 示例插件指向 `sparklin-drive-legacy`；工作流示例改用 `plugins/my-drive` |

验证：`cargo test --workspace --release` 全绿（框架自身 6+3+1+6 项 + 文档测试）。

### 30.3 两个仓库的分工

| 仓库 | 定位 |
|---|---|
| **rustomp** | 框架本体：Circuit 契约、Engine、GUI 层、导入、对拍、脚手架。往下会引入 MNA 装配器与闭集器件库 |
| **sparklin-drive-legacy** | 一个**已完成的插件**，冻结在 1-D WDF 求解器上，独立维护 |

---

## E31：元件库精度规范落盘（2026-10-23）

讨论后确立三条原则性结论，写进 **`docs/component-library.md`**（设计决策文档，后续实现以此为准）：

1. **元件精度不影响延迟** —— 延迟固定 15 样本（由过采样 FIR 决定），加参数只影响 CPU 与数值刚度。
2. **成本结构分两类**：非线性静态项（exp/除法，随「器件数 × 迭代次数」涨）与线性动态项（结电容，只加矩阵项、**不增加未知量**，+5~10%）。
   → 更正了早前"BJT 结电容 +30~40%"的错误估计。
3. **静态给足（D3）、动态按需（多数 C0，大结电容/高阻节点才 C1）**。

精度改用**双轴**定义（原 L1/L2/L3 把静态与动态混在一起）：

| 静态 | 含义 | 动态 | 含义 |
|---|---|---|---|
| D1 | `Is, N` | C0 | 无电容 |
| D2 | + `Rs` | C1 | + 结电容 `Cj` |
| D3 | + 高注入/漏电/Early | C2 | + 扩散/恢复 `Tf, Tr` |

（换算：L1≈D2+C0、L2≈D3+C0、L3≈D3+C2）

**逐器件推荐**（详见文档表格）：

| 器件 | 档位 | 关键理由 |
|---|---|---|
| 锗 BJT | **D3+C0** | 漏电决定整级工作点（Fuzz 偏置就靠它） |
| 锗二极管 | **D2+C0** | 0.3V 低拐点 + 大 Rs = 圆头削波 |
| 1N4007 | **D2+C1** | 大 `Rs` 软化拐点；`Cjo` 比 1N4148 大 5~7 倍 |
| 2N7000 | **D2+C1** | 踏板里只做削波/开关，跨导与体二极管是主角 |
| BC549C / J201 | D3+C0 | 现有参数已够 |
| OP07 | 宏模型（1 内部节点） | **压摆率是性格核心**，需补 |
| 1N4148 | D2+C0 | 已有活体证据：省 `Cjo/tt` 只差 0.056 pp |

文档还写了：**两个验收实验的模板与容差**（DC 扫描 <2%；1 kHz 谐波 <0.1 pp、10 kHz <0.3 pp）、
**CPU 按非线性节点数的预算表**、与「闭集器件模型 + MNA 装配器」路线的衔接（`Device` trait 骨架）、
以及四个待定项（温度、锗参数来源、R/C 非理想性、运放输出级）。

下一步：等用户提供 LTspice 风格模型 → 按本文档的字段映射导入。

---

## E32：电路导入与元件映射规范落盘（2026-10-23）

结论写进 **`docs/circuit-import.md`**。

### 32.1 可插拔性

`rustomp-import` 的 `Importer` trait + `registry()` 已具备插件化；新增格式只需实现结构体并注册。
导入器**只负责**产出中间表示（元件类别 + 节点 + 值 + 来源型号），
不求解电路、不生成 DSP、不擅自决定映射结果。

### 32.2 格式优先级：**先网表，后原理图**

| 格式 | 工作量 | 决策 |
|---|---|---|
| `SpiceNetlist`（`.net/.cir`，LTspice 的 View → SPICE Netlist、KiCad、ngspice） | 小 | **优先实现** |
| `LtspiceAsc`（`.asc`，原理图） | 大（坐标推连通性 + 旋转/镜像 + `.lib`） | 后续 |
| `BreadSpice`（`.bread`） | — | 已完成 |

### 32.3 元件映射：分层流水线（用户已确认）

| 级 | 手段 | 标记 | 定位 |
|---|---|---|---|
| 1 | 精确命中 | `Exact` | — |
| 2 | **归一化 + 别名**（`MMBT4148`→`1N4148`） | `Alias` | **常规** |
| 3 | **项目 `mapping.toml`**（`2N5088`→`BC549C`） | `Project` | **常规** |
| 4 | **类别默认** | `ClassDefault` | **防呆兜底**（必须写进 notes） |
| 5 | 参数直通（用外部 `.model` 构造器件） | `Params` | **power user 开发接口，默认关闭，不进发布路径** |
| 6 | 未解析 | `Unresolved` | 按 `[strict]`：error / warn / 用默认 |

原则：**替换必须可见、可测** —— `report()` 打印映射表，oracle 基线必须与映射配置同源。

### 32.4 引脚绑定：按**引脚名**，不按位置

沿用现有规则：`1/2`（无源）、`a/k`（二极管）、`C/B/E`（BJT）、`D/G/S`（JFET/MOS）、
`IN+/IN-/V+/V-/OUT`（运放）、`1/2/3`（电位器）。外部命名不一致时用别名表补，**禁止按位置猜**。

规范还给出：`DeviceClass` 枚举、IR 扩展（`ImportedElement` + `Resolved`）、
`mapping.toml` 的完整字段（aliases / defaults / strict）、以及三项待定。

---

## E33：声明式构建规范落盘（2026-10-23）

用户确认：生成物放 **`src/generated/`**（可读可 diff）、参数格式化只支持 `percent/db/plain/enum`，
并把 **Windows / Linux / standalone** 排进路线、**按平台可插拔**。
结论写进 **`docs/declarative-build.md`**。

### 33.1 核心设计：两个平面

| 平面 | 在哪跑 | 产物 |
|---|---|---|
| **代码生成**（`rustomp gen`） | 任意主机，确定性 | `<project>/plugin/src/generated/*.rs` |
| **打包**（`rustomp pack`） | **各平台本地**（需要本平台工具链/签名） | `.au/.vst3/.clap/.exe/安装包` |

**为什么不交叉编译**：VST3 SDK 要 MSVC、GUI 要 X11/Wayland 头文件、签名工具各平台不同。
正确做法是 **CI 矩阵**（macos-14 / windows-2022 / ubuntu-22.04）原生构建。

### 33.2 打包后端契约（可插拔的落点）

```
scripts/pack/<platform>/<format>.sh
```

输入靠环境变量（`PLUGIN_DIR` / `VARIANT` / `TARGET_TRIPLE` / `PROFILE` / `OUT_DIR` / `SIGN_MODE`），
约定：**幂等**、失败非零退出、成功时打印 `ARTIFACT: <路径>`、身份只能从 `rustomp.toml` 与
`generated/identity.rs` 读（不得硬编码）。每个后端附**能力声明**（platforms / formats / requires），
驱动在打包前校验，不支持时明确报错而非静默跳过。

### 33.3 平台矩阵（目标状态）

| 平台 | au | vst3 | clap | standalone | 安装包 |
|---|---|---|---|---|---|
| macOS | ✅ | ✅ | ✅ | ⬜ S4 | ⬜ M4 |
| Windows | ❌（无 AU） | ✅ | ✅ | ⬜ S4 | ⬜ M4 |
| Linux | ❌（无 AU） | ✅ | ✅ | ⬜ S4 | ⬜ M4 |
| au3 | ⬜（需完整 Xcode） | | | | |
| **aax** | ❌ **GPL-3 与 Avid 协议冲突**（ADR-005 / R10） | | | | |

### 33.4 代码生成侧

生成到 `<project>/plugin/src/generated/`：`identity.rs`、`params.rs`（含 `GuiAccess`）、
`panel.rs`、`plugin_impl.rs`、`platform/{macos,windows,linux}.rs`（可空片段）。
头部标 `@generated`、内容确定性、`rustomp gen --check` 防漂移，手改一律进 `local.rs`。

### 33.5 分阶段

| 阶段 | 内容 | 验收 |
|---|---|---|
| S1 | `rustomp gen`（参数表 → 样板代码；身份/面板迁到 `src/generated/`） | 与 Sparklin Drive legacy 手写版对照：测试与 `check-au` 结果一致 |
| S2 | `rustomp pack` + macOS 三个后端（收敛现有脚本） | 本机一条命令产出 AU/VST3/CLAP |
| S3 | Windows / Linux 后端 + CI 矩阵 | 三平台都能产出 `ARTIFACT:` |
| S4 | standalone（三平台） | 能出声 |
| S5 | 安装包与签名 | M4 |

---

## E34：导入分立器件模型（models.add）（2026-10-23）

用户提供 `models.add`（LTspice 风格）。写了 **`tools/meta/ltspice_models.py`** 清洗并验证。

### 34.1 清洗策略

按**每种器件类型的合法参数白名单**过滤（白名单取自 ngspice 手册）。
LTspice/厂商专用字段（`MFG=` / `Type=` / `Vceo=` / `Icrating=` / `mtriode=`）会被丢掉，
否则 ngspice 直接报错。

产物（`devices/`）：

| 文件 | 用途 |
|---|---|
| `models.add` | 原始输入（冻结） |
| `devices.lib` | 清洗后的 SPICE 库（ngspice / oracle 用） |
| `devices.json` | 归一化参数（供以后生成 Rust 常量） |
| `REPORT.md` | 人读报告：保留/丢弃参数、类别、建议档位、ngspice 可用性 |

### 34.2 结果（5 个分立器件）

| 型号 | 类别 | 丢弃 | ngspice 实测 |
|---|---|---|---|
| `AC127` | GeNpn | `vceo icrating mfg` | Ib=10 µA, Vce=5 V → **Vbe=0.142 V, Ic=1.30 mA (hFE≈130)** ✅ |
| `AC128` | GePnp | 同上 | — ✅ |
| `1N34A` | GeDiode | `type` | **Vf@1mA=0.253 V**，Vf@10µA=0.065 V ✅ |
| `1N4007` | SiDiode | `mfg type` | Vf@1mA=0.555 V ⚠️（见下） |
| `2N7000` | NMos | 全部（VDMOS） | ❌ ngspice 不支持 |

锗管实测非常符合物理直觉：`Vbe=0.142 V`、`hFE≈130`、二极管 `Vf@1mA=0.25 V` ——
说明清洗后模型是**有效且可用**的。

### 34.3 两个硬问题

**① `2N7000` 的 `VDMOS(...)` 是 LTspice 专有格式，ngspice 完全不认**

```
warning, can't find model '2n7000'
could not find a valid modelname
```

试了"近似转 MOS level 1"：`Vto=1.6, Kp=.17, Rs=.75` → `Id@Vgs=5V,Vds=5V = 0.70 A`，
而真实 2N7000 在 `Vgs=5V` 只有 0.1~0.2 A，**差 4 倍以上**。
（LTspice 的 VDMOS 自带 triode 限制，`Kp` 的定义也与 level-1 不同。）

→ 需要 ngspice 兼容的 2N7000 模型，或授权我们手写等价 subckt（体二极管 + Rds(on) 限制）。

**② `1N4007` 的 `CJO=10p / TT=100n` 像是通用默认值**

数据手册典型值是 `Cj≈15 pF`、`trr≈30 µs`。对削波影响有限（仍是 D2+C1），
但若要做高频细节，建议换实测模型。

### 34.4 运放：`models.add` 里只有符号

`RC4558` / `TL072` 两段是 **LTspice 符号定义**（`SYMBOLTYPE`/`LINE`/`PIN`），
真实模型指向外部 `.301` 文件（`..\sym\EXTRA\AudioOpamp\TI\RC4558.301`）。
**没有模型本体，无法对拍**；且两者都是双运放，涉及"一个封装两个单元共用电源"的建模。

详见 `docs/component-library.md` §10。

---

## E35：器件库第一块——通用二极管 + 两个 SPICE 产物（2026-10-23）

用户确认：**2N7000 手写**、**1N4007 用实测模型**、**运放只保留一个通用器件（CPU 友好），不做双运放**。

### 35.1 SPICE 侧

新增 `devices/handwritten.lib`（手写/覆盖，与 `models.add` 分离）：

- **1N4007 实测版**：DC 沿用原模型（`Vf@1mA=0.555 V` 不变），动态改 `CJO=15p / TT=5µs`
  （原模型 `Cjo=10p / tt=100n` 是通用默认值）；
- **2N7000 手写子电路** `N2N7000`：体二极管 + level-1 NMOS（`Vto=1.6, Kp=0.035, λ=0.02`）
  + 串联 `Rd=2 / Rs=1`。ngspice 实测转移特性：

  | Vgs | 3 V | 4 V | 5 V | 6 V | 10 V |
  |---|---|---|---|---|---|
  | Id (Vds=5V) | 37.7 mA | 110 mA | **221 mA** | 367 mA | 837 mA（等效 Rds≈6Ω） |

  与数据手册一致（`Vgs=5V` 约 0.2 A、`Rds(on)` 几 Ω）。之前"VDMOS 直接转 level-1"会给出 0.70 A，**高估 3 倍**。

另新增 `devices/reference.lib`（从 breadspice 迁入 1N4148，供对拍自包含），
并把这些拼装固化到 `devices/build.sh`：`models.add → parsed.lib`（可用 `--skip` 跳过被覆盖项）
`+ handwritten.lib + reference.lib → devices.lib`。

### 35.2 Rust 侧

`crates/rustomp-dsp/src/devices/` 建立（元件库的落点）：

- `diode.rs`：通用 `Diode { is, n_vt, rs }`（静态 **D2**），带内层牛顿解 `Rs`、
  `series()`、`voltage_for_current()`（测试/拟合用）；
- 库内型号（闭集）：`D_1N4148` / `D_1N34A` / `D_1N4007`；
- `devices/mod.rs` 里列出进度表（BJT/JFET/MOS/运放待补）。

### 35.3 对拍结果：**完全一致**

`crates/rustomp-dsp/tests/devices.rs`（电流源注入 + `.op`，取节点电压）：

```
1N4148  I=1µA..10mA   ngspice vs 我们：差 ±0.00%
1N34A   I=1µA..10mA   差 ±0.00%
1N4007  I=1µA..10mA   差 ±0.00%
2N7000  手写子电路：转移特性单调，Id@Vgs=5V=0.221A ✓   ← 后被 E38 修正为 0.196 A（RS 悬空）
```

**踩坑（值得记）**：一开始所有点都差 **−0.05%**，原因是热电压取了 300 K（`VT=0.025852`），
而 SPICE 默认参考温度是 **27 °C**（`VT=0.025865`）。改成 27 °C 后误差归零。

### 35.4 下一步

| 顺序 | 内容 |
|---|---|
| 1 | `Bjt`（D3：`Is/Bf/Vaf/Ikf/Nk/Ise/Ne/Br/Isc/Nc` + `Rb/Re/Rc`）+ `BC549C`/`AC127`/`AC128`，与 ngspice 做 V-I 对拍 |
| 2 | `Jfet`（`Beta/Vto/Lambda`）+ `J201` |
| 3 | `Nmos`（DSP 侧，按已标定的 2N7000 参数） |
| 4 | 通用 `OpAmp`（1 内部节点 + 压摆率 + 摆幅）——实现随 MNA 装配器 |

---

## E36：器件库第二块——BJT（Gummel-Poon 静态 D3）（2026-10-23）

### 36.1 模型内容

`crates/rustomp-dsp/src/devices/bjt.rs`：`Bjt` = Gummel-Poon **静态**模型

- 传输电流 `Ict = Is/qb·(e_be − e_bc)`，基极区电荷因子
  `qb = q1/2·(1+√(1+4q2))`，`q1 = 1/(1 − vbc/Vaf − vbe/Var)`，`q2 = Is/Ikf·(e_be−1) + Is/Ikr·(e_bc−1)`
- 两个结电流 `Ibe = Is/Bf·(e_be−1) + Ise·(e_vbe/Ne/Vt − 1)`，`Ibc` 对称
- **体电阻 `Rb/Re/Rc`**：端电压与本征结电压之间做一次 2 维牛顿
- 库内型号：`BC549C`（硅）/ `AC127`（锗 NPN）/ `AC128`（锗 PNP）

### 36.2 对拍结果（Vce=5V，Vbe 扫描，Ic 与 Ib 都测）

| 器件 | Vbe | Ic 误差 | Ib 误差 |
|---|---|---|---|
| BC549C | 0.55 / 0.60 / 0.65 / 0.70 | −0.00% / +0.01% / +0.09% / **+0.67%** | ≤0.01% |
| AC127 | 0.05 / 0.10 / 0.15 / 0.20 | **−0.00%** | −0.00% |
| AC128 (PNP) | 同上 | **−0.00%** | −0.00% |

### 36.3 三个踩坑（都很值钱）

**① 数值雅可比写错 → AC127 在高电流点差 8%**
第一版 `terminal()` 里手推的解析雅可比有误。改成"**直接对残差做中心差分**"（几乎不可能写错的形式）后，
0.2 V 处的 8.17% 立刻变成 0.00%。
→ 教训：**嵌套内层牛顿的雅可比不要手推**，除非有对拍兜底。

**② PNP 的电压约定**
SPICE 内部用 `type = -1` 把电压取反。等价做法：把 `(veb, vcb)` 直接喂给 NPN 公式，返回电流取负。
一开始我传了 `(-veb, 0)`，结果 PNP 完全不导通（因为变成反偏）。

**③ 测 Ib 时把发射极电流当成了基极电流**
PNP 测试电路里基极直接接地 → `i(veb)` 测到的是 `Ie ≈ Ic`，于是"hFE≈1"这种荒谬结果。
给基极也加一个 0 V 源单独测 `Ib` 后才对上。

### 36.4 已知偏差

BC549C 在 `Ic≈4.6 mA` 处偏 +0.67%。可能原因是未建模的 `Nk=0.6339`
（`Nk` 在 SPICE3 的 GP 里语义模糊）。**决定不猜**：远低于 5% 门槛，先记录现状；
若将来某电路正好工作在大电流区再专项处理。

### 36.5 下一步

`Jfet`（`Beta/Vto/Lambda`）+ `J201` → 然后 `Nmos`（按已标定的 2N7000 参数）→ 运放（随 MNA）。

---

## E37：器件库第三块——JFET（Shichman-Hodges D3）（2026-10-23）

### 37.1 模型内容

`crates/rustomp-dsp/src/devices/jfet.rs`：

```
vov = Vgs − Vto
饱和区 (vov ≤ Vds): Id = Beta·vov²·(1 + Lambda·Vds)
三极管区 (vov > Vds): Id = Beta·Vds·(2·vov − Vds)·(1 + Lambda·Vds)
```
+ 体电阻 `Rd/Rs`：因为电流只有一条通路，压降是 `Id` 的一维反馈，所以**只需标量牛顿**
（比 BJT 的 2 维简单一个量级）。`vds < 0` 时按对称性交换漏源。
库内型号：`J201`（`Beta=1.621m, Vto=-0.6, Lambda=2.236m, Rd=Rs=1`）。

### 37.2 对拍结果：**完全一致（0.00%）**

| 测试 | 点 | 误差 |
|---|---|---|
| 转移特性（Vds=5V，Vgs −0.55…0） | 5 点 | **+0.00%** |
| 输出特性（Vgs=0，Vds 0.2/0.5/1/3） | 4 点 | **+0.00%** |

注意三极管区那两点（Vds=0.2、0.5 V）也精确一致 —— 说明 `Rd/Rs` 的一维牛顿解法是对的
（这两点若省掉体电阻会差 10~20%）。

### 37.3 器件库当前状态

| 器件 | 静态模型 | 对拍 |
|---|---|---|
| 二极管 `1N4148` / `1N34A` / `1N4007` | ✅ D2 | 0.00% |
| BJT `BC549C` / `AC127` / `AC128` | ✅ D3 | ≤0.09%（BC549C 大电流点 0.67%） |
| JFET `J201` | ✅ D3 | 0.00% |
| MOSFET `2N7000` | ⬜ DSP 侧待做（SPICE 侧手写子电路已标定） | — |
| 通用运放 | ⬜ 随 MNA 装配器 | — |

### 37.4 下一步

`Nmos`（DSP 侧，按已标定的 2N7000 参数）→ 运放 → **MNA 装配器**（那时器件模型才真正被"用起来"）。

---

## E38：器件库收官——MOSFET + 运放（2026-10-23）

器件层的"闭集清单"到此齐全：二极管、BJT、JFET、MOSFET、运放。

### 38.1 MOSFET（`devices/mosfet.rs`）

- level-1（Shichman-Hodges）：饱和 `Id = Kp/2·vov²·(1+λVds)`、三极管区 `Kp·(vov·Vds − Vds²/2)`
- `Rd/Rs`：与 JFET 相同的**一维标量牛顿**
- 体二极管单独作为 `Diode` 单口器件（`NMOS_2N7000_BODY`），便于 MNA 装配
- 常量 `NMOS_2N7000`：`Vto=1.6 Kp=0.035 λ=0.02 Rd=2 Rs=1`

对拍（与 `N2N7000` 子电路）：转移特性 5 点 + 三极管区 3 点 + 体二极管 3 点，**全部 ≤0.01%**。

### 38.2 对拍过程中发现 SPICE 侧的两个 bug（值得记）

**① 源极电阻是悬空的**
`MCH DI G S S` + `RS S SI`：沟道源接的是 `S`，`SI` 没人用 → `RS` 形同虚设。
改正为 `MCH DI G SI S` 后，`Vgs=5V` 的 Id 从 0.221 A 降到 0.196 A（−11%）。

**② MOS 内建结抢走了体二极管**
原先靠 level-1 MOS 自带的 bulk-drain 结当体二极管，但 ngspice-47 的 MOS1 结电流
实际上按 `N=1` 处理（`N=1.5` 被忽略），与显式二极管差 100~1000 倍。
改成 `IS=0` 关掉内建结 + 显式 `DBD`（`RS=Rd+Rs=3Ω`），两侧参数一一对应，误差立刻归零。

### 38.3 运放（`devices/opamp.rs`）

单极点宏模型：`A(s) = A0/(1+s/ωp)`，`ωp = 2π·GBW/A0`，外加压摆率与输出钳位。

**离散化踩了两个坑：**

| 方案 | 结果 |
|---|---|
| 显式欧拉 `dv = ωp(A0·vd − v)·dt` | **发散**（闭环时间常数 `1/(ωp·A0)` = 0.27 µs ≪ 采样周期 5.2 µs） |
| 梯形法（Tustin） | 稳定，但 54 kHz 处频率轴被扭曲 +36%，等效 -3dB 点只有 36 kHz（应 54.5 kHz） |
| **精确极点 `v[n] = a·v[n−1] + b·vd[n]`** ✅ | 4× 过采样下 -3dB 点 54 kHz，误差 <2% |

最终形式对 `vd[n]` 是线性的 → `v_out` 可写进 MNA 的代数环（无延迟），状态只剩一个数。
压摆率/输出钳位是非线性的，作为事后处理（饱和不频繁时足够）。

### 38.4 器件库状态（全部完成）

| 器件 | 静态 | 动态 | 对拍 |
|---|---|---|---|
| 二极管 ×3 | ✅ D2 | 1N4007 C1 | 0.00% |
| BJT ×3 | ✅ D3 | C0（待补 Cje/Cjc/Tf） | ≤0.09% |
| JFET `J201` | ✅ D3 | C0 | 0.00% |
| MOSFET `2N7000` | ✅ D3 | C1（Cgs/Cgd 待补） | ≤0.01% |
| 运放 `OP07` | ✅ 单极点 | C1 | -3dB 误差 <2%（4× OS） |

**下一步：`MnaSystem<N>` 装配器** —— 5 类器件模型都能接上了。

---

## E39：通用 MNA 装配器（2026-10-23）

器件库之后的第一块"骨架"：`crates/rustomp-dsp/src/mna.rs`（约 500 行）。

### 39.1 设计：器件只写"电流函数"

```rust
pub enum Element { Diode{..}, Bjt{..}, Jfet{..}, Mosfet{..}, Resistor{..}, Capacitor{..}, OpAmp{..} }

// 三端器件的全部"电路知识"就是这个闭包：
sys.multiport(&[c, b, e], v, |vv| vec![ic, ib, -(ic+ib)]);
```

装配器对**端电压做数值差分**得到雅可比（`n+1` 次模型调用），然后按
`I ≈ I₀ + J·(V−V₀)` 装成"导纳 + 等效电流源"。这个决定的价值在下面第 2 条坑里体现得非常清楚。

### 39.2 对拍结果（`tests/mna.rs`，5 个测试）

| 电路 | 与 ngspice 的偏差 |
|---|---|
| 二极管削波（6 个 DC 点） | **≤1.6 µV** |
| BJT 共射偏置（BC549C） | **≤123 µV** |
| JFET 共源（J201） | **34 µV** |
| RC + 削波瞬态（48 kHz，240 点） | **≤17 mV**（BE vs 梯形，符合预期） |
| 运放非反相 10× | DC 增益 **10.998**；饱和时正确钉在 7.8 V |

### 39.3 四个坑（按代价排序）

**① gmin 步进"只降不升"的循环写错 → 每个节点挂了 1 kΩ**
第一版：`gmin=1e-3` 收敛就 `break`，于是漏电导留在结果里，0.2 V 被分压成 0.1 V。
现象极具误导性（活像"两只反并联二极管各等于 1 kΩ"）。
改正：gmin 必须**一路降到 0**，中间各级只做热启动。

**② 瞬态对拍必须重采样**
ngspice `.tran` 是变步长（首个步长 1e-11 s），直接按索引比会得到 286 mV 的"误差"。
用 `RawFile::resample(dt)` 后立刻降到 17 mV。—— 这个坑 DEVLOG 里已经记过一次，又踩了一次。

**③ 运放的 DC 与瞬态不是同一个方程**
直流必须用**连续极限** `v_out = A0·vd`；用离散式 `a·v_out_prev + b·vd` 会解出
"第一拍的增益"（`b = 19.6` 而不是 `A0 = 5e5`），DC 增益算出 279。
另外运放饱和是**非线性**的：线性 VCVS 解不出饱和点，改成"解出越界就钉在限幅上重解"的分段线性迭代。
（测试电路里 R2 也必须接 VREF 而不是地，否则运放会试图把 4.5 V 偏置放大 11 倍。）

**④ 运放在 192 kHz 下 -3dB 偏低 26%**
"零延迟进代数环"的代价：`v_out` 是 `vd[n]` 的代数函数，近 Nyquist 存在 `b/(2+bk)` 的地板。
音频带内误差可接受，且该频段会被抽取滤波器滤掉 —— 记档，暂不优化。

### 39.4 下一步

用 MNA 路径重写 Sparklin Drive（或其中一个级）做**回归验证**，
再决定 S1 声明式代码生成器怎么生成 `Element` 列表。

---

## E40：晚间收尾 —— 文档与现状同步（2026-10-23）

今晚一共 5 个提交，把"元件库 + MNA 装配器"从规范变成了可运行、可对拍的代码：

| 提交 | 内容 | 对拍结果 |
|---|---|---|
| `5a46a86` | 二极管（D2）+ 手写 2N7000 + 实测 1N4007 | 0.00%（1 µA~10 mA） |
| `faca331` | BJT（Gummel-Poon D3，含 Rb/Re/Rc） | Ic/Ib ≤0.09% |
| `4e9ef08` | JFET（Shichman-Hodges D3） | 0.00% |
| `43e775c` | MOSFET（level-1）+ 运放（单极点宏模型） | ≤0.01% / -3 dB <2% |
| `f4ea745` | MNA 装配器（Element + Netlist + 牛顿） | DC µV 级、瞬态 17 mV |

本次只做**文档对齐**，不改代码：

1. **根 `README.md` 重构**：
   - 顶部加"当前状态"表（元件库 ✅ / MNA ✅ / 示例插件已拆出 / 下一步）；
   - "三层"改成按**代码位置**划分（器件库 → MNA → 框架机制 → SPICE 模型库 → 插件外壳），
     删掉已不存在的 `circuits/`；
   - 核心特性改成"闭集元件库 + 两套求解器共用器件模型 + 精度分档"；
   - "质量把关"拆成三层：**器件级**（V-I 逐点）/ **装配器级**（整机 DC 与瞬态）/ **整机**（历史 fakedist 数据）；
   - 限制与路线图按现状重写（动态档未补、BC549C 的 0.67%、运放 192 kHz 的 26%、下一步 MNA 重写）。
2. **`crates/rustomp-dsp/README.md` 重写**：原文还停在"尚未完成：元件库"，现按
   器件库 / MNA 装配器 / Engine / 历史对拍 四部分重写，把三条踩坑教训（VT=27 °C、
   PNP 电压约定、嵌套雅可比）与三条 MNA 坑写进去。
3. **`docs/component-library.md`** 修正过期项：
   - §8「运放输出级：只有单极点、需补压摆率」→ 已实现（压摆率 + 输出钳位 + 饱和钉住）；
   - §10 的 2N7000 标定值 `0.221 A` → `0.196 A`（E38 修正了悬空的 `RS`）。

> 经验：**文档里最危险的不是"没写"，而是"写了但已过期"**。
> `crates/rustomp-dsp/README.md` 那句"尚未完成：元件库"差一点就把今晚的成果藏起来了。

---

## E41：补齐日常效果器所需器件（肖特基 + 三色 LED + BC559C）（2026-10-23）

用户提醒：breadspice 的 `models.lib` 里还有 **1N5817、红/绿/蓝 LED、BC559C**。
这批加上之后，"日常做效果器要用到的分立器件"基本齐了。

### 41.1 新增器件

| 型号 | 类型 | 参数（清洗后） | 用途 |
|---|---|---|---|
| `1N5817` | 肖特基二极管 | `Is=31.7µ N=1.373 Rs=.051` | 电源反接保护、检波、硬削波 |
| `LedRed` | LED | `Is=93.2p N=3.73 Rs=42m` | 削波/波形整形（Vf≈1.85 V@20 mA） |
| `LedGreen` | LED | `Is=93.2p N=4.61 Rs=42m` | 单色 LED 削波（Vf≈2.29 V） |
| `LedBlue` | LED | `Is=93.2p N=7.47 Rs=42m` | 高拐点削波（Vf≈3.71 V） |
| `BC559C` | 硅 PNP（BC549C 互补） | `Is=1.02f Bf=401.6 Vaf=34.62 Rc=1.151` | 差分/缓冲、互补对称级 |

LED 之所以能在同一个 `Diode` 模型里表达，是因为它们的"高 Vf"完全由 `N`（3.73/4.61/7.47）
与极小的 `Is` 决定 —— **不需要单独的器件类型**。

### 41.2 对拍结果（全部一次通过，除一个上界 bug）

| 器件 | 覆盖 | 误差 |
|---|---|---|
| `1N5817` | 1 µA ~ 10 mA（5 点） | **0.00%** |
| `LedRed` / `LedGreen` / `LedBlue` | 1 µA ~ 10 mA（5 点 × 3） | **0.00%** |
| `BC559C` | Vbe 0.55~0.70，Ic 与 Ib 都测 | **±0.01%** |

### 41.3 一个"看起来像模型错、其实是工具错"的坑

绿 LED 在 `10 mA` 处差 **−9.31%**，我们的值恰好是 **2.00000 V** ——
一看就不是物理误差，而是 `voltage_for_current()` 的**二分上界写死成 `[-2, 2] V`**，
绿 LED 的 Vf（2.205 V）被截断在 2.0 V。

改成"上界从 0.1 V 起 ×1.5 自动撑开"后，蓝 LED（3.57 V）也一并正确。
→ 教训：**当误差恰好是个整数时，先怀疑测量工具，而不是模型。**

### 41.4 未建模的部分（按精度表）

- LED 的**反向击穿**（`BV=4/5`）与结电容 `Cjo≈3p`：踏板里 LED 基本只正向用（或作削波），
  属 C1 档，暂不做；
- `1N5817` 的 `Cjo=190p`：肖特基结电容不小，若用于高频检波需补 C1；
- `BC559C` 的 `Nk=.512`：与 BC549C 同样未建模（见 E36 的说明，不为拟合而猜语义）。

### 41.5 器件库现状（14 个 model/subckt）

二极管 4 种 + LED 3 种、BJT 4 种、JFET、MOSFET、运放 —— **日常踏板电路够用了**。
下一步仍是：用 MNA 重写 Sparklin Drive 做回归。

---

## E42：Sparklin Drive 的 MNA 重写与回归验证（2026-10-23）

同一条网表（legacy 的 `netlist.cir`）用 **通用 MNA 装配器**重新搭一遍，
与 ngspice 基线（完整 OP07A 子电路）和 legacy WDF 实现三方对照。

代码：`crates/rustomp-dsp/tests/sparklin_mna.rs`（10 节点 + 3 条电压源支路，`Netlist<14>`）。

### 42.1 结果：**整机指标几乎逐位重合**

| 项目 | MNA 实现 | ngspice 基线 | legacy WDF | 偏差 |
|---|---|---|---|---|
| DC OUT | 4.499991 V | 4.499935 V | 4.49999 V | **+0.056 mV** |
| DC IN− | 4.499991 V | 4.499724 V | — | +0.267 mV（OP07 输入偏置电流未建模） |
| DC 偏置中点 | 4.500000 V | 4.499986 V | — | +0.014 mV |
| **THD @0.2 V** | **27.579 %** | **27.5682 %** | 27.650 % | **+0.011 pp** |
| h2–h9 | 3.92/22.46/2.43/11.94/1.83/7.65/1.47/5.34 | 3.920/22.470/2.430/11.960/1.837/7.674/1.483/5.374 | — | **≤0.03 pp** |
| THD @1 mV | 0.2704 % | 0.2830 % | 0.283 % | −0.013 pp |
| 频响 50 Hz–10 kHz | 见下 | | | **≤0.50 dB** |
| 削波不对称（相对均值） | 上 0.4044 / 下 0.4258 V | 上 0.404 / 下 0.426 V | — | ✓ 方向一致 |

频响逐点（小信号，音量抽头）：

| 频率 | MNA | 基线 | 差 |
|---|---|---|---|
| 50 Hz | +0.54 dB | +1.04 dB | −0.50 |
| 100 Hz | +9.61 | +9.67 | −0.06 |
| 500 Hz | +23.69 | +23.72 | −0.03 |
| 1 kHz | +26.91 | +26.99 | −0.08 |
| 2 kHz | +28.16 | +28.29 | −0.13 |
| 5 kHz | +27.71 | +27.96 | −0.25 |
| 10 kHz | +25.52 | +25.92 | −0.40 |

高频端那一点点单调偏低，正是运放离散化在 192 kHz 下的已知效应（E38 记档），量级 0.4 dB。

> 结论：**MNA 路径的精度已经和 legacy 的一维牛顿实现同级、甚至略好**
> （THD +0.011 pp vs +0.034 pp）。通用装配器没有为"通用"付出精度代价。

### 42.2 这个回归抓到了一个真 bug：直流求解时电容必须开路

第一版 DC 工作点 OUT 偏了 **+40.8 mV**。原因不是器件、不是拓扑，而是
`solve_dc()` 用 `dt = 1.0` 去装配电容 → `g = C/dt` 把 **1 µF 变成了 1 MΩ 的对地电阻**，
输出经 C3 漏到音量电位器上。改成 `dc == true` 时直接 `return`（开路）后，偏差降到 0.056 mV。

→ 教训：**"用一个大 dt 近似直流"只在 dt → ∞ 的意义上成立**，`dt = 1 s` 对 µF 级电容远远不够。
DC 分析必须显式开路，不能靠伴随模型凑。

### 42.3 两个测量坑（都是"看起来像模型错"）

**① 低频增益偏高 4 dB**
50 Hz 处 MNA 给出 +5.14 dB，基线 +1.04 dB。看起来像输入高通（C1×R4 ≈ 69 Hz）算错了。
真相是**预热时间不够**：C3 × 音量电位器的 0.5 s 直流恢复还在衰减，那条指数尾巴的频谱
泄漏进了 50 Hz 的基波 bin。把低频点的预热从 1.5 s 提到 3 s 后立刻变成 +0.54 dB。

**② 削波不对称性判据写错**
用 `|min| > max` 判断"负向摆幅更大"，在直流没 settle 时会因为输出整体上移而失败
（实测 min=−0.13/max=+0.71）。改成以**稳态均值为中心**比较上下摆幅才是物理量。

### 42.4 性能与用法

- 5 个测试合计 **~15 s**（3 s 的 0.2 V 运行用 `OnceLock` 在两个测试间共享，省一半）
- 目前 MNA 只用于**离线验证**；实时路径仍是 `Circuit`/`Engine`（legacy 的 WDF 实现）。
  若要把它变成实时电路，还需要：解析雅可比（现在每个器件每步 `n+1` 次模型调用）+ 固定迭代次数。

### 42.5 下一步

`Circuit` 的 MNA 适配层（把 `Netlist` 包成 `Circuit`），或用声明式生成器（S1）直接生成
`Element` 列表 —— 两条路都以这次的精度基线为验收线。

---

## E43：把 MNA 做成实时电路（`Circuit` 适配层）（2026-10-23）

E42 证明了 MNA 的精度；这一步回答"**它到底吃多少 CPU**"。

代码：
- 框架侧（`mna.rs`）：`step_fixed()`（定次迭代）、`stamp_linear`/`stamp_nonlinear` 拆分、
  **固定电位节点**、二极管解析雅可比、`set_resistance()`；
- 电路侧（`tests/sparklin_mna.rs`）：`SparklinMna`（`impl Circuit`，含 `Params`）。

### 43.1 结果

| 迭代次数 | 耗时（192 kHz） | 单核占用 | THD @0.2 V（离线 27.579%） |
|---|---|---|---|
| 1 | 1127 ns | 21.6 % | 29.045 %（**+1.47 pp，不够**） |
| **2（默认）** | **2072 ns** | **39.8 %** | **27.677 %（+0.098 pp）** |
| 3 | 2809 ns | 53.9 % | 27.591 %（+0.012 pp） |

参照：legacy WDF 实现（**手工把整个电路化简到 1 个未知量**）约 824 ns。

→ **通用装配器付出约 2.5 倍的时间**，换来"任意拓扑都能跑"。
对 v2 是划算的：以后加一级 BJT 前级不用再手推方程。

### 43.2 优化路径（8267 → 2072 ns，4 倍）

| 优化 | 说明 |
|---|---|
| ① 节点折叠 14 → 11 | 偏置用 Thevenin（2 × 987k）；电平电位器空载时抽头 = `v(VOL)·level`，省一个节点 |
| ② LU 内层改切片 `zip` | 去掉 `a[row][k]` 的双重边界检查；对角线够大就跳过主元搜索；用乘倒数代替除 |
| ③ **固定电位节点** | 电源轨与输入源不再是未知量（11 → 9）；LU 是立方关系，这一步最立竿见影 |
| ④ 线性部分每拍只装一次 | `stamp_linear`/`stamp_nonlinear` 拆分，牛顿迭代里只重装 3 只二极管 |

分项（`examples/mna_profile`）：二极管 ~150 ns/只、9×9 LU ≈ 240 ns、矩阵 clone 24 ns。

### 43.3 四个"以为会快、其实没用/有害"的尝试

1. **线性外推初值 `2v − v_prev2`**：多次迭代时有用（legacy 靠它把迭代 3.48 → 2.00），
   但**只走 1 次迭代时反而有害** —— 外推点离真解更远，单步结果更差（THD 27.8% → 29.0%）。
2. **放宽二极管收敛阈值 1e-9 → 1e-6**：省一次 `exp`（约 15%），但高电流处内层解只准到 ~1.4 µV，
   导致雅可比失真 —— `conductance_matches_numeric_derivative` 直接报 10% 偏差。
   → **雅可比精度必须优先于这点性能**，已回退。
3. **部分主元搜索**：MNA 矩阵天然对角占优，搜索与行交换纯属浪费。
4. **`Mna::clone`**：24 ns，不是瓶颈（原以为是）。

### 43.4 还没做的优化（按收益排序）

| 做法 | 预计收益 |
|---|---|
| **预计算稀疏 LU 结构**（拓扑固定，消除顺序可离线算一次） | 240 ns → ~60 ns，整体 ~1.3 µs |
| 二极管用上一拍结电压热启动 | 每只 150 → ~80 ns |
| `solve` 直接吃工作矩阵（省一次 81 个 f64 的拷贝） | ~30 ns |

按第一项估算，2 次迭代可降到 ~1.3 µs（25% 单核）。

### 43.5 诚实的对比

legacy 的 824 ns 来自"**手工把电路化简成 1 个未知量**"；MNA 的 2072 ns 是
"9 个未知量 + 2 次牛顿 + 通用装配"。两者服务对象不同：

- 单级削波电路 → 手推实现更划算，也更容易调到极致；
- 多管/多级电路 → MNA 是唯一现实的选择（手推的复杂度会爆炸）。

这正是**保留两套求解器**（ADR-012）的理由，现在有了实测数字支撑。

---

## E44：S1a —— 网表 → MNA 装配代码（2026-10-23）

现在"把电路搬进框架"不再需要手写 `Netlist`：给一个 SPICE 网表，生成器直接吐出装配代码。

```bash
cargo run -q -p rustomp-import -- <网表.cir> --mna <输出.rs> [--check] [--output-node N]
```

### 44.1 链路

```text
sparklin_drive.cir
  → spice 导入器（解析 R/C/D/V/X，跳过 .model/.subckt 体）
  → ImportedCircuit（IR）
  → mna_codegen：导线合并 → 节点编号 → 模型映射 → 激励分类 → 代码
  → tests/generated/sparklin_drive.rs（入库）
  → tests/sparklin_generated.rs  include! 后跑 THD
```

### 44.2 结果：**与手写网表逐位一致**

| 指标 | 生成电路 | 手写网表 | ngspice |
|---|---|---|---|
| 运放输出（DC） | 4.499991 V | 4.499991 V | 4.499935 V |
| IN+ / IN−（DC） | 4.500000 / 4.499991 | 同 | 4.499724 |
| 音量抽头（DC） | 0.000000 V | 0 | ~0 |
| **THD @0.2 V** | **27.579 %** | **27.579 %** | 27.5682 % |
| h2..h9 | 3.92/22.46/2.43/11.94/1.83/7.65/1.47/5.34 | 同 | 同 |

生成的代码（节选）：

```rust
pub fn build(fs: f64) -> Netlist<N> {
    let mut net = Netlist::<N>::new();
    net.opamp(OP07.model(1.0 / fs), node::N_T11L, node::N_T10L, node::N_T11R, 11);
    net.capacitor(node::N_T11L, node::N_T21L, 4.7e-9);      // CC1
    net.resistor(node::N_T10L, node::N_T11R, 250000.0);     // RP1A（GAIN）
    net.diode(D_1N4148, node::N_T11R, node::N_T10L);        // DD1
    ...
    net.fix_node(node::N_RAIL_LP, 9.0);   // VB1
    net.fix_node(IN, 0.0);                // 输入端口
    net
}
```

### 44.3 生成器做了四件"看起来不像生成器该管"的事

1. **导线合并**：breadspice 把面包板连接线生成为 `0.001 Ω` 电阻（RW1…RW5）。
   不合并的话矩阵会出现 1000 S 的对角项，条件数爆炸；并查集把它们并成一个节点。
2. **模型映射是闭集**：`1N4148` → `D_1N4148`……未命中直接**报错**并列出现有型号，
   不做静默兜底（ADR-013：替换必须可见）。
3. **激励分类**：纯 DC 源 → **固定电位节点**（`fix_node`）；带 `SIN/AC/PULSE` 的源 → **输入端口**。
4. **自环跳过**：电位器"短接的那一半"在网表里长成 `RP1B n_t1L n_t1L 250k`，
   两端同节点，对电路无影响（否则会白占一次装配）。

### 44.4 一个"编译通过、跑起来也不报错、就是没声音"的坑

生成器里的 `FIXED_IN`（输入端口在 `net.fixed` 里的下标）第一版写死成 `0`。
但固定节点是按"电源轨在前、输入端在后"装配的，于是 `0` 其实是 **9 V 轨**的下标 ——
`process(x)` 每拍把音频**写进了电源轨**，输入节点一直是 0。

现象极具欺骗性：编译通过、不 panic、输出"有信号"，只是 **THD 0.237%**（几乎是纯线性）。
修好后立刻回到 27.579%。

→ 教训：**生成的常量必须由生成器算出来，不能"看起来是 0 就写 0"**；
   而且必须有**行为级回归**（THD/工作点），光比对文本会发现不了。

### 44.5 漂移守卫

`crates/rustomp-import/tests/codegen.rs::generated_file_is_up_to_date`
重新生成一次并与仓库里的生成物逐行比较，不一致就报出第一处差异与重新生成命令。
配合 `--check` 可以放进 CI。

另外两个测试守边界：未知型号必须报错并点名；`0.001 Ω` 导线必须合并。

### 44.6 S1a 的边界（明确不做）

| 未支持 | 说明 |
|---|---|
| BJT/JFET/MOS 的**子电路**模型 | 库里有器件模型，但 `Q/J/M` 卡的引脚→模型映射还没接（下一块） |
| `.asc`（LTspice 原理图） | 先做网表，符合 ADR-013 的格式优先级 |
| 电位器/开关的**参数化** | 现在生成的是"50% 位置"的定值；参数化属 S1b |
| 多电源/浮地电压源 | 只支持一端接地的源 |

---

## E45：S1b —— 参数化（电位器/开关）+ 器件卡片映射（2026-10-23）

S1a 生成的只是"定值电路"；这一步让它变成**能用的实时电路**。

### 45.1 网表里多出的指令

```text
* @rustomp in=VS1 out=n_t27R
* @rustomp param RP1A gain pot max=500k              # R = max·gain
* @rustomp param RP2B level pot max=500k invert      # R = max·(1−level)
* @rustomp param RP2A level pot max=500k             # R = max·level
* @rustomp switch symmetric n_t10L n_t5L closed=0.001 open=1e12
```

生成的代码从"一堆常量"升级成：

```rust
pub struct Params { pub gain: f64, pub level: f64, pub symmetric: bool }

pub struct Ckt { net: Netlist<N>, params: Params, fs: f64 }
impl rustomp_dsp::Circuit for Ckt { /* new / set_params / reset / process */ }
```

→ **生成的电路直接能塞进 `Engine<Ckt>`**，参数变更只改几个电阻值（`set_resistance`），
不重建网表。同一个 `Params` 既被 `build()` 用（初始值）也被 `set_params()` 用（更新）。

### 45.2 器件卡片

`Q/J/M` 卡片现在能映射：

| 卡片 | 生成 | 说明 |
|---|---|---|
| `Q1 c b 0 BC549C` | `net.bjt(BC549C, true, c, b, GND)` | 极性由型号常量决定（`BC559C`/`AC128` → PNP） |
| `J1 d g 0 J201` | `net.jfet(J201, d, g, s)` | |
| `M1 d g s 0 2N7000` | `net.mosfet(NMOS_2N7000, d, g, s)` | |

### 45.3 验证结果

**A. 参数化（Sparklin Drive，生成物 `Ckt`）**

| 项目 | 结果 |
|---|---|
| 默认参数 THD @0.2 V | **27.677 %**（离线求解器 27.579%，ngspice 27.5682%） |
| 对称开关（短接 D3） | h2 **4.002 % → 0.000 %**（legacy 实测 3.92 → 0.049） |
| GAIN 电位器 0.15 / 0.5 / 1.0 | THD **14.20 / 25.87 / 28.62 %**（单调） |
| 直流工作点 | 运放输出 4.499991 V、IN+ 4.500000、抽头 0.000000 |

一条测试同时覆盖了三件事：开关真的接到了正确的元件、电位器真的改了阻值、
参数更新的时序没有滞后。

**B. 器件映射（共射前级，`Q` 卡片）**

| 量 | 生成电路 | ngspice | 差 |
|---|---|---|---|
| Vb | 0.679100 V | 0.679098 V | **+2.2 µV** |
| Ic | 2.0432 mA | 2.0379 mA | **+0.26 %**（未建模的 `Nk`，E36 已记档） |
| 小信号增益 | 143× | — | 共射反相放大 ✓ |

ngspice 直接吃**生成器用的同一个网表文件**（测试里剔除 `.end` 后接 `.op`），
避免两边各写一份悄悄漂移。

### 45.4 三个新坑

**① 直流求解把「压摆率限制」也应用到了工作点上**

`commit()` 里运放状态统一走 `post()`（含压摆率），于是 `solve_dc` 结束时状态只能
从 0 跳到 `SR/fs = 1.302 V`。第一拍带着 1.302 V 的错误偏移起步，凭空多出一个瞬态：
运放输出 4.5 V → **2.18 V**，抽头 -0.86 V。

修法：`commit(v, dt, dc)` —— `dc = true` 时只钳摆幅、**不限摆率**（直流没有"每拍最多变多少"）。

这条是**行为测试**抓到的：之前的离线 THD 测试只看 3 s 后的稳态，完全漏掉了开场这一下。
教训与 E44 相同：**要在最容易被忽略的时刻取样**（第 0 拍、参数刚改完的那一拍）。

**② `set_params` 写成了旧值**

生成器把 `build()` 里的表达式 `max * p.gain` 机械替换成 `self.params.gain`，
于是 `set_params` 用**旧参数**算出电阻 —— 参数永远滞后一次。
正确是替换成函数的形参 `params.gain`。测试（GAIN 扫描单调）能抓到。

**③ 参数元件的下标必须是模块级 `const`**

第一版在 `build()` 里用 `let IDX_RP1A = net.push_resistor(...)`，`set_params` 看不到。
改成生成 `pub const IDX_RP1A: usize = 5;`（下标由生成器顺序累加算出）。

### 45.5 生成器现在的完整能力

| 输入 | 支持 |
|---|---|
| 元件 | `R C L(占位) D Q J M X` + DC/SIN/AC/PULSE 电压源 |
| 器件型号 | 二极管 4 + LED 3 + BJT 4 + JFET 1 + MOS 1 + 运放 1（闭集，未命中即报错） |
| 参数 | 电位器（正/反向）、开关 |
| 输出 | 节点常量 + `build()` + `Params` + `impl Circuit` |
| 守卫 | 漂移检查（两个夹具）、未知型号报错、导线合并 |

仍不做：`.asc` 原理图、多电源/浮地源、非线性电容、BJT 的 `Cje/Cjc`（属器件库的动态档）。

---

## E46：阶段收尾 —— 文档与"外部设计器"边界（2026-10-23）

S1a/S1b 让**整条链闭环**了，这一节只做两件事：把文档对齐到现状，
并把「哪些事本项目不做」写成契约（因为接下来会分别开发两个独立设计器）。

### 46.1 本阶段的链路

```text
面包板 / 原理图（设计器，独立项目）
   │  .cir 网表 + `* @rustomp` 指令
   ▼
rustomp-import --mna
   │  生成的 Rust（Netlist + Params + impl Circuit）
   ▼
rustomp-dsp（MNA 装配器 / 器件库 / 实时路径）
   │
   ▼
插件（Engine + rustomp-gui + nih-plug/clap-wrapper）
```

### 46.2 文档改动

| 文件 | 改了什么 |
|---|---|
| `README.md` | 顶部状态表加"实时路径 ✅ / 代码生成 ✅"并点明闭环；核心特性加"网表即源码"；仓库结构补 `mna_codegen`、`tests/generated`；质量把关新增 **2.5 生成电路** 一层；新增 **§与外部设计器的接口**；路线图与限制按现状重写 |
| `docs/circuit-import.md` | 状态从"待实现"改为 **S1a/S1b 已实现**；导入器表补 `SpiceNetlist`；明确映射分层**只做了 1（精确命中）与 6（报错）**；新增 §6.5（指令、产物、漂移守卫、行为级验收）；新增 **§10 与外部设计器的接口** |
| `docs/declarative-build.md` | §3.4 扩到 S1a/S1b（参数化 + 产物）；路线表拆成 S1a✅ / S1b✅ / S1c；新增 **§3.6 与外部设计器的接口**（含"为什么设计器不直接生成 Rust"）；变更记录 |
| `docs/component-library.md` | 注明 §10.1b 的表同时也是**代码生成器的闭集映射表** |
| `crates/rustomp-dsp/README.md` | MNA 章节补参数化用法（`Ckt` + `Params`）；待办更新 |
| `crates/rustomp-import/README.md` | **新建**：导入器、指令、产物、codegen 的拓扑处理规则、验证矩阵、更新流程、明确不做的事 |
| `DEVLOG.md` | 本节（E46）+ 变更一览表 |

### 46.3 写进契约的三条硬要求

对未来的**原理图设计器**：

1. **端口必须显式标注**（`in=` / `out=`）—— 不猜哪个节点是输出；
2. **可调元件必须显式声明**（`param` / `switch`）—— 不从 `250k + 250k` 反推电位器；
3. **型号必须落在闭集表**里 —— 不在表里在导入阶段就报错。

理由：生成的电路**会直接进插件的发布路径**。任何"靠猜的默认值"都可能变成噪音，
而且很难在几周后复盘。这三条已经在 `docs/circuit-import.md` §10 与
`docs/declarative-build.md` §3.6 落盘。

对未来的 **GUI 设计器**：输出 `rustomp.toml` 的 `[ui]` 段（`rustomp_gui::PanelSpec`）+ PNG 素材，
由 `build.rs` 生成面板声明 —— 这条路径在 E25 就已经打通，不需要改框架。

### 46.4 为什么不建议设计器直接生成 Rust

网表是**可被 ngspice 验证**的中间格式，我们所有的对拍（器件级、装配器级、整机级）
都锚在它上面。设计器一旦直接产出代码，对拍链就断了，"生成的电路到底对不对"只能靠耳朵。

所以设计器的职责边界很清楚：**画图 → 网表 + 指令**。剩下的交给生成器与对拍。

---

## E47：把对拍铺进 CI（阶段 A）（2026-10-25）

目标：**"验证过"必须是可自动复核的事实，而不是跑过一次的记忆。**

### 47.1 做了什么

| 项 | 内容 |
|---|---|
| `scripts/verify.sh` | 一键验证入口，本地与 CI 共用；`--only fast\|oracle\|all`、`--report <md>`、`--no-fmt` |
| `Oracle::discover_or_skip()` | 统一"跳过还是失败"的判定：本地跳过并打印原因；设 `RUSTOMP_REQUIRE_NGSPICE=1` 时 `panic!` |
| `.github/workflows/ci.yml` | 拆成 `fast` / `oracle` / `fmt` 三个 job；对拍结果写进 job summary |
| 9 处测试 | 从各自的 `Oracle::discover() else { eprintln!(跳过) ; return }` 收敛到同一个 helper |

### 47.2 为什么"缺 ngspice 必须失败"

原来的写法是每个测试自己判断：

```rust
let Some(oracle) = Oracle::discover() else { eprintln!("跳过"); return; };
```

这在本地很友好，在 CI 里却是**最坏的假阳性**：runner 上 `brew install` 一旦失败，
四层对拍全部静默跳过，CI 依旧绿灯 —— 而"对拍过"正是这个项目唯一的质量承诺。

现在：CI 的 oracle job 里 `RUSTOMP_REQUIRE_NGSPICE=1`，同一个 helper 变成 `panic!`，
而且 workflow 里 `brew install ngspice` **刻意不加 `continue-on-error`**。
本地开发仍可跳过（不装 ngspice 也能跑 `--only fast`）。

### 47.3 分层与耗时（本机实测）

| 层 | 内容 | 耗时 |
|---|---|---|
| fmt | `cargo fmt --all --check`（不阻塞） | 1 s |
| workspace | 全部单元/集成测试（无 ngspice 也能跑） | ~25 s |
| drift | 两份生成物 `--check` | ~2 s |
| devices | 二极管 ×4 + LED ×3 + BJT ×4 + JFET + MOS 的 V-I | ~1 s |
| mna | 削波 / BJT / JFET / RC 瞬态 / 运放 | ~1 s |
| sparklin | 整机（手写网表）DC + THD + 频响 | ~15 s |
| generated | 生成电路（S1a/S1b）DC + THD + 开关分支 | ~5 s |

合计约 **50 s**（release，缓存已热）。CI 上两个 job 并行，各自 ~3 分钟（含编译）。

### 47.4 对拍数字进 job summary

`./scripts/verify.sh --only oracle --report "$GITHUB_STEP_SUMMARY"` 会输出：

```markdown
| 层 | 状态 |
|---|---|
| 器件级 V-I（二极管 / LED / BJT / JFET / MOS） | ✅ |
| 装配器级（削波 / BJT / JFET / RC 瞬态 / 运放） | ✅ |
| 整机级（Sparklin Drive，手写网表） | ✅ |
| 生成电路（S1a / S1b） | ✅ |
```

外加一个折叠块，里面是**实际误差数字**（每点的 V-I 偏差、THD、频响 dB 差）。
翻历史构建就知道"这次精度是多少"，而不是只看一句 passed。

### 47.5 两个脚本 bug（都是自己踩的）

1. **用中文名字当日志文件名**：tier 名里有 `/` 和空格 → 路径不存在、报告表格被拆成单词。
   改成 `run_tier <slug> <显示名> <命令>`，slug 只进文件名，显示名只进报告。
2. **`set -u` 下残留变量**：删掉 `RAN` 后忘了 fmt 分支还在用它 → `unbound variable`。
   顺带把 fmt 的"不阻塞"从"不算层"改成"算一层但状态是 `warn`"，报告里显示 ⚠️。

### 47.6 下一步（阶段 D）

`rustomp gen`：`[[params]]` → 插件侧 `params.rs` / `plugin_impl.rs`，
让"生成电路 → 生成插件"这条链真正闭环。GUI 之前的部分到那时就齐了。

---

## E48：S1c —— 插件侧代码生成（阶段 D）（2026-10-25）

S1a/S1b 让"网表 → 电路"闭环；这一步把**插件外壳**也变成生成物。
做完之后，"给个 `rustomp.toml` + 一个电路"就能长出一个可编译、可加载的插件（**无 GUI**）。

### 48.1 新增 `crates/rustomp-gen`

```bash
cargo run -q -p rustomp-gen -- <项目目录>            # 生成
cargo run -q -p rustomp-gen -- <项目目录> --check    # 只比对（CI 防漂移）
```

| 生成文件 | 内容 |
|---|---|
| `identity.rs` | 名字 / vendor / CLAP id / 三套 VST3 id。**变体差异用 `#[cfg]` 选择**，所以文件是静态的 |
| `params.rs` | `PluginParams` 结构体 + `Default` + `to_dsp()`（把平滑后的值拍成电路参数） |
| `plugin_impl.rs` | `Plugin` / `ClapPlugin` / `Vst3Plugin` + `nih_export_*!` + 界面钩子 |
| `mod.rs` | 模块汇总 |

`to_dsp()` 是关键的一小段生成代码：

```rust
pub fn to_dsp(&self) -> sparklin_drive_circuit::Params {
    sparklin_drive_circuit::Params {
        gain: self.gain.smoothed.next() as f64,   // float：电位器位置，平滑
        symmetric: self.mode.value(),             // bool：直接取值
    }
}
```

**生成器不懂 DSP**：`process()` 只是"取参数快照 → `Engine::set_params` → `process` → 写活动量"。

### 48.2 一次真实编译验证（不是"看起来对"）

```text
./scripts/new-plugin.sh "Gen Demo"
  → plugins/gen-demo/{rustomp.toml, circuit/, plugin/}
  → rustomp-gen 写出 4 个文件
  → cargo build -p gen-demo           ✅ 33 s
  → nm target/debug/libgen_demo.dylib
      _clap_entry          (CLAP)
      _GetPluginFactory    (VST3)
```

**"能编译 + 导出两个入口"** 是这一步的验收线：只有这样才能保证后面
"生成 Sparklin Drive 装进 Logic 听声音"不会再被外壳问题绊住。

### 48.3 三处踩坑

**① `Params` 名字冲突（编译不过）**
生成的结构体叫 `Params`，与 nih-plug 的 trait `Params` 撞名：
`#[derive(Params)]` 和 `Arc<dyn Params>` 被 glob import 互相遮蔽。改成 **`PluginParams`**。

**② build.rs 不该再存在**
原来 `build.rs` 按 feature 生成 `identity.rs` 到 `OUT_DIR`。现在变体用 `#[cfg]` 表达，
文件是静态的 —— 于是 `build.rs` 在无 GUI 阶段被删掉，身份文件入库、可 review、可 `--check`。

**③ 模板 shell 脚本里 heredoc 反引号**
`cat <<EOT`（未加引号）会把反引号里的内容当命令执行 ——
`已生成（\`$DEST/plugin/src/generated/\`）` 直接把路径当命令跑了一遍。
改成 `${DEST}/...` 无引号写法。

### 48.4 模板结构变化（GUI 之前）

| 变化 | 原因 |
|---|---|
| `template/plugin/build.rs` 删除 | 身份不再需要构建期生成 |
| `template/plugin/src/lib.rs` 变成 10 行（`mod generated;`） | 内容全部生成；手写只留壳 |
| `template/plugin/Cargo.toml` 去掉 `nih_plug_egui` / `rustomp-gui`，加 `gui = []` feature | 这一阶段无界面；界面在 S1d 以手写 `editor.rs` 接回 |
| `rustomp.toml` 新增 `[circuit]`（`plugin_type` / `params_type`）与 `[[params]]` | 生成器需要知道"哪个类型是电路""哪些参数进 DSP" |

`plugin_impl.rs` 里预留的钩子：

```rust
#[cfg(feature = "gui")]
fn make_editor(params: Arc<PluginParams>, activity: Arc<AtomicU32>) -> Option<Box<dyn Editor>> {
    crate::editor::make(params, activity)   // 手写
}
#[cfg(not(feature = "gui"))]
fn make_editor(..) -> Option<Box<dyn Editor>> { None }
```

生成器不掺和 GUI，界面代码永远是手写的 —— 这正好符合"进 GUI 之前先把框架梳理完"。

### 48.5 校验（生成前就挡住）

`package` 必须是 kebab-case、`name/vendor` 非空、**VST3 id 恰好 16 字节**（每个变体）、
参数 id 唯一且合法、`circuit` 字段合法且**同一字段不被两个参数映射**、`gui = true` 时必须有 `[ui]`。
一次报**全部**问题。

单测覆盖：生成 → 再生成（`unchanged`）→ 手改一个文件后 `--check` 报漂移 → 非法 manifest 报错。

### 48.6 现在这条链是完整的（到 GUI 之前）

```text
rustomp.toml ──► rustomp-gen ──► plugin/src/generated/{identity,params,plugin_impl}.rs
      │
      └─ 网表 .cir ──► rustomp-import --mna ──► circuit/src/generated/…rs（Netlist + Params + impl Circuit）

两者 + Engine + scripts/build-all.sh ──► AU / VST3 / CLAP
```

---

## E49：生成第一个产物 —— Sparklin Drive v2（无 GUI）（2026-10-25）

S1a/S1b/S1c 之后第一次用**全生成链路**产出一个能听的插件：没有手写电路、没有手写外壳。

### 49.1 链路

```text
legacy 网表快照 ──(加 4 行 @rustomp 指令)──► circuit/netlist/sparklin_drive.cir
   └─ rustomp-import --mna ──► circuit/src/generated/sparklin_drive.rs（12 未知量）
                                  └─ circuit/src/lib.rs：`include!` + `pub type SparklinDrive = Ckt;`

rustomp.toml（2 个参数：gain / level）──► rustomp-gen ──► plugin/src/generated/{identity,params,plugin_impl}
                                  └─ plugin/src/lib.rs：`mod generated;`（10 行）

scripts/build-all.sh ──► Sparklin Drive Mono / Stereo（AU + VST3 + CLAP）
```

### 49.2 实测

| 项 | 结果 |
|---|---|
| 引擎级 THD @0.2 V（48 kHz + 4× 过采样） | **27.65 %**（离线 MNA 27.579 %、ngspice 27.5682 %、legacy WDF 27.650 %） |
| 引擎级 THD @1 mV | 0.277 %（基线 0.283 %） |
| 处理速度 | 3 s 音频 / 0.59 s ≈ **5× 实时**（单声道） |
| AU 自检（au-smoke，绕过系统注册表） | Mono：1 声道 ✅；Stereo：2 声道 ✅；RMS 0.356、无非有限值 |
| 安装 | `Sparklin Drive Mono` (aufx/**Sp02**/HpEf)、`Sparklin Drive Stereo` (**Sp03**) 均已装好 |

生成过程中 `new-plugin.sh` → `rustomp-gen` → `rustomp-import` → `build-all.sh` 全程无手写代码。

### 49.3 三个踩坑

**① `plugin_meta.py` 在无 GUI 阶段直接崩**
`[assets] switch_up` 缺 KeyError、`[ui]` 缺失导致 `--shell` 不可用。
改成 `.get()` + 空字典，`_asset()` 未声明时返回空串（构建脚本自己判断）。

**② `nih_export_vst3!` 末尾的分号不能删**
编译器会报 `semicolon_in_expressions_from_non_local_macros` 警告，看起来像我们的问题；
删掉分号反而直接编译失败（宏展开成 item）。**警告来自 nih-plug 自己的宏卫生**，保留分号、忽略警告。

**③ 生成的常量会告 dead_code**
`identity.rs` 里按 `cfg` 分叉的常量在特定 feature 组合下用不到 → 文件顶部加 `#![allow(dead_code)]`。

### 49.4 一个仓库约定上的注意点

`plugins/` 是 gitignored，但 workspace 注册写在受控的 `Cargo.toml` 里 ——
所以**插件项目的 workspace 注册是本地改动，不要提交**（否则 CI 会找不到目录而失败）。
下游插件的正式归宿是独立仓库（同 `sparklin-drive-legacy`）。

### 49.5 界面与开关（按既定决策）

- **无 GUI**：`editor()` 返回 `None`，宿主显示通用参数界面；界面属 S1d。
- **无开关**：按 D-005，开关是"插件内的电路分支"。本次只做**非对称分支**（原机接法）；
  对称模式需要"2 份电路 + 插件层切换"，属后续工作。

---

## E50：AB 对比暴露的真 bug —— 2 次牛顿迭代导致"次谐波极限环"（2026-10-25）

用户把新版（MNA 生成）与 legacy（WDF）装进 Logic 做 A/B，反馈：
**音色取向相同，但新版高频更毛更亮，且混入了难以察觉却让整体劣化的浑浊刺耳成分，"有数字味"**。
接入信号源看 FFT：**正弦一切正常，一换方波新版就露馅**（基频以下出现大片隆起 + 谐波间锯齿杂散）。

### 50.1 根因：`NEWTON_ITERS = 2` 欠收敛 → 次谐波极限环

离线台架复现（880 Hz、Gain 50%、2 s 预热后取 0.2 s，dB 相对基频）：

| 实现 | f/2 | f/4 | f/8 | f/16 | 谐波阶梯 |
|---|---|---|---|---|---|
| legacy（WDF，收敛） | −80 | −86 | −70 | −83 | 平滑 |
| **MNA @2 次迭代** | **+1** | −2 | **+12** | **+19** | 锯齿（h6=+7、h10=+7、h16=+14…） |
| MNA @3 次 | −82 | −86 | −71 | −83 | 平滑（但仍不平整） |
| **MNA @4 次** | −80 | −86 | −70 | −83 | 平滑 |

正弦输入时 2 次迭代的次谐波是 −109 dB（干净），**方波/快速边沿才会触发** ——
这解释了"为什么正弦看不出来、一上方波就露馅"。

### 50.2 修复

1. `mna_codegen` 新增网表指令 **`* @rustomp newton=<n>`**，默认值从 2 改成 **3**；
   Sparklin Drive 用 **4**（3 次虽然次谐波已降到 −82 dB，但方波的高次谐波仍不平整）。
2. **回归测试**（`tests/sparklin_generated.rs::no_subharmonics_under_square_input`）：
   880 Hz 方波下 f/2、f/4、f/8、f/16 必须都低于基频 60 dB。实测 −70 dB（f/8）。
3. `Netlist::step_fixed` 的文档写明：**固定迭代次数是"用精度换确定性"的交易，
   次数不够会产生极限环；必须用方波验证**。

### 50.3 效果

| 指标 | 2 次迭代（旧） | 4 次迭代（新） |
|---|---|---|
| 方波次谐波 | **+19 dB** | **−70 dB** |
| THD @0.2 V | 27.65 % | **27.58 %**（离线 27.579 / ngspice 27.5682） |
| 正弦谐波 h1–h21 | 高次抬高 2–4 dB | 与 legacy 逐次差 ≤1 dB |
| 每内层样本耗时 | ~1.0 µs | **~1.87 µs** |
| 48 kHz 单声道 | ~20 % 单核 | **~36 %** |
| 48 kHz 立体声（4×） | ~40 % | **~72 %** |

**用 2 倍 CPU 换回正确性，值得。**

### 50.4 教训（写下来给以后的自己）

1. **正弦测试不足以验证非线性求解器**：这个 bug 在 27.6% 的 THD、h2–h9 全部对上 0.1 pp 的情况下依然存在。
   非线性求解器的验收必须包含**最"难算"的输入**：方波、突变的阶跃、最大摆幅。
2. **"固定迭代次数"必须配"结果形态"的检查**：固定次数是为了实时确定性，
   但没有任何收敛判据时，只能靠**行为级回归**（次谐波电平）兜底。
3. **THD 对不上 0.1 pp 并不代表没问题**：2 次迭代的 THD 27.65% 看起来"很好"，
   但它同时藏着一个 −0 dB 的次谐波。
4. 用户的主观描述（"更毛更亮 + 浑浊刺耳 + 数字味"）精确对应量化结果
   （高次谐波 +2–4 dB、次谐波 +19 dB）—— **听感和指标是对得上的**。

---

## E51：电位器拧到端点 → 整块静音（0 Ω 电阻）= 框架 bug（2026-10-25）

用户实测：**新版音量拉到 100% 就没声，拉回 99% 又有声**。

### 51.1 根因：`R = 0` 让导纳变成 ∞

电位器在这个电路里是**两个电阻**：

```text
level = t:  R_bottom = 500k·t        （地 → 抽头）
            R_top    = 500k·(1−t)    （抽头 → C3）
```

`t = 1` 时 `R_top = 0 Ω`；MNA 用**导纳**表示电阻 → `g = 1/0 = ∞` → 矩阵全 NaN →
引擎的数值熔断器判定"出事了" → 复位并输出静音。**所以是"静音"，不是"没声但有输出"。**

同一个坑还有一处：**`gain = 0`**（反馈电阻 = 0 Ω）→ 同样整块静音。
复现记录：

```
gain=0.50 level=1.00 → 峰值 0.0000，非有限值 20000   ← 全 NaN
gain=0.00 level=0.50 → 峰值 0.0000，非有限值 20000   ← 全 NaN
```

### 51.2 修复：电阻夹到 1 mΩ（单一入口）

```rust
pub const MIN_RESISTANCE: f64 = 1e-3;   // 1 mΩ ≈ 数值上的"短路"

impl Mna<N> {
    pub fn resistor(&mut self, a: usize, b: usize, r: f64) {
        self.conductance(a, b, 1.0 / floor_resistance(r));   // ← 唯一装配入口
    }
}
```

同时 `Netlist::resistor` / `push_resistor` / `set_resistance` 存值时也夹紧，
保证"读回来的值"与"装配时用的值"一致。

1 mΩ 与生成器里"开关闭合 = 1 mΩ"是同一个约定 —— **框架里只保留一种"短路"的定义**。

### 51.3 回归测试

`tests/sparklin_generated.rs::pot_extremes_stay_finite_and_monotonic`：

| gain | level | 峰值 |
|---|---|---|
| 0.0 | 0.0 | 0.0000（静音，正确） |
| 0.0 | 0.5 | 0.1046（退化成单位增益缓冲，不再有增益） |
| 0.0 | 1.0 | 0.2093 |
| 0.5 | 0.0 | 0.0000（静音，正确） |
| **0.5** | **0.5** | 0.5258 |
| **0.5** | **1.0** | **1.0516**（满音量，比 0.99 的 1.0410 略大 ✓ 单调） |
| 1.0 | 0.5 | 0.5283 |
| 1.0 | 1.0 | 1.0567 |

测试断言：**每个端点都必须输出有限值** + 音量随 level 单调 + `gain=0` 不得比 50% 更响。

### 51.4 教训

1. **"参数端点是 bug 的温床"**：中间值一切正常，端点才会触发（0 Ω、0 长度、满量程）。
   凡是生成"由参数算出来的元件值"的地方，都要问一句：**t=0 和 t=1 时这个表达式等于几？**
2. **框架必须比使用者更保守**：生成器可以算错（毕竟是我们写的模板代码），
   但 MNA 装配器不该允许 `1/0` 这种值进矩阵。**在最底层的单一入口夹紧，比在每个生成表达式里加 `max()` 更可靠。**
3. 引擎的 NaN 熔断在这里帮了大忙：它把"NaN 扩散成整首歌都毁了"变成了"这块没声"。
   但**熔断只是止损，不是修复** —— 用户听到的"没声"正是熔断在工作。

---

## E52：AB 测试复盘落盘（阶段总结）（2026-10-25）

这次 AB（v2 生成路径 vs legacy 手推 WDF）不只是一次听感对比，
它**用两个真 bug 换来了四条可复用的工程规则**。完整复盘写在
**`docs/ab-test-sparklin-v2.md`**（时间线、四张 FFT 的判读、量化对照、9 条经验、发布前清单），
本节只留摘要与索引。

### 52.1 这次 AB 抓到的两个 bug

| # | 现象 | 根因 | 修复 |
|---|---|---|---|
| 1 | 高频更毛更亮 + 浑浊刺耳"数字味" | `NEWTON_ITERS=2` 欠收敛 → **次谐波极限环**（方波下 f/8 +12 dB、f/16 +19 dB） | 网表指令 `newton=`，默认 3、SD 用 4；方波次谐波回归测试 |
| 2 | 音量 100% 静音 | 电位器上臂 `500k·(1−t)` 在 t=1 时为 **0 Ω** → `1/0=∞` → NaN → 熔断 | 框架 `MIN_RESISTANCE = 1 mΩ`，在唯一装配入口夹紧；端点回归测试 |

### 52.2 沉淀的四条规则（详见复盘文档 §4）

1. **测试信号要分层**：正弦只验稳态谐波，**查不出求解器欠收敛**；方波/阶跃必须单独验。
2. **验收要包含"不该出现的分量"**：次谐波、非谐波杂散、DC 漂移、NaN 计数 ——
   只验"该有的对不对"，永远发现不了"多了不该有的"。
3. **参数端点是 bug 温床**：凡是"由参数算元件值"的地方，先问 `t=0 / t=1` 等于几。
4. **数值实现层就是音色的一部分**：讨论"两条实现是否同一台机器"时，
   求解器（迭代次数/收敛判据/积分方法）必须和器件模型同等对待 ——
   我这次的预测偏差就出在只算了积分方法的线性差异。

### 52.3 索引

- 复盘文档：`docs/ab-test-sparklin-v2.md`
- 相关条目：E49（首次全生成产物）、E50（次谐波）、E51（端点静音）
- 发布前清单：复盘文档 §5（构建前 / 客观验收 / 主观验收三组勾选项）

### 52.4 当前两版的可测差异

只剩 **9 kHz 以上新版低 2–4 dB**（精确极点 vs 梯形法在半样本延迟上的差别）。
这是两种积分方法的真实差异、方向是"新版更接近模拟原型"，暂不处理。

---

## E53：用 Rust FFT 复核 AB（取代肉眼读分析器）（2026-10-25）

用户对 1 kHz 方波截图提出两个疑点（9–10 kHz 隆起、极低频有能量）。分析器是对数轴 + 平滑，
不适合读绝对数，所以把测量搬进 Rust：**同一段信号 → 两边完整 `Engine` → `rustfft 6`**。

- 框架侧台架：`crates/rustomp-dsp/examples/abfft.rs`（用入库的 S1b 夹具，任何人可复现）
- legacy 侧台架：`sparklin-drive-legacy/plugins/sparklin-drive/circuit/examples/abfft.rs`
- 采集：3 s 预热 + 1 s（1000 个整周期，矩形窗零泄漏，1 Hz 分辨率）

结果（详见 `docs/ab-test-sparklin-v2.md` §7）：

| 疑点 | 实测 | 判定 |
|---|---|---|
| 9–10 kHz 隆起 | h9 差 **+0.22 dB**、h10 差 **−5.0 dB**；6–10 kHz 频段差 +0.16 dB | **不成立**（旧版 2 次迭代的伪影） |
| 极低频能量 | LF 峰值 **−103.7 dB**（两边一致） | **不成立** |

真实差异（1 kHz 方波）：

- **奇次谐波逐次 +0.04…+0.26 dB**（v2 略"利"），THD 45.35 vs 44.85（+0.50 pp，完全由这 0.2 dB 解释）；
- **偶次谐波相对差 1–12 dB，但绝对电平 −42…−68 dB**（比相邻奇次低 30–50 dB）→ 听不见；
- 分频段能量差 ≤0.22 dB；RMS 差 0.028 dB；DC 一致。

**结论：可听范围内两版差 ≤0.5 dB，且差异集中在"最不利的位置"（偶次谐波 / 22 kHz 以上）。
已经不是音色差异，而是两种积分方法在小数点后一位的分歧。**

顺带把"怎么复现旧版问题"也写进文档：把 `newton=4` 改成 `2`，LF 峰值会从 −103 dB 窜到 +10 dB 量级。

---

## E55：偶次谐波对比信号的归一化事故（2026-10-25）

做完 E54 的 `even_abex.wav`（v2 偶次 / legacy 偶次 / 差值 三段），用户听完立刻指出：
**"差值段能听得很清楚，音量接近前两段"**。

复查文件电平：三段峰值**都是 −1.0 dBFS**。原因是我对**每一段各自**做了峰值归一化：

| 段 | 真实电平 | 我施加的增益 | 结果 |
|---|---|---|---|
| v2 偶次 | −39.4 dBFS RMS | +30.8 dB | 峰值 −1.5 dBFS |
| legacy 偶次 | −38.9 dBFS RMS | +30.8 dB | 峰值 −1.0 dBFS |
| **差值** | **−51.8 dBFS RMS** | **+38.2 dB** | 峰值 −1.0 dBFS ← 多给了 7.4 dB |

**把真实存在的 12.3 dB 差距抹掉了。** 用户听到的"差不多响"是文件造成的，不是电路。

### 修复

1. `abex.wav` 改成**三段共用同一增益**（+30.8 dB）→ 差值段实测 RMS **−20.9 dBFS**，
   比前两段（−8.6 / −8.1）低 12.3 dB，如实反映关系；
2. 单独保留 `even_diff_boost.wav`（+38.2 dB）作为**诊断**用，并在文件名/文档里标注"放大过"；
3. 新增**决定性可听性实验**：
   - `swap_a_carrier_v2.wav` = v2 原始输出（RMS −7.91 dBFS）
   - `swap_b_v2_with_legacy_even.wav` = (v2 − v2 的偶次) + legacy 的偶次（RMS −7.91 dBFS）
   - 两者之差恰好 = 偶次差值：**RMS −51.8 dBFS / 峰值 −39.2 dBFS**
   - 若正常监听音量下听不出 A/B 差别 → 偶次谐波差异在真实信号里不可听。

### 教训

> **要对比的两个信号，绝不能用各自的归一化增益。**
> 归一化会把你正要测量的东西（电平差）当成"要消除的差异"抹平。

同类陷阱：A/B 两段做了响度匹配后再对比音色（合理），但**用响度匹配去对比"电平差异"**（自欺）。
判断标准很简单：**归一化之后，你还能不能回答原来的问题？**
这次不能 —— 原问题就是"差值有多响"。

### 顺带

- 这条也说明**"人耳 + 可疑文件"比"指标 + 自以为是"更可靠**：
  我的数字（−51.8 dBFS）一直是对的，错的是我把它做成了"听起来一样响"的文件。
- 已把修正后的数字与实验方法写进 `docs/ab-test-sparklin-v2.md` §8。

---

## E56：4 次迭代仍然欠收敛 —— 提到 8 次后与 ngspice 重合（2026-10-25）

E55 之后用户问"那 12 dB 的差值到底从哪来"，并要求用 ngspice 定案。

### 56.1 先修掉一个对比方法的漏洞

第一版 ngspice 对比是**激励不等价**的：给 ngspice 喂 1 ns 边沿的理想方波（它 1 µs 步长根本采不到），
给我们的电路喂 192 kHz 理想方波 —— 比出来的是激励差异。

改成**同一条带限方波**（奇次谐波 1…19 求和），ngspice 侧用 10 个串联 `SIN` 源精确合成（不依赖边沿分辨率）。
台架：`crates/rustomp-dsp/examples/abcmp_ngspice.rs`。

### 56.2 结果：4 次迭代是欠收敛的

| h | ngspice | 我们 4 次 | 我们 8 次 | 我们 16 次 |
|---|---|---|---|---|
| 2 | −32.1 | −29.5 | −32.3 | −32.3 |
| 4 | −37.3 | −36.0 | −37.7 | −37.7 |
| 6 | −40.7 | −43.1 | −41.1 | −41.1 |
| 8 | −43.6 | **−54.0** | −44.1 | −44.1 |
| 12 | −50.4 | **−45.8** | −50.9 | −50.7 |
| 14 | −56.3 | **−45.7** | −56.6 | −56.2 |
| 16 | −58.6 | **−47.7** | −61.0 | −62.5 |
| 20 | −44.0 | **−58.0** | −45.4 | −45.7 |
| **基频** | **0.51784** | **0.56508（+0.76 dB）** | **0.51789（+0.0004 dB）** | 0.51787 |

- **4 次**：基频偏 **+0.76 dB**，偶次谐波上下乱跳 ±10~14 dB；
- **8 次**：基频与 ngspice 差 **0.0004 dB**，偶次谐波逐次差 ≤2 dB；16 次不再变化 → **收敛**。

> 为什么奇次/THD 一直"看起来对"：偶次谐波比奇次低 30–50 dB。
> 同一份残差落在奇次上只有 0.2 dB（听不出），落在偶次上就是 ±10 dB（序列变锯齿）。
> **低电平分量是收敛性的放大器。**

### 56.3 修正后

网表 `newton=4` → **`newton=8`**（插件与框架夹具同步），重新生成、重建。

| 指标 | 4 次 | **8 次** | legacy |
|---|---|---|---|
| 方波基频 | 0.51830 | **0.51776** | 0.51756 |
| 方波 THD | 45.351 % | **44.698 %** | 44.848 % |
| 偶次差值 RMS | −51.8 dBFS | **−64.4 dBFS** | — |
| 全带差值 RMS | −39.7 dBFS | **−45.0 dBFS** | — |
| 引擎 THD @0.2 V | 27.577 % | **27.579 %**（与离线一致） | — |

偶次谐波与 legacy 的逐次差从"最大 12 dB"收到 **≤1 dB（h16 处 2.8 dB）**。

### 56.4 CPU（机器冷却后实测）

| 迭代 | ns/内层样本 | 48k 单声道 | 48k 立体声 |
|---|---|---|---|
| 4 | 2020 | 39 % | 78 % |
| **8** | **3812** | **73 %** | **146 %** |

→ **单声道 8 次迭代可实时（68%）；立体声 146% 会掉采样。**
所以下一步（选项 B）必须做**自适应迭代**：常态跑 4 次，残差大才补到 8 次（上限 8）。

> 顺带记一个测坑：连续高强度编译/台架一小时后，同一段基准从 2020 ns 涨到 6700 ns（约 3×），
> **i9 在降频**。CPU 数字必须在机器冷却、且不与编译并行时测，否则会把"散热"当成"算法慢"。

---

## E57：自适应牛顿迭代（修 CPU）+ 一次失败的微优化（2026-10-25）

8 次迭代解决了音色（E56），但立体声爆音、单声道偶发爆音。这一节是"把 CPU 要回来"。

### 57.1 做了什么

1. **自适应提前退出**（`Netlist::step_fixed`）：`iters` 变成**上限**，
   每步算完比较"两次迭代间的最大相对变化"，小于 `fixed_tol`（默认 `1e-6`）就停。
   代码里还顺带记了 `last_iters` 方便诊断。
2. **LU 只处理真正的未知量**（生成器把固定电位节点排到最大下标上）：
   12 个下标里有 3 个是"尸体"（电源轨、输入、地），`UNKNOWNS=10`。
3. 网表 `newton=8`（上限 8）。

### 57.2 效果：迭代次数真的降下来了

| 方案 | ns/内层样本（配对测量） | 与固定 8 次的输出差 |
|---|---|---|
| 固定 8 次 | 3420 | — |
| **自适应 tol=1e-6** | **1132（3.0×）** | **−227 dB（等价）** |
| 固定 4 次 | 1750 | −84 dB |

实测 `last_iters` 稳定落在 **2 次**——热启动 + 外推让绝大多数样本一两次就收敛。
（也就解释了为什么"固定次数"一直是错的：它既不够、又浪费。）

### 57.3 失败的微优化：LU 裁剪只值 1.09×

原本预期"12 行砍到 10 行 → LU 降 (10/12)³ ≈ 0.58"。同进程配对实测只有 **1.09×**。
原因：**每次迭代的 LU 已经不是瓶颈**——真正的大头是"每样本的固定开销"
（`Mna::new` 清零、`stamp_linear` 重建全部线性元件、`commit` 遍历元件）。
迭代次数从 8 降到 2 之后，这个固定开销占比更大。

→ 下一步若要再提速，应该攻"每样本固定开销"（缓存线性基 + 只 patch 随状态变化的 RHS 项），
而不是继续压 LU。

### 57.4 一个必须记下来的测量坑：这台机器的绝对性能会漂移 3–5×

同一段引擎基准，一个小时内测到过 **2020 / 3812 / 6700 / 11000 / 14400 ns**（内层样本），
legacy 也从历史 824 ns 涨到 3900 ns（4.7×）—— 期间 `pmset -g therm` 显示 `CPU_Speed_Limit=100`，
但实际吞吐差了 3–5 倍（长会话 + 连续编译 + Hackintosh 的电源管理）。

**结论：以后 CPU 数字必须满足三个条件才可信**
1. 机器空闲 ≥1 分钟；
2. **不与编译/台架并行**；
3. 有**配对参照**（同进程测两个方案，或同命令里交替测 v2/legacy 取比值）。

只看绝对值会把"散热"当成"算法变慢"，我已经为这个坑浪费了两轮判断。

### 57.5 现状

- 音色：与 legacy 无可感差别（用户确认），与 ngspice 基频差 0.0004 dB；
- CPU：迭代次数已从 8 降到 ~2；**但绝对 CPU 仍待用户在宿主里确认**
  （本机测量不可信；用户上一版报告"立体声严重爆音、单声道偶发"）。

---

## E58：双分支 + 开关切换（一个插件里两份完整电路）（2026-10-25）

按决策 D-005（设计器仓库 `docs/00-decisions.md`）实现：**开关不是电路元件，而是"两份完整电路之间的选择"**。

### 58.1 框架：`rustomp_dsp::Branch<A, B>`

`crates/rustomp-dsp/src/branch.rs` —— 把两个 `Circuit` 打包成一个 `Circuit`：

```rust
pub type Branch = rustomp_dsp::Branch<asym::Ckt, sym::Ckt>;
pub type Params = rustomp_dsp::BranchParams<asym::Params, sym::Params>;
// plug-in 侧仍然只有 Engine<Branch>，过采样/bypass/延迟全部不变
```

- **不在切换中时只跑当前分支**（CPU 不翻倍）；
- 切换分两段：**预热 5 ms**（两路都跑，输出仍全走旧分支）+ **10 ms 线性交叉淡化**；
- 只在**选择位变化**时启动（`set_params` 每样本都在调，不能重复触发）。

### 58.2 为什么要"预热"：一个实测抓到的咔哒

第一版只有 10 ms 淡化，测试报**相邻样本跳变 0.295**。定位：切进来的那一支状态是**冷的**
（还停在直流工作点），突然喂进半途中的信号 → 输入高通 / 反馈电容产生起始瞬态；
这个瞬态乘上淡化权重 0.09 就已经是 0.099 的台阶。

加 5 ms 预热（≈ 输入高通 68 Hz 时间常数的 1/3）后，跳变降到 0.10 —— 而那正是**信号自身削波边沿**的陡度。

→ 教训：**"冷状态"必须在淡入之前先跑热**，否则淡入只是在给一个错误的起始条件配音量包络。

### 58.3 测试判据也要改（差点误判）

原来的断言是"相邻样本跳变 < 0.06"。但这个电路的**削波边沿本身**就是 0.10～0.31（取决于采样率），
固定阈值会误报。正确判据：**拿稳态信号自身的最大跳变当基准**，切换期间不得超过它的 1.5 倍：

```
切换期间最大跳变 0.3061 · 稳态信号自身 0.3079 · 比值 0.99   ✓
```

### 58.4 生成器：`params_builder`

分支的参数不是平坦结构体（`BranchParams { a, b, select_b }`），生成的 `to_dsp()` 不能再写字面量。
新增 `[circuit] params_builder = "make_params"`：设了就按 `[[params]]` **声明顺序**生成

```rust
sparklin_drive_circuit::make_params(self.gain.smoothed.next() as f64, self.symmetric.value(), self.level.smoothed.next() as f64)
```

顺带踩了一个小坑：我在 manifest 里把 `symmetric` 插在 `gain` 和 `level` 之间，
而电路侧的 `make_params(gain, level, symmetric)` 顺序不同 → 编译期就炸了（类型不同兜住了）。
**约定：manifest 的声明顺序 = 构造函数的参数顺序 = 面板上的排列顺序。**

### 58.5 插件：两份网表

| 分支 | 网表 | 削波环 |
|---|---|---|
| `asym` | `sparklin_drive_asym.cir` | 1 只 ∥ 2 只串联（原机） |
| `sym` | `sparklin_drive_sym.cir` | 用一根 ≤0.01 Ω 导线短接 D3 → 1 只 ∥ 1 只 |

`sym` 不是手改的：由 codegen 的"导线合并"规则把它两端并成一个节点，于是 D3 自动变成自环被跳过。

### 58.6 实测

| 项目 | 结果 |
|---|---|
| 非对称分支 THD @0.2 V | **27.58 %**（ngspice 基线 27.5682 %） |
| 对称分支 h2 | **0.000 %**（非对称 3.922 %）→ 开关真的换了拓扑 |
| 切换平滑性 | 跳变比值 **0.99**（不高于信号自身的削波边沿） |
| AU/VST3/CLAP | 两个变体均构建安装成功，au-smoke 通过 |

插件现在是 3 个参数：**Gain / Clip Mode（开关）/ Level**。
注意：Logic 的通用参数面板里 Clip Mode 是复选框（无 GUI 阶段）。

### 58.7 与设计器决策的对应

D-005 说"n 个开关 ⇒ 2ⁿ 份完整电路"。这次的 `Branch<A,B>` 正好是 n=1 的实现；
n=2 时用 `Branch<Branch<A,B>, Branch<C,D>>` 即可嵌套（每层各有自己的淡化），暂不需要改动。

---

## E59：进 GUI 之前的清单核对 + AB 复盘补全（2026-10-25）

用户要求"把进 GUI 之前的清单过一遍，补文档，特别是把 AB 测试的最后过程补全"。

### 59.1 新增 `docs/pre-gui-checklist.md`

把"从网表到可安装插件"拆成 9 节逐项核对，每项都给**可复核的证据**（测试名/命令/文件）：

| 节 | 内容 |
|---|---|
| 1 | 器件库（14 型号；动态档标为未做，不阻塞 GUI） |
| 2 | MNA 装配器与实时路径（自适应迭代、LU 裁剪、数值兜底） |
| 3 | 代码生成 S1a/S1b/S1c + 多分支 + 漂移守卫 |
| 4 | 插件外壳（身份/参数/三格式/单双声道） |
| 5 | 整机与 AB 结论 |
| 6 | 验证与 CI（84 用例、8 层 verify.sh、两层 CI） |
| 7 | **GUI 阶段会依赖的接口**（ParamAccess / PanelSpec / gui feature / 参数 id / 延迟对齐） |
| 8 | 未完成项按优先级（P1 taper、P1 动态档、P2 映射分层、P2 稀疏 LU、P3 打包矩阵） |
| 9 | 一条命令复核 |

结论：**GUI 只加"显示与交互"，不改 DSP、不改生成器、不改契约。**

### 59.2 AB 复盘补全（`docs/ab-test-sparklin-v2.md`）

原文停在 §9（newton 4→8）。补齐了三节：

- **§10 自适应迭代**：固定次数本身就是错的方子；配对数 3420→1132 ns（3.0×）、输出差 −227 dB、
  平均 2 次；并记下"LU 裁剪只值 1.09×"这个**推翻 E43 路线判断**的发现（瓶颈是每样本固定开销）。
- **§11 开关**：两份完整电路 + 预热 5 ms + 淡化 10 ms；非对称 THD 27.58 %、对称 h2 0.000 %、
  切换跳变比值 0.99；两个坑（冷状态要预热、判据要用比值）。
- **§12 最终状态**：把这次 AB 关掉的四个问题列成表（次谐波 / 偶次锯齿 / 端点静音 / CPU），
  并给出"同一台机器"的**可操作定义**：可听范围内差异小于一次电平校准误差，
  剩下的差别只出现在"任何两个求解器都不会一致"的地方。
- 顺带在 §7 顶部加了⚠️：那一批数据取自 `newton=4`，看当前数字请看 §11。

§13 把 13 条方法学收获浓缩成表（正弦查不出欠收敛、验收要含"不该出现的分量"、
参数端点是温床、数值层就是音色、对比信号不能各自归一化、分析器看趋势台架出数字、
CPU 必须配对测量、冷状态要预热……）。

### 59.3 README 同步

- 路线图：勾掉"实时路径优化（自适应迭代）"与"多分支（开关）"；
  把"预计算稀疏 LU"划掉并注明**已证明不是瓶颈**（E57 推翻了自己 E43 的判断）；
- 状态表加"进 GUI 之前 ✅ 清单全过"，下一步改为 **GUI（S1d）**；
- 三层表补 `branch.rs`，去掉"egui"（模板已无 GUI）。

### 59.4 当前规模

- Rust 代码 **8638 行**（4 个框架 crate + 生成器）；
- 测试用例 **84** 个，`verify.sh` 8 层全绿（含 4 层 ngspice 对拍）；
- 文档：README + `docs/` 5 篇（component-library / circuit-import / declarative-build /
  ab-test-sparklin-v2 / pre-gui-checklist）+ DEVLOG 59 节。

---

## E60：GUI 契约升级 —— 控件数组化 + 旋钮整图旋转（2026-10-25）

GUI 设计器开工前，先把运行时要支持的形态定下来。两条动议来自设计器评审（`rustomp-box-designer/docs/05-spec-review.md`）：

1. **解除 LED / 拨杆 / 按钮的数量限制**（原来是 `Option<T>`，各至多 1 个）；
2. **旋钮改为整图旋转**（原来是"底图上的旋钮本体 + 运行时画一根指针"）。

### 60.1 改动

| 项 | 之前 | 现在 |
|---|---|---|
| LED / 拨杆 / 按钮 | `Option<T>`，各 1 个 | **`&'static [T]`，不限** |
| 控件 id | `ui.id().with(param)` | **`with((kind, index))`** —— 两个控件绑同一参数也不会被 egui 合并 |
| 旋钮渲染 | 只有指针线 | `KnobRender::{Rotate, Pointer}`，**默认 Rotate** |
| 旋钮素材 | 压平进底图 | **独立贴图**（`KnobSpec.image` + `image_size`） |
| 跨类型叠放 | 固定类型顺序 | **固定且不可重排**，新增 `DRAW_ORDER` 常量：`panel → leds → knobs → toggles → buttons` |
| 校验 | 无 | **`validate_panel(spec, assets)`**：返回全部问题 |

### 60.2 两条新约定（写进契约）

**① 素材指针朝上 ⇔ 参数 0.5**
整图旋转时，"0° 指向哪"由素材决定。钉死这条约定后：

- 运行时**不需要** `pointer_angle_deg` 字段；
- 但反过来要求 `angle_min + angle_max == 0`（对称范围），否则指针会整体偏转；
- 设计器只暴露"总转角"（默认 270°），导出时写 `−travel/2 .. +travel/2`。

**② 锚点默认中心；开关锚点 = 六边形中心**
两态素材都按"六边形中心重合"归一化后，`y_up == y_down`
—— legacy 那个"基线差 1.53、切换时跳一下"的坑从机制上消失。

### 60.3 校验器拦住的四类错误

`validate_panel` 在构建/导出时就会报：

- `Rotate` 旋钮的角度范围不对称（指针会偏）；
- `Rotate` 旋钮缺贴图 / `image_size` 非法（旋转中心错）；
- 拨杆引用了不存在的贴图名（运行时那块是空白）；
- `press_scale` 不在 (0, 1]、`radius` 为 0（点不到）。

### 60.4 生成器与示例

- `plugin_meta.py --panel-rust` 改出数组 + `render` + `image`/`image_size`；
  支持 `travel_deg`（设计器友好）自动展开成对称的 `angle_min/max`；
- `plugin_meta.py --shell` 现在把 `[assets]` 的**所有键**导出成 `ASSET_<UPPER>`
  （旋钮贴图也走这张表）；
- 新增 `crates/rustomp-gui/examples/panel_check.rs`：把生成的 `PanelSpec` 字面量放进去，
  ① 编译期验证字面量写法 ② 运行 `validate_panel`。

```
✓ 面板声明通过校验：1 个旋钮 / 1 个 LED / 1 个拨杆 / 0 个按钮（画布 104×54 mm）
  固定绘制顺序：panel → leds → knobs → toggles → buttons
```

### 60.5 对旧代码的影响

- legacy 插件（`sparklin-drive-legacy`）用它自己快照里的 `rustomp-gui` 与 `plugin_meta.py`，
  **不受影响**（归档仓库不动）；
- 主仓库的 v2 插件当前是**无 GUI**，所以这次改动只是"把能力提前备好"；
- 真正的编译验证要等 S1d（把 `gui` feature 打开 + 手写 `editor.rs`）。

---

## E61：S1d —— 第一个带界面的插件（Sparklin Drive v2 面板）（2026-10-26）

无 GUI 那条链已经验证完了（E49–E60）。这一步把界面接上，用**新契约**（E60：数组化 + 整图旋转）。

### 61.1 关键决定：素材坐标改用**毫米**

legacy 面板的 SVG 单位是 **pt**（170.08 × 317.48 pt = **60 × 112 mm**，正好 1590B 竖放），
窗口 340×635 px（1 pt = 2 px = 144 dpi），导出 680×1269 px（1 pt = 4 px ≈ **288 dpi**）。

v2 改成 **1 素材单位 = 1 mm**（与 `rustomp.toml` 的 mm 化、`gb4728-symbols-kit` 的约定一致）：

| 项 | 值 |
|---|---|
| 面板物理尺寸 | 60 × 112 mm |
| 逻辑窗口 | `mm × 6 px/mm` = 360 × 672 |
| 面板素材 | `mm × 12 px/mm` = **720 × 1344**（= 304.8 dpi，比 legacy 高 6%） |
| 旋钮素材 | 22 × 22 mm = 264 × 264 px |

实测数据与"PNG 里的 dpi 字段是假的"这个坑，记在设计器仓库 `docs/03-assets.md` §6。

### 61.2 面板怎么来的（还没有设计器）

1. `panel.png`：**Inkscape 直接渲染 legacy 的 `panel.svg`**，`--export-width=720` → 720×1344（12 px/mm）；
2. `switch_up/down.png`：复用 legacy 的两张（95×101 px ≈ 289 dpi，同样清晰度）；
3. `knob.png`：**程序生成**（Python 手写 PNG：深色圆 + 奶白描边环 + 朝上的指针，4× 超采样抗锯齿）
   —— 指针朝上 ⇔ 参数 0.5，符合 E60 的约定。

坐标从 legacy 的 pt 换算到 mm（×0.352778）：旋钮 (15.32, 25.26)/(44.64, 24.85) mm、
LED (30.00, 11.94) mm r=3.0、拨杆 x=25.93 / y_up=38.26 / y_down=38.80 / pivot=(30.10, 42.98) mm。

### 61.3 生成器补了两块

- `rustomp-gen` 在 `gui = true` 时，`params.rs` 额外生成 `#[persist = "editor-state"]
  editor_state: Arc<EguiState>`（窗口尺寸记忆），初始尺寸取 `[ui]` 的 `window_*`；
- `mod.rs` 同时生成 `pub mod panel;`，于是 `plugin/src/lib.rs` 依旧只有 `mod generated;`。

`panel.rs` 目前由 `tools/meta/plugin_meta.py --panel-rust` 生成（**入库**，与 legacy 同一套脚本）；
把这一步搬进 `rustomp-gen` 是后续 TODO（避免两套发射器）。

### 61.4 手写的只有 `editor.rs`（约 100 行）

参数 id ↔ nih-plug 参数的桥（`ParamAccess`）+ `create_egui_editor`：

```rust
fn get(&self, id: &str) -> Option<f32> {
    match id {
        "gain" => Some(self.params.gain.value()),
        "level" => Some(self.params.level.value()),
        "symmetric" => Some(if self.params.symmetric.value() { 1.0 } else { 0.0 }),
        "clip_indicator" => Some(self.activity),   // 虚拟 id：削波指示
        _ => None,
    }
}
```

`lib.rs` 增加：

```rust
#[cfg(feature = "gui")]
mod editor;
```

`plugin/Cargo.toml` 加 `nih_plug_egui` + `rustomp-gui`，并设 `default = ["gui"]`（无界面构建用 `--no-default-features`）。

### 61.5 验证

| 项 | 结果 |
|---|---|
| 编译 | `cargo check -p sparklin-drive`（含 GUI）通过 |
| 面板自洽性 | `validate_panel` 通过：2 旋钮 / 1 拨杆 / 1 LED，画布 60×112 mm |
| 构建安装 | mono + stereo 均成功（Sp02 / Sp03） |
| AU 界面能力 | `au-smoke`：**提供自定义界面（CocoaUI）** ✓ |
| 音频 | 渲染 512 帧：非有限值 0、RMS 0.356170（与无 GUI 版完全一致） |

### 61.6 还没做的（清单）

| # | 事项 |
|---|---|
| 1 | `panel.rs` 由 `rustomp-gen` 生成（去掉对 python 的依赖） |
| 2 | `template/` + `new-plugin.sh` 支持 `--gui`（现在要手工改 Cargo.toml + 写 editor.rs） |
| 3 | 面板素材仍复用 legacy 的美术；等 box-designer 出来再做原创面板 |
| 4 | 拖动/滚轮手感与 legacy 对齐（`drag_range`、双击回中）——先看用户的实机反馈 |

---

## E62：S1d 联调 —— 三个界面 bug 的根因与修复（2026-10-26）

用户装上第一个带界面的 v2 后报三个问题：**LED 不闪 / 踩钉按不下去 / 旋钮边缘像被切过**。
三个都定位到了具体代码，且都属于"契约没接全"，不是美术问题。

### 62.1 LED 不闪：`Ckt::activity()` 走的是 trait 默认值 0

引擎侧链路本来是全的：`Circuit::activity()` → `Engine` 的"快起慢落"包络 → `activity_indicator()`
→ 生成代码写进 `AtomicU32` → GUI 的 `clip_indicator`。

但**生成的 `Ckt` 没有实现 `activity()`** → 走 trait 默认返回 `0.0` → 灯永远不亮。

修复：`mna_codegen` 生成实现——

```rust
// process() 末尾：网表里所有二极管正向电流的最大值
let mut imax = 0.0f64;
for e in &self.net.elems {
    if let rustomp_dsp::Element::Diode { model, a, k } = e { … }
}
self.activity = (imax / ACTIVITY_I_REF).clamp(0.0, 1.0) as f32;
```

`ACTIVITY_I_REF` 做成**网表指令** `* @rustomp activity_i=50u`（默认 50 µA）：
不同电路的二极管电流量级差很多 —— 本电路 0.2 V 输入时只有 ~30 µA，
一开始写死 1 mA 时活动量只有 0.032（灯等于不亮）。改成 50 µA 后 **0.647** ✓。
**这个量只驱动 LED，不影响 DSP**。

### 62.2 踩钉按不下去：根本没有 bypass 参数

面板上画了 3PDT，但 `rustomp.toml` 里**没有 `[[ui.buttons]]`**（因为插件没有 `bypass` 参数），
所以运行时那块只是美术。

修复三处：

1. `Engine` 增加 **`set_bypass(bool)`** + `is_bypassed()`：
   复用原有的 5 ms 交叉淡化与 15 样本干路对齐，只把 `mix` 的目标从写死改成可设；
   顺带修了"指数淡化有无穷长尾巴"——到位后吸附（`< 1e-5`）。
2. `rustomp-gen`：`bypass` 是**保留参数 id** —— 不进 `make_params`，而是在 `process()` 开头
   `for e in engines { e.set_bypass(...) }`。
3. `rustomp.toml` 加 `[[params]] id="bypass"` + `[[ui.buttons]]`（3PDT，center/r=5.4mm）。

新增回归测试 `bypass_passes_dry_signal`：淡出后与"延迟 15 样本的干信号"逐点比较 →
**偏差 0.00e0**；切回后又与输入不同（确认回到湿路）。

### 62.3 旋钮边缘像被切过：`decode_png` 把 alpha 提前与固定底色合成

`decode_png` 原本对 RGBA 做 `alpha × (0xfa,0xf1,0xe0)` 混合并输出**不透明**像素。
贴图是叠在面板美术上的，提前按某个固定底色合成 = 贴图边缘的像素永远对不上下面的东西，
于是圆形旋钮看着像带一个方形"底"、边缘不干净。

修复：保留 alpha，交给 GPU 混合（`Color32::from_rgba_unmultiplied`）。
**教训**：贴图解码不要"替 GPU 做混合"，尤其是不知道下面是什么的时候。

### 62.4 面板素材也换了（用户提供的新 SVG）

用户把新素材放进 `rustomp-box-designer`：`knob_round_dark.svg`（15 mm，指针朝上）、
`3pdt.svg`（10.76 mm）、`led_{red,green,blue}.svg`（6.27 mm）。全部按 **mm 画**，与我们的坐标系天然一致。

- `knob.png`：Inkscape `--export-width=180`（15 mm × 12 px/mm）；
- `panel.svg`：脚本把 3PDT 与 LED 的图层**内联**进 60×112 mm 的面板（含丝印：LEVEL/CLIP/GAIN、
  Sparklin Drive、BYPASS、HONEYPIE EFFECTS），再 `--export-width=720`（12 px/mm）；
- 旋钮**不进底图**（运行时整图旋转），面板上只留虚线刻度环。

### 62.5 验证

| 项 | 结果 |
|---|---|
| 电路测试 | 6 个全过（新增 activity / bypass 两个） |
| 活动量 | 静音 0.000 → 削波 **0.647** |
| bypass | 与干路偏差 **0.00e0**；切回恢复湿声 |
| AU | CocoaUI ✓、纯音频 ✓、渲染 RMS 与无 GUI 版一致（0.356170） |

---

## E63：全项目复盘 + 平台策略（2026-10-26）

从 E35（器件库第一块）到今天，项目走完了「器件 → 装配器 → 实时路径 → 代码生成 → 界面」整条链。
这一节做两件事：**把整条链的状态钉在文档里**，以及**定下平台策略**。

### 63.1 现在的链路（一条命令各段都能复核）

```text
网表 .cir ──rustomp-import --mna──► circuit/src/generated/*.rs
                                       （Netlist + Params + impl Circuit + activity）
rustomp.toml ──rustomp-gen──► plugin/src/generated/{identity,params,plugin_impl,mod,panel}.rs
                                       （身份/参数表/Plugin 实现/模块汇总/面板声明）

两者 + Engine + editor.rs（手写 ~100 行）+ scripts/build-all.sh ──► AU / VST3 / CLAP
```

| 块 | 状态 | 关键证据 |
|---|---|---|
| 器件库 14 型号 | ✅ | 二极管 0.00%、BJT ≤0.09%、JFET 0.00%、MOS ≤0.01%、运放 <2% |
| MNA 装配器 | ✅ | 整机 DC µV 级、瞬态 17 mV、THD 27.579% vs ngspice 27.5682% |
| 实时路径 | ✅ | 自适应牛顿（上限 8 / 残差 1e-6）、平均 2 次、方波次谐波 −70 dB |
| 代码生成 | ✅ | S1a/S1b/S1c/S1d 全通；生成物有漂移守卫 |
| 多分支（开关） | ✅ | `Branch<A,B>`，预热 5 ms + 淡化 10 ms，对称分支 h2 = 0.000% |
| 界面 | ✅ | 毫米坐标 60×112 mm、12 px/mm、旋钮整图旋转、多控件、LED 活动量、true-bypass |
| 验证 | ✅ | **91 个用例 / verify.sh 8 层**（4 层 ngspice 对拍） |
| 规模 | — | Rust 9139 行（5 个 crate）、文档 6500+ 行、74 个提交 |

### 63.2 平台策略（ADR-015）

**rustomp 默认只面向 macOS。** 其他平台不走"本仓库交叉编译"，而是**独立打包工具仓库**：

```text
rustomp（框架 + 构建脚本，macOS 原生）
   ├── rustomp-win-plugin    （Windows 打包：VST3/CLAP + standalone/安装包）
   └── rustomp-linux-plugin  （Linux 打包：VST3/CLAP + standalone/包）
```

开发者想要哪个平台，就把对应仓库拉下来接进现有工作流（它是**打包层**，不碰 DSP/电路/代码生成）。

为什么这样做：

1. **DSP 与打包天然解耦**：`Circuit` / `Engine` / 代码生成都是纯 Rust，跨平台编译不是难点；
   难点在**打包与宿主兼容**（clap-wrapper + CMake + 签名 + 各种宿主缓存），每个平台都要独立验证。
2. **省时间**：现在只有 macOS 可验证；Windows/Linux 的打包脚本写出来也没有环境验证，等于猜。
3. **ADR-014 已经预留了契约**：`scripts/pack/<platform>/<format>.sh` 成功打印 `ARTIFACT: <路径>`。
   外部打包工具只要满足这个契约，就能无缝接上。

所以本仓库的定位收敛为：**框架 + macOS 打包脚本 + 契约**。
跨平台能力靠"可插拔的打包工具"扩展，而不是靠仓库内条件编译。

### 63.3 复盘：这轮最值钱的七条经验

| # | 经验 | 出处 |
|---|---|---|
| 1 | **正弦测试查不出欠收敛**，必须补方波/阶跃 | E50 |
| 2 | 验收要包含"**不该出现的分量**"（次谐波/杂散/NaN/DC） | E50 |
| 3 | **参数端点是 bug 温床**（`t=0 / t=1` 先算一遍） | E51 |
| 4 | **数值实现层就是音色的一部分** | E52 |
| 5 | 对比信号**不能各自归一化** | E55 |
| 6 | **冷状态要先预热**再淡入；反复出现的"切换咔哒" | E58 |
| 7 | 贴图**不要替 GPU 做 alpha 混合**；CPU 数字必须**配对测量** | E62 / E57 |

### 63.4 文档盘点（这次逐一校对）

| 文档 | 这次改了什么 |
|---|---|
| `README.md` | 状态表补 GUI/平台策略；结构补 `rustomp-gen`/`rustomp-gui`；路线图重写 |
| `docs/component-library.md` | 状态改"已实现"；补动态档现状与已知偏差 |
| `docs/circuit-import.md` | 指令表补 `newton=`/`activity_i=`；标注 `switch` 为 legacy；补多分支导出 |
| `docs/declarative-build.md` | S1c/S1d 状态；新增 §4.0 **平台策略**；平台矩阵改"外部工具" |
| `docs/pre-gui-checklist.md` | 标注"GUI 已开工"，清单转为验收基线 |
| `crates/rustomp-dsp/README.md` | 补 `Branch`、`set_bypass`、自适应迭代、`activity()` |
| `crates/rustomp-import/README.md` | 补新指令与"生成物含 activity" |
| `crates/rustomp-gen/README.md` | 补 gui=true 的行为、`bypass` 保留 id、`params_builder` |
| `crates/rustomp-gui/README.md` | **新增**：界面层契约（面板声明/绘制顺序/贴图/分辨率） |

### 63.5 下一步

**box-designer 开发**（GUI 设计器）：契约已由真实面板验证过（E61/E62），
`rustomp-box-designer` 里的 `04-open-questions.md` 还差 Q4（控件能力）等几条拍板。

---

## E64：box-designer D0 落地 —— 契约第一次被"另一个程序"消费（2026-10-26）

`rustomp-box-designer` 从"只有文档"变成"能用"：零依赖浏览器端设计器，
画布编辑 + L1 预览 + 导出 `panel.svg` / `panel.png` / 控件贴图 / `[assets]`+`[ui]` 标记块。
用设计器复刻了真实的 Sparklin Drive 面板（`examples/sparklin.box.json`），
导出的 `panel.png` **720×1344**、`knob.png` **180×180** —— 与手搓那份逐像素同规格。

### 64.1 契约经受住了检验（这次是真的"第二份实现"）

`[ui]` 的字段一个都没改：设计器照着 `docs/01-contract-panel-toml.md` 直接产出
`center = [x, y]`、`image_size`、`travel_deg`、`pivot`、`off_color`…，
`rustomp` 侧不需要任何改动。**这是契约第一次被独立实现消费**，
比"自己写自己验证"有力得多。

### 64.2 两条运行时行为必须被"照抄"，而不能"看起来差不多"

| 行为 | 设计器必须复刻的细节 | 不照抄的后果 |
|---|---|---|
| 旋钮 | 指针朝上 ⇔ 参数 0.5，`angle = (v-0.5) × travel` | 设计器里居中、装上插件偏 135° |
| 脚踏按下 | **先用背景色方块抹掉原位，再把原美术按 `press_scale` 居中贴回** | 设计器里"缩小"，宿主里出现双影 |

### 64.3 一个框架侧的小结论：`buttons.radius` 必须盖住美术

按下时是用 `2r × 2r` 的**方块**擦除，所以 `radius` 必须 ≥ 美术外接圆半径，
否则原美术会露出一圈（3PDT 外圈 r=5.38 mm，`radius` 取 5.4 才干净）。
已写进 `crates/rustomp-gui/README.md`。

> 顺带记下可改进点（不急）：擦除用方块、按钮是圆的，几何上不匹配；
> 面板若在按钮下面放了渐变/图案，擦出来的方块会露馅。
> 真要修的话是"圆形擦除 + 圆形裁剪贴回"，需要改运行时并重新验一遍 AU。

### 64.4 素材层面的三个坑（设计器侧解决，写进 box-designer D-006）

1. 素材 path 坐标常常远在 viewBox 之外（标签甚至有 `M253,393`）→ 内联必须用
   **嵌套 `<svg viewBox>`**，`<g transform>` 会把图形甩到面板外；
2. 每个素材都有 `.cls-1/.cls-2` → 内联前把 `<style>` **展开成内联 style**，否则互相覆盖；
3. **自闭合标签的 style 加在 `/` 之后会整条被丢弃** → 圆变黑块
   （这个 bug 是"导出图里 LED 是个黑圆"暴露出来的，已加回归测试）。

### 64.5 下一步

D1：把设计器产物真正接进插件项目（替换手搓的 `panel.png`/`knob.png`），
并补设计器的 D1 功能（安全边距/对齐吸附/图层显隐）。

---

## E65：D0.5 —— 设计器产物接进真插件（2026-10-26）

box-designer 的导出第一次进发布路径：Sparklin Drive 的 `panel.png` / 旋钮贴图 / 拨杆两态贴图
全部改成**设计器产物**，`[assets]` + `[ui]` 也是设计器生成的整块。

### 65.1 换掉了什么

| 之前（手搓） | 现在（设计器） |
|---|---|
| `panel.png`（脚本拼 SVG → 720×1344） | `panel.png`（`.box.json` → 同样 720×1344） |
| `knob.png`（程序画的 264×264 指针图） | `knob_round_dark.png`（用户素材，**180×180 = 15 mm × 12 px/mm**） |
| `switch_up/down.png`（legacy 美术） | `toggle_small_up/down.png`（用户素材 8.35×8.89 mm） |
| 丝印是 Helvetica 文字（靠宿主字体渲染） | 丝印是 `label_*.svg` **矢量 path**（不依赖字体） |

音频路径完全没动：AU 自检的 RMS 与之前**逐位相同**（0.356170），CocoaUI 正常。

### 65.2 顺手补的一块：素材字节由生成器负责

`editor.rs` 以前写死四行 `include_bytes!` + 一个 `IMAGES` 数组，素材改名就得改手写代码。
现在 `rustomp-gen` 多生成一个 **`generated/assets.rs`**：

```rust
pub static PANEL_PNG: &[u8] = include_bytes!("../../../assets/panel.png");
pub static IMAGES: &[(&str, &[u8])] = &[
    ("knob_round_dark", include_bytes!("../../../assets/knob_round_dark.png")),
    …
];
```

于是 `editor.rs` 只剩一行 `use crate::generated::assets::{IMAGES, PANEL_PNG};`——
**设计器改名/加图后重新生成即可，不用碰手写文件**。

两个必须注意的点（都加了测试）：

1. `include_bytes!` 相对**源文件**：生成物在 `plugin/src/generated/`，素材相对项目根 → 必须是 **`../../../`**；
   一开始写成 `../../` 直接 "file not found"。
2. 生成器现在会**先检查素材文件是否存在**，缺文件在生成阶段报错（而不是等 cargo 才炸出 include 路径）。

### 65.3 一个 Python 生成器的坑：整数不是 f32

设计器的 `ui.toml` 写 `svg_width = 60`（整数），而 `--panel-rust` 直接原样吐出 `svg_w: 60,`
→ Rust 报 `expected f32, found integer`。修法是在 `plugin_meta.py` 里统一用 `rf()` 格式化浮点
（`60` → `60.0`）。**TOML 里 `60` 和 `60.0` 是两个类型，生成器不能假设。**

### 65.4 验证

| 项 | 结果 |
|---|---|
| `cargo test -p rustomp-gen` | 14 个（新增 3 个：assets.rs 的路径层级 / 空素材 / mod.rs 模块开关） |
| 插件面板自检 | `validate_panel` 通过（2 旋钮 / 1 拨杆 / 1 LED / 1 脚踏） |
| `build-all.sh` | Sp02 / Sp03 重新构建安装 |
| AU 自检 | CocoaUI ✓、纯音频 ✓、RMS **0.356170**（与无界面版、与手搓面板版都一致） |
| `verify.sh` | 8 层全绿 |

---

## E66：踩钉不旁通 / LED 不灭 —— 手写参数桥漏了一个分支（2026-10-26）

用户报两个现象：**踩下 3PDT 不 bypass**、**bypass 后 LED 还亮着**。
两个根因都很具体，而且第一个是"设计缺陷"而不是"笔误"。

### 66.1 根因一：手写的 `ParamAccess` 桥漏了 `bypass`

GUI 点按钮走的是 `params.set("bypass", 1.0)`，而这个桥是**手写的 match**：

```rust
fn set(&self, id: &str, value: f32) {
    match id {
        "gain" => …,
        "level" => …,
        "symmetric" => …,
        _ => {}          // ← bypass 掉进这里，静默无操作
    }
}
```

`get`/`set` 各少一支**不会编译失败**，只是"点了没反应"。这类 bug 靠 review 抓不住，
所以修法不是补一行，而是**把桥也交给生成器**：

```text
[[params]]  →  generated/bridge.rs  →  impl ParamAccess for ParamBridge
```

生成器知道所有参数（含类型），`bool` 走 `value > 0.5`，`clip_indicator` 是虚拟只读 id。
从此**加参数不用改手写代码**，也不会漏。

回归测试：`bridge_covers_every_param_in_get_and_set` —— 每个参数 id 在桥里必须出现 ≥2 次。

### 66.2 根因二：旁通时电路还在跑，活动量照样高

true-bypass 只把输出渐变到干路，**电路本身仍在处理**（这是设计如此：切回来要无缝）。
于是 `activity_indicator()` 在旁通期间仍然是满的 → 灯照旧闪。

修法：指示电平乘上旁通权重

```rust
pub fn activity_indicator(&self) -> f32 {
    self.activity_env * self.mix as f32   // mix → 0 时灯自然灭
}
```

顺带把"灯熄灭"和"声音切换"绑在同一个 5 ms 渐变上，观感一致。
测试：`bypass_passes_dry_signal` 里加断言 —— 旁通后活动量必须 < 0.01（实测 **0.000**）。

### 66.3 顺带做掉：`panel.rs` 的生成搬进 `rustomp-gen`

设计器契约落地后一直有两个发射器（`plugin_meta.py --panel-rust` + `rustomp-gen`），
现在统一到 `rustomp-gen`：`[ui]` → `generated/panel.rs`。`plugin_meta.py --panel-rust`
保留一行**明确报错**（防止旧脚本静默产出第二份）。

同时 `build-all.sh` 开头加了一步"重新生成插件代码"，避免"改了 toml 但忘了跑生成器"。

### 66.4 新增：脚踏的"按住态"贴图

设计师给了 `3pdt_alt-hold.svg`（同画布、内圈更小 = 按下感）。
`ButtonSpec` 增加 `image_hold: &'static str` + `image_size`：

- 有按住态 → 按下时**整张换上**（不缩放）；
- 没有 → 退回旧行为（从底图抠那块缩小贴回）。

预览里也复刻同一套逻辑（`src/exporter.js::buttonPress`），所以设计器与宿主一致。

### 66.5 验证

| 项 | 结果 |
|---|---|
| `cargo test -p rustomp-gen` | **16 个**（新增：桥覆盖所有参数、panel.rs 浮点/hold 字段） |
| 电路测试 | 6 个；bypass 后活动量 **0.000** |
| `build-all.sh` | 重新生成 → Mono/Stereo 构建安装 |
| AU 自检 | CocoaUI ✓、RMS **0.356170**（不变） |
| `verify.sh` | 8 层全绿 |

---

## E67：用设计器导出的文件重做 GUI —— 交接流程实测（2026-10-27）

第一次**按用户视角**走完"设计器 → 插件"：只拿 `ui.toml` + `panel.svg` 两个文件，重做 Sparklin Drive 的界面。

### 67.1 实际做了什么（6 步，全程机械操作）

| 步 | 动作 | 结果 |
|---|---|---|
| 1 | `[assets]` + `[ui]` 整段替换进 `rustomp.toml` | 15 个段落，其余段未动 |
| 2 | `panel.svg` → `panel.png`（Inkscape `--export-width=720`） | 720×1344 = 12 px/mm |
| 3 | 补齐 4 张控件贴图（`knob_round_dark` / `toggle_small_up|down` / `3pdt_alt_hold`） | **导出弹窗里没有这些**，要靠 `export.mjs` 或手工栅格化 |
| 4 | `rustomp-gen` | 只重写了 `panel.rs`（其它 6 个文件未变） |
| 5 | `cargo test -p sparklin-drive --features gui` | `validate_panel` 通过 |
| 6 | `build-all.sh` + `au_smoke` | CocoaUI ✓、RMS **0.356170**（与之前逐位相同） |

### 67.2 交接时发现的两处"自动了"的差异

替换前 diff 一下旧块与新 `ui.toml`，只有两行变了：

```diff
- radius = 2.249     # LED
+ radius = 2.247
- radius = 7.2       # 脚踏
+ radius = 7.175
```

这不是手改的，而是 D-014 之后**半径改成由素材推导**：

- LED `radius` = 素材镜片内圈（`r_inner_mm`）
- 脚踏 `radius` = 美术宽/2（`14.35 / 2 = 7.175`）

也就是说，**`[ui]` 里这两个数不再需要人维护**，素材一换它们自己就对。

### 67.3 一个环境坑：终端读不到 `~/Downloads`

macOS TCC 会直接拒绝（`Operation not permitted`），Finder 自动化也没授权。
所以"下载 → 命令行处理"这一步天然断裂。两个对策：

1. 导出后立刻 `cp` 到项目目录或 `~/develop/inbox/`；
2. 或者干脆用 `scripts/export.mjs` 直接输出到插件 `assets/`（生成物逐字节相同）。

已写进 runbook（`rustomp-box-designer/docs/06-export-runbook.md`）的「关于文件在哪」。

### 67.4 结论

- **交接是可重复的**：6 步、无手改、无试错；
- 唯一"缺件"是控件贴图 —— 因为弹窗只导出矢量（`ui.toml` + `panel.svg`），
  PNG 贴图属于"素材导出"而非"布局导出"，runbook 里已明确列出；
- 音频侧零影响（RMS 逐位相同），说明界面与 DSP 的边界仍然是干净的。

---

## E68：拿用户真实下载件走完整安装（2026-10-27）

用户把设计器导出的 `sparklin-drive-assets.zip` 放进仓库，这次**完全按交付路径**装了一遍，
并且这是**用户自绘**的设计：手写体 `Sparklin Drive` logo、`HONEYPIE EFFECTS` 字标、
两个小三角（都是**外部导入的 SVG**），删掉了原来的 CLIP 标签。

### 68.1 装了什么（按顺序）

| 步 | 动作 | 结果 |
|---|---|---|
| 1 | 解压 zip | **8 个文件**：`panel.{svg,png}` + 4 张贴图 + `ui.toml` + `panel.box.json` |
| 2 | 校验尺寸 | `panel.png` 720×1344、`knob` 240×240、`toggle` 100×107、`3pdt_alt_hold` 172×172（都是 mm × 12） |
| 3 | 拷进 `plugins/sparklin-drive/assets/` | 底图 shasum 与 zip 里那份**逐字节相同** |
| 4 | `[assets]`+`[ui]` 换成用户的 `ui.toml` | 与上一版**完全一致**（diff 只有标记行） |
| 5 | `rustomp-gen` | **已写入 0 个文件（7 个未变）** |
| 6 | `cargo test --features gui` | `validate_panel` 通过 |
| 7 | `build-all.sh` | Mono/Stereo 重装，1m25s |
| 8 | `au_smoke` | CocoaUI ✓、RMS **0.356170**（与所有历史版本逐位相同） |

### 68.2 关键观察：美术改动只动 `panel.png`

第 5 步 `rustomp-gen` **一个文件都没写**，因为用户这次只改美术：

- 加了 3 个导入 SVG（logo / 字标 / 三角）
- 标签位置从 `y=35.3` 挪到 `y=38`、删了 CLIP
- 控件的 `[ui]`（旋钮中心/行程、拨杆两态、LED、脚踏）一个字没动

也就是说**美术层与控件层彻底解耦**：重新导出不会顺手把布局改动踩掉，
反之改布局也不会动底图。这是设计器分层（D-003）真正兑现的地方。

### 68.3 修掉一个只在 CLI 侧出现的 bug

`scripts/export.mjs` / `preview.mjs` 之前只加载内置素材清单，
遇到设计稿里的 `imported[]` 直接崩（`素材缺少 viewBox` / `素材文件读不到：imp_*`）。
浏览器路径（`mergedAssets`）一直是对的，所以**只有 CLI 会炸** —— 典型的"两条实现分叉"。

已修（box-designer `fc1d059`，D-016），并记下：

> 浏览器 canvas PNG 与 CLI Inkscape PNG **像素尺寸一致、字节不同**。
> 交付给人用 → 浏览器 zip；要让 CI 逐字节可复现 → CLI。

### 68.4 现在的插件长什么样

用户设计的效果（底图 + 运行时叠加的旋钮/拨杆/LED）：

- 顶部：红色指示灯（削波指示）
- 上部：两颗 20 mm 圆钮（LEVEL / GAIN），标签是矢量字
- 中部：小拨杆（Clip Mode），下面整行手写体 `Sparklin Drive`
- 下部：14.35 mm 3PDT（带按住态）
- 右下：`HONEYPIE EFFECTS` 字标 + 两个三角装饰

`build/au_smoke` 的 RMS 与无界面版、与历次面板版**完全一致** —— 界面确实一点没碰音频。

---

## E69：生态边界成文（`docs/ecosystem.md`，2026-10-27）

拆仓前先把"谁负责什么"写成文档。218 行，一页说清六件事：

1. **仓库地图**：`rustomp`（框架）/ `rustomp-mac|win|linux-plugin`（打包）/
   `sparklin-drive`（参考插件）/ `rustomp-box-designer` / `rustomp-circuit-designer` /
   `sparklin-drive-legacy`（冻结），以及它们之间的依赖方向（只有两种：git tag 依赖、产物单向导出）。
2. **三个契约**：`rustomp.toml`（清单）、packer 的 `ARTIFACT:` 行、设计器的 `ui.toml` + 素材。
   除此之外跨仓不共享任何东西。
3. **版本约束**：`rustomp.toml` 新增 `[requires]`（framework/packer/designer），
   `rustomp-gen` 与 packer 各校验一次 —— 不匹配明确报错，而不是产出"能编但行为不同"的插件。
4. **文件归属表**：判断标准一句话 —— *提到 clap-wrapper / codesign / CMake / Logic / .component 的文件就该去 mac 工具*。
5. **拆分顺序**：tag + 契约可执行测试 → 拆 mac 打包 → 独立插件仓 → 电路设计器 D0，每条都有完成标准。
6. **已知风险**：nih-plug 是 git rev 依赖、universal 要两个 target、**standalone 其实还没做**（v0.1 先不出）、
   现在只是 ad-hoc 签名（公证单列）、`plugins/` 是 gitignore 却是 workspace member。

另外把「拆仓期纪律」写死了：**只搬不改**；要改契约就在第 0 步做掉。

### 69.1 第 0 步为什么值得花 30 分钟

ADR-014 早就写了 packer 契约，但**没有任何代码验证它**。文档里不成立的事情，
拆仓后一定会变成"mac 工具偷偷知道 Logic 的缓存路径"。所以第 0 步的产物是一个 30 行的
`tests/packer_contract.sh`：假插件 + 假 packer，断言入参、`ARTIFACT:` 行、以及
**框架源码里不许出现平台专属词**。有了它，"加一个 Windows 打包工具"就是加一个脚本，而不是改框架。

---

## E70：拆仓第 0 步 —— 工作区自洽，并发现"插件不能住在本仓里"（2026-10-27）

按 `docs/split-plan.md` 的 §0 开工。做完三件事，其中第三件是**计划里没有的**、但影响后续所有步骤。

### 70.1 全新 clone 解析不了 workspace（已修）

```text
$ git clone … /tmp/c1 && cd /tmp/c1 && cargo metadata
error: failed to load manifest for workspace member `/private/tmp/c1/plugins/sparklin-drive/plugin`
```

根 `Cargo.toml` 的 `members` 里有插件，而 `plugins/` 在 `.gitignore` 里。

修法：根 workspace 去掉两个插件成员 + `exclude = ["plugins"]`；插件项目自带 `Cargo.toml` workspace
（`members = ["plugin","circuit"]` + `[workspace.package]` + `[profile.release]`）。

### 70.2 连带修好的四处

| 位置 | 原来 | 现在 |
|---|---|---|
| `plugin/Cargo.toml` / `circuit/Cargo.toml` | `path` / `workspace = true` 混用 | 统一走插件自己的 `[workspace.dependencies]` |
| `scripts/gen-bundler-toml.sh` | 写 `rustomp/bundler.toml` | 写**插件项目根**的 `bundler.toml`（xtask 从 workspace 根读） |
| `scripts/build-variant.sh` / `build-au.sh` | 在 rustomp 根 `cargo nih-plug bundle-universal` | 在**插件项目根**构建；产物路径 `$PLUGIN_DIR/target/bundled/…` |
| `scripts/plugin-env.sh` | 只认 `plugins/*` | 新增 `$RUSTOMP_GUEST` 与 `--print-dir`（供脚本探测） |

### 70.3 真正的坑：`cargo-nih-plug` 会 chdir 到"最顶层"的 `Cargo.toml`

修完 workspace 后构建报 `cannot specify features for packages outside of workspace`。
翻 `nih_plug_xtask` 源码才看清：

```rust
let workspace_root = project_dir.ancestors()
    .filter(|dir| dir.join("Cargo.toml").exists())
    .last();                        // ← 取**最顶层**那个
std::env::set_current_dir(workspace_root)?;
```

插件住在 `rustomp/plugins/<name>/` 时，祖先里最顶层的是 `rustomp/Cargo.toml`，
于是 xtask 把 cwd 切到 rustomp 根 —— 那里没有这个包，`-p` 自然"outside of workspace"。

**结论：插件项目必须住在 rustomp 目录树之外。** 这条直接影响三件事：

1. `target/` 落在**插件项目根**（不是 `plugin/`）；
2. `bundler.toml` 必须在插件项目根；
3. `new-plugin.sh` 必须生成到仓外（`$RUSTOMP_GUEST`，默认 `../<slug>`），
   并在模板里按实际相对路径填 `rustomp` 的 path 依赖（`../rustomp/crates/…`）。

于是把 `plugins/sparklin-drive` 移到 `~/develop/sparklin-drive`（同级），
这恰好是拆仓后的最终位置 —— **提前到位，不是绕路**。

### 70.4 顺带一个 bash 坑（记过的老毛病又犯）

```bash
echo "bundler.toml → $OUT（$PLUGIN_NAME）"
# 报：OUT（: unbound variable
```

中文括号紧跟变量名时，bash 把全角字符算进了变量名。**`${VAR}` 一律加花括号**（AGENTS 里的第 2 条）。

### 70.5 验证

| 项 | 结果 |
|---|---|
| 全新 clone `cargo metadata` | ✅ 通过 |
| 框架自测 `cargo test --workspace` | ✅ 19 个测试目标全绿 |
| 插件自测（自己 workspace） | ✅ 电路 6 个用例通过 |
| `RUSTOMP_GUEST=… ./scripts/build-all.sh` | ✅ Sp02 / Sp03 重装，au_smoke 通过（1m14s） |
| `new-plugin.sh "Gain Test"` | ✅ 生成到 `../gain-test`，`cargo check` 通过（模板补了 workspace 根与 `repository` 字段） |

---

## E71：拆仓第 1 步 —— macOS 打包独立成 `rustomp-mac-plugin`（2026-10-27）

框架里最后一块"平台专属"的东西搬走了。`rustomp` 现在只剩 DSP / 代码生成 / 面板契约。

### 71.1 搬走了什么

| 去 `rustomp-mac-plugin` | 说明 |
|---|---|
| `scripts/{build-variant,build-au,check-au,gen-bundler-toml,fetch-third-party,reset-logic-cache}.sh` | 打包流程 |
| `cmake/CMakeLists.txt`、`patches/*.patch` | clap-wrapper → AUv2 与 AUv2 host classification 补丁 |
| `tools/au-smoke/`（C）、`tools/clap-probe/` | macOS 专属验证工具 |
| `third_party/`（12 MB 缓存） | clap-wrapper / CLAP SDK / AudioUnitSDK |

`rustomp` 侧 `scripts/build-all.sh` 变成**瘦包装**：找不到打包工具就打印 clone 命令，绝不自己编。

### 71.2 新增的两个东西

1. **`pack.sh`（契约 B 的实现）**

   ```bash
   ./pack.sh <plugin_dir> [--variant mono|stereo|dual]... [--format au|vst3|clap]... [--no-install]
   # 成功：逐行 ARTIFACT: <绝对路径>
   ```

2. **`tests/packer_contract.sh`（契约 B 的可执行测试）** —— 假插件 + 假 packer，验证三件事：

   - 插件目录与其余参数**原样透传**（`--variant stereo --no-install` 都到得了 packer）；
   - `ARTIFACT:` 行**原样透出**；
   - **框架源码里不许出现平台专属词**（`clap-wrapper|codesign|CMakeLists|.component|Logic`，
     只扫代码与脚本、跳过注释与文档）。

   它已经进 `verify.sh` 的快速层。以后加 Windows/Linux 打包工具，就是"再多一个符合契约的脚本"。

### 71.3 拆分时的三个适配点

| 问题 | 处理 |
|---|---|
| 打包工具要读 `rustomp.toml`，但 `plugin_meta.py` 在框架里 | `plugin-env.sh` 改为从 `$RUSTOMP_ROOT` 取（自动探测 `<plugin_dir>/../rustomp`），失败时给明确提示 |
| `bundler.toml` 该写在哪 | 写**插件项目根**（cargo-nih-plug 的 workspace 根），不是框架目录 |
| `--no-install`（CI 只想要产物） | `build-variant.sh` 支持；`pack.sh` 透传 |

另外给插件项目加了 `[requires]`（`framework/packer/designer`），`pack.sh` 启动时校验 `packer` 版本。

### 71.4 验收（拆前 / 拆后对比）

| 项 | 拆前 | 拆后 |
|---|---|---|
| 构建入口 | `rustomp/scripts/build-all.sh`（自己实现） | `rustomp/scripts/build-all.sh` → `rustomp-mac-plugin/pack.sh` |
| 产物 | Sp02 / Sp03 | Sp02 / Sp03 **同一份源码** |
| `au_smoke` | CocoaUI ✓ / RMS 0.356170 | CocoaUI ✓ / **RMS 0.356170** |
| ARTIFACT 行 | — | 6 条（2 变体 × AU/VST3/CLAP） |
| 框架里的平台词 | 一堆 | **0**（测试断言） |

`doctor.sh` 在本机全绿：cargo 1.99.1、Xcode CLT、cmake 4.3.3、两个 rust target、框架、third_party。

### 71.5 遗留

- standalone 仍未实现（v0.2）；签名仍是 ad-hoc（对外分发要 Developer ID + 公证）；
- `rustomp/template/` 仍留在框架里（生成新插件），但生成目标已改为仓外（E70）。

---

## E72：拆仓第 2 步 —— `sparklin-drive` 独立发布，全新 clone 一条命令出插件（2026-10-27）

https://github.com/amonisachicken/sparklin-drive （`0240df1`，tag `v0.1.0`）

### 72.1 依赖改造：从 path 到 git tag

```diff
-rustomp-dsp = { path = "../rustomp/crates/rustomp-dsp" }
+rustomp-dsp = { git = "https://github.com/amonisachicken/rustomp", tag = "v0.1.0" }
```

开发期想改框架怎么办？**不改 `Cargo.toml`**，用 `.cargo/config.toml` 的 `[patch]`（Cargo 1.56+ 支持）：

```toml
[patch."https://github.com/amonisachicken/rustomp"]
rustomp-dsp = { path = "../rustomp/crates/rustomp-dsp" }
```

`config.toml` 入 gitignore，仓库里只放 `config.toml.example`。

**这一步同时验证了第 0 步的 tag 是真能用的**：`Cargo.lock` 解析出

```text
source = "git+https://github.com/amonisachicken/rustomp?tag=v0.1.0#29cbb12a…"
```

（如果当初没修 workspace，这里会直接 fail —— 这就是为什么"先自洽、再打 tag"的顺序不能反。）

### 72.2 `scripts/build.sh`：四个子命令

```text
./scripts/build.sh doctor    # 体检（缺什么给"怎么装"）
./scripts/build.sh           # doctor → rustomp-gen --check → pack.sh → 安装 → ARTIFACT 列表
./scripts/build.sh test      # cargo test（不需要 ngspice）
./scripts/build.sh verify    # 需要 ngspice：整机对拍
```

三个关键设计：

1. **构建前跑 `rustomp-gen --check`** —— 改了网表却没重新生成，这里就 fail（ADR-014 的漂移守卫不能在插件仓里丢）；
2. 缺打包工具时打印**可复制的 clone 命令**，不猜路径；
3. 结尾打印 `ARTIFACT:` 列表（与打包工具契约同形），CI 直接抓。

> 顺手修了一个交互小坑：`./scripts/build.sh --no-install` 之前会被当成"未知子命令"，
> 现在以 `-` 开头就按 build 处理并把参数透传。

### 72.3 CI：干净 runner 上的证明

`.github/workflows/build.yml` 在 `macos-14` 上 checkout **三个仓**（插件 / rustomp@tag / packer），
跑 `doctor → test → build --no-install`，然后从日志里抓 `ARTIFACT:` 行，把产物上传。

这才是"任何人拉取后能构建"的证据 —— 用全新 clone，而不是本机已有缓存。

### 72.4 验收（真·全新 clone）

```text
$ git clone git@github.com:amonisachicken/sparklin-drive.git /tmp/sd-fresh
$ cd /tmp/sd-fresh && ./scripts/build.sh --no-install
...
ARTIFACT: .../au-mono/Sparklin Drive Mono.component
ARTIFACT: /private/tmp/sd-fresh/target/bundled/Sparklin Drive Mono.vst3
ARTIFACT: /private/tmp/sd-fresh/target/bundled/Sparklin Drive Mono.clap
ARTIFACT: .../au-stereo/Sparklin Drive Stereo.component
ARTIFACT: /private/tmp/sd-fresh/target/bundled/Sparklin Drive Stereo.vst3
ARTIFACT: /private/tmp/sd-fresh/target/bundled/Sparklin Drive Stereo.clap
```

`au_smoke`：CocoaUI ✓、**RMS 0.356170**（与所有历史版本逐位相同）。

### 72.5 仓库内容（35 个文件，440 KB）

| 目录 | 内容 |
|---|---|
| `circuit/netlist/*.cir` | 电路"源码"（两份完整网表，ngspice 可直接跑） |
| `circuit/src/generated/*.rs` | 网表 → 装配代码（入库 + `--check`） |
| `plugin/src/editor.rs` | 手写界面层（约 90 行） |
| `assets/` + `design/panel.box.json` | 设计器产物与源文件（含 3 个导入 SVG） |
| `rustomp.toml` | 含 `[requires]`（framework/packer/designer） |
| `scripts/build.sh`、`BUILDING.md`、`.github/workflows/build.yml` | 一键构建与文档 |

---

## E73：两个收口决定 —— 打包工具打 tag / 参考插件即模板（2026-10-27）

### 73.1 `rustomp-mac-plugin` 也打了 `v0.1.0`

CI 里的 `PACKER_TAG` 从 `master` 改成 `v0.1.0`。现在**三个仓全部钉 tag**：

| 仓 | tag | 谁钉它 |
|---|---|---|
| `rustomp` | `v0.1.0` | 插件仓的 `Cargo.toml`（git 依赖） |
| `rustomp-mac-plugin` | `v0.1.0` | 插件仓 CI 的 `PACKER_TAG` |
| `sparklin-drive` | `v0.1.0` | 使用者自己 |

于是"任何人都能复现一次构建"这件事**在仓与仓之间成立**，不只在本机。

### 73.2 `sparklin-drive` 兼任模板：新增 `MAKING-YOUR-OWN.md`

比维护第二份模板可靠 —— 模板最容易腐烂，而参考插件天天在 CI 里跑。

文档结构（按动手顺序）：先跑通再改 → 改身份 → 换电路 → 换面板 → 改参数表 →
发布检查清单 → 六个易踩的坑 → 延伸阅读。

两处值得强调的内容：

1. **AU 四字符码 / VST3 id 发布后不可改**：宿主按 `(manufacturer, subtype)` 认插件，
   改了等于换了一个插件，用户的工程与自动化全部失效。所以这一步必须在第一次发布前定稿；
2. **"插件放进别的 Rust 项目目录里会炸"** 写进了坑列表（E70 踩过的那个
   `cargo-nih-plug` chdir 到最顶层 `Cargo.toml` 的问题）。

另外把 `Cargo.toml` 的 `repository` 与 `rustomp.toml` 的 `url` 指向了本仓
（之前还指着 rustomp —— 复制粘贴模板时的典型残留）。

---

## E74：隔离工作区验证 —— 抓出两个"本机看不出来"的缺陷（2026-10-27）

目标：**不依赖本机任何已有状态**，从 GitHub 拉三个仓到全新目录，跑 `./scripts/build.sh`。

做法：`~/develop/.iso-verify-<HHMM>/{rustomp, rustomp-mac-plugin, sparklin-drive}`，
三个仓都 `clone --depth 1 --branch <tag>`，显式
`RUSTOMP_ROOT=$ISO/rustomp RUSTOMP_MAC_PLUGIN=$ISO/rustomp-mac-plugin`。

**第一轮直接失败，而且失败得很有价值。**

### 74.1 缺陷一：tag `v0.1.0` 里的 `pack.sh` 是坏的

```text
pack.sh: line 40: PLUGIN_DIR: unbound variable
```

原因是 bash 把**全角字符**算进了变量名：`"...（已有 $PLUGIN_DIR）"` → 变量名成了 `PLUGIN_DIR）`。
本机之所以没炸，是因为修复只留在**工作区没提交**，而 tag 打在了修复之前。

→ 提交修复（6 处 `${VAR}` + `PLUGIN_DIR` 冲突判断），打 **`v0.1.1`**（不改写已发布 tag）。

> 教训：**"本机能跑"不等于"tag 能用"**。tag 必须在一个干净 checkout 里验证过才算数。

### 74.2 缺陷二：`identity.rs` 生成物漂移

第二轮：`rustomp-gen --check` 直接 fail：

```text
以下生成物与 rustomp.toml 不一致：
  ✗ plugin/src/generated/identity.rs
```

原因是我上一轮把 `rustomp.toml` 的 `url` 改成本仓，却**忘了重新生成**；
本机因为之前跑过 `build.sh`（内含 `--check`）所以是绿的，干净 checkout 里就露馅了。

→ 重新生成；并把 **`build.sh test` 也加上 `check_generated`**
（否则"测试绿、发布出去的是旧生成物"这种情况拦不住）。打 **`v0.1.2`**。

### 74.3 第三轮：通过

```text
$ ./scripts/build.sh --no-install
==> 环境体检        ✓ 11 项
==> 校验生成物      ✓ 7 个文件一致
==> [1/5] 构建 Sparklin Drive Mono（通用二进制，feature=mono）
...
ARTIFACT: $ISO/rustomp-mac-plugin/build/au-mono/Sparklin Drive Mono.component
ARTIFACT: $ISO/sparklin-drive/target/bundled/Sparklin Drive Mono.vst3
ARTIFACT: $ISO/sparklin-drive/target/bundled/Sparklin Drive Mono.clap
ARTIFACT: $ISO/rustomp-mac-plugin/build/au-stereo/Sparklin Drive Stereo.component
ARTIFACT: $ISO/sparklin-drive/target/bundled/Sparklin Drive Stereo.vst3
ARTIFACT: $ISO/sparklin-drive/target/bundled/Sparklin Drive Stereo.clap
```

| 检查项 | 结果 |
|---|---|
| 用时 | **3 分 39 秒**（全新 target 冷编译 + 两次 clap-wrapper CMake） |
| `au_smoke` | 两个变体都 CocoaUI ✓、RMS **0.356170** |
| 产物位置 | 全在隔离目录内（`$ISO/...`） |
| 依赖来源 | `rustomp-dsp/gui/oracle = git+…rustomp?tag=v0.1.0#29cbb12a`、`nih_plug = git+…rev=de42101` —— **无任何 path 依赖** |
| 本机三个仓 | 验证前后都 `未提交改动 0 个`（没被污染） |
| 隔离目录体积 | 683 MB（其中 `sparklin-drive` 572 MB 是 target/） |

### 74.4 结论与后续

1. **隔离验证要成为发布前的固定动作**（"本机能跑"证明不了 tag 可用）；
2. `v0.1.0`（packer）与 `v0.1.1`（插件）保留但**标记为不可用**，当前可用组合 =
   `rustomp v0.1.0` + `rustomp-mac-plugin v0.1.1` + `sparklin-drive v0.1.2`；
3. CI（`.github/workflows/build.yml`）已经是"三仓 checkout + 干净 runner"，
   与这次手动隔离验证等价 —— 但它还没跑过一次（仓库刚建），下次 push 时会验证。

---

## E75：CI 真跑一次 —— 抓出三处"绿着但其实没干活"（2026-10-27）

`sparklin-drive` 的 workflow 建好后从没真跑过。点了一次 `workflow_dispatch`，
第一次的结果是 **`✓ success`** —— 但它是**假的**。

### 75.1 假绿的三层原因

| # | 现象 | 根因 |
|---|---|---|
| 1 | 构建在 `[1/5]` 就挂：`error: no such command: nih-plug` | runner 上没有 `cargo-nih-plug`（本机早就装了，所以 doctor 也漏检） |
| 2 | 构建失败，但 **step 仍然绿** | `./scripts/build.sh … \| tee /tmp/build.log` —— 退出码是 `tee` 的，失败被吞 |
| 3 | 上传步骤只吐警告 | `No files were found with the provided path: …/dist` 不算失败（`if-no-files-found` 默认 `warn`） |

三处都是"**看起来通过了，其实什么都没产出**"的典型。

### 75.2 修法

```diff
+      - name: cargo-nih-plug
+        run: cargo install --git https://github.com/robbert-vdh/nih-plug.git --rev de42101 cargo-nih-plug --locked
...
       - name: Build
-        run: ./scripts/build.sh --no-install | tee /tmp/build.log
+        run: |
+          set -o pipefail            # 关键
+          ./scripts/build.sh --no-install 2>&1 | tee /tmp/build.log
...
       - uses: actions/upload-artifact@v4
         with:
+          if-no-files-found: error
```

收集产物的步骤也加了断言：数量 < 1 直接 `exit 1`。

打包工具侧（v0.1.2）同步补齐：`doctor.sh` 检查 `cargo-nih-plug`、`pack.sh` 版本改为
`git describe`、新增 `deps.lock`（集中钉住 nih-plug rev / cargo-nih-plug 安装命令 / 第三方 commit）。

### 75.3 第二个坑：`pipefail` + `grep -q` 的 SIGPIPE

修完第一层后，CI 报 `✗ 没有 cargo-nih-plug`，但 install 步骤日志里明明写着
`Installed package cargo-nih-plug`。原因是 doctor 的探测方式：

```bash
cargo --list | awk '{print $1}' | grep -qx nih-plug   # ← 在 set -o pipefail 下会误报
```

`grep -q` 匹配成功就退出 → 上游 `awk` 收到 SIGPIPE（141）→ 整条管道非 0。
本机命令少、awk 早就写完了，所以**只在 CI 复现**。

修法：不用管道 —— `command -v cargo-nih-plug` 优先，退路用 `case "$(cargo --list)" in *nih-plug*)`。

> 这条和 E70 的"全角字符算进变量名"是一类东西：**bash 的细节会挑环境发作**。
> 结论还是那句：本机能跑 ≠ 能用，必须在干净环境里跑一遍。

### 75.4 最终结果

```text
✓ macos in 4m22s
  Doctor ✓ 11 项（含 cargo-nih-plug）
  Test   ✓ 7 个用例
  Build  完成：Mono / Stereo，RMS 0.356170
  Collect artifacts  收集到 6 个产物
  Upload ✓ sparklin-drive-macos（.component ×2 / .vst3 ×2 / .clap ×2）
```

把 CI 的产物下回本机，用 `au_smoke` 实测：

| 产物 | 结果 |
|---|---|
| `Sparklin Drive Mono.component` | CocoaUI ✓、RMS **0.356170** |
| `Sparklin Drive Stereo.component` | CocoaUI ✓、RMS **0.356170** |

也就是：**GitHub 的机器打出来的 AU，和本机的逐位一致**。

### 75.5 顺带：隔离验证脚本

`rustomp/scripts/verify-isolation.sh`（E74 的手动过程脚本化）：

```bash
./scripts/verify-isolation.sh          # 自动取各仓最新 tag，全新目录跑真实构建
./scripts/verify-isolation.sh --fast   # 只出 mono AU（约 3 分钟）
```

检查 6 项：三仓可 clone / 退出码 / ARTIFACT 行数与落点 / au_smoke + RMS / 依赖全来自 git tag /
**本机仓未被改动**。已写进 `docs/ecosystem.md` §6.1 作为发布前固定动作。

---

## E76：新机上缺框架时给出克隆指引（2026-10-27）

用户报告：在一台 **M1 MacBook Air** 上拉取仓库后成功构建出可用插件。
接着提了一个很实际的诉求：**新机上如果只 clone 了插件仓，跑 `build.sh` 时要明确告诉他去克隆 rustomp，并给出链接。**

### 76.1 原来的行为（不够好）

| 场景 | 原来的提示 |
|---|---|
| 缺插件仓（打包工具） | ✅ 已经打印 clone 命令 |
| **缺 rustomp 框架** | 只有一句 `没找到 rustomp 框架（只影响生成物漂移校验，不影响打包）` —— 既没链接，也**说错了**（打包工具靠框架里的 `plugin_meta.py` 读 `rustomp.toml`，缺了必然失败） |

更糟的是 `check_generated()` 会**静默跳过**漂移守卫 —— 这正是 E74/E75 里
"发布出去的是旧生成物"那类问题的入口。

### 76.2 现在的行为

```text
$ ./scripts/build.sh            # 新机上：只有插件仓 + 打包工具
  ✗ 找不到 rustomp 框架 —— 读 rustomp.toml、校验生成物都要用它

  装一次即可（用它钉的 tag，与 Cargo.toml 一致）：
    git clone --depth 1 --branch v0.1.0 https://github.com/amonisachicken/rustomp.git ~/develop/rustomp
    export RUSTOMP_ROOT=~/develop/rustomp        # 建议写进 shell profile

  或者放到本仓的**同级目录**（最省事，脚本会自动找到）：
    git clone --depth 1 --branch v0.1.0 https://github.com/amonisachicken/rustomp.git ../rustomp

  仓库：https://github.com/amonisachicken/rustomp
```

三个细节：

1. **tag 自动取自本仓 `Cargo.toml`**（`tag = "..."`）——所以提示永远不会过期；
2. **"放到同级目录"是最省事的路径**：脚本本来就会找 `../rustomp`，克隆完不用设任何环境变量；
3. `check_generated()` 不再静默跳过，要跳过必须显式
   `RUSTOMP_SKIP_DRIFT_CHECK=1`（且会打印警告）。

打包工具的 `doctor.sh` 也同步给出同样的指引（直接跑 `pack.sh` 的人也能看到）。

### 76.3 怎么验证的（模拟新机）

```bash
/tmp/sim-new/{sparklin-drive, rustomp-mac-plugin, fakehome}   # 没有 rustomp
HOME=/tmp/sim-new/fakehome RUSTOMP_MAC_PLUGIN=/tmp/sim-new/rustomp-mac-plugin \
  ./scripts/build.sh doctor
```

- `fakehome` 里只软链 `.cargo/.rustup`（否则 rustup 没有默认 toolchain 会干扰观察）；
- 结果：打印上面的指引；
- **照着指引做**（`git clone --branch v0.1.0 … ../rustomp`）后再跑 `doctor` → 立刻全绿
  （`✓ rustomp 框架：/tmp/sim-new/rustomp`）。

### 76.4 回归

`./scripts/verify-isolation.sh --fast`（自动取各仓最新 tag）：

```text
✓ rustomp @ v0.1.0    ✓ rustomp-mac-plugin @ v0.1.4    ✓ sparklin-drive @ v0.1.4
✓ build.sh 退出码 0（172s）   ✓ ARTIFACT 1 条   ✓ au_smoke   ✓ RMS 0.356170
✓ 框架依赖来自 git tag（3 条）  ✓ 本机仓状态未变化
```

---

## E77：依赖位置统一为「本仓同级目录」（2026-10-27）

用户的提醒很实际：**每台机器的目录布局都不一样**。之前提示里写 `~/develop/rustomp`，
等于假设了"大家都用 ~/develop"，而且给了两条做法（绝对路径 + 同级）反而让人犹豫。

### 77.1 现在只有一种布局

```text
some-dir/
  rustomp/                 ← 框架（读清单 / 校验生成物）
  rustomp-mac-plugin/      ← 打包工具
  sparklin-drive/          ← 插件项目（build.sh 在这里）
```

- `build.sh`：`PACKER=$ROOT/../rustomp-mac-plugin`、`FRAMEWORK=$ROOT/../rustomp`
- `locate_framework()`：显式 `RUSTOMP_ROOT` → **同级目录**（删掉了 `$HOME/develop` 兜底）
- 缺依赖时打印目录树 + **一条 `cd` 命令**，克隆完直接重跑，**不需要设任何环境变量**
- 打包工具的 `doctor.sh` 同样：`RUSTOMP_ROOT` → 插件项目的同级 → 打包工具自己的同级

### 77.2 提示长这样

```text
  ✗ 找不到 rustomp 框架 —— 读 rustomp.toml、校验生成物都要用它

  约定：三个仓放在**同一个目录**下（本仓的同级）

    /tmp/sim2/
      rustomp/                 ← 缺这个
      rustomp-mac-plugin/
      my-drive/    ← 你在这里

  在这个目录里执行（tag 与本仓 Cargo.toml 一致）：

    cd "/tmp/sim2"
    git clone --depth 1 --branch v0.1.0 https://github.com/amonisachicken/rustomp.git rustomp

  克隆完直接重跑 ./scripts/build.sh 即可，**不需要设任何环境变量**。
```

注意两点：目录名取自实际位置（模拟里我故意把仓库命名成 `my-drive`，提示也跟着变）；
tag 取自本仓 `Cargo.toml`，将来升 tag 不用改文案。

### 77.3 CI 也跟着简化

三个仓 checkout 到**同一目录**（`path: rustomp` / `rustomp-mac-plugin` / `sparklin-drive`），
于是 CI 里**不再有** `RUSTOMP_ROOT` / `RUSTOMP_MAC_PLUGIN` 任何环境变量 ——
CI 验证的就是用户本地的同一种布局。

### 77.4 验证

| 项 | 结果 |
|---|---|
| 模拟新机（`/tmp/sim2` 只有 `my-drive`，HOME 指向空目录） | doctor 打印目录树 + 克隆命令 |
| 照提示克隆到同级后重跑 | `✓ rustomp 框架：/tmp/sim2/rustomp`，全绿 |
| 打包工具直接调用（同级无框架） | 打印 `cd …/my-drive/.. && git clone … rustomp` |
| 隔离回归（**不设任何环境变量**，自动取最新 tag） | `v0.1.0 + v0.1.5 + v0.1.5`，179 s，RMS 0.356170 |

最后一项意义最大：`verify-isolation.sh` 已经把 `export RUSTOMP_ROOT/RUSTOMP_MAC_PLUGIN` 删了，
现在它验证的就是"三个仓扔进一个目录、什么都不设也能构建"。

---

## E78：第二次隔离验证 —— 专测「依赖缺失」的引导是否好使（2026-10-27）

用户要求：**在全新目录里只 clone 插件仓**，看它能不能自己把依赖一路引到位。

这次不是"三个仓一起 clone"，而是模拟真实新用户：**从零开始，照着提示做**。

### 78.1 三段式引导（实测输出）

**第 1 次运行**（只有插件仓）：

```text
  ✗ 找不到 rustomp 框架 —— 读 rustomp.toml、校验生成物都要用它
  约定：三个仓放在**同一个目录**下（本仓的同级）
    /tmp/deps-clean-1359/
      rustomp/                 ← 缺这个
      rustomp-mac-plugin/
      sparklin-drive/    ← 你在这里
  在这个目录里执行（tag 与本仓 Cargo.toml 一致）：
    cd "/tmp/deps-clean-1359"
    git clone --depth 1 --branch v0.1.0 https://github.com/amonisachicken/rustomp.git rustomp
```

**第 2 次运行**（框架就位，缺打包工具）：

```text
  ✓ 框架：/tmp/deps-clean-1359/rustomp
  ✗ 找不到打包工具（…/rustomp-mac-plugin/pack.sh）
  按约定，它和框架一起放在**本仓的同级目录**。依次执行：
    cd "/tmp/deps-clean-1359"
    git clone https://github.com/amonisachicken/rustomp-mac-plugin.git
    git -C rustomp-mac-plugin checkout v0.1.5      # 钉到本版用过的 tag
    ./rustomp-mac-plugin/scripts/fetch-third-party.sh        # 首次（约 12 MB）
```

**第 3 次运行**：真构建 → 6 条 `ARTIFACT`、`au_smoke` 两个变体都过、**RMS 0.356170**。

### 78.2 顺手修掉的两个提示缺陷

| 问题 | 修法 |
|---|---|
| `cd … && git clone …` 后面紧跟 `./rustomp-mac-plugin/scripts/fetch…` —— 在同一个 shell 里 `cd` 过之后这条相对路径是**错的** | 拆成三行独立命令，逐行照抄即可 |
| "钉版本"那行硬编码了 `v0.1.4`（早就不是最新） | 新增 `rustomp.toml` 的 `[requires] packer_tag`，提示从这里读；版本随 manifest 走 |

（对应 `sparklin-drive v0.1.6`。）

### 78.3 结果

| 检查项 | 结果 |
|---|---|
| 只 clone 插件仓 → 提示 | ✅ 引导到框架 |
| 照做 → 提示 | ✅ 引导到打包工具（含 `fetch-third-party`） |
| 照做 → 构建 | ✅ 3m33s，6 条 ARTIFACT |
| 产物位置 | ✅ 全在 `/tmp/deps-clean-…` 里 |
| `au_smoke` | ✅ Mono / Stereo 都通过，RMS **0.356170** |
| 依赖来源 | ✅ 3 条 `git+…?tag=v0.1.0`，无 path 依赖 |
| 全程环境变量 | ✅ 一个都没设 |
| 本机三个仓 | ✅ 验证前后都是 0 个未提交 |

临时工作区 682 MB，验证后已删除。

### 78.4 结论

"新用户从零开始"这条路是**可自助走通的**：每一步出错都直接告诉你在哪个目录敲什么命令，
而且命令逐行可执行、tag 不会过期。剩下的手工步骤只有两处：
`fetch-third-party.sh`（第三方源码，约 12 MB）和 `cargo-nih-plug` 的安装（`doctor` 会给命令）。

---

## E79：把"依赖与获取方式"摆到 README 最前面（2026-10-27）

两次隔离验证暴露出一个规律：**卡住新人的从来不是代码，是"我该装什么"。**
所以把依赖从 `BUILDING.md`（第二份文档）提到 `README.md` 的**第一屏**。

### 79.1 README 现在的顺序

```text
# Sparklin Drive                    ← 一句话 + 管线图
## 依赖与获取方式（先读这段）          ← 新增，紧跟标题
   ### 目录布局（三个仓同级 + 为什么不需要环境变量）
   ### 一次装齐（三段命令，tag 分别写在哪）
   ### 系统依赖（7 行表：为什么需要 / 怎么查 / 怎么装）
   ### 自查（./scripts/build.sh doctor）
## 快速开始                          ← 依赖齐了之后的一条命令
## 这不是"贴模拟味的建模"，是算电路
...
```

依赖表里每一行都带"**为什么需要**"，因为"装 X 有什么用"比"装 X"更能促成人动手：

| 依赖 | 为什么需要 |
|---|---|
| 两个 rust target | universal 二进制（Intel + Apple Silicon） |
| Xcode 命令行工具 | 链接系统框架、`codesign`、`plutil` |
| cmake | 把 CLAP 包成 AUv2（clap-wrapper） |
| python3 ≥ 3.11 | 读 `rustomp.toml`（`tomllib`） |
| cargo-nih-plug | `cargo nih-plug` 子命令（打 bundle） |

`cargo-nih-plug` 那行特意写了 `--rev de42101`，并在下面注明**必须与打包工具的 `deps.lock` 一致**。

### 79.2 BUILDING.md：补"版本要配套"

三个组件各钉各的版本，最容易错配，所以单独一张表说清"版本写在哪"：

| 组件 | 版本写在哪 |
|---|---|
| 框架 `rustomp` | 本仓 `Cargo.toml` 的 `tag = "v0.1.0"` |
| 打包工具 | 本仓 `rustomp.toml` 的 `[requires] packer_tag = "v0.1.5"` |
| `cargo-nih-plug` | 打包工具的 `deps.lock`（rev `de42101`） |

同时把 `~/develop/...` 这类绝对路径全删了（产物路径改成 `<同级目录>/rustomp-mac-plugin/...`），
常见问题补两条：`no such command: nih-plug`、`requires.packer 版本不匹配`。

### 79.3 MAKING-YOUR-OWN.md：fork 之后必须改 CI

"拿它当模板"这条路上最隐蔽的一步是 **CI 里的 `repository:` 还指着原作者**。
已写进发布检查清单，并补了 `[requires]` 三个字段的说明、以及"忘了 `fetch-third-party.sh`"这个坑。

### 79.4 校验

- 三份 md 的**本地链接全部有效**（脚本扫过 `[text](target)`）；
- 没有 `~/develop` 之类绝对路径；
- `./scripts/build.sh doctor` 仍全绿（文档改动不影响构建路径）。

`sparklin-drive` 打 `v0.1.7`（README 116 → 167 行、BUILDING 120 → 145、MAKING-YOUR-OWN 161 → 182）。

---

## E80：电路导入对齐首选设计器（`rustomp-circuit-designer`）（2026-10-27）

**背景**：专用原理图设计器已经能画图、能仿真、能出网表，于是它取代 breadspice
成为**首选**设计器；本仓的导入层必须真接上，而不是“各自看起来都能跑”。

### 80.1 新增导入器：直接吃工程文件

`crates/rustomp-import/src/designer.rs`：`CircuitDesigner` 读 `.design.json`
（设计器的一等产物），不用先导出 `.cir`。

| 决定 | 理由 |
|---|---|
| **引脚几何不在本仓**，运行时读设计器的 `symbols/symbols.json` | 同一个量只能有一个来源；“复制一份”必然腐化 |
| 几何规则（`to_world` / T 型接点 / 最小坐标命名）**逐行照搬** | 与设计器网表逐行一致，否则“画面正常、网表孤立” |
| 拿设计器自己导出的 `.cir` 当裁判（`tests/designer.rs`） | 两个实现相互制约，漂移立刻红 |
| `registry()` 顺序：`.design.json` > `.bread` > `.cir` | 专职设计器的工程文件能给出更多上下文 |

### 80.2 三个真·不对齐（都是沉默错误）

| # | 现象 | 根因 | 修法 |
|---|---|---|---|
| 1 | 生成的 `Ckt` **忽略输入**，`process(x)` 里 `let _ = x;` | 设计器网表是**纯拓扑**，写 `in=IN`（节点名）；codegen 只认元件名 → `has_input = false` | `in=` 支持节点名；显式输入节点也归入“固定电位节点”参与编号 |
| 2 | 电位器参数**静默丢弃**（`Params` 是空结构体） | 设计器写 `param P1A`（位号），codegen 找 `RP1A`（网表元件名） | 设计器改为 `param RP1A`；本仓保留“加 R 前缀”的兼容层并打印提示 |
| 3 | `--spice` 导出得到 `CC1`/`DD1` | `to_spice()` 无条件拼前缀，而名字已带前缀 | 名字已以器件前缀开头就不再拼 |

### 80.3 顺手抓到的两个求解器 bug（只有真跑设计器产物才暴露）

1. **固定电位节点的值没写回解向量**：`Mna::solve_len` 只解未知量，固定节点（电源轨/输入）
   的槽位永远是 0；而电容的 `v_prev`、非线性器件的线性化点都直接读 `v[]` ——
   于是 8.2 V 的轨被当成 0 V。修法：`Netlist::fill_fixed()` 在牛顿每轮求解后补回（
   `step()` / `step_fixed()` / `solve_dc()` 都走同一条路径）。
2. **线性外推热启动在陡沿处会把人送进沟里**：实时路径用 `2·v − v_prev2` 热启动，
   遇到管子从截止冲进饱和这种陡沿会给出一个远离解的初值，固定次数的牛顿从那里出发直接发散
   （实测第 77 个样本炸到 1e25 V，迭代从 3 加到 40 都没用）。修法：跳变 > 10% 就退回上一拍解；
   稳态仍外推，性能不变。

### 80.4 已知问题（未修，记在这里）

用户自己保存的 `pretty-face`（两极锗管 AC127 法兹，27 元件 / 30 导线）在 **ngspice 里正常**
（v(out) ±0.2 V、THD 66%），但在本仓实时路径上仍会在第 78 个样本发散。
已排除：牛顿次数（3/6/8/12/40 都一样）、gmin（1e-12 无影响）、模型参数（与 `devices/devices.lib` 一致）。
**下一步怀疑对象**：`Bjt::terminal()` 的内层牛顿（`Rb=100 Ω / Rc=1.5 Ω` 的大信号求解），
它在极端结电压下可能返回不收敛的解。

> 导入层本身没问题：同一份工程文件生成的网表与设计器导出**逐行相同**，
> 低通（10k+100n）的 1 kHz 衰减与理论 `1/√(1+(f/fc)²)` 也对得上（`tests/designer_generated.rs`）。

### 80.5 验证

| 项 | 结果 |
|---|---|
| 设计器工程 → 网表 vs 设计器导出的 `.cir` | 两个夹具（`pretty-face` / `lowpass`）**逐行相同** |
| 设计器工程 → `--mna` → 编译 → 正弦 | 10k/100n 低通：通带 0.998、1 kHz 衰减与理论差 < 0.02 |
| 漂移守卫 | 新增 `designer_project_is_up_to_date`（工程 → 生成物） |
| `cargo test -p rustomp-import` | 9 + 4 + 5 全过 |
| `cargo test -p rustomp-dsp` | 除 `realtime_cpu_cost` 外全过；该项在本机环境下 **baseline 也失败**（`mediaanalysisd` 占 102% CPU），与本轮改动无关 |

---

# 4. 技术路线决策（ADR 风格）

### ADR-001 电路分块策略 ✅ 已定

| 电路部分 | 占比 | 方法 | 理由 |
|---|---|---|---|
| 线性：输入/输出缓冲、运放增益级、tone stack（RC 网络） | 元件数 70%+ | MNA → 传递函数 → 双线性变换 → **IIR 直接实现** | 精确、无迭代、极便宜 |
| 非线性核心：二极管/BJT/JFET 削波、电子管级 | 音色灵魂 | **WDF + 标量牛顿**，4~8x 过采样 | 每级 1 未知数，实测 2.3 次收敛 |
| 时基/数字：BBD delay、LFO、弹簧混响 | 视产品 | 独立模块（BBD = 时钟 + 低通级联 + 压缩扩展器） | 不属于电路建模范畴 |

### ADR-002 线性块与非线性块的耦合方式 ⏳ 待定（M0 必须定）

这是**整个项目最容易翻车的架构决策**：线性 IIR 块与非线性 WDF 块之间存在**无延迟环路（delay-free loop）**问题。三个选项：

| 方案 | 优点 | 缺点 |
|---|---|---|
| **A. 整块统一 WDF 树**（运放建成 nullor/norator 适配器） | 物理最干净、无相位误差、可建模运放饱和 | 工作量最大，需处理运放非理想特性 |
| **B. 分块 + 1 采样延迟** | 实现最简单 | 引入相位误差，高增益反馈电路中可听 |
| **C. 整块 MNA + 全牛顿**（不拆） | 最易从网表自动生成，实现最直白 | CPU 高 3~5 倍，收敛性更脆弱 |

**倾向**：主路径 **A**，同时把 **C** 实现为 oracle 兜底 —— 同一网表两套实现互相对拍，是性价比最高的正确性保障手段。

> **2026-10-23 更新（据此更定）**：oracle 优先级改为——
> ① 解析解（能算的就算）→ ② **自研 MNA + 牛顿**（一等 oracle，固定步长、可注入状态、可对比中间节点电压）→ ③ ngspice（独立第三方交叉验证，后置，见 E3）。
> 理由：自研 MNA 本来就是方案 C 的兜底，又比 ngspice 更适合逐样本对拍。

### ADR-003 参数化 = 电路建模的核心卖点 ✅ 已定方向

旋钮 = 电位器 = 可变电阻 = WDF 端口阻抗。要做到工业级需处理三件事（竞品常翻车处）：

1. **电位器 taper**（A/B/C 对数/线性曲线），不是线性映射
2. **电位器作为分压器时整条支路阻抗随抽头变化**，不只是两端阻值
3. **参数变化时的状态一致性**（改元件值需重算 WDF 状态变量，否则 zipper noise / 爆音）

> 做到这三点才能宣称"Drive 旋钮在 1:1 位置与真实硬件音色一致"——这是黑箱建模（Neural DSP / TONEX）无法宣称的。

### ADR-004 生产代码骨架 ✅ 已定

```
rustomp/
├── crates/
│   ├── rustomp-dsp/        # 元件(R/L/C/D/BJT)、适配器、牛顿求解、过采样
│   │                       # no_std 友好、零堆分配、f32/f64 泛型
│   ├── rustomp-circuits/   # 具体电路 = 网表描述 + 元件值（本项目 IP 所在）
│   ├── rustomp-plugin/     # nih-plug（VST3 / CLAP / AUv2）+ 参数 + GUI
│   └── rustomp-oracle/     # 离线对拍：ngspice / 解析解 / 实测录音（仅 CI，不进产品）
└── experiments/            # 一次性验证代码，非生产
```

**音频线程硬约束**（不可协商）：零堆分配、无锁、无 panic、参数平滑、NaN 熔断、denormal 处理、采样率变更时重建状态。

### ADR-005 许可证：GPL-3.0-or-later ✅ 已定

**决定**：本项目采用 **GPL-3.0-or-later**。

依据与后果：

| 依赖 | 许可证 | 在 GPL-3 项目中的状态 |
|---|---|---|
| breadspice | GPL-3.0 | ✅ **同协议，可直接拷贝代码**（保留 SPDX 头 + 版权声明 + 在 §8 登记） |
| nih-plug | ISC | ✅ 兼容 |
| VST3 SDK | GPL-3.0 / 专有 双许可 | ✅ **GPL-3 项目天然免费使用**（这正是 nih-plug 选 VST3 的前提） |
| clap-wrapper | MIT | ✅ 兼容 |
| AudioUnitSDK (Apple) | Apache-2.0 | ✅ 兼容 |
| ngspice | 核心 BSD-3（部分可选 LGPL） | ✅ 兼容；仅用 CLI 二进制时不触发 LGPL |
| ffmpeg | LGPL/GPL | ✅（仅开发期工具，不分发） |
| Guitarix / DIYLC | GPL-3 | ✅ 现在可以用（与本项目同协议） |

**必须记住的约束（新风险 R10）**：**GPL-3 与 AAX 不兼容**。AAX SDK 需签 Avid 专有协议，与 GPL 的传染性冲突。本项目不做 Pro Tools，但**这个门从此关上了**——如果将来要支持 PT，必须重新评估（可能需双许可或重写架构）。

### ADR-006 目标格式与构建链：AUv2 优先（Logic）✅ 已定（E4 已验证）

**事实核查（重要）**：nih-plug **不提供 AU 导出**。官方 README 原文：*"NIH-plug can currently export VST3 and CLAP plugins."* 源码无 `nih_export_au`，`nih_plug_xtask` 也无 AU 目标。`prepare.md` 此处错误。

**决定**：构建链采用两级：

```
rustomp-plugin (nih-plug)  ──►  .clap
                                    │  clap-wrapper (MIT, CMake)
                                    ▼
                        .component (AUv2)  +  .vst3
```

- 宿主目标：**Logic Pro**。AUv2 已足够（Logic 支持 AUv2/AUv3；AUv2 更成熟、工具链简单）。
- `clap-wrapper` 已核实支持 `wrapasauv2.cpp` / `wrapasauv3.mm` / `wrapasvst3.cpp`，MIT 许可，依赖 Apple AudioUnitSDK（Apache-2.0）。
- **代价**：多一层 CMake/Objective-C++ 构建；需在仓库中固定 clap-wrapper 版本（submodule 或 vendored）。
- **验收**：`auval -v aufx <subtype> <manu>` 通过 + Logic 能扫到并加载 + 处理器无爆音。

### ADR-007 首款产品：二极管削波类失真 ✅ 已定

- 产品形态：经典「输入缓冲 → 运放增益级 → 对地二极管削波 → tone → 输出」类失真。
- 元件：主打 1N4148（已有 SPICE 模型）；**非对称削波配置（如 1N4148 + 1N5817 / + LED）作为差异化点** —— 在电路建模下这就是"换器件/改数值"，几乎零成本，而黑箱建模做不到。
- 待用户提供自研电路原理图（Q2）。暂时以公开经典拓扑（如 MXR Distortion+ / MXR D+ 类）作 M0 练手（**只学电路拓扑，不抄商标/外观/命名**）。
- 必须用完整二极管模型（Is/N/**Rs**/Cjo/M/tt，见 E2 §3.9）。

---

### ADR-015 平台策略：macOS 默认 + 外部打包工具 ✅ 已定（2026-10-26）

**结论**

- `rustomp` 本仓库**只面向 macOS**（框架 + macOS 打包脚本 + 平台契约）；
- Windows / Linux 的打包做成**独立仓库**：`rustomp-win-plugin` / `rustomp-linux-plugin`；
- 开发者按需拉取对应仓库接进现有工作流（打包层，不碰 DSP/电路/代码生成）。

**理由**

1. DSP、代码生成都是纯 Rust，跨平台**编译**不是难点；难点在**打包与宿主兼容**
   （clap-wrapper/CMake/签名/宿主缓存），每个平台都必须在本机验证过才算数。
2. 只有 macOS 能验证；先写 Windows/Linux 脚本等于猜，改起来成本更高。
3. ADR-014 已定好打包后端契约（`ARTIFACT:` 一行），外部工具可以无缝接入。

**影响**

- `docs/declarative-build.md` 的平台矩阵要区分"本仓库提供"与"外部工具提供"；
- CI 只跑 macOS 矩阵；Windows/Linux 的 CI 交给对应仓库；
- 以后要"官方支持某平台"，判定标准是：该平台的打包工具仓库能产出并验证一组 `ARTIFACT:`。

### ADR-014 声明式构建与平台可插拔 ✅ 已定（2026-10-23）

- 插件项目的"声明"只有两份：`rustomp.toml` + `circuit/`（手写 DSP）；
- **代码生成与打包是两个平面**：生成在 host 上确定性完成、产物入库 `src/generated/`；
  打包必须在**各自平台原生**执行（不交叉编译）；
- 打包后端 = `scripts/pack/<platform>/<format>.sh` + 能力声明，驱动按 `[build].formats` 调度；
- 平台范围：macOS（au/vst3/clap）、Windows 与 Linux（vst3/clap）、三平台 standalone；
  **AAX 不可做**（GPL-3 与 Avid 协议冲突）；
- 参数表达力先限定为 `percent/db/plain/enum`，参数值统一为"电位器位置 0..1"。

### ADR-013 电路导入与元件映射 ✅ 已定（2026-10-23）

- 导入器**一律插件化**（`Importer` + `registry()`），导入器不得擅自做映射决策；
- 格式优先级：**先 `SpiceNetlist`，`.asc` 后置**；
- 映射分层：**别名 + 项目映射为常规路径，类别默认作防呆兜底，参数直通仅作 power user 开发接口且默认关闭**；
- **引脚按名绑定**（`C/B/E`、`D/G/S`、`IN+/IN-/V+/V-/OUT`、`a/k`），禁止按位置猜；
- 替换必须**可见可测**（`report()` + 与 oracle 基线同源）。

### ADR-012 元件库精度规范：静态给足、动态按需 ✅ 已定（2026-10-23）

- 精度用**双轴**表示：静态 `D1/D2/D3`、动态 `C0/C1/C2`（详见 `docs/component-library.md`）。
- **推荐基线**：静态 D3、动态 C0；只有大结电容/高阻节点（1N4007、2N7000）升到 C1。
- 依据：① 延迟与精度无关（固定 15 样本）；② 电容是线性项，不增加牛顿维度，代价仅 +5~10%；
  ③ 已有实证——1N4148 省掉 `Cjo/tt` 在 1 kHz 削波下谐波只差 0.056 pp。
- 精度升级**不改变型号身份**（`const BC549C` 的字段可增，网表与实例不变）。
- 器件参数命名与 SPICE 对齐，便于导入 LTspice 模型。

### ADR-011 架构调整：框架 / 插件分离 ✅ 已定（2026-10-23）

**决定**：框架与插件项目分离，但保持**单 repo、结构同构**。

- 框架**无状态**：不含任何插件专属信息；
- 插件以 `plugins/<name>/rustomp.toml` + `circuit/` + `assets/` + 少量胶水代码构成；
- 分发采用**混合模式**：脚手架生成同构目录，但依赖指向框架的 tag/rev（便于统一修 bug）；
- 电路描述用**声明式宏**，拓扑初期由人给出；
- 许可证保持 **GPL-3**。

**理由**：纯 fork 模板会导致框架修复要在 N 个下游手工合并（本项目已踩过的
`&mut self` 性能坑、牛顿容差、拨杆对齐等都会重复）；纯依赖库又会让插件难以定制。
混合模式取两者之长。

### ADR-010 CPU 优化：只做数值等价项，多线程暂不启用 ✅ 已定

- A 类优化已落地：**1592 → 824 ns（1.93x），立体声 CPU 15% → 7.8%**，
  h2–h9 与优化前**逐位一致**。
- 有效手段：外层牛顿线性外推初值（1.7x）、热路径去除法（~14%）。
- 明确记录了几个"看起来该有效其实无效"的方向（内层热启动、f64→f32、放宽容差、
  `&mut self` 热循环），避免以后重复踩。
- **多线程暂不启用**：实测立体声可 1.97x，但单声道变体零收益、且引入实时调度风险。

### ADR-009 CPU 优化与 GUI 的评估结论 ⏳ 待拍板

- **CPU**：瓶颈是二极管的内层牛顿（高电流时 315 ns/次 vs 低电流 10 ns）；
  过采样 FIR 只占 4.6%，**f64→f32 几乎没有收益**（exp 只快 3.6%）。
  只做"数值等价"优化即可把 15% → 6~8%。
  验收标准写死在 E15.3（h2–h9 误差 ≤0.01 pp）。
- **GUI**：AU 的界面经由 clap-wrapper 的 Cocoa 桥，路已通（符号已在二进制里）；
  待选 egui / vizia / 自绘，倾向先用 egui 做可用界面。

### ADR-008 插件一律出「单声道版 + 立体声版」两个独立身份 ✅ 已定（强制）

**决定**：今后每个插件都构建两个只声明单一声道布局的独立插件：

| 版本 | 布局 | AU 码（示例） |
|---|---|---|
| `Xxx Mono` | `{1,1}` | `aufx / RFm1 / Amch` |
| `Xxx Stereo` | `{2,2}` | `aufx / RFs1 / Amch` |

**理由**（实测，见 E13/E14）：

1. 宿主在插入菜单里**按通道条的声道数过滤 AU**；单声道音轨需要 `in=1`。
2. "一个插件同时支持两种布局"依赖宿主正确探测多布局。我们的实测：
   nih-plug 通过 `clap.audio-ports-config` 暴露 Mono+Stereo，但 clap-wrapper 只认
   `clap.configurable-audio-ports`，导致 AU 只报 `{2,2}` ——
   即使打了补丁让 AU 报出两组，Logic 的插入菜单依然不认（还要配合清缓存）。
3. 单一布局没有歧义，且**新身份 = 新缓存条目**，不受宿主陈旧缓存影响。

**代价**：用户会在插件列表里看到两个条目（Waves / UAD 等商业厂商都是这么做的，
用户已有预期）；构建时间翻倍（`build-all.sh`）。

**实现**：`crates/rustomp-plugin` 的 `mono` / `stereo` feature +
`scripts/build-variant.sh`；发布包只含这两个变体。

---

# 5. 里程碑与验收标准

| 里程碑 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| **E1** | WDF 削波级可行性 spike | DC 误差 < 1e-5V；10s 无 NaN；4 级 @4x < 10% 单核 | ✅ 已完成 |
| **M0-a** | workspace 骨架 + **CLAP→AUv2 构建链打通**（空插件直通） | 通用二进制；`.clap` 嵌入 `.component`；进程内冒烟测试通过 | ✅ **已完成**（E4；`auval` 待环境修复，见 E5） |
| **M0-b** | 把电路级 DSP 接进插件（4x 过采样 + GAIN/LEVEL），完整 1N4148 模型（含 Rs） | Logic 里能听声；CPU 可测；与 oracle 对拍 THD | ✅ **已完成**（E10–E12；THD 27.648% vs 27.616%，CPU 15%） |
| **M0-c** | 宿主兼容（AU 分类 / 单声道 / 布局） | Logic 单声道 + 立体声轨道均可加载使用 | ✅ **已完成**（E13–E14，ADR-008） |
| **M1** | 对拍基础设施（**必须早于写电路**）：THD/频响扫频、null test、**自研 MNA+牛顿 oracle** | 20Hz–20kHz THD/频响误差 < 0.5dB | ⬜ |
| **M2** | 目标效果器：原理图 → 网表 → **逐级**验证 → 含 9V 电源内阻与电源轨 | 每一级单独对拍通过后才串联；**二极管必须用完整模型（Is/N/Rs/Cjo/M/tt）** | ⬜ |
| **M3** | 参数化（taper、元件容差）→ 与硬件 A/B 盲听 | 听不出区别 | ⬜ |
| **M4** | 商业面（GPL-3 分发）：签名、公证、安装包 | — | ⬜ |
| **M5** | ngspice 独立交叉验证 | 与 MNA oracle 结果一致 | 🔧 **工具已就绪**（ngspice-47 已编好，见 E3） |

> M0 备注：既然 AU 靠 `clap-wrapper` 外层包裹，**这是当前最大的技术未知量**（仓 CMake + Objective-C++ + 仅 CLT 环境）。建议 M0 的第一个 commit 就是：一个空白 nih-plug 插件 → 打成 CLAP → 包成 AUv2 → `auval` 通过。**链路不通就不要往下写 DSP。**

> M2 备注：9V 电池内阻 / 电源退耦 对 Fuzz 类音色影响巨大，不可当作理想电压源处理。

---

# 6. 已知风险清单

| # | 风险 | 缓解 |
|---|---|---|
| R1 | WDF 符号/拓扑错误静默发生（见 3.5.1） | M1 对拍基础设施前置；每级必须有数值 oracle |
| R2 | 无延迟环路导致要么加延迟（相位误差）要么整块牛顿（CPU） | ADR-002 在 M0 定案；先用 MNA-oracle 量化 B 方案的相位误差 |
| R3 | ~~GPL 污染（Guitarix / ngspice / FAUST）~~ → **已解除**：项目本身是 GPL-3，上述均可使用；ngspice 核心更是 BSD-3 | 保留 §8 依赖许可证登记表；每个引入的第三方代码在源码里保留 SPDX 头 |
| R8 | **nih-plug 不造 AU**，需靠 `clap-wrapper` 二次打包（CMake + ObjC++） | M0 第一件事就打通；固定 clap-wrapper 版本；不行则备选：手写 AUv2 壳（AudioUnitSDK）或用 `nice-plug` 分支 |
| R9 | 本机 Intel + macOS 13 属 Homebrew Tier 3，**无 bottle**，ngspice 需源码编译 | 走 ADR-002 更新后的顺序：自研 MNA oracle 先行，ngspice 后置；真需要时 `./configure --without-x --disable-openmp` 手编 |
| R10 | **GPL-3 与 AAX 不兼容**（Avid 专有协议 vs 传染性） | 不做 Pro Tools（已定）；将来若要支持必须重新评估 |
| R11 | AU 在 Logic 中加载需要正确的 Info.plist / 签名；分发需 Developer ID + 公证 | 开发期用 ad-hoc 签名；M4 处理公证 |
| R12 | 固定拓扑硬编码 → 后期改电路要重写代码 | `rustomp-circuits` 用声明式网表 DSL，元件值参数化，拓扑改动尽量只改数据 |
| R13 | 本机 `AudioComponentRegistrar` 缓存损坏 → `auval` 与 Logic 均看不到任何第三方 AU | 需 `sudo killall -9 AudioComponentRegistrar` 或重启（E5）；期间用 `tools/au-smoke` 做验证 |
| R14 | AU 在 Apple Silicon 上无法在本机实测（本机是 Intel） | arm64 切片目前只验证到“编译产物存在”；发版前需在 Apple Silicon 上跑一次冒烟测试 |
| R15 | `vst3-sys` 在 nih-plug 里是 **git 分支依赖**（非固定 rev） | 必须把 `Cargo.lock` 入库； CI 用 `--locked` |
| R4 | 仅 CLT 无完整 Xcode，AU 导出/`auval` 可能受阻 | M0 第一件事就打通 VST3 与 AU 两条构建链 |
| R5 | `nih-plug` 需 git 依赖且镜像索引不到 | 固定 commit hash；考虑 vendoring 或用 `nice-plug` fork |
| R6 | 非线性求解在极端输入（大瞬态、DC 阶跃）下不收敛 → 爆音 | 牛顿迭代次数上限 + 阻尼 + 失败回退到上一解 + NaN 熔断 |
| R7 | 目标效果器类型未定，难度可能被严重低估（Fuzz ≫ 过载） | §7 Q1 必须先答 |

---

# 7. 待确认问题

### 已确认（2026-10-23）

| # | 问题 | 回答 |
|---|---|---|
| A1 | 哪类效果器 | **二极管削波类失真**（首款）；仍是踏板类，不涉 BBD/混响 |
| A2 | 开源/闭源 | **GPL 开源** → 可放心复用开源成果（ADR-005） |
| A3 | 目标平台 | **macOS / Logic（AU）优先**；不做 Pro Tools |
| A4 | 网表来源 | 已有 `amonisachicken/breadspice`（GPL-3）可复用（E2） |

### 待确认

1. **自研电路的原理图/元件清单？** 能否提供（哪两个二极管、运放型号、供电 9V？）—— 这直接决定 M2 的网表。
2. **硬件 A/B 条件**：能否录成对测试信号（同一干声过硬件 vs 过插件）？有多少台样机？这是验收的唯一硬标准。
3. **Logic 版本 + 最低 macOS 版本？**（影响 AUv2/AUv3 选择、是否需要 arm64 通用二进制）
4. 是否需要同时发 **VST3**？（clap-wrapper 可一次出，成本几乎为零）

---

# 8. 真实可用的参考资料

| 类别 | 资料 | 备注 |
|---|---|---|
| 理论根基 | Fettweis, *Wave Digital Filters: Theory and Practice* (1986) | 适配器公式的原始出处 |
| 教科书 | Wanhammar *Digital Filters*（WDF 章）；Zölzer *DAFx* 2nd ed. | |
| 非线性 WDF | **Kurt Werner 博士论文 + `WaveDigitalFilters.jl`** | 含 BJT/电子管非线性与牛顿法，最贴近本项目需求 |
| 生产级参考实现 | `chowdsp_wdf`（Jatin Chowdhury，C++ header-only） | 读代码学工程细节（含近似 exp、SIMD） |
| 教学/原型 | `pywdf`（Python）、`wdmodels`（C++） | 算法验证 |
| **本项目可直接复用** | **`amonisachicken/breadspice`**（GPL-3）：ngspice 驱动、`models.lib`、网表生成 | 见 E2 §3.7 |
| AU 打包 | **`free-audio/clap-wrapper`**（MIT）+ Apple **AudioUnitSDK**（Apache-2.0） | CLAP → AUv2/AUv3/VST3 |
| 插件框架 | **`robbert-vdh/nih-plug`**（ISC，git 依赖） | VST3 + CLAP，无 AU |
| 可链接（GPL-3 项目） | Guitarix（GPL-3）、ngspice（核心 BSD-3） | 现在没有许可证障碍，但 ngspice 仍不可用于实时音频 |

---

# 9. 变更记录

| 日期 | 内容 |
|---|---|
| 2026-10-23 | 建立本文件；完成阶段 0 可行性验证与 E1 实验；勘误 `prepare.md`；确定 ADR-001/003/004，ADR-002 待定 |
| 2026-10-23 | 用户确认：**GPL-3 开源** / **二极管削波失真** / **macOS+Logic(AU) 优先** / 提供 breadspice 仓库。新增 E2（breadspice 侦察）、E3（ngspice 安装受阻）；新增 ADR-005/006/007；修正 §2.5（ngspice 实为 BSD-3）与新增 §2.7（nih-plug 无 AU）；ADR-002 改为自研 MNA oracle 优先；风险表重写并新增 R8–R12 |
| 2026-10-23 | **定位 Logic 看不到插件的真因**（E6）：① 我们的 AU plist 缺 `AudioComponents`（增量构建被 CMake 默认模板覆盖）→ 已在构建脚本中无条件修复 + 断言 + 冒烟测试内校验；② 本机 system 域 `AudioComponentRegistrar` 已死（exit -9），客户端看不到任何第三方 AU（BYOD 对照同样失败）→ 需 `sudo` 修复或重启 |
| 2026-10-23 | **修复窗口重开后界面消失**（E22）：TextureHandle 绑定旧的 egui Context，重开窗口后失效；在 `build` 回调（每次 spawn 执行）里清空纹理缓存 |
| 2026-10-27 | **生态边界成文**（E69）：新增 `docs/ecosystem.md`（仓库地图 / 三个契约 / `[requires]` 版本约束 / 文件归属表 / 拆分顺序与完成标准 / 六条风险）；定下"拆仓期只搬不改"与第 0 步的契约可执行测试 |
| 2026-10-27 | **文档防呆**（E79）：README 顶部新增「依赖与获取方式」（布局/三段命令/7 行系统依赖表/自查）；BUILDING 补"三仓版本配套"表与两条常见问题；MAKING-YOUR-OWN 补 fork 后要改 CI repository。插件打 v0.1.7；三份 md 链接全部有效、无绝对路径 |
| 2026-10-27 | **第二次隔离验证：专测依赖引导**（E78）：从"只有插件仓"起步，照提示一路补齐框架与打包工具 → 6 条 ARTIFACT、RMS 0.356170、零环境变量。顺手修掉"cd 后相对路径失效"与"硬编码 packer tag"（新增 `[requires] packer_tag`，插件打 v0.1.6） |
| 2026-10-27 | **依赖位置统一为同级目录**（E77）：删掉 `~/develop` 之类绝对路径假设与 `$HOME/develop` 兜底；提示改为"目录树 + 一条 cd 命令"；CI 与 verify-isolation 都不再设环境变量。隔离回归 `v0.1.0 + v0.1.5 + v0.1.5`（179s，RMS 0.356170） |
| 2026-10-27 | **新机指引**（E76）：缺框架时打印克隆命令（tag 自动取自 Cargo.toml）+ "放同级目录"最省事路径；漂移守卫不再静默跳过（`RUSTOMP_SKIP_DRIFT_CHECK=1` 才能跳）；packer doctor 同步。模拟新机验证 + 隔离回归（v0.1.0 + v0.1.4 + v0.1.4，172s） |
| 2026-10-27 | **CI 真跑一次**（E75）：第一次是**假绿**（缺 cargo-nih-plug + `| tee` 吞掉失败 + 上传空目录只警告）→ 加 pipefail / if-no-files-found: error / 产物数量断言；再修 `pipefail` + `grep -q` 的 SIGPIPE 误报。最终 4m22s 真绿：6 个产物，下回本机 au_smoke 实测 RMS 0.356170 |
| 2026-10-27 | **隔离工作区验证**（E74）：全新目录 clone 三仓（钉 tag）跑 build.sh，抓出两个"本机看不出来"的缺陷 —— ① tag v0.1.0 的 pack.sh 因全角变量 unbound（修复只留在工作区未提交）→ packer 打 v0.1.1；② identity.rs 生成物漂移（改 url 没重生成）→ 插件打 v0.1.2，并把 test 也加上 --check。第三轮通过：3 分 39 秒出 6 条 ARTIFACT、RMS 0.356170、依赖全为 git tag、本机仓零污染 |
| 2026-10-27 | **收口：三仓全钉 tag + 参考插件即模板**（E73）：packer 打 v0.1.0、CI 钉 tag；sparklin-drive 新增 `MAKING-YOUR-OWN.md`（四步改造 + 发布清单 + 六个坑） |
| 2026-10-27 | **拆仓第 2 步：参考插件独立发布**（E72）：sparklin-drive 仓（依赖钉 rustomp tag `v0.1.0`）；`scripts/build.sh` 四子命令（含构建前 `rustomp-gen --check`）；CI 三仓 checkout；**全新 clone → 一条命令 → 6 条 ARTIFACT**，RMS 0.356170 不变 |
| 2026-10-27 | **拆仓第 1 步：macOS 打包独立**（E71）：新增 rustomp-mac-plugin（pack.sh 契约 B + doctor.sh），框架侧 build-all.sh 变瘦包装；新增 tests/packer_contract.sh（ARTIFACT 透传 + 框架无平台词）并进 verify.sh；拆前/拆后 au_smoke 与 RMS 0.356170 完全一致 |
| 2026-10-27 | **拆仓第 0 步：工作区自洽**（E70）：全新 clone 的 workspace 失败已修；发现 `cargo-nih-plug` 会 chdir 到**最顶层** `Cargo.toml` → **插件必须住在本仓之外**；插件移到 `../sparklin-drive`，`new-plugin.sh` 改为生成到 `$RUSTOMP_GUEST`；Sp02/Sp03 重建通过 |
| 2026-10-27 | **真实下载件完整安装**（E68）：用户自绘设计（3 个导入 SVG）8 文件 zip → 装上 Sp02/Sp03；`rustomp-gen` 写 0 个文件（只改美术 → 只动 panel.png）；修掉 CLI 不认 `imported[]` 的 bug；RMS 逐位不变 |
| 2026-10-27 | **用设计器导出的文件重做 GUI**（E67）：只拿 `ui.toml` + `panel.svg` 走完 6 步交接；LED/脚踏半径已改"由素材推导"（2.249→2.247、7.2→7.175）；补 `docs/06-export-runbook.md` |
| 2026-10-26 | **踩钉不旁通 / 灯不灭**（E66）：① 手写 `ParamAccess` 桥漏 `bypass` 分支 → 参数桥改由 `[[params]]` 生成 + 覆盖测试；② 旁通时电路仍在跑 → `activity × mix`；③ `panel.rs` 生成搬进 `rustomp-gen`；④ 脚踏增 `image_hold`；`build-all.sh` 加"先重生成" |
| 2026-10-26 | **D0.5：设计器产物进发布路径**（E65）：面板/旋钮/拨杆贴图与 `[assets]`+`[ui]` 全部由设计器生成，音频 RMS 逐位不变；新增 `generated/assets.rs`（素材字节由生成器提供，`editor.rs` 不再写死素材名）；修了 `--panel-rust` 的整数→f32 与 include 路径层级两个坑 |
| 2026-10-26 | **box-designer D0**（E64）：设计器可用（零依赖 ES modules + 原生 SVG），复刻真实 Sparklin Drive 面板，导出 panel.png 720×1344 / knob 180×180；**契约首次被独立实现消费且无需改动**；记下"按钮 radius 必须盖住美术"与三个素材内联坑 |
| 2026-10-26 | **全项目复盘 + 平台策略**（E63 + ADR-015）：链路五段全部完成（器件/MNA/实时/生成/界面），91 用例 / verify.sh 8 层；确立 **macOS 默认 + Windows/Linux 独立打包工具仓库**；逐一校对 9 份文档（新增 `crates/rustomp-gui/README.md`）；沉淀七条经验 |
| 2026-10-26 | **S1d 联调：三个界面 bug**（E62）：① LED 不闪 = 生成的 `Ckt` 没实现 `activity()`（补二极管电流指标 + 网表指令 `activity_i=`，0.032 → 0.647）；② 踩钉无效 = 没有 bypass 参数（Engine 加 `set_bypass`、`bypass` 成为保留 id、下发 `[[ui.buttons]]`，干路偏差 0.00e0）；③ 旋钮边缘被切 = `decode_png` 提前把 alpha 与固定底色合成（改为保留 alpha 交给 GPU）。面板素材换成用户新给的 mm 制 SVG（knob 15mm / 3pdt 10.76mm / led 6.27mm） |
| 2026-10-26 | **S1d：第一个带界面的插件**（E61）：素材坐标改毫米（60×112 mm、12 px/mm = 720×1344）；panel 由 Inkscape 渲染 legacy SVG，knob 程序生成（指针朝上=50%），拨杆复用 legacy 贴图；生成器在 gui=true 时补 `editor_state` 与 `pub mod panel`；手写只有 `editor.rs`（~100 行）。实测：validate_panel 通过、AU 报 CocoaUI、音频与无 GUI 版一致 |
| 2026-10-25 | **GUI 契约升级**（E60）：LED/拨杆/按钮 Option<T> 改为数组（数量不限）、控件 id 改为 (kind,index)、旋钮支持**整图旋转**（素材指针朝上 ⇔ 参数 0.5，角度范围必须对称）、新增固定绘制顺序常量与 validate_panel() 校验器；plugin_meta.py 同步（travel_deg、[assets] 全键导出）；examples/panel_check.rs 编译+校验生成的 PanelSpec |
| 2026-10-25 | **进 GUI 前的清单 + AB 复盘补全**（E59）：新增 `docs/pre-gui-checklist.md`（9 节逐项状态 + GUI 依赖的接口 + 未完成项优先级）；AB 复盘补 §10（自适应迭代）、§11（开关）、§12（最终状态与"同一台机器"的定义）、§13（13 条方法学收获）；README 路线图/状态表同步（并把"稀疏 LU"划掉——E57 证明不是瓶颈） |
| 2026-10-25 | **双分支 + 开关切换**（E58）：框架新增 `Branch<A,B>`（两份完整电路 + 5 ms 预热 + 10 ms 淡化，空闲时只跑当前分支）；生成器新增 `params_builder`；插件拆成 asym/sym 两份网表。实测：非对称 THD 27.58%、对称 h2 **0.000 %**、切换跳变比值 0.99（无咔哒）；教训：切进来的分支必须先"预热"，否则冷状态瞬态会从淡入窗口漏出 |
| 2026-10-25 | **自适应牛顿迭代**（E57）：`step_fixed` 改为「iters 为上限 + 残差提前退出」（tol=1e-6），实测平均只需 **2 次**（配对测量：比固定 8 次快 **3.0×**、输出差 −227 dB）；LU 裁剪固定节点只值 1.09×（瓶颈在每样本固定开销）。另记测量坑：本机绝对性能一小时内漂移 3–5×，CPU 数字必须配对测量 |
| 2026-10-25 | **4 次迭代仍欠收敛 → 提到 8 次**（E56）：用「同一条带限方波」喂 ngspice 定案，4 次时基频偏 +0.76 dB、偶次 ±10…14 dB；8 次为 +0.0004 dB（16 次不再变）。改 `newton=8` 后偶次差值 −51.8→**−64.4 dBFS**、THD 27.579%（与离线一致）；CPU 单声道 73% / 立体声 146% → 下一步做自适应迭代。另记测坑：机器降频会让基准虚高约 3× |
| 2026-10-25 | **修偶次谐波对比音的归一化事故**（E55）：`abex.wav` 曾对三段**各自**峰值归一化，把差值真实低 12.3 dB 抹成"一样响"（用户当场听出）。改成三段共用增益（差值 −20.9 vs −8.6 dBFS），并新增决定性实验：`swap_a/swap_b`（原始电平，只换偶次谐波，两者差 −51.8 dBFS）。教训：对比两段绝不能用各自归一化 |
| 2026-10-25 | **偶次谐波对比音**（E54）：新增 `examples/abeven.rs`（FFT 置零非偶次 → IFFT 重建 → 最小 WAV 写入），两侧 `abfft` 加 `AB_DUMP_DIR` 波形 dump。数字：偶次分量 −39.4/−38.9 dBFS、**偶次差值仅 −51.8 dBFS**；产出 `ab-signals/{even_v2,even_legacy,even_diff,full_diff,even_abex}.wav`（不入库，可再生）| 
| 2026-10-25 | **Rust FFT 复核 AB**（E53）：新增 `crates/rustomp-dsp/examples/abfft.rs`（框架侧台架，用入库夹具）；1 kHz 正弦/方波两侧全量对比：9–10 kHz 无隆起（h9 +0.22 dB、h10 −5 dB）、极低频 −103.7 dB 两边一致；真实差异仅奇次 +0.2 dB（THD +0.5 pp）、偶次在 −42…−68 dB 处相对差大但听不见。结论：可听范围内 ≤0.5 dB |
| 2026-10-25 | **AB 测试复盘落盘**（E52）：新增 `docs/ab-test-sparklin-v2.md`（全过程时间线 + 四张 FFT 判读 + 量化对照 + 9 条经验 + 发布前清单）；沉淀四条规则：测试信号要分层（正弦查不出欠收敛）、验收要含「不该出现的分量」、参数端点是 bug 温床、数值实现层就是音色的一部分 |
| 2026-10-25 | **修电位器端点静音**（E51）：`level=1` 时下臂 0 Ω、`gain=0` 时反馈电阻 0 Ω → `1/0=∞` → NaN → 引擎熔断静音。框架在唯一装配入口把电阻夹到 `MIN_RESISTANCE`(1 mΩ)；新增回归测试（8 个端点全部有限 + 音量单调）。教训：参数端点是 bug 温床，框架必须比使用者更保守 |
| 2026-10-25 | **修 AB 对比暴露的次谐波 bug**（E50）：`NEWTON_ITERS=2` 在方波/快速边沿下欠收敛 → 次谐波极限环（f/8 +12 dB、f/16 +19 dB，正弦完全看不出来）。改成网表指令 `newton=`（默认 3，Sparklin Drive 用 4），THD 27.65→27.58%，方波次谐波 +19→−70 dB，代价是 CPU 2 倍（单声道 36%/立体声 72%）。新增回归测试：方波次谐波必须 < −60 dB |
| 2026-10-25 | **第一个全生成产物：Sparklin Drive v2（无 GUI）**（E49）：网表 → 电路、toml → 外壳、build-all 出 AU/VST3/CLAP；引擎级 THD **27.65%**（基线 27.568%）、5× 实时、AU 单/立体声自检通过并已安装。踩坑：plugin_meta 在无 GUI 阶段崩（assets/ui 需可选）、`nih_export_vst3!` 分号不能删、生成常量需 allow(dead_code) |
| 2026-10-25 | **S1c：插件侧代码生成**（E48）：新增 `crates/rustomp-gen`（`rustomp.toml` → identity/params/plugin_impl + `--check`）；模板改为无 GUI 外壳（删 build.rs、lib.rs 只剩 `mod generated;`）；`new-plugin.sh` 自动生成。实测：生成的项目 `cargo build` 通过、cdylib 导出 `clap_entry` + `GetPluginFactory`。踩坑：结构体叫 `Params` 与 nih-plug trait 撞名（改 `PluginParams`）、heredoc 反引号被执行 |
| 2026-10-25 | **把对拍铺进 CI**（E47）：新增 `scripts/verify.sh`（本地/CI 同一入口，分层 + markdown 报告）、`Oracle::discover_or_skip()` + `RUSTOMP_REQUIRE_NGSPICE`（缺 ngspice 在 CI 里是**失败**而不是静默跳过）、CI 拆 fast/oracle/fmt 三个 job 并把对拍数字写进 job summary。全量约 50 s |
| 2026-10-23 | **阶段收尾：文档对齐 + 外部设计器契约**（E46）：README 状态/质量把关/路线图重写并新增「与外部设计器的接口」；`circuit-import.md` 状态改为已实现、明确映射分层只做了 1+6 级、新增指令与产物章节、新增设计器接口；`declarative-build.md` 拆 S1a/S1b/S1c 并新增 §3.6；新建 `crates/rustomp-import/README.md`。三条硬要求（端口显式、参数显式、型号闭集）落盘 |
| 2026-10-23 | **S1b：参数化 + 器件卡片**（E45）：`param/switch` 指令生成 `Params` + `impl Circuit` 的 `Ckt`；Q/J/M 卡片映射。验证：默认参数 THD 27.677%、对称开关把 h2 从 4.00% 打到 0.00%、GAIN 扫描单调；BJT 前级 Vb 差 2.2 µV、Ic 差 0.26%（未建模 Nk）。踩坑：`solve_dc` 的提交错误地应用了压摆率限制（运放状态被钳到 SR/fs，开场凭空多出瞬态）、`set_params` 用旧值、参数下标必须是 const |
| 2026-10-23 | **S1a：网表 → MNA 代码生成**（E44）：`rustomp-import --mna` 从 SPICE 网表生成装配代码；导线合并（0.001Ω）、闭集模型映射（未命中即报错）、DC 源→固定节点、信号源→输入端口；生成电路 THD 27.579% 与手写网表**逐位一致**。踩坑：`FIXED_IN` 写死 0 导致音频被写进 9V 轨（编译通过、不报错、THD 只有 0.237%）→ 生成常量必须由生成器算，且必须有行为级回归 |
| 2026-10-23 | **MNA 实时化**（E43）：`step_fixed` + 固定电位节点 + 解析二极管雅可比；2 次迭代 2072 ns/样本（192 kHz 单核 39.8%），THD +0.098 pp；相对 legacy 手推实现（824 ns）慢 2.5×，换来任意拓扑能力。踩坑：放宽内层牛顿容差会让雅可比失真、单次迭代时线性外推反而有害 |
| 2026-10-23 | **Sparklin Drive 的 MNA 重写 + 回归验证**（E42）：整机 THD 27.579% vs ngspice 27.5682%（+0.011 pp，h2–h9 ≤0.03 pp）、DC 工作点 ≤0.27 mV、频响 ≤0.5 dB、削波不对称方向一致。抓到真 bug：`solve_dc` 用 `dt=1` 装配电容，1 µF 变成 1 MΩ 漏电（DC 偏 41 mV）→ DC 必须开路。测量坑：低频预热不足导致 50 Hz 偏高 4 dB |
| 2026-10-23 | **补齐日常所需器件**（E41）：从 breadspice 迁入 `1N5817`（肖特基）、红/绿/蓝 LED、`BC559C`(PNP)；对拍全部 0.00%（LED 与肖特基 1 µA~10 mA、BC559C 的 Ic/Ib ±0.01%）。踩坑：`voltage_for_current` 的二分上界写死 2 V，把绿 LED 截断成 2.00000 V（差 9.3%）——**误差恰好是整数时先怀疑工具** |
| 2026-10-23 | **晚间收尾：文档与现状同步**（E40）：README 重构（三层定位、器件级/装配器级对拍表、限制与路线图按现状重写）、`crates/rustomp-dsp/README.md` 全面重写（器件库 + MNA + Engine）、`docs/component-library.md` §8/§10 修正过期项（运放输出级已实现、2N7000 标定值修正为 0.196 A）。4 个提交：`5a46a86`(二极管) `faca331`(BJT) `4e9ef08`(JFET) `43e775c`(MOS+运放) `f4ea745`(MNA) |
| 2026-10-23 | **通用 MNA 装配器**（E39）：`Element` 枚举 + 数值雅可比多端装配 + gmin 步进牛顿；5 个对拍测试（削波/BJT/JFET/RC 瞬态/运放）全部通过（DC µV 级、瞬态 17 mV）。踩坑：gmin 循环写错留 1kΩ 漏电、变步长未重采样、运放 DC 与瞬态方程不同、192 kHz 下 -3dB 偏低 26% |
| 2026-10-23 | **器件库收官：MOSFET + 运放**（E38）：2N7000 沟道/体二极管全 0.00~0.01%（顺带修正 SPICE 子电路两处 bug：Rs 悬空、MOS 内建结抢体二极管）；运放单极点宏模型离散化三方案对比，最终用精确极点（Tustin 在 54 kHz 扭曲 36%、显式欧拉发散） |
| 2026-10-23 | **器件库第三块：JFET**（E37）：Shichman-Hodges 静态 D3 + Rd/Rs 的一维牛顿；`J201` 转移/输出特性与 ngspice **0.00%**（含三极管区） |
| 2026-10-23 | **器件库第二块：BJT**（E36）：Gummel-Poon 静态 D3（含 Rb/Re/Rc 的 2 维内层牛顿）+ BC549C/AC127/AC128；Ic 与 Ib 对拍：Ge 两管 ≤0.01%、BC549C ≤0.09%（大电流点 0.67%，疑 Nk 未建模，不猜）；踩坑三条（手推雅可比错、PNP 电压约定、Ib 测错） |
| 2026-10-23 | **器件库第一块：二极管**（E35）：`devices/` 拆成 parsed/handwritten/reference 三段 + `build.sh`；手写 2N7000 子电路（Id@Vgs=5V=0.221A，Rds≈6Ω）与实测 1N4007（Cjo=15p）；Rust 侧通用 `Diode` + 三个型号，V-I 与 ngspice **误差 0.00%**（发现并修正 VT 应为 27°C） |
| 2026-10-23 | **导入分立器件模型**（E34）：新增 `tools/meta/ltspice_models.py`（按 ngspice 参数白名单清洗），产出 `devices/{devices.lib,devices.json,REPORT.md}`；锗管/锗二极管/1N4007 均验证可用（Ge: Vbe=0.142V, hFE≈130；1N34A Vf@1mA=0.253V）；发现 2N7000 的 VDMOS 格式 ngspice 不支持，运放只有符号无模型 |
| 2026-10-23 | **声明式构建规范落盘**（E33 + ADR-014）：代码生成（host，入 `src/generated/`）与打包（各平台原生）两个平面；打包后端契约 + 能力声明；Windows/Linux/standalone 入路线，AAX 排除；S1–S5 路线与 CI 矩阵验收 |
| 2026-10-23 | **电路导入与映射规范落盘**（E32 + ADR-013）：导入器插件化；格式先网表后 `.asc`；映射分层（别名+项目映射常规、类别默认防呆、参数直通为 power user）；引脚按名绑定；替换必须可见可测 |
| 2026-10-23 | **元件库精度规范落盘**（E31 + ADR-012）：确立「延迟与精度无关」「静态/动态双轴」「静态给足、动态按需」；逐器件给出档位（锗 BJT 需 D3、锗二极管 D2、1N4007/2N7000 到 C1）；写出验收实验模板与 CPU 预算。文档：`docs/component-library.md` |
| 2026-10-23 | **插件拆分为独立仓库**（E30）：`sparklin-drive-legacy`（独立 git 仓库，完整快照，可脱离框架构建）；主仓库变为纯框架，`/plugins/` 成为下游输出目录，脚本改为按 `PLUGIN_DIR` 定位 |
| 2026-10-23 | **收尾 1/2/3**（E29）：历史残留归档到 `experiments/`（新增 README 说明不参与构建）；README 全文路径校对（脚本扫描 + 工作流一节重写）；新增 GitHub Actions CI（macOS，装 ngspice 则跑活体对拍，fmt 暂不阻塞）。本地 `cargo test --workspace --release` 17 个目标全绿 |
| 2026-10-23 | **架构调整第 6 步：脚手架**（E28）：`template/` + `scripts/new-plugin.sh`，一条命令生成同构空插件并注册 workspace；实测生成的 Demo Drive `cargo check` 通过、测试 2 项通过 |
| 2026-10-23 | **架构调整第 5 步：可插拔导入**（E27）：新增 `rustomp-import`（IR + `Importer` trait + breadspice 实现 + `--spice/--skeleton/--json` CLI）；生成的元件行与旧管线逐行一致，**删掉 nlgen 与 bread_to_circuit.py，框架不再依赖外部 git 仓库** |
| 2026-10-23 | **架构调整第 4 步：对拍接口**（E26）：新增 `rustomp-oracle`（ngspice 驱动 + ASCII rawfile 解析 + **变步长重采样** + 统一测量函数）；插件新增活体对拍测试，h2–h9 最大偏差 0.056 pp；删除 Python 解析器 |
| 2026-10-23 | **架构调整第 3 步：通用界面层**（E25）：新增 `crates/rustomp-gui`（PanelSpec/控件/ParamAccess/纹理生命周期/draw），界面声明由 build.rs 从 rustomp.toml 生成；插件外壳 559→296 行，框架内无素材与坐标 |
| 2026-10-23 | **架构调整第 2 步：框架/电路分离**（E24）：引入 `Circuit` trait 与 `Engine<C>`；`fakedist` 移入 `plugins/sparklin-drive/circuit`，插件外壳移入 `plugin/`，身份由 `build.rs` 从 `rustomp.toml` 生成；框架内已无任何插件痕迹 |
| 2026-10-23 | **拨杆对齐 + 按下缩小修复**（E21）：两态贴图六边形基线相差 1.53，改为分别偏移（以水平两顶点连线为基准）；按下反馈改用 `Response::is_pointer_button_down_on()` 并以六边形基线中点为缩放原点 |
| 2026-10-23 | **开源发布**（E20）：定位为「Rust 电路建模框架 + Sparklin Drive 示例插件」并新增根 README；拨杆发糙的真因是 PNG alpha 被忽略（改不透明导出）；拨杆改为底图删除 + 独立贴图；已推送 GitHub |
| 2026-10-23 | **界面打磨**（E19）：拨杆用真实双状态贴图（六角形对齐、无重影）、LED 加外发光拉开对比、bypass 改为按住缩 0.9 倍（松开恢复）、厂商名统一为 HONEYPIE EFFECTS |
| 2026-10-23 | **GUI 完成 + 新功能 + 改名**（E18）：egui + 内嵌 PNG 面板；新增对称削波模式（ngspice 参考 h2 3.92%→0.049%）、true-bypass（15 样本对齐 + 5ms 淡化）、削波指示 LED；插件改名 **Sparklin Drive / HONEYPIE EFFECTS**（`aufx/SPm1/SPs1/HpEf`）。au-smoke 确认 4 参数 + CocoaUI + 无乐器/MIDI 属性 |
| 2026-10-23 | **无 GUI 阶段冻结**（E17）：打 tag `v0.1.0-nogui`，二进制存档到 `dist/v0.1.0-nogui/`（3 变体 × 3 格式 + SHA256 + README）；评估面板 SVG（竖版 1:1.87，指针是画死的 rect、字体未嵌入、透明底），GUI 方案定为 resvg + nih_plug_egui |
| 2026-10-23 | **CPU A 类优化完成**（E16）：1592 → 824 ns（1.93x），CPU 15% → **7.8%**，h2–h9 逐位不变。关键手段是外层牛顿线性外推（迭代 3.48→2.00）；记录了 6 个「实测无效」的方向。多线程原型实测 1.97x，但因单声道变体零收益 + 实时风险，暂不启用（ADR-010） |
| 2026-10-23 | **技术评估**（E15）：实测 CPU 分解（瓶颈＝二极管内层牛顿，FIR 仅 4.6%，f64→f32 无收益），给出优化收益表与音色验收标准；梳理 GUI 三层技术栈（AUv2 CocoaUI → clap-wrapper 桥 → baseview → vizia/egui），新 ADR-009 待拍板 |
| 2026-10-23 | ✅ **M0 完成**：Waves 式声道变体生效，`Rustomp Fuzz Mono` 在 Logic 单声道音轨中出现（E14.5）。新增 `scripts/build-all.sh` 作为标准构建入口；确立硬性约定「每个插件都出 mono/stereo 两个独立身份」（ADR-008） |
| 2026-10-23 | **Waves 式声道变体**（E14）：新增 `mono`/`stereo` feature 与 `scripts/build-variant.sh`，产出三个独立身份（RFz1 双布局 / **RFm1 单声道** / **RFs1 立体声**），均已安装并通过渲染自检。发现 Logic 的 pref 缓存从未真正刷新（`MIDIProtocol` 仍是修复前的 1）——Logic 运行时会把内存值写回，故新增 `reset-logic-cache.sh`（先查 Logic 是否在跑）。另修 bash 3.2 的全角括号变量名坑 |
| 2026-10-23 | **修复 Logic 菜单里找不到（E13 续）**：真因是声道布局——nih-plug 有 Mono+Stereo 两种 `audio-ports-config`，但 clap-wrapper 只认 `configurable-audio-ports`，导致 AU 只报 `{2,2}`，单声道音轨的插入菜单把它过滤掉。补丁让 wrapper 用 `audio-ports-config` 枚举并 `select()`；实测 AU 现在同时报 2/2 与 1/1，单/立体声渲染均正常 |
| 2026-10-23 | **修复 Logic 分类错误**（E13）：clap-wrapper 无条件宣称 MIDI 属性 → Logic 缓存 `MIDIProtocol=1` → 被当成 MIDI 控制效果器只出现在软件乐器槽。用 `patches/0001-*` 让 AU 只在真有 note port 时才宣称 MIDI；au-smoke 增加该检查 |
| 2026-10-23 | **修复自激 bug**（E12）：定位到牛顿无阻尼 + `v_out` 硬钳位不一致 + 状态无限幅，三者叠加导致瞬态后 ~10kHz 极限环；改为阻尼牛顿+残差判据+失败兜底+状态限幅+去掉钳位+引擎级复位。精度不变（h2 3.915→3.915），新增瞬态回归测试。同时取证：AU 是标准 `aufx`（1 进 1 出、{2,2}），Logic 插槽异常需对照实验 |
| 2026-10-23 | **4x 过采样 + 插件接线完成**（E11）：`Rustomp Fuzz`（aufx/RFz1/Amch）已安装；发现 **1x 直接跑 48k 电路会失效**（3kHz 以上输出归零），4x 是正确性下限；THD 27.648% vs 基线 27.616% |
| 2026-10-23 | **fakedist v1 的 WDF 实现完成并对拍通过**（E10）：DC 精确、频响 ≤0.31dB、THD 27.650% vs 27.616%、h2–h9 逐次误差 ≤0.03pp。踩坑：串联二极管的 N 也要翻倍（h2 归零的元凶）、`.four` 不能当基准。未做：4x 过采样、Cjo/tt、插件接线 |
| 2026-10-23 | **breadspice 管线跑通 + 基线建立**（E9）：`.bread → circuit.json → netlist.cir → op/ac/tran+THD` 一条命令；电路识别为「OP07 反馈环非对称二极管削波」；基线 THD 27.57%@0.2V、峰值增益 +28.42dB@2.72kHz；发现 0.5s 直流恢复（blocking distortion）与 3 个测量/解析坑 |
| 2026-10-23 | **ngspice 验证并装进 PATH**（E8）：ngspice-47 可用，`.tran`/`.four`(THD)/ASCII rawfile 实测通过；新增 `scripts/build-ngspice.sh`、`tools/oracle/smoke/`（含基线数字：1kHz/1V/10k/47nF → THD 0.3538%，基波 0.3199V） |
| 2026-10-23 | **导入 v1 网表**（E7）：判定为 EasyEDA Pro / PADS-PowerPCB ASCII（非 SPICE），写了 `tools/netlist/easyeda_asc.py` 解析器，生成 `circuits/v1-fuzz/{netlist.json,NETLIST.md,CIRCUIT.md}`。发现 3 个硬问题：无 OP07/1N4148、LED2 与 1N4007 同向并联疑似符号方向相反、9 处元件值冲突 |
| 2026-10-23 | **M0-a 完成**（E4）：workspace + nih-plug 空插件 + 通用二进制 CLAP/VST3 + clap-wrapper AUv2 + 安装 + 冒烟测试通过。新增 `scripts/fetch-third-party.sh`、`scripts/build-au.sh`、`tools/au-smoke`、`cmake/CMakeLists.txt`。踩坑：AudioUnitSDK 需 1.1.0、制造商码需大写、bundle id 被 nih-plug 写死、AU 参数名走 cfNameString。发现本机 registrar 缓存损坏（E5，新增 R13/R14/R15）。**ngspice-47 编译成功**（前置依赖：Homebrew readline 8.3，系统 4.2 无 `rl_reset_after_signal`） |

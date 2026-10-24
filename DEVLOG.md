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

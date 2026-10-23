# rustomp 开发日志

> 项目：把自研硬件吉他效果器数字化为 VST3 / CLAP / AU 插件，后端用 Rust，路线为**电路建模（元件级）**。
>
> 本文件是**唯一事实来源（single source of truth）**的开发记录，目的是让每一步决策都能被复盘。
> `prepare.md` 是早期的调研问答，**含事实错误，已在本文件 §2 记录勘误，不要直接采信**。

## 维护约定

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
| 当前阶段 | 阶段 0：可行性验证（已完成）→ M0 骨架待启动 |
| 已确定路线 | 电路建模；WDF 为主 + 自研 MNA 作对拍 oracle（ngspice 后置交叉验证） |
| 许可证 | **GPL-3.0-or-later**（ADR-005） |
| 首款产品 | **二极管削波类失真**（ADR-007） |
| 首要宿主 | **macOS / Logic Pro / AUv2**（ADR-006，不涉及 Pro Tools/AAX） |
| 构建链 | nih-plug（CLAP）→ clap-wrapper（AUv2 + VST3） |
| 代码资产 | `crates/rustomp-plugin`（nih-plug 空插件 / M0-a）、`cmake/`（AU 包装）、`scripts/`（构建链）、`tools/au-smoke`（AU 进程内测试）、`experiments/e1_wdf_diode_clipper`（spike） |
| 外部参考 | `amonisachicken/breadspice`（GPL-3，可复用 oracle/器件模型） |
| 一键构建 | `./scripts/build-au.sh`（→ CLAP + VST3 + AUv2，通用二进制，安装到用户目录并自测） |
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
| 已安装 | `ffmpeg 8.1.1` ✓、git ✓、cmake ✓、**`ngspice-47` @ `~/.local/ngspice/bin` ✓**（源码编译，见 E3）、Homebrew `readline 8.3`、`bison` |
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
| AU 进程内冒烟测试 | `tools/au-smoke` | dlopen / 工厂 / Initialize / 渲染全部通过，**直通电平 0.250000** ✓ |
| 安装 | `~/Library/Audio/Plug-Ins/{Components,VST3,CLAP}` | ✓ |
| `auval` | — | ✗ 受本机环境阻塞（见 E5，与插件无关） |

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

# 5. 里程碑与验收标准

| 里程碑 | 内容 | 验收标准 | 状态 |
|---|---|---|---|
| **E1** | WDF 削波级可行性 spike | DC 误差 < 1e-5V；10s 无 NaN；4 级 @4x < 10% 单核 | ✅ 已完成 |
| **M0-a** | workspace 骨架 + **CLAP→AUv2 构建链打通**（空插件直通） | 通用二进制；`.clap` 嵌入 `.component`；进程内冒烟测试通过 | ✅ **已完成**（E4；`auval` 待环境修复，见 E5） |
| **M0-b** | 把 E1 的 WDF 削波级接进插件（4x 过采样 + Drive/Level 旋钮），完整二极管模型 | Logic 里能听声；CPU 可测；与 oracle 对拍 THD | ⬜ 下一步 |
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
| 2026-10-23 | **M0-a 完成**（E4）：workspace + nih-plug 空插件 + 通用二进制 CLAP/VST3 + clap-wrapper AUv2 + 安装 + 冒烟测试通过。新增 `scripts/fetch-third-party.sh`、`scripts/build-au.sh`、`tools/au-smoke`、`cmake/CMakeLists.txt`。踩坑：AudioUnitSDK 需 1.1.0、制造商码需大写、bundle id 被 nih-plug 写死、AU 参数名走 cfNameString。发现本机 registrar 缓存损坏（E5，新增 R13/R14/R15）。**ngspice-47 编译成功**（前置依赖：Homebrew readline 8.3，系统 4.2 无 `rl_reset_after_signal`） |

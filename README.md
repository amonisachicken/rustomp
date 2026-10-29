# rustomp

**用 Rust 做电路级建模的音频效果器框架。**

> 目标：把「一块模拟效果器数字化」变成可复用、可验证的工程流程 ——
> 原理图 / 面包板 → SPICE 网表 → 与 ngspice 对拍的 DSP → VST3 / CLAP / AUv2 插件。

```text
原理图设计器 ─┐
             ├─ 网表 ──► rustomp-import --mna ──► Circuit（Rust）
面包板/手写 ─┘                                     │
                                                   ├─ Engine（2×/4× 过采样、旁通、NaN 保护）
面板设计器 ──► [ui] + 素材 ──► rustomp-gen ──► 插件外壳 ─┘
                                                   │
                        mac / win / linux 打包器 ──► ARTIFACT: <路径>
```

**本仓只做框架**：器件 / MNA / 实时 / 代码生成 / 面板契约 / ngspice 对拍。
平台打包与具体插件都是独立仓库。当前版本 **`v0.1.2`**（2026-10-29），
配套的打包器与参考插件版本见 [`docs/status.md`](docs/status.md)。

---

## 1. 产品介绍

它解决的是**「把电路变成插件」中间那段可验证的工程链**：

| 交付物 | 用来做什么 |
|---|---|
| **Rust crates** | `rustomp-dsp` / `rustomp-gui` / `rustomp-gen` / `rustomp-import` / `rustomp-oracle` |
| **器件库** | 闭集器件模型，参数与 SPICE 对齐，逐型号与 ngspice 对拍 |
| **生成器** | 网表 → 可实时运行的 `Circuit`；`rustomp.toml` → 插件身份/参数/面板 |
| **脚手架** | `template/` + `scripts/new-plugin.sh`：生成一个可构建的插件项目 |
| **验证体系** | `verify.sh` 9 层：fmt / 测试 / 契约 / 2×漂移守卫 / 四层 ngspice 对拍 |
| **契约** | `rustomp.toml`（A）、`ARTIFACT:`（B）、`[ui]`+素材（C），打包器/设计器据此接入 |

**面向谁**：想用「真实电路 + Rust」做效果器、并且要求**每个数字都能对拍**的开发者。
**不做什么**：不是 DAW、不是插件合集、不内置打包器、不提供黑箱神经网络建模。

### 当前状态（摘要）

| 项 | 状态 |
|---|---|
| 器件库 | ✅ 14 个型号（二极管×4 / LED×3 / BJT×4 / JFET / MOSFET / 运放） |
| MNA 装配器 | ✅ 任意拓扑、5 类器件、牛顿求解（DC µV 级 / 瞬态 mV 级） |
| 实时路径 | ✅ 自适应牛顿、大跳变子步进、2×/4× 过采样、分支/旁通、NaN 熔断 |
| 代码生成 | ✅ 网表 → 电路、`rustomp.toml` → 插件外壳、`[ui]` → 面板 |
| 设计器 | ✅ 原理图设计器（`.design.json` 可直读）/ 面板设计器 D0 |
| 打包 | ✅ macOS（AU/VST3/CLAP + DMG）、Windows（VST3/CLAP + MSI）、Linux（VST3/CLAP + tar.gz） |
| 参考插件 | ✅ `sparklin-drive v0.1.9` 三平台 prerelease 已发布（未签名） |
| 下一步 | 器件动态档补齐、A/C taper、预设、pkg + 签名、arm64 真机、设计器 D1 |

> 完整状态、已知限制与待做项见 [`docs/status.md`](docs/status.md)；
> 验证数字见 [`docs/verification.md`](docs/verification.md)。

---

## 2. 架构

| 层 | 位置 | 作用 |
|---|---|---|
| **器件库** | `crates/rustomp-dsp/src/devices/` | 二极管 / BJT / JFET / MOSFET / 运放，闭集 const 型号 |
| **电路装配** | `crates/rustomp-dsp/src/mna.rs` | `Element` + `Netlist<N>` + 牛顿求解（含实时档 `step_fixed`） |
| **实时机制** | `crates/rustomp-dsp/src/{engine,oversample,circuit,branch}.rs` | `Circuit` 契约、过采样、旁通、分支、NaN 保护 |
| **界面层** | `crates/rustomp-gui` | `PanelSpec`、参数桥、纹理生命周期、面板校验 |
| **工具链** | `crates/rustomp-{import,gen,oracle}` | 网表导入 / 代码生成 / ngspice 对拍 |
| **插件外壳** | `template/` + `scripts/new-plugin.sh` | nih-plug 外壳、单/立体声独立变体 |

```text
devices/                器件库（SPICE 侧）：models.add → devices.lib、手写子电路、对拍报告
crates/                 框架 crates（dsp / gui / gen / import / oracle）
template/               插件脚手架（new-plugin.sh 复制后替换占位符）
scripts/                new-plugin / build-all / verify / 网表 / 基线 / 隔离验证
tools/                  meta（读 rustomp.toml）、oracle（ngspice 冒烟）
tests/                  packer_contract.sh（契约 A/B）
docs/                   当前文档（status / architecture / verification / workflows / 规范）
docs/history/           历史归档（旧计划 / 旧实验 / 旧说明）
DEVLOG.md               历史：按 E 编号的决策、实验与踩坑
```

两条链都以**可验证的中间格式**为界：电路侧是 SPICE 网表（ngspice 可直接跑），
面板侧是 `rustomp.toml` 的 `[ui]`/`[assets]`（素材缺失在编译期报错）。
细节见 [`docs/architecture.md`](docs/architecture.md)。

---

## 3. 功能范围

### 能做

| 类别 | 内容 |
|---|---|
| **建模** | 闭集器件库；任意拓扑 MNA；参数化电位器/开关；两套求解器（MNA / 一维牛顿） |
| **精度** | 器件 V-I 逐点对拍；DC µV / 瞬态 mV；静态/动态精度分档（ADR-012） |
| **实时** | 自适应牛顿、大跳变子步进 + DC 恢复、2×/4× 过采样（延迟都 15 样本）、分支交叉淡化、true-bypass、NaN 熔断 |
| **生成** | 网表 → `Circuit`；`rustomp.toml` → 身份/参数/Plugin/面板；`--check` 漂移守卫 |
| **界面** | 声明式贴图面板（毫米坐标、12 px/mm、旋钮/拨杆/LED/3PDT） |
| **验证** | `verify.sh` 9 层 + 三平台 release workflow |
| **生态** | 三个平台打包器（契约 B）；原理图/面板设计器（契约 C） |

### 暂不（有明确边界）

| 项 | 状态 |
|---|---|
| 器件动态档 | BJT 结电容/渡越时间已补（含锗管扩散电荷）；二极管 `Cjo/tt`、MOSFET `Cgs/Cgd` 未做 |
| 电位器 A/C taper | 只有线性 B |
| 预设 / 状态管理 | 未做（编辑器窗口尺寸由 nih-plug 持久化） |
| `pkg` 安装包 / 签名公证 | 未做；三个平台安装包是 DMG / MSI / tar.gz，且**未签名** |
| Windows / Linux arm64 | 打包参数与静态校验就绪，仍待真机验证 |
| 设计器 D1 | 拖拽连线、更多控件；当前 D0 可用 |
| `--gui` 脚手架 / `panel.rs` 单一发射器 | 仍要手工加 `gui` feature；`panel.rs` 仍由 `plugin_meta.py` 生成 |

### 明确不做

| 不做 | 原因 |
|---|---|
| **AAX** | 需要 Avid 授权，且与 GPL-3 冲突 |
| **standalone** | 需要音频设备/设置持久化，三个打包器都明确不做 |
| **DAW / 宿主** | 本项目只做插件侧框架 |
| **黑箱神经网络建模** | 目标是可验证的电路级建模，不是采样/拟合 |
| **平台打包** | 独立仓库（ADR-015）；本仓只做分派与契约 |

---

## 4. 部署与运行

### 依赖

| 依赖 | 用途 |
|---|---|
| Rust stable | 编译框架与插件 |
| `python3 ≥ 3.11` | 读 `rustomp.toml`（`tomllib`） |
| `ngspice`（推荐） | 活体对拍；本地缺失会跳过，CI 里缺失即失败 |
| 本平台打包工具 + 其依赖 | 出插件/安装包；见各打包仓 README |

### 快速开始

```bash
cargo test --workspace              # 框架自检（不需要 ngspice / 插件）
./scripts/verify.sh --only fast     # 测试 + 漂移守卫 + 契约测试
./scripts/verify.sh --only oracle   # 四层 ngspice 对拍（需要 ngspice）

./scripts/new-plugin.sh "My Drive"  # 生成 ../my-drive/（框架同级）
./scripts/make-netlist.sh ../my-drive/circuit/netlist
./scripts/sim-baseline.sh ../my-drive/circuit/netlist
./scripts/build-all.sh ../my-drive  # 按平台自动分派打包器
```

### 目录布局

```text
some-dir/
  rustomp/                    ← 框架
  rustomp-<平台>-plugin/      ← 本平台的打包工具（mac / win / linux）
  my-drive/                   ← 插件项目（住在框架之外）
```

### 打包（三平台）

| 平台 | 打包仓 | 产物 | 默认安装 |
|---|---|---|---|
| macOS | `rustomp-mac-plugin v0.1.7` | AU / VST3 / CLAP（universal）+ DMG | `~/Library/Audio/Plug-Ins/` |
| Windows | `rustomp-win-plugin v0.3.0` | VST3 / CLAP（x64/arm64）+ MSI ×2 | `%LOCALAPPDATA%\Programs\Common\` |
| Linux | `rustomp-linux-plugin v0.1.0` | VST3 / CLAP + tar.gz | `~/.vst3` / `~/.clap` |

`./scripts/build-all.sh` 按 `uname` 分派到对应 `pack.sh`，只透传 `ARTIFACT:`；
`RUSTOMP_PACKER=/path/to/pack.sh` 可显式指定。各平台的依赖、签名与宿主验证见其 README。

### 验证与 CI

- 详细命令、逐层数字与性能基线：[`docs/verification.md`](docs/verification.md)
- 操作手册（新建插件 / 电路流程 / 打包 / 常见问题）：[`docs/workflows.md`](docs/workflows.md)
- 框架 CI：macOS，`fast`（测试 + 契约 + 漂移）与 `oracle`（四层活体对拍）
- 三平台发布：参考插件 `sparklin-drive` 的 `release.yml`（tag 触发，未签名 prerelease）

---

## 5. 契约与生态

框架只保证三个跨仓契约（完整说明见 [`docs/ecosystem.md`](docs/ecosystem.md)）：

| 契约 | 内容 | 谁消费 |
|---|---|---|
| **A：`rustomp.toml`** | 插件身份 / 参数 / 素材 / `[ui]` / `[requires]`；由 `tools/meta/plugin_meta.py` 读取 | 生成器、打包器、设计器 |
| **B：命令行与产物** | `pack.sh <plugin_dir> --format …`；成功时 stdout 逐行 `ARTIFACT: <路径>` | 框架 `build-all.sh`、CI、插件 `build.sh` |
| **C：`[ui]` + 素材** | 毫米坐标、12 px/mm；素材由设计器导出，生成器在编译期校验 | 面板设计器、`rustomp-gui` |

**仓库**：

- 打包器：`rustomp-mac-plugin` / `rustomp-win-plugin` / `rustomp-linux-plugin`
- 设计器：`rustomp-circuit-designer`（原理图，**首选**）、`rustomp-box-designer`（面板）
- 参考插件：`sparklin-drive`（v0.1.9）；历史手推实现：`sparklin-drive-legacy`

**版本配套**：插件 `rustomp.toml` 的 `[requires]` 声明 `framework` / `packer`，
`rustomp-gen` 与打包器启动时各校验一次；下游依赖一律钉 git tag，不钉 `master`。
当前组合：`rustomp v0.1.2` + mac `v0.1.7` + win `v0.3.0` + linux `v0.1.0` + `sparklin-drive v0.1.9`。

> 新平台 = 实现一个符合契约 B 的 `pack.sh`，再在 `build-all.sh` 的分派里加一行。

---

## 6. 文档

全部文档见 **[`docs/README.md`](docs/README.md)**（含索引与"何时读哪篇"）。

**当前文档**（随开发更新）：

| 文档 | 内容 |
|---|---|
| [`docs/status.md`](docs/status.md) | 当前版本、能力、平台、限制、下一步 |
| [`docs/architecture.md`](docs/architecture.md) | 分层、数据流、crates、仓库结构 |
| [`docs/verification.md`](docs/verification.md) | 验证体系、对拍数字、性能、CI/发布 |
| [`docs/workflows.md`](docs/workflows.md) | 新建插件 / 电路流程 / 打包 / 排障 |
| [`docs/ecosystem.md`](docs/ecosystem.md) | 8 个仓的边界与三个契约 |
| [`docs/declarative-build.md`](docs/declarative-build.md) | 生成 vs 打包两个平面、平台矩阵 |
| [`docs/component-library.md`](docs/component-library.md) | 器件精度规范 |
| [`docs/circuit-import.md`](docs/circuit-import.md) | 导入与映射规范 |

**历史**（只保留证据，不再更新）：[`DEVLOG.md`](DEVLOG.md) +
[`docs/history/`](docs/history/README.md)（旧计划、AB 测试、旧说明、波形图）。

> **维护规则**：当前状态写在 `status.md` / `verification.md`；一旦描述过时，
> 就移到 `docs/history/` 并在原处留指针。DEVLOG 只追加。详见 [`docs/README.md`](docs/README.md)。

---

## 7. 许可证

**GNU GPL v3.0-or-later**（[`LICENSE`](LICENSE)）。第三方依赖与许可证登记见
`DEVLOG.md` 的 ADR-005。

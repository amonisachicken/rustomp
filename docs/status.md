# 当前状态（status）

> **这是项目的当前快照**，随开发更新。
> 历史决策、实验过程与旧结论见 [`../DEVLOG.md`](../DEVLOG.md) 与 [`history/`](history/README.md)；
> 本文件只回答"现在能做什么、验证到什么程度、还差什么"。
>
> 最后更新：**2026-10-29**（`rustomp v0.1.2`）

---

## 1. 版本与配套

| 组件 | 当前版本 | 说明 |
|---|---|---|
| **rustomp（本仓）** | `v0.1.2` | 框架 / 代码生成 / 对拍 |
| `rustomp-mac-plugin` | `v0.1.7` | macOS：AU / VST3 / CLAP（universal）+ DMG |
| `rustomp-win-plugin` | `v0.3.0` | Windows：VST3 / CLAP（x64/arm64）+ MSI |
| `rustomp-linux-plugin` | `v0.1.0` | Linux：VST3 / CLAP + 可复现 tar.gz |
| 参考插件 `sparklin-drive` | `v0.1.9` | 三平台 prerelease 已发布（未签名） |
| `nih-plug` / `cargo-nih-plug` | rev `de42101` | 三个打包仓的 `deps.lock` 必须一致 |

完整的依赖方向、版本约束与当前组合见 [`ecosystem.md`](ecosystem.md)。

---

## 2. 能力快照

| 能力 | 状态 | 证据 / 入口 |
|---|---|---|
| **器件库** | ✅ 14 个型号闭集（二极管×4、LED×3、BJT×4、JFET、MOSFET、运放） | `crates/rustomp-dsp/tests/devices.rs` |
| **MNA 装配器** | ✅ 任意拓扑、5 类器件、牛顿求解；DC 对拍 µV 级、瞬态 mV 级 | `crates/rustomp-dsp/tests/mna.rs` |
| **实时路径** | ✅ `step_fixed` + 固定电位节点 + 自适应牛顿（上限 8、残差 1e-6）+ 大跳变子步进/失败恢复 | `step_fixed`、DEVLOG E57/E95 |
| **过采样** | ✅ **2× / 4× 可切换**（共用多相 FIR），两者上报延迟都是 15 样本 | `Oversampler`、DEVLOG E91 |
| **分支 / 旁通** | ✅ `Branch<A,B>`（预热 + 交叉淡化）、true-bypass、NaN 熔断 | `branch.rs`、`engine.rs` |
| **代码生成（电路）** | ✅ SPICE 网表 → `Netlist`/`Params`/`impl Circuit`，含参数/开关、器件卡片、漂移守卫 | `rustomp-import --mna` |
| **代码生成（插件）** | ✅ `rustomp.toml` → 身份 / 参数 / Plugin 实现 / 面板模块 | `rustomp-gen`、`--check` |
| **界面** | ✅ 声明式 `PanelSpec`（毫米坐标、12 px/mm、旋钮/拨杆/LED/3PDT） | `crates/rustomp-gui`、`[ui]` |
| **设计器** | ✅ `rustomp-circuit-designer`（首选，`.design.json` 可直接导入）/ `rustomp-box-designer` D0 | 独立仓库 |
| **验证** | ✅ `verify.sh` 9 层（fmt / 测试 / 契约 / 2×漂移 / 四层 ngspice 对拍） | [`verification.md`](verification.md) |
| **三平台打包** | ✅ macOS（AU/VST3/CLAP+DMG）、Windows（VST3/CLAP+MSI）、Linux（VST3/CLAP+tar.gz） | 三个打包仓，`build-all.sh` 分派 |
| **三平台 Release** | ✅ tag 触发 macOS / Windows / Linux 构建并发布（未签名 prerelease） | `sparklin-drive` release workflow，DEVLOG E100 |

---

## 3. 平台与分发

| | macOS | Windows | Linux |
|---|---|---|---|
| 格式 | AUv2 / VST3 / CLAP | VST3 / CLAP | VST3 / CLAP |
| 架构 | universal（x86_64 + arm64） | x64 / arm64（各自产物） | x64 / arm64（各自产物） |
| 安装包 | DMG（用户级）；`pkg` 未做 | MSI（系统级 / 用户级两个包） | tar.gz（`install.sh` + SHA256SUMS） |
| 签名 | ad-hoc（未公证） | 未做 Authenticode | 无（发行版包管理器的事） |
| 打包仓 | `rustomp-mac-plugin v0.1.7` | `rustomp-win-plugin v0.3.0` | `rustomp-linux-plugin v0.1.0` |

平台支持矩阵与"为什么分仓"见 [`declarative-build.md`](declarative-build.md) §4.0/§4.4。

---

## 4. 验证基线（摘要）

完整方法、命令与逐项数字见 [`verification.md`](verification.md)。

| 层 | 结果 |
|---|---|
| 器件级 | 二极管/LED `Vf` 0.00%；BJT `Ic/Ib` ≤0.09%；JFET 0.00%；MOSFET ≤0.01%；OP07 <2% |
| MNA 装配级 | 二极管削波 DC ≤1.6 µV；BJT 偏置 ≤123 µV；JFET 34 µV；RC+削波瞬态 ≤17 mV |
| 生成电路 | 与手写/ngspice 逐位或 ≤0.1 pp；漂移守卫逐字节一致 |
| 整机（Sparklin Drive） | THD 27.579% vs ngspice 27.5682%（+0.011 pp） |
| 打包契约 | 三平台分派、`ARTIFACT:` 透传、`[requires]`、无平台词：5/5 |

> 性能数字**强依赖电路与机器**，只用于同机配对比较；代表值与测量条件见 `verification.md` §5。

---

## 5. 已知限制（当前）

**器件与求解器**

- **二极管**：`Cjo/tt` 未建模（所有型号，包括 `1N4007`；Rust 侧只有静态 D2）。
- **BJT**：结电容/渡越时间已补（C1/C2，`ft` 与 ngspice 差 <5%）；**四个型号的扩散电荷全开**
  （E85，含锗管 `Tf=600 ns`）。
- **MOSFET**：`Cgs/Cgd` 仍是 C0 档。
- **运放**：单极点 + 压摆率 + 输出钳位；未建模失调、输入偏置电流、共模范围、噪声。
  192 kHz 下 -3 dB 偏低约 26%（无延迟进代数环的代价，近 Nyquist 有增益地板）。
- **BC549C** 在 `Ic≈4.6 mA` 处偏 +0.67%：疑似未建模 `Nk`，没有为拟合去猜 SPICE 语义。
- **代码生成映射**只做了「精确命中 / 报错」两级；别名、项目 `mapping.toml`、类别兜底、
  参数直通仍未实现；不支持 `.asc`、多电源/浮地源、非线性电容。
- **实时路径**单线程（多线程原型有收益，但单声道变体零收益 + 调度风险，暂不启用）；
  固定延迟 15 样本。

**界面**

- 声明式贴图面板已可用；还没有数值显示、素材热重载、多主题。
- `template/` + `new-plugin.sh` 尚未支持 `--gui`（要手工加 `gui` feature 并写 `editor.rs`）。
- `panel.rs` 仍由 `tools/meta/plugin_meta.py` 生成；搬进 `rustomp-gen`（单一发射器）是 TODO。

**打包 / 分发**

- macOS 只有 ad-hoc 签名（无 Developer ID / 公证）；DMG 只装当前用户。
- Windows MSI 是两个固定范围的包，没有"向导里选范围"的单入口；无 Authenticode。
- Linux 分发包的 glibc 基线随构建机（CI 用 Ubuntu 22.04 / glibc 2.35）。
- Windows / Linux 的 **arm64** 只有交叉参数与静态校验，仍待 ARM 真机验证。
- 三个打包仓自身没有 CI；由参考插件的 release workflow 覆盖。

---

## 6. 下一步（未完成）

- [ ] 器件动态档补齐：二极管 `Cjo/tt`、MOSFET `Cgs/Cgd`。
- [ ] 电位器 A/C taper。
- [ ] 预设与状态管理。
- [ ] `pkg` 安装包 + Developer ID 签名/公证；Windows Authenticode。
- [ ] Windows / Linux arm64 真机验证。
- [ ] 设计器 D1（原理图拖拽连线、面板更多控件）。
- [ ] `--gui` 脚手架与 `panel.rs` 并入 `rustomp-gen`。
- [ ] 更多示例电路（BJT 前级 / 光电压缩 / BBD）。

已完成的里程碑与历史路线见 [`history/milestones.md`](history/milestones.md)。

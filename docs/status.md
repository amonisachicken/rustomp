# 当前状态（status）

> **这是项目的当前快照**，随开发更新。
> 历史决策、实验过程与旧结论见 [`../DEVLOG.md`](../DEVLOG.md) 与 [`history/`](history/README.md)；
> 本文件只回答"现在能做什么、验证到什么程度、还差什么"。
>
> 最后更新：**2026-11-23**（`rustomp v0.1.4`：电感 / 耦合电感（变压器）+ 设计器 v0.2.0，见 §8）

---

## 1. 版本与配套

| 组件 | 当前版本 | 说明 |
|---|---|---|
| **rustomp（本仓）** | `v0.1.4` | 框架 / 代码生成 / 对拍 |
| `rustomp-mac-plugin` | `v0.1.8` | macOS：AU / VST3 / CLAP（universal）+ DMG |
| `rustomp-win-plugin` | `v0.3.0` | Windows：VST3 / CLAP（x64/arm64）+ MSI |
| `rustomp-linux-plugin` | `v0.1.0` | Linux：VST3 / CLAP + 可复现 tar.gz |
| 参考插件 `sparklin-drive` | `v0.1.10` | 三平台正式版已发布（未签名） |
| 设计器 `rustomp-circuit-designer` | `v0.2.0` | 电感 + 6 脚变压器（符号 / 展开 / 抽头滑块） |
| `nih-plug` / `cargo-nih-plug` | rev `de42101` | 三个打包仓的 `deps.lock` 必须一致 |

完整的依赖方向、版本约束与当前组合见 [`ecosystem.md`](ecosystem.md)。

---

## 2. 能力快照

| 能力 | 状态 | 证据 / 入口 |
|---|---|---|
| **器件库** | ✅ 19 个型号闭集（二极管×4、LED×3、BJT×4、JFET、MOSFET、运放、**电子管×5**） | `crates/rustomp-dsp/tests/{devices,triode}.rs` |
| **线性元件** | ✅ R / C / **L（含绕线电阻 `rdc`）** / **耦合电感（变压器，2..=4 绕组）**；都不占未知量，DC 退化成 `rdc` | `mna.rs`、`tests/mna.rs`、`tests/generated_inductor.rs`、`tests/generated_transformer.rs` |
| **MNA 装配器** | ✅ 任意拓扑、5 类器件、牛顿求解；DC 对拍 µV 级、瞬态 mV 级；**固定电源轨 = 理想电压源**（等价性回归） | `crates/rustomp-dsp/tests/mna.rs` |
| **实时路径** | ✅ `step_fixed` + 固定电位节点 + 自适应牛顿（上限 8、残差 1e-6）+ 大跳变子步进/失败恢复 | `step_fixed`、DEVLOG E57/E95 |
| **DC 工作点** | ✅ gmin 步进 + **KCL 残差复检**（不达标换起点重试）+ 诊断字段 | `solve_dc`、`last_dc_resid`、DEVLOG E101 |
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
- **电子管**：只有三极管（Koren 模型，12AX7/12AT7/12AU7/12AY7/6SN7）；**五极管 / 束射四极管未做**
  （Koren 的五极管模型多一个帘栅支路 + `ATAN` 膝，需要单独标定）；没有灯丝、麦克风效应、噪声。
- **电感 / 变压器**：只有理想 L + 串阻 + 互感 `K`（ngspice 的 `L` 没有磁芯模型，
  饱和度/磁滞做不了）；**没有"理想变压器"（k=1）**元素 —— 紧耦合请拆成励磁电感 + 漏感；
  最多 4 个绕组；**设计器还没有变压器符号**（M3：6 脚、初/次级带中心抽头，抽头默认 50%）。
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
- [x] 设计器变压器符号（M3）：6 脚封装、初/次级中心抽头可滑动（默认 50%），
      导出 4 绕组 + 6 个 `K`（`rustomp-circuit-designer v0.2.0`）；
      `.design.json` 直读也已支持（导入器侧同样展开）。
- [ ] 电位器 A/C taper。
- [ ] 预设与状态管理。
- [ ] `pkg` 安装包 + Developer ID 签名/公证；Windows Authenticode。
- [ ] Windows / Linux arm64 真机验证。
- [ ] 设计器 D1（原理图拖拽连线、面板更多控件）。
- [ ] `--gui` 脚手架与 `panel.rs` 并入 `rustomp-gen`。
- [ ] 五极管 / 束射四极管（EL34 / 6L6 / 6550，Koren 五极管模型 + 帘栅支路）。
- [ ] 电子管示例插件（12AX7 双级 + tone stack）。
- [ ] 更多示例电路（BJT 前级 / 光电压缩 / BBD）。

已完成的里程碑与历史路线见 [`history/milestones.md`](history/milestones.md)。

---

## 7. 下游回归修复（`v0.1.3`，2026-10-29）

下游项目 **Freedist**（4×BJT 离散运放 fuzz）提交的问题报告逐条处理完毕；
原文与处置表归档在 [`history/downstream-report-freedist.md`](history/downstream-report-freedist.md)。

| 项 | 结论 | 影响 |
|---|---|---|
| **P0 固定端雅可比重复计入** | ✅ 真 bug，已修 | 固定端是**正偏结**（PNP 射极接 VCC）时 DC 会落到伪解（−147 V）；修后与 ngspice 一致（1.51 V） |
| P1 DC 求解鲁棒性 | ✅ 加 KCL 残差复检 + 换起点重试 + 诊断字段 + `strict_dc` | 不再接受"Δv 很小但 KCL 不成立"的伪解 |
| P2 `1M` 被当成 milli | ✅ 导入器硬校验（`pot max < 1 Ω`）+ 可疑值 warning；设计器把电位器 `M` 规范成 `meg` | 不会再静默把偏置网络短接 |
| P3 4×BJT 实时开销 | ✅ 文档化 + `strip_bjt_body_resistance()` 杠杆 + `bjt_body_bench` 基准 | 数值路径未改（避免伤精度） |
| P4 打包器 doctor/build 工具链不一致 | ✅ mac 打包仓新增 `lib-toolchain.sh` | 用真实 rustc 的 `target-libdir` 判断，不再"doctor 绿、构建 E0463" |
| P5 `[ui]` 整数被静默忽略 | ✅ `rustomp-gen` 统一接受整数（含参数 `default`） | `drag_full_range_px = 220` 之类不再失效 |
| 小观察（重复 `if` / 熔断静默 / 1 mΩ 条件数） | ✅ 修两处、文档化一处 | 生成物更干净；`guard_trips()` 可诊断 |

**对既有电路的影响**：Sparklin Drive 的对拍数字**未变**（27.579 % / h2 3.92 %，逐位一致）；
受影响的只有"固定电位端子上接非线性器件且该端有显著电导"的电路 —— 那些电路以前算的是错的。

---

## 8. 电感 / 变压器（`v0.1.4`，2026-11-23）

M0–M3 四个阶段，框架与设计器各发一版（`rustomp v0.1.4` + `rustomp-circuit-designer v0.2.0`）。

| 阶段 | 内容 |
|---|---|
| **M0** 装配器 | `Element::Inductor`：`g = 1/(rdc + L/dt)` + 历史电流，DC 退化成 `rdc`（0 夹到 1 mΩ）；`commit()` 用提交点电压反算 `i_prev`（复位不"砰"） |
| **M1** 导入/生成 | `L` 严格解析（多一个 token 就报错——ngspice 会把它当别的意思，实测差 10×）；`net.inductor(...)`；解析结果改成四态 `Element/Skip/Warn/Fatal`，不再"出错降级成 note 就跳过这一行" |
| **M2** 耦合电感 | `Element::CoupledInductors`（2..=4 绕组，不占未知量）、`Mna::stamp_port_admittance`、`inductance_matrix`（自感 + k → 完整矩阵）、`K` 解析 + 分组 + Cholesky 正定校验；`H = (I + dt·L⁻¹R)⁻¹` 避免灾难性抵消 |
| **M3** 设计器 | 电感 / 6 脚变压器符号（自绘，20 个符号）、`l` 单键、抽头滑块（默认 50%）、展开成 4 绕组 + 6 个 `K`；`.design.json` 直读也做同样展开 |

**验收**（全部有测试守着）：

| 判据 | 结果 |
|---|---|
| RL 阶跃 vs 解析（dt=τ/20 → τ/80） | 0.90% → 0.23%（一阶收敛）；vs ngspice 0.45% |
| LC 谐振频率 | 503.25 vs 503.29 Hz（0.01%） |
| 变压器匝比 / 反射阻抗 | 0.49949 vs 0.49950；3981.9 vs 3998.9 Ω（0.42%） |
| 6 脚变压器（4 绕组，抽头 50%） | 抽头分压 0.5000；匝比 0.4995 |
| 生成 + 实时（RL 低通 / 2 绕组变压器） | 0.8399 vs 解析 0.8461（−0.73%）/ 0.06889 vs ngspice 0.06879 V（0.14%） |

**两个示例插件**（本地，用来验证"L 真的能做出声音"）：

| 插件 | 电路 | 实测 |
|---|---|---|
| `sparklin-notch` | L + **电容倍增器**（OP07 反相放大器把 C 放大 1+A 倍），FREQ 电位器扫 f0 | 241 Hz（−48.6 dB）→ 774 Hz（−27.2 dB）；CPU 15.6% 单核 |
| `sparklin-x10` | 1:10 升压音频变压器（Lp 1 H / Ls 100 H / k 0.9999） | 1 kHz **9.43（+19.5 dB）**，20 Hz 7.54；CPU 7.2% 单核 |

两个插件都已装上本机（AU `Sn02/Sn03`、`Xn02/Xn03`），在 Logic 里实测正常。

# 已完成里程碑（归档）

> 归档：2026-10-29。这里是"已完成"的路线图项，不再维护；当前状态与待做项见
> [`../status.md`](../status.md)，详细实验过程见 [`../../DEVLOG.md`](../../DEVLOG.md)。

| 阶段 | 完成内容 | 证据 |
|---|---|---|
| **M0 基础链路** | nih-plug 外壳、CLAP/VST3 导出、clap-wrapper → AUv2、安装 + 冒烟测试 | DEVLOG E4–E6、E11–E14 |
| **真旁通与界面** | true-bypass（15 样本对齐 + 5 ms 淡化）、egui 面板、旋钮/拨杆/LED/3PDT | E18–E22 |
| **器件库** | 二极管×4、LED×3、BJT×4、JFET、MOSFET、运放；全部与 ngspice 对拍 | E34–E41 |
| **MNA 装配器** | 任意拓扑、5 类器件、牛顿求解；DC µV / 瞬态 mV | E39、E42 |
| **实时路径** | `step_fixed`、固定节点、解析雅可比、自适应牛顿（平均 2 次） | E43、E57 |
| **S1a/S1b 电路生成** | 网表 → `Netlist`/`Params`/`impl Circuit`；参数/开关；生成物与手写逐位一致 | E44、E45 |
| **S1c 插件生成** | `rustomp.toml` → identity/params/plugin_impl；`--check` 漂移守卫 | E48 |
| **S1d 界面** | `[ui]` → `panel.rs`；毫米坐标 / 12 px/mm / 整图旋转 / true-bypass | E60–E62 |
| **多分支** | `Branch<A,B>` 两份完整电路 + 预热/交叉淡化 | E58 |
| **CI + 对拍** | `verify.sh` 分层、缺 ngspice 即失败、job summary 数字 | E47 |
| **过采样/热路径** | 2×/4× 共用多相实现、BJT 热路径优化、延迟统一 15 样本 | E89–E92 |
| **大跳变恢复** | 子步进 + DC 恢复 + 失败重置，杜绝永久冻结 | E95 |
| **参考插件** | `sparklin-drive` 独立仓；v2 生成路径 + 面板 + 基线 | E72、E71 |
| **设计器** | `rustomp-circuit-designer`（`.design.json` 可直读）/ `rustomp-box-designer` D0 | E64、E66 |
| **拆仓/打包** | mac v0.1.7、win v0.3.0、linux v0.1.0；契约 B 三个实现 | E71、E96、E99 |
| **三平台 Release** | `sparklin-drive v0.1.9`：DMG / MSI×2 / tar.gz，tag 触发 | E100 |
| **框架 v0.1.2** | 三平台分派、`[requires]` 校验、契约测试 5 步 | 发布记录、E97 |

> 更早的论文/调研与逐次实验（器件拟合、AB 测试、性能对比）都在 DEVLOG，不在这里重复。

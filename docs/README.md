# 文档索引

本目录分两类：**当前文档**（随开发更新）与 **历史归档**（不再修改，只保留证据）。
README 只放"能开工"的摘要，细节都在这里。

---

## 当前文档

| 文档 | 内容 | 什么时候读 |
|---|---|---|
| [`status.md`](status.md) | **当前状态**：版本配套、能力、平台、验证摘要、已知限制、下一步 | 想知道"现在到哪了" |
| [`architecture.md`](architecture.md) | 架构：分层、数据流、crates、仓库结构、关键机制 | 想改框架/理解数据流 |
| [`verification.md`](verification.md) | 验证体系、逐层对拍数字、性能基线、CI/发布验证 | 改 DSP / 准备发版 |
| [`workflows.md`](workflows.md) | 从零到发布：新建插件、电路流程、打包、CI、常见问题 | 第一次用 / 日常操作 |
| [`ecosystem.md`](ecosystem.md) | **仓库边界**：8 个仓、契约 A/B/C、版本约束、风险 | 跨仓改动 / 版本配套 |
| [`declarative-build.md`](declarative-build.md) | 声明式构建：生成平面 vs 打包平面、后端契约、平台矩阵 | 改打包/生成契约 |
| [`component-library.md`](component-library.md) | 元件库精度规范：档位、逐器件推荐、验收模板 | 加器件 / 调模型 |
| [`circuit-import.md`](circuit-import.md) | 电路导入与映射：可插拔 Importer、指令、映射分层 | 接新网表/设计器 |

**组件与工具文档**

| 文档 | 内容 |
|---|---|
| [`../crates/rustomp-dsp/README.md`](../crates/rustomp-dsp/README.md) | 器件库、MNA、Engine、器件对拍 |
| [`../crates/rustomp-gui/README.md`](../crates/rustomp-gui/README.md) | `PanelSpec`、控件、绘制顺序、贴图生命周期 |
| [`../crates/rustomp-gen/README.md`](../crates/rustomp-gen/README.md) | `rustomp.toml` → 插件代码、三条硬规则 |
| [`../crates/rustomp-import/README.md`](../crates/rustomp-import/README.md) | 导入器 + 网表 → 装配代码、漂移守卫 |
| [`../devices/REPORT.md`](../devices/REPORT.md) | 器件模型清洗报告（保留/丢弃了哪些参数） |
| [`../tools/oracle/smoke/README.md`](../tools/oracle/smoke/README.md) | ngspice 冒烟测试与基线数字 |
| [`../experiments/README.md`](../experiments/README.md) | 归档实验代码（不参与构建） |

---

## 历史归档

**历史 = `DEVLOG.md` + `history/`**。归档文档**不再更新**（只允许修正链接/加归档说明）。

| 位置 | 内容 |
|---|---|
| [`../DEVLOG.md`](../DEVLOG.md) | **主历史**：按 E 编号记录每个决策、实验、实测数字、踩坑与修复 |
| [`history/README.md`](history/README.md) | 归档索引 + 归档流程 |
| [`history/milestones.md`](history/milestones.md) | 已完成里程碑（S1a–S1d、打包、三平台发布…） |
| [`history/repo-split.md`](history/repo-split.md) | 拆仓时的文件归属、顺序与开工前问题 |
| [`history/split-plan.md`](history/split-plan.md) | 拆仓执行清单（逐文件、验证命令） |
| [`history/ab-test-sparklin-v2.md`](history/ab-test-sparklin-v2.md) | AB 测试复盘（FFT 判读、13 条经验） |
| [`history/pre-gui-checklist.md`](history/pre-gui-checklist.md) | 进 GUI 之前的验收清单（已全部通过） |
| [`history/macos-host-compat.md`](history/macos-host-compat.md) | 早期 Logic/AU 兼容说明（现由 mac 打包仓维护） |
| [`history/sparklin-drive-legacy.md`](history/sparklin-drive-legacy.md) | 早期示例插件说明（现为独立仓 `sparklin-drive`） |
| [`history/plots/`](history/plots/) | 历史波形图（880 Hz / 1 kHz 正弦/方波） |

---

## 文档维护规则（重要）

1. **当前 vs 历史**
   - 当前事实写进 `status.md` / `architecture.md` / `verification.md` / `workflows.md` / 规范文档；
   - 一旦某段描述过时（版本、平台状态、旧实现、旧测量），**移到 `history/`**，
     不要留在当前文档里"凑字数"。
2. **归档一条内容时**
   - 新建/追加到 `history/<主题>.md`，顶部写：
     `> 归档：YYYY-MM-DD；原因；替代文档见 …`；
   - 在当前文档原位置留**一行指针**（或直接删除并在 history 索引里登记）；
   - 在 `history/README.md` 的索引里登记。
3. **不修改历史**：归档文档只允许修正链接、补归档说明；不要用新结论改写旧记录
   （新结论写进当前文档或新 DEVLOG 条目）。
4. **DEVLOG 只追加**：每轮开发新增 E 编号；旧的 E 条目不修改（勘误写进新条目）。
5. **README 只放摘要**：状态细节进 `status.md`，操作细节进 `workflows.md`，
   数字细节进 `verification.md`。
6. **链接检查**：移动/归档文档后跑一次本地链接检查（所有 Markdown 相对链接的目标都存在），
   并确认 `README.md`、`ecosystem.md`、归档索引里的路径已更新。

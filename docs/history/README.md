# 历史归档

> **这里的内容不再更新。** 只保留当时的结论与证据；新结论写进当前文档或新 DEVLOG 条目。
> 归档时在文首加一行 `> 归档：YYYY-MM-DD；原因；替代文档见 …`。

主历史是仓库根目录的 [`../../DEVLOG.md`](../../DEVLOG.md)：按 **E 编号**记录每个决策、
实验数据、踩坑与修复；只追加，不改旧条目（勘误写进新条目）。本目录存放从当前文档中
**移出的整篇/整节内容**。

---

## 索引

| 文档 | 内容 | 归档原因 | 现在的替代 |
|---|---|---|---|
| [`milestones.md`](milestones.md) | 已完成里程碑（M0 → 三平台发布） | 完成项不再占用"当前状态" | [`../status.md`](../status.md) 的"下一步" |
| [`repo-split.md`](repo-split.md) | 拆仓文件归属、顺序、开工前问题 | 拆仓已完成 | [`../ecosystem.md`](../ecosystem.md) |
| [`split-plan.md`](split-plan.md) | 拆仓逐文件执行清单 | 拆仓已完成 | 同上 |
| [`ab-test-sparklin-v2.md`](ab-test-sparklin-v2.md) | AB 测试复盘（v2 vs legacy） | 一次性实验记录 | [`../verification.md`](../verification.md) |
| [`pre-gui-checklist.md`](pre-gui-checklist.md) | 进 GUI 前的验收清单 | 已全部通过 | [`../status.md`](../status.md) |
| [`macos-host-compat.md`](macos-host-compat.md) | 早期 Logic / AU 兼容说明 | 已迁到 mac 打包仓 | [rustomp-mac-plugin docs/03-logic-compat.md](https://github.com/amonisachicken/rustomp-mac-plugin/blob/master/docs/03-logic-compat.md) |
| [`sparklin-drive-legacy.md`](sparklin-drive-legacy.md) | 早期示例插件说明 | 插件已独立成仓 | [sparklin-drive](https://github.com/amonisachicken/sparklin-drive) |
| [`plots/`](plots/) | 历史波形图（880 Hz / 1 kHz 正弦/方波） | 对应早期 AB/整机验证 | [`../verification.md`](../verification.md) |

---

## 归档流程

1. **判断**：这段内容是否已经不能代表"现在"？（版本变了、平台变了、实现换了、实验结束了）
2. **落盘**：新建/追加 `history/<主题>.md`，顶部写归档日期、原因、替代文档；
   保留原始数字与结论，不改写历史。
3. **留指针**：在当前文档原位置留一行"历史见 …"，或直接删除并在本索引登记。
4. **登记**：更新本页索引（内容 / 原因 / 替代）。
5. **查链接**：移动文件后修正相对链接，并跑一次链接检查（见 [`../README.md`](../README.md) 规则）。

> 新增开发内容时：**先更新当前文档/`status.md`**；被替换下来的旧描述才进这里。

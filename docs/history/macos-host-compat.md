# 早期 macOS 宿主兼容说明（归档）

> 归档：2026-10-29。原因：框架已不再包含任何平台打包代码，AUv2/Logic 兼容由
> **[rustomp-mac-plugin](https://github.com/amonisachicken/rustomp-mac-plugin)** 维护。
> 替代文档：[mac 打包仓 `docs/03-logic-compat.md`](https://github.com/amonisachicken/rustomp-mac-plugin/blob/master/docs/03-logic-compat.md)。

下面保留的是框架仓早期对 Logic/AU 兼容问题的说明，仅作历史参考。

AUv2 由 `clap-wrapper` 包装。为了让 Logic 正确识别，当时需要三个补丁：

| 问题 | 症状 | 补丁 |
|---|---|---|
| 无条件宣称 `kMusicDeviceProperty_*` | 被当乐器，只出现在软件乐器插槽 | 仅在 `aumu` 时宣称 |
| 无条件宣称 MIDI 属性 | 被当 MIDI 控制效果器 | 仅真有 note port 时宣称 |
| 只认 `configurable-audio-ports`（nih-plug 只有 `audio-ports-config`） | AU 只报立体声，单声道轨菜单里消失 | 用旧扩展枚举布局并 `select()` |

另外：**插件分开发布单声道版与立体声版**（ADR-008），避免宿主按声道数过滤时出错。
两个变体各自是新的 AU 身份（新 subtype + bundle id 后缀），也顺带绕开陈旧缓存。

> 这些补丁与排查脚本现在都在 `rustomp-mac-plugin`（`patches/`、`tools/au-smoke`、
> `scripts/check-au.sh`、`scripts/reset-logic-cache.sh`）。

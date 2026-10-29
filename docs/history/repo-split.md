# 拆仓历史：文件归属、顺序与开工前问题（归档）

> 归档：2026-10-29。拆仓已完成；当前仓库边界与契约见 [`../ecosystem.md`](../ecosystem.md)，
> 逐文件执行清单见 [`split-plan.md`](split-plan.md)，完成里程碑见 [`milestones.md`](milestones.md)。

## 1. 文件归属（拆仓时的搬运清单）

| 去 `rustomp-mac-plugin` | 留在 `rustomp` |
|---|---|
| `scripts/build-all.sh` `build-variant.sh` `build-au.sh` `check-au.sh` | `scripts/verify.sh`（对拍 / 漂移守卫） |
| `scripts/gen-bundler-toml.sh` `reset-logic-cache.sh` | `scripts/new-plugin.sh`（脚手架，与平台无关） |
| `cmake/CMakeLists.txt`、`bundler.toml` 模板 | `tools/meta/`（manifest 读取 = **契约 A 的一部分**） |
| `patches/0001-auv2-host-classification-and-layouts.patch` | `tools/oracle/`（ngspice 对拍） |
| `tools/au-smoke/`（C）、`tools/clap-probe/` | `crates/{rustomp-dsp,rustomp-gui,rustomp-gen,rustomp-import,rustomp-oracle}` |
| — | `scripts/plugin-env.sh` `make-netlist.sh` `sim-baseline.sh` `build-ngspice.sh`、`docs/` |

**判断标准**：提到 `clap-wrapper`、`codesign`、`CMake`、`Logic`、`.component` 的，去 mac 工具；
提到 `vcvars`、`MSVC`、`WiX`、`%LOCALAPPDATA%`、`PE Machine` 的，去 win 工具。

`scripts/fetch-third-party.sh`（拉 ngspice 源码）服务"在本机编 ngspice 做对拍"，
属于验证环境，不进 mac 打包工具。

`rustomp` 保留一个瘦包装，保证"一条命令"体验不丢（按平台分派，框架不认识平台细节）：

```bash
# rustomp/scripts/build-all.sh
PACKER="$RUSTOMP_PACKER"                         # 显式指定优先
# macOS   → 同级 rustomp-mac-plugin/pack.sh
# Windows → 同级 rustomp-win-plugin/pack.sh
# Linux   → 同级 rustomp-linux-plugin/pack.sh
# 其他    → 报错，提示用 RUSTOMP_PACKER 指定一个新后端
exec "$PACKER/pack.sh" "$PLUGIN_DIR" "$@"        # 只透传 ARTIFACT: 行
```

## 2. 拆分顺序与完成标准

| # | 任务 | 完成标准 |
|---|---|---|
| **0** | 给 `rustomp` 打 tag `v0.1.0`；把契约 B 写成可执行测试 | ✅ 已完成：`tests/packer_contract.sh` 已进 `verify.sh` |
| **1** | 拆出 `rustomp-mac-plugin` | ✅ 已完成（DEVLOG E71）：框架里平台词为 0；拆前/后 `au_smoke` + RMS 0.356170 一致 |
| **2** | 独立发布 `sparklin-drive` | ✅ 已完成（E72）：全新 clone → 一条命令 → 6 条 `ARTIFACT`；CI 三仓 checkout |
| **3** | `rustomp-circuit-designer` D0 | 符号库 + 网表生成 + 一键校验；拖拽连线留 D1 |
| **4** | 拆出 `rustomp-win-plugin` | ✅ 已完成（E96，v0.3.0）：VST3/CLAP + MSI，本机 DAW 实测；框架零改动 |
| **5** | 拆出 `rustomp-linux-plugin` | ✅ 已完成（E99，v0.1.0）：VST3/CLAP + 可复现 tar.gz；框架只加平台分派 |

**拆仓期纪律：只搬不改。** 需要改契约（如 `[requires]`）就在第 0 步做掉。

## 3. 开工前的待确认项（全部已由后续实现回答）

1. 第一个 tag：定为 `v0.1.0`；
2. 参考插件仓是否带 `design/panel.box.json`：带（可追溯、可 fork）；
3. `sparklin-drive` 是否兼任模板：是（比维护第二份模板可靠）。

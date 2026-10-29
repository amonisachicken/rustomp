# 生态与仓库边界（ecosystem）

> **这份文档回答一个问题：这么多仓库，谁负责什么？**
>
> 状态：**结论已定**（2026-10-27）；**Windows 打包工具已落地**（2026-10-29，`rustomp-win-plugin` v0.3.0）。拆仓工作进行中见 §5 的进度表。
> 相关决策：ADR-014（声明式构建两个平面）、ADR-015（macOS 默认 + 外部打包工具）。

---

## 1. 仓库地图

```text
                        ┌─────────────────────────────┐
                        │  rustomp（框架，本仓）        │
                        │  DSP / 器件 / MNA / 实时路径  │
                        │  代码生成 + macOS 契约        │
                        └───────────┬─────────────────┘
                                    │ 被依赖（git tag）
        ┌───────────────────────────┼───────────────────────────┐
        ▼                           ▼                           ▼
┌──────────────────┐      ┌──────────────────┐      ┌────────────────────┐
│ sparklin-drive   │      │ rustomp-mac-     │      │ rustomp-box-       │
│ 参考插件（v2）    │─────►│ plugin（打包）    │      │ designer（面板）    │
│ 电路 + GUI 产物   │ 调用  │ AU/VST3/CLAP     │      │ → ui.toml + PNG    │
└──────────────────┘      └──────────────────┘      └────────────────────┘
                                    ▲
                          ┌─────────┴──────────┐
                          │ rustomp-win-plugin  │  （✅ v0.3.0，VST3/CLAP + MSI）
                          │ rustomp-linux-plugin│  （⬜ 未开工）
                          └────────────────────┘

另有独立仓（不在本图数据流上）：
- rustomp-circuit-designer  **首选**原理图设计器：原理图 → `.design.json` 工程（可直读）/ `.cir` 网表
- sparklin-drive-legacy     冻结归档：手推实现 + 基线数据（只读参考）
```

| 仓库 | 负责 | 不负责 |
|---|---|---|
| **rustomp**（本仓） | 器件模型、MNA 装配器、实时路径、代码生成、面板契约、ngspice 对拍 | 任何平台打包、素材美术、插件身份之外的业务 |
| **rustomp-mac-plugin** | macOS 打包：AUv2 / VST3 / CLAP（universal）、clap-wrapper 补丁、签名、安装、AU 自检 | DSP、代码生成、素材 |
| **rustomp-win-plugin** | Windows 打包：VST3 / CLAP（x64 + arm64 各自产物）、MSI（系统级 / 用户级两个包）、clap-probe + pluginval + PE 架构自检 | DSP、代码生成、素材、**AU / standalone**（平台不存在 / 不在范围） |
| **rustomp-linux-plugin** | Linux 打包与宿主验证（未开工） | 同上 |
| **sparklin-drive** | 一个**完整可构建**的参考插件：电路网表、生成物、面板产物、`build.sh` | 框架代码（只通过 tag 依赖） |
| **rustomp-box-designer** | 面板编辑 → `ui.toml` + 素材 PNG（零依赖浏览器端） | DSP、插件身份 |
| **rustomp-circuit-designer** | 原理图 → 工程文件 `.design.json`（**首选**，可直接导入）/ `.cir` 网表 + 指令 | 生成 Rust、打包 |
| **sparklin-drive-legacy** | 冻结的历史基线（WDF 手推实现 + AB 数据） | 一切活跃开发 |

---

## 2. 三个契约（跨仓协作的全部接口）

生态里**只有三个契约**，改动它们等于改公共 API：

### 契约 A：`rustomp.toml`（插件项目的清单）

```toml
[plugin] [au] [build] [circuit] [[params]] [assets] [ui] [requires]
```

- 由**插件作者**维护 `[plugin] [au] [circuit] [[params]]`；
- 由**设计器**写 `[assets]` + `[ui]`（标记块内，其余不许碰）；
- 由 `rustomp-gen` 读全部，生成 `plugin/src/generated/*.rs`；
- 由**打包工具**读 `[plugin] [au] [build]`。

规范：`docs/declarative-build.md`、`rustomp-box-designer/docs/01-contract-panel-toml.md`。

### 契约 B：打包工具的命令行与产物

```bash
<packer> <plugin_dir> --variant mono|stereo|dual --format au|vst3|clap|msi [--arch x64|arm64|both] [--install|--no-install]
# 成功：stdout 逐行 "ARTIFACT: /abs/path"（可被脚本抓取）
# 失败：非 0 退出 + 人话原因（缺 Xcode CLT / 缺 MSVC / 缺 rust target / 素材缺失…）
```

两个已落地的实现（入口都叫 `pack.sh`，参数按各自平台能力略异）：

| 实现 | 支持的 `--format` | 默认安装位置 |
|---|---|---|
| `rustomp-mac-plugin` | `au` / `vst3` / `clap` | `~/Library/Audio/Plug-Ins/` |
| `rustomp-win-plugin` | `vst3` / `clap` / `msi`（+ `--arch x64` / `arm64` / `both`） | `%LOCALAPPDATA%\Programs\Common\` |

**框架侧永远不知道平台的细节**（clap-wrapper / codesign / CMake），只知道这一行 `ARTIFACT:`。

### 契约 C：设计器导出的 `ui.toml` + 素材

```toml
# >>> rustomp-box-designer … # <<<（标记块内整段替换）
[assets] panel = "assets/panel.png" …    # 素材相对项目根
[ui]     svg_width / svg_height / window_* / background / [[ui.knobs|leds|toggles|buttons]]
```

素材密度：**12 px/mm**；坐标：**毫米，浮点**；旋钮行程固定 270°（指针朝上 ⇔ 0.5）。

---

## 3. 依赖方向与版本约束

```text
插件项目  ──git tag──►  rustomp（框架 crates）
插件项目  ──调用────►  平台打包工具
设计器    ──导出────►  插件项目的 [assets]/[ui]（单向）
```

**唯一允许的循环**：打包工具需要 `rustomp-gen` 的产物，但**不需要 rustomp 的源码**（生成物入库）。

跨仓版本用 `rustomp.toml` 的 `[requires]` 显式声明，由 `rustomp-gen` 与 packer 启动时各校验一次：

```toml
[requires]
framework = ">=0.1,<0.2"   # rustomp crates（插件 Cargo.toml 的 git tag 应对应）
packer    = ">=0.1"        # rustomp-mac-plugin / win / linux
designer  = ">=0.1"        # 仅记录（用于解释 ui.toml 的字段版本）
```

不匹配就**明确报错**，不要产出一个"能编但行为不同"的插件。

---

## 4. 文件归属（拆分时的搬运清单）

| 去 `rustomp-mac-plugin` | 留在 `rustomp` |
|---|---|
| `scripts/build-all.sh` `build-variant.sh` `build-au.sh` `check-au.sh` | `scripts/verify.sh`（对拍 / 漂移守卫） |
| `scripts/gen-bundler-toml.sh` `plugin-env.sh` `reset-logic-cache.sh` | `scripts/make-netlist.sh` `sim-baseline.sh` `build-ngspice.sh` |
| `cmake/CMakeLists.txt`、`bundler.toml` 模板 | `tools/meta/`（manifest 读取 = **契约 A 的一部分**） |
| `patches/0001-auv2-host-classification-and-layouts.patch` | `tools/oracle/`（ngspice 对拍） |
| `tools/au-smoke/`（C）、`tools/clap-probe/` | `crates/{rustomp-dsp,rustomp-gui,rustomp-gen,rustomp-import,rustomp-oracle}` |
| `scripts/new-plugin.sh`（建议搬或复制一份） | `docs/`（契约与生态文档） |

**判断标准**：一个文件如果提到 `clap-wrapper`、`codesign`、`CMake`、`Logic`、`.component`，它就该去 mac 工具；
如果提到 `vcvars`、`MSVC`、`WiX`、`%LOCALAPPDATA%`、`PE Machine`，它就该去 win 工具。

`scripts/fetch-third-party.sh`（拉 ngspice 源码）服务的是"在本机编 ngspice 做对拍"，
属于验证环境，**不进 mac 打包工具**；需要时由开发者在 `rustomp` 里跑。

`rustomp` 保留一个瘦包装，保证"一键"体验不丢：

```bash
# rustomp/scripts/build-all.sh（拆分后）
exec "${RUSTOMP_MAC_PLUGIN:-$HOME/develop/rustomp-mac-plugin}/pack.sh" "$@"
# 找不到则报错并打印 clone 命令，绝不偷偷自己编
```

---

## 5. 拆分顺序与完成标准

| # | 任务 | 完成标准 |
|---|---|---|
| **0** | 给 `rustomp` 打 tag `v0.1.0`；把契约 B 写成可执行测试（见 §6） | ✅ **已完成**：`v0.1.0` 已推；`tests/packer_contract.sh` 已进 `verify.sh` |
| **1** | 拆出 `rustomp-mac-plugin` | ✅ **已完成**（DEVLOG E71）：框架里平台词为 0（测试断言）；拆前/后 `au_smoke` + RMS 0.356170 一致 |
| **2** | 独立发布 `sparklin-drive` | ✅ **已完成**（DEVLOG E72）：全新 clone → `./scripts/build.sh` → 6 条 `ARTIFACT`；CI 三仓 checkout |
| **3** | `rustomp-circuit-designer` D0 | 符号库 + 网表生成 + 一键校验（`rustomp-import` + ngspice 偏差显示）；拖拽连线留 D1 |
| **4** | 拆出 `rustomp-win-plugin` | ✅ **已完成**（2026-10-29，`v0.3.0`）：VST3/CLAP + MSI 全链路，本机 DAW 实测出声；**框架零改动**（契约 A/B 首次被第二个平台独立实现消费） |

**拆仓期纪律：只搬不改。** 需要改契约（如 `[requires]`）就在第 0 步做掉。

> 逐文件搬运清单、验证命令与操作顺序见 **`docs/split-plan.md`**。

---

## 6. 契约 B 的可执行测试（第 0 步的产物）

```bash
# rustomp/tests/packer_contract.sh —— 用假插件 + 假 packer 验证
#  1) 入参（plugin_dir / variant / format）能被解析
#  2) 成功时 stdout 有且仅有 "ARTIFACT: <绝对路径>"
#  3) 框架侧不出现平台专属词（clap-wrapper / codesign / CMake / Logic / .component）
```

有了它，"加一个 Windows/Linux 打包工具"就是**加一个符合契约的脚本**，而不是改框架。
Windows 工具（`rustomp-win-plugin` v0.3.0）就是照这条路落地的：**框架与契约零改动**，
两个平台隔着同一份 `ARTIFACT:` 输出对接。

### 6.1 发布前固定动作：隔离验证

```bash
./scripts/verify-isolation.sh            # 用各仓最新 semver tag，全新目录，跑真实构建
./scripts/verify-isolation.sh --fast     # 只出 mono 的 AU（约 3 分钟）
./scripts/verify-isolation.sh --keep     # 失败时保留工作区排查
```

它检查：三仓能 clone 到 → `build.sh` 退出码 → `ARTIFACT` 行数与落点 → `au_smoke` + RMS 一致 →
**依赖全部来自 git tag** → 本机仓没被改动。

> 存在的理由很具体：DEVLOG E74 里两个缺陷（坏的 `pack.sh`、`engine/identity.rs` 漂移）
> 都是"本机能跑、tag 是坏的"。**「本机能跑」证明不了「tag 能用」。**

---

## 7. 已知风险

| 风险 | 影响 | 对策 |
|---|---|---|
| `nih_plug` 是 **git rev 依赖**（非 crates.io） | 新手以为是网络问题；rev 漂移会改变行为 | 打包工具维护 `deps.lock`（rev + 补丁 sha），`doctor` 子命令先行检查 |
| universal 需要两个 rust target | `bundle-universal` 失败信息不直观 | `doctor` 检查 `rustup target list --installed`，缺了直接给出安装命令 |
| **standalone 其实还没做**（文档里是 ⬜） | 若列进 v0.1 会吃不消 | mac 工具 **v0.1 只出 AU/VST3/CLAP**；standalone 单列 v0.2（要处理音频设备/设置持久化） |
| 现在是 **ad-hoc 签名**（`codesign -s -`） | 对外分发会被 Gatekeeper 拦 | 签名 + 公证（Developer ID + notarytool）**单独一项**，不混进拆仓 |
| ~~`plugins/` 是 gitignore 却是 workspace member~~ | 全新 clone **解析不了 workspace**（已修） | 根 `Cargo.toml` 去掉插件成员 + `exclude = ["plugins"]`；插件自带 workspace；用 `RUSTOMP_GUEST=/path` 指路 |
| **`cargo-nih-plug` 会 chdir 到"最顶层"的 `Cargo.toml`** | 插件嵌在本仓 `plugins/` 里时，会拉到 rustomp 根 → `cannot specify features for packages outside of workspace` | **插件项目必须住在本仓之外**（默认 `../<slug>`）；`new-plugin.sh` 已改为生成到 `$RUSTOMP_GUEST`；`target/` 与 `bundler.toml` 都落在插件项目根 |
| 插件仓的生成物 | 用户可能编到过期代码 | `build.sh` 先跑 `rustomp-gen --check`（漂移即 fail），不允许跳过 |
| Windows **只能用 MSVC**（`-gnu` 与宿主 ABI 不兼容） | 用户装了 MSYS2 的 rustc 会莫名编不出可加载的插件 | `doctor.sh` 对 `rustc -vV` 的 host 做硬校验并给出装法；`deps.lock` 钉 rust target |
| Windows 两架构 **identity 相同**，`both` 默认只装 x64 | 同机装两份会让宿主扫到重复插件 | 工具层只装 x64；文档明确“一台机器只装一种” |
| MSI **单包双范围不可行**（实测会两处都写，W-010） | 想要“一个安装包向导里选范围”会得到坏安装 | 按范围固定出**两个 MSI**（不同 `UpgradeCode` = 两个产品身份）；要单入口得再加 EXE 引导器 |
| arm64 在 x64 机器上只能**静态校验** | “产物存在”≠“宿主能加载” | `RUSTOMP_REQUIRE_VERIFY=1` 时主动失败；文档写明待 ARM 机器验证 |

---

## 8. 命名与许可

| 项 | 决定 |
|---|---|
| 打包仓命名 | `rustomp-mac-plugin` / `rustomp-win-plugin` / `rustomp-linux-plugin`（与 ADR-015 一致） |
| 参考插件仓名 | **`sparklin-drive`**（v2 引擎）；与 `sparklin-drive-legacy`（冻结归档）在 README 里互相指明 |
| 代码许可 | **GPL-3.0-or-later**（链接 rustomp 决定） |
| 素材/美术 | 建议在插件仓显式声明（同 GPL 或 CC BY-SA），避免 fork 时的犹豫 |
| **tag 约定** | 每个仓发布即打 tag；下游**一律钉 tag**，不钉 `master`。**tag 必须在干净 checkout 里验证过才算数**（DEVLOG E74 就是没验证导致 v0.1.0 是坏的） |
| **依赖布局** | 各仓**必须同级**（`some-dir/{rustomp, rustomp-mac-plugin, rustomp-win-plugin, sparklin-drive}`，按平台取用）——脚本按此自动发现，不假设任何绝对路径 |
| **当前可用组合** | macOS：`rustomp v0.1.1` + `rustomp-mac-plugin v0.1.6` + `sparklin-drive v0.1.8`；Windows：`rustomp ≥ v0.1.1`（工具在 `7a186e4` 上验证）+ `rustomp-win-plugin v0.3.0` |（更早的 mac packer `v0.1.0`–`v0.1.5` 与插件 `v0.1.0`–`v0.1.7` 请勿使用：`v0.1.5` 及更早的 packer 会触发 clap-wrapper 的 CPM 联网下载，见 DEVLOG E88.3/E95） |
| **CI 必须真跑过** | 第一次 CI 是"假绿"（缺 cargo-nih-plug + `\| tee` 吞掉失败 + 上传空目录只警告），见 DEVLOG E75。判据：**产物数量 ≥1 且能下载** |

---

## 9. 参考插件的仓库内容（契约 C 的落地样板）

```text
sparklin-drive/
  rustomp.toml                 # 含 [requires]
  circuit/netlist/*.cir        # 电路"源码"（ngspice 可直接跑）
  circuit/src/generated/*.rs    # 生成物入库（--check 防漂移）
  circuit/tests/engine.rs      # 电路行为测试
  plugin/src/{lib.rs,editor.rs,generated/*.rs}
  assets/{panel.png,panel.svg,knob_round_dark.png,toggle_small_{up,down}.png,3pdt_alt_hold.png}
  design/panel.box.json        # 设计器源文件（可回设计器继续改）
  scripts/build.sh             # doctor / 构建 / test / verify
  BUILDING.md  LICENSE  README.md  .github/workflows/build.yml
```

`build.sh` 的四条硬要求：

1. `doctor` 子命令检查 Toolchain / Xcode CLT / rust targets / packer / 网络；
2. 构建前 `rustomp-gen --check`（漂移即 fail）；
3. 缺 packer 时打印**一行可复制的 clone 命令**；
4. 结尾打印 `ARTIFACT:` 列表（与契约 B 同形），便于 CI 抓取。

CI：在**干净 runner** 上 checkout 三个仓（插件 / rustomp@tag / mac 工具@tag）→ `./build.sh` → 上传产物。
只有这样才能证明"任何人拉取后能构建"。

---

## 10. 待确认（开工前）

1. `rustomp` 的第一个 tag（建议 **`v0.1.0`**）；
2. 参考插件仓里是否带 `design/panel.box.json`（建议**带**，面板可追溯、可 fork）；
3. `sparklin-drive` 是否兼任"插件模板"（建议**是**，比维护第二份模板可靠）。

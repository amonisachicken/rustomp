# 工作流（从零到可发布）

> 这是"现在怎么做"的操作手册。规范细节见
> [`component-library.md`](component-library.md)、[`circuit-import.md`](circuit-import.md)、
> [`declarative-build.md`](declarative-build.md)；平台差异见三个打包仓的 README。

---

## 0. 依赖与目录布局

**框架本身**：

| 依赖 | 用途 |
|---|---|
| Rust stable | 编译框架与插件 |
| `python3 ≥ 3.11` | 读 `rustomp.toml`（`tomllib`） |
| `ngspice`（推荐） | 活体对拍；没有时本地跳过、CI 里失败 |
| 平台打包工具 | 出 AU/VST3/CLAP/安装包（mac/win/linux 三选一） |

**目录布局**（同级；脚本自动发现，不需要环境变量）：

```text
some-dir/
  rustomp/                    ← 框架（本仓）
  rustomp-<平台>-plugin/      ← 本平台的打包工具
  my-effect/                  ← 插件项目（new-plugin.sh 生成）
```

```bash
# 框架自检（不需要 ngspice、不需要插件）
cargo test --workspace
./scripts/verify.sh --only fast

# 需要 ngspice 的四层对拍
./scripts/verify.sh --only oracle
```

---

## 1. 新建一个插件

```bash
./scripts/new-plugin.sh "My Drive"        # 生成 ../my-drive/（框架同级）
```

生成后：

1. 改 `../my-drive/rustomp.toml`：显示名 / AU 四字符码 / CLAP id / VST3 id /
   `[[params]]` / `[assets]` / `[ui]` / `[requires]`；
2. 重新生成插件侧代码：

   ```bash
   cargo run -q -p rustomp-gen -- ../my-drive
   cargo run -q -p rustomp-gen -- ../my-drive --check   # 防漂移
   ```

3. 写电路（见下一节）；
4. 打包：

   ```bash
   ./scripts/build-all.sh ../my-drive                     # 按平台自动分派
   ./scripts/build-all.sh ../my-drive --format dmg        # macOS：额外出 DMG
   ./scripts/build-all.sh ../my-drive --format msi        # Windows：额外出 MSI
   ./scripts/build-all.sh ../my-drive --format targz      # Linux：额外出 tar.gz
   ```

> 插件必须住在**框架目录树之外**：`cargo-nih-plug` 会把最顶层 `Cargo.toml` 当 workspace 根。
> `new-plugin.sh` 默认生成到框架同级；`RUSTOMP_GUEST=/path` 可改。

---

## 2. 电路建模

### 2.1 画图 → 网表

首选 [rustomp-circuit-designer](https://github.com/amonisachicken/rustomp-circuit-designer)：
原理图工程 `.design.json` 可直接导入；也可以导出 `.cir`。面包板（breadspice `.bread`）仍支持，
但不再是默认路径。

生成 SPICE 网表与基线：

```bash
./scripts/make-netlist.sh ../my-drive/circuit/netlist
./scripts/sim-baseline.sh ../my-drive/circuit/netlist
```

### 2.2 网表 → 可运行电路

```bash
# 设计器工程（推荐）
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- ../my-drive/circuit/<设计>.design.json \
  --mna ../my-drive/circuit/src/generated/circuit.rs

# 或者直接喂 .cir
cargo run -q -p rustomp-import -- ../my-drive/circuit/netlist/<网表>.cir \
  --mna ../my-drive/circuit/src/generated/circuit.rs
```

生成物是 `Netlist` + `Params` + `impl Circuit`。参数/开关来自网表指令
（`* @rustomp param=...` / `switch=...`）；器件型号必须落在闭集表里，未命中直接报错。

### 2.3 对拍

```bash
(cd ../my-drive && cargo test -p my-drive-circuit --release)
(cd ../my-drive && cargo test -p my-drive-circuit --release --test oracle)  # 真跑 ngspice
```

写测试锁住 h2–h9 / THD / 频响；参考实现见 `sparklin-drive` 的 `baseline.rs` / `oracle.rs`。

---

## 3. 打包与安装

| 平台 | 入口 | 默认安装 | 安装包 |
|---|---|---|---|
| macOS | `pack.sh`（`build-all` 自动选中） | `~/Library/Audio/Plug-Ins/` | `--format dmg` → `<Name> Installer.dmg` |
| Windows（MSYS） | 同上 | `%LOCALAPPDATA%\Programs\Common\` | `--format msi` → 系统级 / 用户级两个 MSI |
| Linux | 同上 | `~/.vst3` / `~/.clap` | `--format targz` → 可复现 tar.gz |

三个打包器都实现契约 B：成功时 stdout 只有 `ARTIFACT: <路径>`，日志走 stderr。
框架的 `build-all.sh` 只做平台分派与参数透传；`RUSTOMP_PACKER=/path/to/pack.sh` 可显式指定。

各平台的依赖、签名、宿主验证见：

- [rustomp-mac-plugin](https://github.com/amonisachicken/rustomp-mac-plugin)（AU/VST3/CLAP + DMG）
- [rustomp-win-plugin](https://github.com/amonisachicken/rustomp-win-plugin)（VST3/CLAP + MSI）
- [rustomp-linux-plugin](https://github.com/amonisachicken/rustomp-linux-plugin)（VST3/CLAP + tar.gz）

---

## 4. CI 与发布

**框架仓 CI**（`.github/workflows/ci.yml`，macOS）：

- `fast`：workspace 测试 + 漂移守卫 + 契约测试；
- `oracle`：四层 ngspice 活体对拍（缺 ngspice 直接失败）；
- 报告写进 job summary，失败日志作为 artifact 上传。

**三平台发布**（参考插件 `sparklin-drive/.github/workflows/release.yml`）：

- push tag `v*` → macOS / Windows / Linux 构建 → 改名 + `SHA256SUMS.txt` → GitHub Release（prerelease）；
- `workflow_dispatch` 只构建（dry-run）；传 `publish_tag=<tag>` 可对已有 tag 补发；
- 产物：macOS DMG + plugins.zip；Windows MSI×2 + plugins.zip；Linux tar.gz。

**未签名**：macOS ad-hoc / Windows 无 Authenticode / Linux 无签名；正式分发前需要签名与公证。

---

## 5. 常见问题

| 现象 | 处理 |
|---|---|
| `cannot specify features for packages outside of workspace` | 插件被放进了某个 Rust workspace 目录树；移到框架同级 |
| `找不到 <平台> 打包工具` | 按提示 clone 到同级；或设 `RUSTOMP_<平台>_PLUGIN` / `RUSTOMP_PACKER` |
| 缺 ngspice 时对拍没跑 | 本地是设计行为；`--only oracle` 会明确失败；CI 用 `RUSTOMP_REQUIRE_NGSPICE=1` |
| 生成物漂移 | 重新跑 `rustomp-gen` / `rustomp-import`，不要手改 `generated/` |
| `requires.packer` 版本不满足 | 打包器 checkout 到插件 `[requires] packer_tag` 或更新版本 |
| 装了插件但宿主菜单里没有 | 见对应打包仓的宿主排障（mac 的 Logic 缓存、Windows 的重复身份、Linux 的扫描路径） |

历史问题与踩坑记录在 [`../DEVLOG.md`](../DEVLOG.md)，按条目编号查。

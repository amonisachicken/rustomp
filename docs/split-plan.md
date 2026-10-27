# 拆仓执行计划（split plan）

> 依据：`docs/ecosystem.md`（仓库边界与三个契约）。本文是**可直接照着敲**的执行清单：
> 每个文件从哪来、到哪去、怎么验证。
>
> 状态：2026-10-27 **§0 已完成**（含一个比预想更深的发现：插件必须住在本仓之外，见 §0.3）。

---

## 0. 开工前必须先解决的阻塞项

### 0.1 全新 clone 现在**解析不了 workspace**

```bash
$ git clone git@github.com:amonisachicken/rustomp.git /tmp/rustomp-clone
$ cd /tmp/rustomp-clone && cargo metadata --no-deps
error: failed to load manifest for workspace member `/private/tmp/rustomp-clone/plugins/sparklin-drive/plugin`
       referenced by workspace at `/private/tmp/rustomp-clone/Cargo.toml`
```

原因：根 `Cargo.toml` 的 `members` 里写着 `plugins/sparklin-drive/{plugin,circuit}`，
但 `plugins/` 在 `.gitignore` 里 —— **仓库里根本没有这个目录**。

影响（这会让任务 2 直接卡死）：

- 插件仓用 `git = "...", tag = "v0.1.0"` 依赖 `rustomp-dsp` 时，cargo 会读 rustomp 的 workspace 清单 → 失败；
- `cargo install --git <rustomp> rustomp-gen` 同理；
- **所以现在还不能打 tag**（打出来的是"用不了"的 tag）。

### 0.2 修法（已完成）

| 改动 | 内容 |
|---|---|
| `Cargo.toml`（根） | `members` 删掉两个插件成员；加 `exclude = ["plugins"]` |
| `plugins/sparklin-drive/Cargo.toml`（**新建**） | 自带一个 workspace：`[workspace] members = ["plugin", "circuit"] resolver = "2"` |
| `scripts/build-variant.sh` | 从 `cargo nih-plug …` 改为在 `"$PLUGIN_DIR/plugin"` 下构建（或加 `--manifest-path`） |
| `scripts/verify.sh` | 只跑框架 `--workspace`（本来就是），确认没有 `-p sparklin-drive` |
| `docs/ecosystem.md` §7 | 把"plugins/ 悬空"风险标为已处理 |

验证（缺一不可）：

```bash
git clone <rustomp> /tmp/c1 && cd /tmp/c1 && cargo metadata --no-deps >/dev/null   # 必须成功
cargo test --workspace --release                                                   # 框架自测
PLUGIN_DIR=$HOME/develop/rustomp/plugins/sparklin-drive ./scripts/build-all.sh      # 仍能出 Sp02/Sp03
```

> **副作用**：根目录不再能 `cargo build -p sparklin-drive`。构建插件改为在插件目录里跑，
> 这正是拆仓后的形态（插件是独立 workspace），所以是"提前到位"，不是回退。

### 0.3 更深的发现：插件**不能**住在 rustomp 目录树里

修完 workspace 后构建报：

```text
error: cannot specify features for packages outside of workspace
```

追到 `nih_plug_xtask` 的源码才发现原因（`nih_plug_xtask/src/lib.rs`）：

```rust
let workspace_root = project_dir.ancestors()
    .filter(|dir| dir.join("Cargo.toml").exists())
    .last();                       // ← 取**最顶层**那个 Cargo.toml
std::env::set_current_dir(workspace_root)?;
```

插件住在 `rustomp/plugins/<name>/` 时，`project_dir` 的祖先里有 `rustomp/Cargo.toml`（顶层），
于是 `cargo nih-plug` 会 chdir 到 **rustomp 根**，而那里没有这个包 → 上面那句报错。

**结论：插件项目必须住在本仓之外。** 三个连带事实：

1. `target/` 落在**插件项目根**（不是 `plugin/` 子目录）；
2. `bundler.toml` 必须写在插件项目根（xtask 从 workspace 根读）；
3. `new-plugin.sh` 改为生成到 `$RUSTOMP_GUEST`（默认 `../<slug>`），并在模板里按实际相对路径填 `rustomp` 的 path 依赖。

这正好是拆仓后的最终形态，所以是"提前到位"。

---

## 1. `rustomp-mac-plugin`（任务 1）

### 1.1 文件树与来源

```text
rustomp-mac-plugin/
├── README.md                     新增（职责 + 快速开始 + 契约 B）
├── LICENSE                       cp    rustomp/LICENSE
├── pack.sh                       新增（入口，转调 scripts/pack-macos.sh）
├── deps.lock                     新增（nih-plug rev + AU 补丁 sha + 最低工具链）
├── scripts/
│   ├── pack-macos.sh             git mv scripts/build-all.sh        （改造为契约 B 入口）
│   ├── build-variant.sh          git mv scripts/build-variant.sh
│   ├── build-au.sh               git mv scripts/build-au.sh
│   ├── check-au.sh               git mv scripts/check-au.sh
│   ├── gen-bundler-toml.sh       git mv scripts/gen-bundler-toml.sh
│   ├── plugin-env.sh             git mv scripts/plugin-env.sh
│   ├── reset-logic-cache.sh      git mv scripts/reset-logic-cache.sh
│   └── doctor.sh                 新增（Rust / Xcode CLT / rust targets / packer 版本 / 网络）
├── cmake/CMakeLists.txt          git mv cmake/CMakeLists.txt
├── patches/                      git mv patches/
├── tools/
│   ├── au-smoke/                 git mv tools/au-smoke/
│   └── clap-probe/               git mv tools/clap-probe/
├── template/                     cp    rustomp/template/（bundle 名等模板）
└── .github/workflows/pack.yml    新增（macos-latest：用假插件跑契约测试 + 真插件构建）
```

**留在 rustomp**：`crates/`、`tools/meta/`、`tools/oracle/`、`scripts/{verify,make-netlist,sim-baseline,build-ngspice}.sh`、`devices/`、`docs/`。

### 1.2 `pack.sh` 的最小职责

```bash
# 1) doctor（必需项失败就退出）
# 2) 读 <plugin_dir>/rustomp.toml（通过 rustomp 的 tools/meta/plugin_meta.py --shell）
# 3) 校验 [requires].packer 与自身版本
# 4) 逐 variant 调用 scripts/build-variant.sh
# 5) 打印 ARTIFACT: <绝对路径>（每个产物一行）
```

### 1.3 rustomp 侧的瘦包装

`scripts/build-all.sh` 改为：

```bash
exec "${RUSTOMP_MAC_PLUGIN:-$HOME/develop/rustomp-mac-plugin}/pack.sh" "$@"
```

找不到就报错并打印 `git clone` 命令（不偷偷自己编）。

### 1.4 完成标准

- [ ] `grep -rE "clap-wrapper|codesign|\.component|Logic" rustomp/crates rustomp/scripts` 只剩文档命中
- [ ] `rustomp/scripts/verify.sh` 不依赖 `patches/`
- [ ] 用同一份插件源码，拆前 / 拆后的 AU 产物 `au_smoke` 与 RMS 一致
- [ ] `pack.sh --format vst3` 打印的 `ARTIFACT:` 能被脚本抓取

---

## 2. `sparklin-drive`（任务 2）

### 2.1 文件树与来源

```text
sparklin-drive/
├── README.md                     新增（是什么 / 怎么构建 / 与 legacy 的关系 / 截图）
├── BUILDING.md                   新增（依赖清单 + 分平台步骤 + 故障排查）
├── LICENSE                       cp    rustomp/LICENSE（GPL-3.0）
├── rustomp.toml                  cp    plugins/sparklin-drive/rustomp.toml（+ [requires]）
├── Cargo.toml                    新增（独立 workspace：members = ["plugin", "circuit"]）
├── rust-toolchain.toml           新增（钉住 edition/toolchain，可选）
├── circuit/
│   ├── Cargo.toml                cp    …/circuit/Cargo.toml（path 依赖 → git tag）
│   ├── netlist/*.cir             cp    …/circuit/netlist/（asym / sym 两份）
│   ├── src/lib.rs                cp    …/circuit/src/lib.rs
│   ├── src/generated/*.rs        cp    …/circuit/src/generated/（入库，--check 防漂移）
│   └── tests/engine.rs           cp    …/circuit/tests/engine.rs
├── plugin/
│   ├── Cargo.toml                cp    …/plugin/Cargo.toml（path → git tag；nih-plug 保持 rev）
│   └── src/{lib.rs,editor.rs,generated/*.rs}
├── assets/                       cp    设计器素材包（panel.png/svg + 4 张贴图）
├── design/panel.box.json         cp    设计器源文件（可回设计器继续改）
├── scripts/build.sh              新增（doctor / 构建 / test / verify）
└── .github/workflows/build.yml   新增（干净 runner：checkout 三仓 → ./build.sh → 上传产物）
```

**不入库**：`target/`、`*.zip`（设计器下载件）、`/tmp`。

### 2.2 `build.sh` 的四个子命令

```text
./build.sh doctor    # rustc/cargo、Xcode CLT、两个 rust target、python3、packer、网络
./build.sh           # doctor → rustomp-gen --check → pack.sh（mono + stereo）→ 安装 → ARTIFACT 列表
./build.sh test      # cargo test（不需要 ngspice）
./build.sh verify    # 需要 ngspice：整机对拍（可选）
```

### 2.3 依赖改造清单（关键）

| 文件 | 改动 |
|---|---|
| `plugin/Cargo.toml` | `rustomp-dsp` / `rustomp-gui` 由 `path` / `workspace` → `git = "…rustomp", tag = "v0.1.0"` |
| `circuit/Cargo.toml` | `rustomp-dsp` 同上；`rustomp-oracle` → `[dev-dependencies]`，同 tag |
| `rustomp.toml` | 新增 `[requires] framework/packer/designer` |
| `Cargo.lock` | 重构后重新生成并入库（可复现构建） |

### 2.4 完成标准

- [ ] **全新目录** clone 本仓 + `./build.sh` → 出 `.component` / `.vst3` / `.clap`
- [ ] `cargo test` 全过（不需要 ngspice）
- [ ] `rustomp-gen --check` 通过（生成物未漂移）
- [ ] CI 在干净 runner 上绿，并上传产物 zip
- [ ] README 里"三行命令"能跑通（clone / build / 装）

---

## 3. 明天的命令序列（建议按此顺序敲）

```bash
# 0) 阻塞项 + 打 tag
cd ~/develop/rustomp
git switch -c split/workspace-self-contained
#   …改 Cargo.toml / 插件 workspace / build-variant.sh…
git clone . /tmp/c1 && (cd /tmp/c1 && cargo metadata --no-deps >/dev/null)   # 必须通过
./scripts/verify.sh
git switch master && git merge --ff-only split/workspace-self-contained
git tag -a v0.1.0 -m "rustomp v0.1.0：框架 + 三个契约（含 GUI）" && git push origin v0.1.0

# 1) mac 打包仓（gh 需先 gh auth login -h github.com）
gh repo create amonisachicken/rustomp-mac-plugin --private=false --description "macOS packaging for rustomp plugins"
#   按 §1.1 搬文件（用 git mv 保留历史：先 clone rustomp 到临时目录再用 filter-repo/subtree，或直接 cp + 单独提交）

# 2) 参考插件仓
gh repo create amonisachicken/sparklin-drive --private=false --description "Sparklin Drive — reference rustomp plugin"
#   按 §2.1 搬文件，改依赖，跑 §2.4 完成标准
```

> 搬文件时优先 `git mv`（保留 blame）；跨仓无法直接 `git mv`，
> 建议在目标仓第一次提交里写清"来源：rustomp@<sha> 路径 xxx"。

---

## 4. 环境备忘

| 项 | 状态 |
|---|---|
| `git push` over SSH | ✅ 可用 |
| `gh` 2.103.2 | 已装，**token 无效** → 需 `gh auth login -h github.com` |
| ngspice | `/usr/local/bin/ngspice`（`build.sh verify` 可选依赖） |
| Xcode CLT / 两个 rust target | 待 `doctor.sh` 实际检查 |
| Logic 缓存 | `scripts/reset-logic-cache.sh`（搬去 mac 工具） |

---

## 5. 一句话纪律

**拆仓期只搬不改。** 任何"顺手优化"都记到 todo，等四个仓都绿了再动。

# rustomp-gen

**插件侧代码生成**（S1c）：`rustomp.toml` → `plugin/src/generated/*.rs`。

```bash
cargo run -q -p rustomp-gen -- <项目目录>           # 生成
cargo run -q -p rustomp-gen -- <项目目录> --check   # 只比对（CI 防漂移）
```

## 生成什么

| 文件 | 内容 |
|---|---|
| `identity.rs` | 名字 / CLAP id / 三套 VST3 id（变体差异用 `cfg` 选择 → 文件是静态的） |
| `params.rs` | `PluginParams` 结构体 + `Default` + `to_dsp()`（拍快照给电路） |
| `plugin_impl.rs` | `Plugin` / `ClapPlugin` / `Vst3Plugin` + 导出宏 + 界面钩子 |
| `mod.rs` | 模块汇总 |

**不生成界面**：`editor()` 在没有 `gui` feature 时返回 `None`。要做界面时（S1d）
手写 `src/editor.rs` 提供 `make(params, activity)` 并开启 `gui` feature ——
生成器不掺和 GUI。

## 生成什么（2026-10-27 起多一个 `assets.rs`）

| 文件 | 内容 |
|---|---|
| `generated/assets.rs` | `PANEL_PNG` + `IMAGES`（`[assets]` 里的 PNG 编进二进制；`include_bytes!` 相对生成物，所以是 `../../../assets/...`） |

> **为什么放在生成器里**：素材是设计器产出的、会被改名/增删的东西。
> 让 `editor.rs` 写死 `include_bytes!` 就等于"设计器一改，手写代码要跟着改"。
> 生成器还会**先检查文件存在**，缺了在生成阶段就报错。

## `gui = true` 时多生成什么（S1d，2026-10-26）

| 文件/字段 | 内容 |
|---|---|
| `generated/params.rs` | 追加 `#[persist = "editor-state"] pub editor_state: Arc<EguiState>`（窗口尺寸记忆） |
| `generated/mod.rs` | 追加 `pub mod panel;` |
| `generated/plugin_impl.rs` | `process()` 开头下发 `set_bypass`（**`bypass` 是保留参数 id**，不进 `make_params`） |

`panel.rs` 目前仍由 `tools/meta/plugin_meta.py --panel-rust` 生成并入库；
搬进本 crate（单一发射器）是 TODO。`params_builder` 收到的是**非保留 id** 的参数，
顺序 = `[[params]]` 声明顺序。

## 为什么结构体叫 `PluginParams`

nih-plug 的 trait 也叫 `Params`。生成物里如果把自己的结构体叫 `Params`，
`#[derive(Params)]` 与 `Arc<dyn Params>` 会因为 glob import 互相遮蔽（实测编译不过）。
所以生成物统一用 `PluginParams`，trait 仍叫 `Params`。

## 三条硬规则

1. **确定性**：同一份 manifest 永远产出同样的字节 → 生成物入库，`--check` 可进 CI；
2. **可读**：生成物是给人 review 的 Rust；
3. **不猜**：manifest 没写的不编造（缺 `[[params]]` 就没有参数）。

## 校验（生成前就挡住）

`package` 必须是 kebab-case、`name/vendor` 非空、**VST3 id 恰好 16 字节**（每个变体）、
参数 id 唯一且合法、`circuit` 字段合法且**同一字段不被两个参数映射**、
`gui = true` 时必须有 `[ui]`。

一次报全部问题，不是只报第一个。

## 生成物的行为验证

单元测试覆盖"生成 → 再生成（unchanged）→ 手改后 `--check` 报漂移 → 非法 manifest 报错"。

真正的编译验证：`new-plugin.sh` 生成的项目能 `cargo build` 并导出
`clap_entry` / `GetPluginFactory`（见 DEVLOG E48）。

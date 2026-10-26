# rustomp-gui

把 `rustomp.toml` 的 `[ui]` 变成一块能看到、能拖动、能踩的面板。

**这一层不碰 DSP**：它只读参数值、写参数值、画贴图。所有"音色"都在 `rustomp-dsp` 里。

## 1. 三层结构

```text
rustomp.toml 的 [ui]          ← 设计师/开发者写的声明
        │ tools/meta/plugin_meta.py --panel-rust（当前）
        │ （TODO：搬进 rustomp-gen，统一发射器）
        ▼
generated/panel.rs            ← pub const PANEL: PanelSpec（数组，长度不限）
        │ 你的 editor.rs（手写 ~100 行）
        ▼
rustomp_gui::draw(...)        ← 一次性画出整块面板，返回控件交互
```

`PanelSpec` 是 `const`，所以**漏素材 / id 拼错在编译期就能挡住**（`validate_panel` 用单测跑）。

## 2. 坐标系：1 单位 = 1 毫米

| 概念 | 约定 |
|---|---|
| `svg_w` / `svg_h` | 面板物理尺寸（mm），Sparklin Drive = 60 × 112（1590B 竖放） |
| 所有 `center` | **面板左上角为原点，单位 mm**，y 向下 |
| 素材密度 | **12 px/mm**（= 304.8 dpi）；逻辑窗口 = mm × 6 px/mm |
| 锚点 | 旋钮/拨杆/LED = 中心；脚踏 = 3PDT 六边形中心 |

> 旧素材是 `pt` 坐标，换算 ×0.352778。**PNG 里的 DPI 元数据是假的**，清晰度只看「像素 ÷ 物理尺寸」。

## 3. `PanelSpec` 的字段

| 控件 | 类型 | 关键字段 |
|---|---|---|
| 旋钮 | `&[KnobSpec]` | `param`、`center`、`radius`、`image` + `image_size`、`render`（`Rotate` / `Pointer`）、`angle_min/angle_max`、`drag_range` |
| 拨杆 | `&[ToggleSpec]` | 两态各自的贴图与中心（**两态归一化后 y_up == y_down**，不会再跳） |
| LED | `&[LedSpec]` | `indicator`（虚拟 id，如 `clip_indicator`）、`center`、`radius`、`off_color` / `on_color` |
| 脚踏 | `&[ButtonSpec]` | `param`（bool）、`center`、`radius`、`press_scale`、`press_depth` |

控件 id 是 `(kind, index)` 复合键，**不会和参数 id 撞名**。

### 三个不可动摇的顺序约定

1. **绘制顺序固定**：`DRAW_ORDER = panel → leds → knobs → toggles → buttons`，不可重排。
2. **旋钮整图旋转**：素材指针朝上 ⇔ 参数 0.5，因此 `angle_min + angle_max == 0`。
3. **开关不在电路里**（D-005）：拨杆/脚踏切的是 `Branch<A,B>` 的两份完整电路，不是拓扑开关。

## 4. 交互

| 动作 | 行为 |
|---|---|
| 拖旋钮 | 纵向拖动 → `drag_range`（默认 260）像素跑完整个范围，按住 Shift 精调 |
| 拖拨杆 | 拖动/点击切两态，可带 `travel_deg` 旋转 |
| 点脚踏 | 触发 bool 参数（true-bypass），按下时**从底图抠出那块缩放**，抬脚弹回 |

参数读写走 `ParamAccess` 抽象：`get_param(id)` / `set_param(id, value)`。
虚拟指示（LED）用 `get_param("clip_indicator")` 读引擎的 `activity_indicator()`。

## 5. 贴图与生命周期

```rust
pub struct Assets { /* include_bytes! 进来的 PNG 字节 */ }
```

- `include_bytes!` **相对源文件**：`plugin/src/lib.rs` 引用素材要写 `../../assets/`；
- 窗口重开必须 `reset_textures()`（纹理随旧的 `egui::Context` 一起失效）；
- `decode_png` **保留 alpha**（`from_rgba_unmultiplied`），交给 GPU 混合：
  提前和固定底色合成会让圆形贴图看着"带一个方底、边缘像被切过"（DEVLOG E62）。

## 6. 自检

```rust
#[test]
fn panel_is_consistent() {
    let problems = rustomp_gui::validate_panel(&PANEL, &ASSETS);
    assert!(problems.is_empty(), "{problems:#?}");
}
```

`validate_panel` 检查：参数 id 是否存在、素材是否提供、旋钮角度是否对称、
控件是否越界、拨杆两态锚点是否归一。示例见 `examples/panel_check.rs`。

## 7. 现状与待办

| 项 | 状态 |
|---|---|
| 旋钮（整图旋转 / 指针）、拨杆、LED、脚踏 | ✅ 已在 Sparklin Drive v2 上跑通 |
| 毫米坐标 + 12 px/mm | ✅ |
| `validate_panel` + 4 个单测 | ✅ |
| 面板热重载（改素材不重启宿主） | ⬜ |
| 自定义字体/位图数字（数值显示） | ⬜ |
| `panel.rs` 由 `rustomp-gen` 生成（统一发射器） | ⬜ TODO |

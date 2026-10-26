// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 框架的**通用界面层**。
//!
//! 只提供"怎么做"：贴图面板、声明式控件、交互、纹理生命周期。
//! **不包含任何素材、坐标或参数名** —— 那些由插件项目通过 [`PanelSpec`] 与 [`Assets`] 提供。
//!
//! 典型用法（在插件外壳里）：
//!
//! ```ignore
//! let params = self.params.clone();
//! create_egui_editor(state.clone(), UiState::default(),
//!     |_ctx, state| rustomp_gui::reset_textures(state),
//!     move |ctx, setter, state| {
//!         let access = MyParamAccess { params: &params, setter };
//!         egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
//!             rustomp_gui::draw(ui, &PANEL, &ASSETS, state, &access, clip_level);
//!         });
//!     })
//! ```
//!
//! `PANEL` 由插件项目的 `build.rs` 从 `rustomp.toml` 生成（见 `plugin_meta.py --panel-rust`）。

use nih_plug_egui::egui;

// ---------------------------------------------------------------- 声明式面板描述

/// 旋钮的渲染方式。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KnobRender {
    /// **整图旋转**（默认）：素材里指针朝上，旋转整张图。
    ///
    /// 约定：**素材指针朝上 ⇔ 参数 0.5**，因此 `angle_min + angle_max` 必须为 0
    /// （对称范围）。设计器只暴露"总转角"，导出时写成 `−travel/2 .. +travel/2`。
    Rotate,
    /// 只画一根指针线（指针颜色固定近白，深色面板慎用）。用于写实风格素材。
    Pointer,
}

/// 一个旋钮（坐标都是**素材坐标**）。
#[derive(Clone, Copy)]
pub struct KnobSpec {
    /// 对应哪个参数（由插件在 `ParamAccess` 里解释）
    pub param: &'static str,
    pub center: (f32, f32),
    /// 命中半径（点击/拖动区域）
    pub radius: f32,
    /// 整图旋转用的贴图名（[`Assets::images`] 里的名字）；`Rotate` 模式必填
    pub image: &'static str,
    /// 贴图绘制尺寸（素材坐标）。旋转围绕 `center`，建议素材是正方形
    pub image_size: (f32, f32),
    pub render: KnobRender,
    /// 指针长度（从圆心到端点）；仅 `Pointer` 模式使用
    pub pointer_len: f32,
    /// 指针线宽；仅 `Pointer` 模式使用
    pub pointer_width: f32,
    /// 指针角度范围（度），**0.5 对应正上方**；`Rotate` 模式要求 `min + max == 0`
    pub angle_min: f32,
    pub angle_max: f32,
    /// 拖动多少屏幕像素走完整个 0..1 行程
    pub drag_range: f32,
}

/// 指示灯（通常是削波指示）。
#[derive(Clone, Copy)]
pub struct LedSpec {
    pub param: &'static str,
    pub center: (f32, f32),
    pub radius: f32,
    /// 熄灭色 / 点亮色（sRGB）
    pub off: (u8, u8, u8),
    pub on: (u8, u8, u8),
}

/// 拨杆开关：两种状态各一张贴图。
#[derive(Clone, Copy)]
pub struct ToggleSpec {
    pub param: &'static str,
    /// 贴图名（在 [`Assets::images`] 里查）
    pub image_up: &'static str,
    pub image_down: &'static str,
    pub x: f32,
    /// 两种状态各自的贴图左上角 y（必须各自对齐到同一基线，否则切换时会跳）
    pub y_up: f32,
    pub y_down: f32,
    pub width: f32,
    pub height: f32,
    /// 缩放反馈的原点（通常是六边形基线中点）
    pub pivot: (f32, f32),
    pub press_scale: f32,
}

/// 按钮（例如 bypass）：外形不随状态变化，只有按住时缩放。
#[derive(Clone, Copy)]
pub struct ButtonSpec {
    pub param: &'static str,
    pub center: (f32, f32),
    /// 命中半径，**必须 ≥ 美术外接圆半径**（按下时要擦掉这块）
    pub radius: f32,
    pub press_scale: f32,
    /// 按下态贴图（""=没有）。有的话按下时**整张换上**（尺寸按 `image_size`），
    /// 不再做"把底图那块缩小贴回"。
    pub image_hold: &'static str,
    /// 按下态贴图的物理尺寸（mm）；`image_hold` 为空时忽略
    pub image_size: (f32, f32),
}

/// 整个面板的声明。
pub struct PanelSpec {
    pub svg_w: f32,
    pub svg_h: f32,
    /// 窗口逻辑尺寸
    pub window: (u32, u32),
    pub background: (u8, u8, u8),
    /// 旋钮（数量不限）
    pub knobs: &'static [KnobSpec],
    /// 指示灯（数量不限）
    pub leds: &'static [LedSpec],
    /// 拨杆（数量不限）
    pub toggles: &'static [ToggleSpec],
    /// 按钮（数量不限）
    pub buttons: &'static [ButtonSpec],
}

/// 绘制顺序是**固定的**（跨类型不可重排）：
///
/// ```text
/// 底色 → panel.png → LED → 旋钮 → 拨杆 → 按钮
/// ```
///
/// 同一类型内部按数组顺序叠放。设计器的层列表必须照这个顺序显示（只读）。
pub const DRAW_ORDER: &str = "panel → leds → knobs → toggles → buttons";

/// 插件提供的素材（框架不持有任何素材）。
pub struct Assets {
    /// 面板底图（PNG 字节）
    pub panel: &'static [u8],
    /// 具名贴图，例如拨杆的两种状态
    pub images: &'static [(&'static str, &'static [u8])],
}

// ---------------------------------------------------------------- 参数访问

/// 框架通过它读写参数，因此**不需要知道参数的类型或名字集合**。
///
/// 约定：布尔参数以 0.0 / 1.0 表示。
pub trait ParamAccess {
    /// 读取当前值（归一化 0..1）；不认识的 id 返回 None
    fn get(&self, id: &str) -> Option<f32>;
    /// 写入值（归一化 0..1）；不认识的 id 忽略
    fn set(&self, id: &str, value: f32);
}

// ---------------------------------------------------------------- 纹理生命周期

/// 跨帧缓存的纹理。
///
/// **必须**在每次新建 egui Context 时调用 [`reset_textures`]：
/// `TextureHandle` 绑定的是创建它的 Context，窗口重开后旧句柄会失效
/// （表现为贴图消失但交互仍在）。
#[derive(Default)]
pub struct UiState {
    panel: Option<egui::TextureHandle>,
    images: Vec<(&'static str, egui::TextureHandle)>,
}

/// 新建上下文后清空纹理缓存（下一帧自动重建）。
pub fn reset_textures(state: &mut UiState) {
    state.panel = None;
    state.images.clear();
}

// ---------------------------------------------------------------- 绘制

/// 绘制整个面板并处理交互。
///
/// * `activity`：活动指示电平（0..1），通常来自 `Engine::activity_indicator()`
pub fn draw(
    ui: &mut egui::Ui,
    spec: &'static PanelSpec,
    assets: &'static Assets,
    state: &mut UiState,
    params: &dyn ParamAccess,
    activity: f32,
) {
    let avail = ui.max_rect();
    let ppp = ui.ctx().pixels_per_point();

    // 保持素材宽高比；宿主给了别的比例就留边而不是拉伸
    let scale = (avail.width() / spec.svg_w).min(avail.height() / spec.svg_h);
    let rect = egui::Rect::from_center_size(
        avail.center(),
        egui::vec2(spec.svg_w * scale, spec.svg_h * scale),
    );
    let to_screen = |p: (f32, f32)| -> egui::Pos2 {
        egui::pos2(rect.left() + p.0 * scale, rect.top() + p.1 * scale)
    };
    let bg = egui::Color32::from_rgb(spec.background.0, spec.background.1, spec.background.2);

    let painter = ui.painter().clone();
    painter.rect_filled(avail, 0.0, bg);

    // 底图：按窗口像素尺寸生成一次
    let want = [
        (rect.width() * ppp).max(1.0) as usize,
        (rect.height() * ppp).max(1.0) as usize,
    ];
    if state
        .panel
        .as_ref()
        .map(|t| t.size() != want)
        .unwrap_or(true)
    {
        if let Some(img) = decode_png(assets.panel, want[0], want[1]) {
            state.panel = Some(ui.ctx().load_texture(
                "rustomp-panel",
                img,
                egui::TextureOptions::LINEAR,
            ));
        }
    }
    let panel_tex = state.panel.clone();
    if let Some(t) = &panel_tex {
        painter.image(
            t.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    // 具名贴图（拨杆两态）
    for (name, bytes) in assets.images {
        if !state.images.iter().any(|(n, _)| n == name) {
            // 贴图按需生成，尺寸由 owner 决定（这里用原始像素宽度）
            if let Some(img) = decode_png(bytes, 0, 0) {
                let tex = ui
                    .ctx()
                    .load_texture(*name, img, egui::TextureOptions::LINEAR);
                state.images.push((name, tex));
            }
        }
    }
    let image_tex = |name: &str| -> Option<egui::TextureHandle> {
        state
            .images
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| t.clone())
    };

    // ---- 指示灯（可多个）----
    for led in spec.leds {
        let c = to_screen(led.center);
        let r = led.radius * scale;
        let level = params.get(led.param).unwrap_or(activity).clamp(0.0, 1.0);
        let t = level;
        let colour = egui::Color32::from_rgb(
            (led.off.0 as f32 + (led.on.0 as f32 - led.off.0 as f32) * t) as u8,
            (led.off.1 as f32 + (led.on.1 as f32 - led.off.1 as f32) * t) as u8,
            (led.off.2 as f32 + (led.on.2 as f32 - led.off.2 as f32) * t) as u8,
        );
        painter.circle_filled(c, r, colour);
    }

    // ---- 旋钮：整图旋转（默认）或指针线；可多个 ----
    for (ki, knob) in spec.knobs.iter().enumerate() {
        let c = to_screen(knob.center);
        let value = params.get(knob.param).unwrap_or(0.5).clamp(0.0, 1.0);
        let ang_deg = knob_angle_deg(value, knob.angle_min, knob.angle_max);

        match knob.render {
            KnobRender::Rotate => {
                if let Some(t) = image_tex(knob.image) {
                    let half = egui::vec2(
                        knob.image_size.0 * scale * 0.5,
                        knob.image_size.1 * scale * 0.5,
                    );
                    paint_rotated_image(&painter, t.id(), c, half, ang_deg.to_radians());
                }
            }
            KnobRender::Pointer => {
                let ang = ang_deg.to_radians();
                let dir = egui::vec2(ang.sin(), -ang.cos());
                painter.line_segment(
                    [c, c + dir * (knob.pointer_len * scale)],
                    egui::Stroke::new(
                        (knob.pointer_width * scale).max(2.0),
                        egui::Color32::from_rgb(0xf5, 0xf5, 0xf5),
                    ),
                );
            }
        }

        let r = knob.radius * scale;
        let hit = egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0));
        // id 用 (类型, 序号)：两个旋钮绑同一个参数时也不会被 egui 合并成一个控件
        let resp = ui.interact(
            hit,
            ui.id().with(("knob", ki)),
            egui::Sense::click_and_drag(),
        );
        let mut v = value;
        if resp.dragged() {
            v -= resp.drag_delta().y / (knob.drag_range * scale).max(1.0);
        }
        if resp.double_clicked() {
            v = 0.5;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                v += scroll / (1000.0 * scale).max(1.0);
            }
        }
        v = v.clamp(0.0, 1.0);
        if (v - value).abs() > f32::EPSILON {
            params.set(knob.param, v);
        }
    }

    // ---- 拨杆开关（可多个）----
    for (ti, tg) in spec.toggles.iter().enumerate() {
        let on = params.get(tg.param).unwrap_or(0.0) > 0.5;
        let (y, tex_name) = if on {
            (tg.y_down, tg.image_down)
        } else {
            (tg.y_up, tg.image_up)
        };
        let sw_rect = egui::Rect::from_min_size(
            to_screen((tg.x, y)),
            egui::vec2(tg.width * scale, tg.height * scale),
        );
        // 点击区 = 两种状态贴图的并集
        let union_h = (tg.y_down - tg.y_up).abs() + tg.height;
        let hit = egui::Rect::from_min_size(
            to_screen((tg.x, tg.y_up.min(tg.y_down))),
            egui::vec2(tg.width * scale, union_h * scale),
        );
        let resp = ui.interact(hit, ui.id().with(("toggle", ti)), egui::Sense::click());
        if resp.clicked() {
            params.set(tg.param, if on { 0.0 } else { 1.0 });
        }
        // 按住时以 pivot 为原点缩放
        let draw_rect = if resp.is_pointer_button_down_on() {
            let p = to_screen(tg.pivot);
            egui::Rect::from_min_size(
                p + (sw_rect.min - p) * tg.press_scale,
                sw_rect.size() * tg.press_scale,
            )
        } else {
            sw_rect
        };
        if let Some(t) = image_tex(tex_name) {
            painter.image(
                t.id(),
                draw_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    // ---- 按钮（bypass/脚踏）：外形不变，按住时缩放；可多个 ----
    for (bi, btn) in spec.buttons.iter().enumerate() {
        let c = to_screen(btn.center);
        let r = btn.radius * scale;
        let resp = ui.interact(
            egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)),
            ui.id().with(("button", bi)),
            egui::Sense::click(),
        );
        let on = params.get(btn.param).unwrap_or(0.0) > 0.5;
        if resp.clicked() {
            params.set(btn.param, if on { 0.0 } else { 1.0 });
        }
        if resp.is_pointer_button_down_on() {
            // 擦掉原位（方块；radius 必须盖住美术，见 README 的约束）
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)),
                0.0,
                bg,
            );
            let hold = image_tex(btn.image_hold);
            if let Some(t) = hold.filter(|_| !btn.image_hold.is_empty()) {
                // 设计师给了按住态：整张换上（与常态同尺寸，不缩放）
                let (w, h) = (btn.image_size.0 * scale, btn.image_size.1 * scale);
                painter.image(
                    t.id(),
                    egui::Rect::from_center_size(c, egui::vec2(w, h)),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else if let Some(t) = &panel_tex {
                // 兜底：从底图取该区域，再贴回缩小版（旧行为）
                let r_svg = r / scale;
                let uv = egui::Rect::from_min_max(
                    egui::pos2(
                        (btn.center.0 - r_svg) / spec.svg_w,
                        (btn.center.1 - r_svg) / spec.svg_h,
                    ),
                    egui::pos2(
                        (btn.center.0 + r_svg) / spec.svg_w,
                        (btn.center.1 + r_svg) / spec.svg_h,
                    ),
                );
                painter.image(
                    t.id(),
                    egui::Rect::from_center_size(
                        c,
                        egui::vec2(r * 2.0 * btn.press_scale, r * 2.0 * btn.press_scale),
                    ),
                    uv,
                    egui::Color32::WHITE,
                );
            }
        }
    }
}

// ---------------------------------------------------------------- 几何（可单测）

/// 参数值 → 指针角度（度）。**0.5 对正上方（0°）**。
///
/// 这是 `rustomp-box-designer` 与运行时之间唯一的"角度契约"：
/// 素材里指针朝上 ⇔ 参数 0.5 ⇒ `angle_min + angle_max == 0`。
#[inline]
pub fn knob_angle_deg(value: f32, angle_min: f32, angle_max: f32) -> f32 {
    angle_min + (angle_max - angle_min) * value.clamp(0.0, 1.0)
}

/// 画一张**绕中心旋转**的贴图（`Mesh` 手工搭四边形，不依赖 egui 的 widget 旋转 API）。
pub fn paint_rotated_image(
    painter: &egui::Painter,
    tex: egui::TextureId,
    center: egui::Pos2,
    half: egui::Vec2,
    angle: f32,
) {
    let (s, c) = angle.sin_cos();
    let rot = |v: egui::Vec2| egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c);
    let mut mesh = egui::Mesh::with_texture(tex);
    let corners = [
        (-half.x, -half.y, 0.0f32, 0.0f32),
        (half.x, -half.y, 1.0, 0.0),
        (half.x, half.y, 1.0, 1.0),
        (-half.x, half.y, 0.0, 1.0),
    ];
    for (x, y, u, v) in corners {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + rot(egui::vec2(x, y)),
            uv: egui::pos2(u, v),
            color: egui::Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

// ---------------------------------------------------------------- 校验（导出/构建时用）

/// 面板声明的静态校验。返回全部问题（不是只报第一个）。
///
/// 设计器导出前、插件构建时都应该跑一遍 —— 这里挡住的每一条，
/// 在运行时都表现为"控件没反应/指针看不到/切换跳一下"这类难查的现象。
pub fn validate_panel(spec: &PanelSpec, assets: &Assets) -> Vec<String> {
    let mut errs = Vec::new();
    let has_image = |name: &str| assets.images.iter().any(|(n, _)| *n == name);

    if !(spec.svg_w > 0.0 && spec.svg_h > 0.0) {
        errs.push(format!("画布尺寸非法：{} × {}", spec.svg_w, spec.svg_h));
    }
    if spec.window.0 == 0 || spec.window.1 == 0 {
        errs.push(format!("窗口尺寸非法：{:?}", spec.window));
    }

    for (i, k) in spec.knobs.iter().enumerate() {
        let tag = format!("knobs[{i}]（param={}）", k.param);
        if k.param.is_empty() {
            errs.push(format!("{tag}：param 为空"));
        }
        if k.radius <= 0.0 {
            errs.push(format!("{tag}：radius 必须 > 0（否则点不到）"));
        }
        if k.render == KnobRender::Rotate {
            // 约定：素材指针朝上 ⇔ 参数 0.5 ⇒ 角度范围必须对称
            if (k.angle_min + k.angle_max).abs() > 1e-3 {
                errs.push(format!(
                    "{tag}：Rotate 模式要求 angle_min + angle_max == 0（现在是 {} + {}），否则指针会整体偏转",
                    k.angle_min, k.angle_max
                ));
            }
            if k.image.is_empty() || !has_image(k.image) {
                errs.push(format!("{tag}：Rotate 模式缺少贴图 `{}`", k.image));
            }
            if !(k.image_size.0 > 0.0 && k.image_size.1 > 0.0) {
                errs.push(format!("{tag}：image_size 非法 {:?}", k.image_size));
            }
        } else if k.pointer_len <= 0.0 {
            errs.push(format!("{tag}：Pointer 模式需要 pointer_len > 0"));
        }
    }

    for (i, l) in spec.leds.iter().enumerate() {
        if l.param.is_empty() {
            errs.push(format!("leds[{i}]：param 为空"));
        }
        if l.radius <= 0.0 {
            errs.push(format!("leds[{i}]：radius 必须 > 0"));
        }
    }

    for (i, t) in spec.toggles.iter().enumerate() {
        let tag = format!("toggles[{i}]（param={}）", t.param);
        if !has_image(t.image_up) || !has_image(t.image_down) {
            errs.push(format!(
                "{tag}：缺少贴图 `{}` / `{}`",
                t.image_up, t.image_down
            ));
        }
        if t.width <= 0.0 || t.height <= 0.0 {
            errs.push(format!("{tag}：尺寸非法 {} × {}", t.width, t.height));
        }
        if t.press_scale <= 0.0 || t.press_scale > 1.0 {
            errs.push(format!(
                "{tag}：press_scale 应在 (0, 1]，现在是 {}",
                t.press_scale
            ));
        }
    }

    for (i, b) in spec.buttons.iter().enumerate() {
        if b.param.is_empty() {
            errs.push(format!("buttons[{i}]：param 为空"));
        }
        if b.radius <= 0.0 {
            errs.push(format!("buttons[{i}]：radius 必须 > 0"));
        }
    }

    errs
}

// ---------------------------------------------------------------- PNG 解码

/// 解码内嵌 PNG，并缩放到目标像素尺寸（`w == 0` 表示保持原始尺寸）。
///
/// **必须保留 alpha**：贴图（旋钮/拨杆）是叠在面板美术上的，
/// 把 alpha 提前与某个固定底色合成会让贴图边缘对不上下面的像素 ——
/// 表现就是"旋钮边缘像被方框切过"（DEVLOG E62）。交给 GPU 混合即可。
pub fn decode_png(data: &'static [u8], w: usize, h: usize) -> Option<egui::ColorImage> {
    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (sw, sh) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::Grayscale => 1,
        _ => return None,
    };
    let dw = if w == 0 { sw } else { w.max(1) };
    let dh = if h == 0 {
        (dw * sh / sw.max(1)).max(1)
    } else {
        h.max(1)
    };
    let mut px = Vec::with_capacity(dw * dh);
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let i = (sy * sw + sx) * channels;
            let (r, g, b) = match channels {
                4 | 3 => (buf[i], buf[i + 1], buf[i + 2]),
                _ => (buf[i], buf[i], buf[i]),
            };
            let a = if channels == 4 { buf[i + 3] } else { 255 };
            // egui 的 Color32 默认按 **unmultiplied** 解释，直接带上 alpha 即可正确混合
            px.push(egui::Color32::from_rgba_unmultiplied(r, g, b, a));
        }
    }
    Some(egui::ColorImage {
        size: [dw, dh],
        pixels: px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static IMGS: [(&str, &[u8]); 2] = [("up", &[]), ("down", &[])];
    static ASSETS: Assets = Assets {
        panel: &[],
        images: &IMGS,
    };

    fn knob(render: KnobRender, min: f32, max: f32) -> KnobSpec {
        KnobSpec {
            param: "gain",
            center: (10.0, 10.0),
            radius: 5.0,
            image: "up",
            image_size: (12.0, 12.0),
            render,
            pointer_len: 4.0,
            pointer_width: 0.5,
            angle_min: min,
            angle_max: max,
            drag_range: 200.0,
        }
    }

    #[test]
    fn angle_mapping_puts_midpoint_up() {
        // −135..+135：0 / 0.5 / 1 → −135 / 0 / +135（0.5 正上方）
        assert_eq!(knob_angle_deg(0.0, -135.0, 135.0), -135.0);
        assert_eq!(knob_angle_deg(0.5, -135.0, 135.0), 0.0);
        assert_eq!(knob_angle_deg(1.0, -135.0, 135.0), 135.0);
        // 越界要夹住
        assert_eq!(knob_angle_deg(2.0, -135.0, 135.0), 135.0);
    }

    #[test]
    fn rotate_requires_symmetric_range() {
        let ks_good: &'static [KnobSpec] =
            Box::leak(vec![knob(KnobRender::Rotate, -135.0, 135.0)].into_boxed_slice());
        let ks_bad: &'static [KnobSpec] =
            Box::leak(vec![knob(KnobRender::Rotate, -20.0, 200.0)].into_boxed_slice());
        let good = PanelSpec {
            svg_w: 100.0,
            svg_h: 50.0,
            window: (300, 150),
            background: (0, 0, 0),
            knobs: ks_good,
            leds: &[],
            toggles: &[],
            buttons: &[],
        };
        assert!(validate_panel(&good, &ASSETS).is_empty());

        let bad = PanelSpec {
            knobs: ks_bad,
            ..good
        };
        let errs = validate_panel(&bad, &ASSETS);
        assert!(
            errs.iter().any(|e| e.contains("angle_min + angle_max")),
            "{errs:?}"
        );
    }

    #[test]
    fn missing_toggle_asset_is_reported() {
        static T: ToggleSpec = ToggleSpec {
            param: "symmetric",
            image_up: "nope",
            image_down: "down",
            x: 1.0,
            y_up: 2.0,
            y_down: 2.0,
            width: 4.0,
            height: 4.0,
            pivot: (3.0, 4.0),
            press_scale: 0.9,
        };
        let spec = PanelSpec {
            svg_w: 100.0,
            svg_h: 50.0,
            window: (300, 150),
            background: (0, 0, 0),
            knobs: &[],
            leds: &[],
            toggles: Box::leak(vec![T].into_boxed_slice()),
            buttons: &[],
        };
        let errs = validate_panel(&spec, &ASSETS);
        assert!(errs.iter().any(|e| e.contains("nope")), "{errs:?}");
    }

    #[test]
    fn multiple_leds_and_toggles_are_allowed() {
        static L: [LedSpec; 3] = [LedSpec {
            param: "clip_indicator",
            center: (1.0, 1.0),
            radius: 1.0,
            off: (0, 0, 0),
            on: (255, 0, 0),
        }; 3];
        static T: [ToggleSpec; 2] = [ToggleSpec {
            param: "symmetric",
            image_up: "up",
            image_down: "down",
            x: 1.0,
            y_up: 2.0,
            y_down: 2.0,
            width: 4.0,
            height: 4.0,
            pivot: (3.0, 4.0),
            press_scale: 0.9,
        }; 2];
        let spec = PanelSpec {
            svg_w: 100.0,
            svg_h: 50.0,
            window: (300, 150),
            background: (0, 0, 0),
            knobs: &[],
            leds: &L,
            toggles: &T,
            buttons: &[],
        };
        assert!(validate_panel(&spec, &ASSETS).is_empty());
    }
}

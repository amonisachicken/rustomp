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

/// 一个旋钮（坐标都是素材 SVG 的 viewBox 坐标）。
#[derive(Clone, Copy)]
pub struct KnobSpec {
    /// 对应哪个参数（由插件在 `ParamAccess` 里解释）
    pub param: &'static str,
    pub center: (f32, f32),
    pub radius: f32,
    /// 指针长度（从圆心到端点）
    pub pointer_len: f32,
    pub pointer_width: f32,
    /// 指针角度范围（度），0.5 对应正上方
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
    pub radius: f32,
    pub press_scale: f32,
}

/// 整个面板的声明。
pub struct PanelSpec {
    pub svg_w: f32,
    pub svg_h: f32,
    /// 窗口逻辑尺寸
    pub window: (u32, u32),
    pub background: (u8, u8, u8),
    pub knobs: &'static [KnobSpec],
    pub led: Option<LedSpec>,
    pub toggle: Option<ToggleSpec>,
    pub button: Option<ButtonSpec>,
}

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
    if state.panel.as_ref().map(|t| t.size() != want).unwrap_or(true) {
        if let Some(img) = decode_png(assets.panel, want[0], want[1]) {
            state.panel =
                Some(ui.ctx().load_texture("rustomp-panel", img, egui::TextureOptions::LINEAR));
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

    // ---- 指示灯 ----
    if let Some(led) = &spec.led {
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

    // ---- 旋钮：指针 + 交互 ----
    for knob in spec.knobs {
        let c = to_screen(knob.center);
        let value = params.get(knob.param).unwrap_or(0.0).clamp(0.0, 1.0);

        let ang = (knob.angle_min + (knob.angle_max - knob.angle_min) * value).to_radians();
        let dir = egui::vec2(ang.sin(), -ang.cos());
        painter.line_segment(
            [c, c + dir * (knob.pointer_len * scale)],
            egui::Stroke::new((knob.pointer_width * scale).max(2.0), egui::Color32::from_rgb(0xf5, 0xf5, 0xf5)),
        );

        let r = knob.radius * scale;
        let hit = egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0));
        let resp = ui.interact(hit, ui.id().with(knob.param), egui::Sense::click_and_drag());
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

    // ---- 拨杆开关 ----
    if let Some(tg) = &spec.toggle {
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
        let resp = ui.interact(hit, ui.id().with(tg.param), egui::Sense::click());
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

    // ---- 按钮（bypass）：外形不变，按住时缩放 ----
    if let Some(btn) = &spec.button {
        let c = to_screen(btn.center);
        let r = btn.radius * scale;
        let resp = ui.interact(
            egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)),
            ui.id().with(btn.param),
            egui::Sense::click(),
        );
        let on = params.get(btn.param).unwrap_or(0.0) > 0.5;
        if resp.clicked() {
            params.set(btn.param, if on { 0.0 } else { 1.0 });
        }
        if resp.is_pointer_button_down_on() {
            if let Some(t) = &panel_tex {
                // 先从底图取该区域（uv 子矩形），抹掉原位，再贴回缩小版
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
                painter.rect_filled(
                    egui::Rect::from_center_size(c, egui::vec2(r * 2.0, r * 2.0)),
                    0.0,
                    bg,
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

// ---------------------------------------------------------------- PNG 解码

/// 解码内嵌 PNG，并缩放到目标像素尺寸（`w == 0` 表示保持原始尺寸）。
///
/// 忽略 alpha 会把透明像素当成黑色（曾在拨杆贴图上表现为"边缘发脏"），
/// 所以这里把 alpha 与面板底色合成。
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
            // alpha 与白底合成（面板底色由上层覆盖，这里只要不留黑边）
            let (r, g, b) = if channels == 4 {
                let a = buf[i + 3] as u16;
                let mix = |c: u8, bg: u8| -> u8 {
                    ((c as u16 * a + bg as u16 * (255 - a)) / 255) as u8
                };
                (mix(r, 0xfa), mix(g, 0xf1), mix(b, 0xe0))
            } else {
                (r, g, b)
            };
            px.push(egui::Color32::from_rgb(r, g, b));
        }
    }
    Some(egui::ColorImage {
        size: [dw, dh],
        pixels: px,
    })
}

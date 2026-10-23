// SPDX-License-Identifier: GPL-3.0-or-later

//! **Sparklin Drive** —— HONEYPIE EFFECTS
//!
//! 电路级建模的失真/法兹（fakedist v1）：单电源 9V、OP07、
//! 反馈环内二极管削波。DSP 全部在 `rustomp-dsp`，与 ngspice 基线有对拍测试。
//!
//! 本文件只负责：参数、采样率、声道实例、延迟上报、以及 egui 界面。
//!
//! 界面：`assets/panel@2x.png`（由 `assets/panel.svg` 光栅化，去掉了指针）
//! 作为底图，指针 / LED / 交互全部程序化绘制。

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use rustomp_dsp::{Engine, Params as DspParams};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

// ---------------------------------------------------------------- 身份
#[cfg(feature = "mono")]
const PLUGIN_NAME: &str = "Sparklin Drive Mono";
#[cfg(feature = "stereo")]
const PLUGIN_NAME: &str = "Sparklin Drive Stereo";
#[cfg(not(any(feature = "mono", feature = "stereo")))]
const PLUGIN_NAME: &str = "Sparklin Drive";

#[cfg(feature = "mono")]
const CLAP_ID: &str = "com.honeypieeffects.sparklindrive.mono";
#[cfg(feature = "stereo")]
const CLAP_ID: &str = "com.honeypieeffects.sparklindrive.stereo";
#[cfg(not(any(feature = "mono", feature = "stereo")))]
const CLAP_ID: &str = "com.honeypieeffects.sparklindrive";

#[cfg(feature = "mono")]
const VST3_ID: [u8; 16] = *b"SparklinDriveMo1";
#[cfg(feature = "stereo")]
const VST3_ID: [u8; 16] = *b"SparklinDriveSt1";
#[cfg(not(any(feature = "mono", feature = "stereo")))]
const VST3_ID: [u8; 16] = *b"SparklinDriveV01";

#[cfg(all(feature = "mono", feature = "stereo"))]
compile_error!("mono 与 stereo 两个 feature 不能同时开启");

#[cfg(feature = "mono")]
const AUDIO_LAYOUTS: &[AudioIOLayout] = &[AudioIOLayout {
    main_input_channels: NonZeroU32::new(1),
    main_output_channels: NonZeroU32::new(1),
    aux_input_ports: &[],
    aux_output_ports: &[],
    names: PortNames::const_default(),
}];

#[cfg(feature = "stereo")]
const AUDIO_LAYOUTS: &[AudioIOLayout] = &[AudioIOLayout {
    main_input_channels: NonZeroU32::new(2),
    main_output_channels: NonZeroU32::new(2),
    aux_input_ports: &[],
    aux_output_ports: &[],
    names: PortNames::const_default(),
}];

#[cfg(not(any(feature = "mono", feature = "stereo")))]
const AUDIO_LAYOUTS: &[AudioIOLayout] = &[
    AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    },
    AudioIOLayout {
        main_input_channels: NonZeroU32::new(1),
        main_output_channels: NonZeroU32::new(1),
        ..AudioIOLayout::const_default()
    },
];

// ---------------------------------------------------------------- 面板几何
// 全部取自 assets/panel.svg 的 viewBox 0 0 170.08 317.48
mod ui {
    pub const SVG_W: f32 = 170.08;
    pub const SVG_H: f32 = 317.48;
    /// 窗口逻辑尺寸（2 倍素材）
    pub const WIN_W: u32 = 340;
    pub const WIN_H: u32 = 635;

    pub const LEVEL: (f32, f32) = (43.41, 71.59);
    pub const GAIN: (f32, f32) = (126.51, 70.43);
    pub const KNOB_R: f32 = 27.86;
    pub const POINTER_LEN: f32 = 24.59;
    pub const POINTER_W: f32 = 0.98;

    pub const LED: (f32, f32) = (85.04, 33.83);
    pub const LED_R: f32 = 8.5;

    /// 削波模式小开关：贴图左上角与尺寸（viewBox 23.68 × 25.21）
    /// 位置由开关的六角形中心与面板原图对齐得出
    pub const MODE_SW_XY: (f32, f32) = (73.49, 108.455);
    pub const MODE_SW_W: f32 = 23.68;
    pub const MODE_SW_H: f32 = 25.21;
    /// 点击区域 = 两种状态贴图的并集（两者 viewBox 相同，即同一个矩形）
    pub const MODE_SW_CENTER: (f32, f32) = (85.33, 121.6);

    /// 下方大开关 = bypass
    pub const BYPASS_SW: (f32, f32) = (85.33, 236.17);
    pub const BYPASS_SW_R: f32 = 22.0;
}

/// 面板底图（`inkscape --export-width=680` 从 assets/panel.svg 生成）
const PANEL_PNG: &[u8] = include_bytes!("../../../assets/panel@2x.png");
/// 削波模式开关的两种状态（用户提供，`inkscape --export-width=95` 生成 4x）
const SWITCH_UP_PNG: &[u8] = include_bytes!("../../../assets/switch_up@4x.png");
const SWITCH_DOWN_PNG: &[u8] = include_bytes!("../../../assets/switch_down@4x.png");

// ---------------------------------------------------------------- 参数
#[derive(Params)]
struct SparklinParams {
    /// GAIN 电位器（线性 taper，0..500kΩ）
    #[id = "gain"]
    gain: FloatParam,
    /// LEVEL 电位器
    #[id = "level"]
    level: FloatParam,
    /// 削波模式：关 = 非对称（3 只二极管，原机）；开 = 对称（2 只）
    #[id = "mode"]
    symmetric: BoolParam,
    /// true-bypass
    #[id = "bypass"]
    bypass: BoolParam,

    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
}

impl Default for SparklinParams {
    fn default() -> Self {
        let pot = |name: &str| {
            FloatParam::new(name, 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage())
        };
        Self {
            gain: pot("Gain"),
            level: pot("Level"),
            symmetric: BoolParam::new("Clip Mode SYM", false)
                .with_value_to_string(Arc::new(|v| if v { "Sym".into() } else { "Asym".into() })),
            bypass: BoolParam::new("Bypass", false),
            editor_state: EguiState::from_size(ui::WIN_W, ui::WIN_H),
        }
    }
}

// ---------------------------------------------------------------- 插件
const MAX_CHANNELS: usize = 2;

pub struct SparklinDrive {
    params: Arc<SparklinParams>,
    engines: [Engine; MAX_CHANNELS],
    /// 削波指示（0..1），GUI 读取
    clip_level: Arc<AtomicU32>, // f32 bits
}

impl Default for SparklinDrive {
    fn default() -> Self {
        Self {
            params: Arc::new(SparklinParams::default()),
            engines: [Engine::new(48_000.0), Engine::new(48_000.0)],
            clip_level: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl Plugin for SparklinDrive {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "HONEYPIE EFFECTS";
    const URL: &'static str = "https://github.com/amonisachicken/rustomp";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = AUDIO_LAYOUTS;
    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let fs = buffer_config.sample_rate as f64;
        for e in self.engines.iter_mut() {
            e.set_sample_rate(fs);
        }
        context.set_latency_samples(Engine::latency_samples());
        true
    }

    fn reset(&mut self) {
        for e in self.engines.iter_mut() {
            e.reset();
        }
        self.params.gain.smoothed.reset(self.params.gain.value());
        self.params.level.smoothed.reset(self.params.level.value());
        self.clip_level.store(0, Ordering::Relaxed);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let symmetric = self.params.symmetric.value();
        let bypass = self.params.bypass.value();
        let mut peak = 0.0f32;

        for mut channel_samples in buffer.iter_samples() {
            let gain = self.params.gain.smoothed.next() as f64;
            let level = self.params.level.smoothed.next() as f64;
            let p = DspParams { gain, level, symmetric, bypass };
            for (ch, sample) in channel_samples.iter_mut().enumerate() {
                if let Some(e) = self.engines.get_mut(ch) {
                    e.set_params(p);
                    *sample = e.process(*sample as f64) as f32;
                    peak = peak.max(e.clip_indicator());
                }
            }
        }

        // 给 GUI 的削波指示（锁-free，原子存 f32 位）
        self.clip_level.store(peak.to_bits(), Ordering::Relaxed);
        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        for e in self.engines.iter_mut() {
            e.reset();
        }
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let clip = self.clip_level.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            UiState::default(),
            |ctx, _state| {
                ctx.set_visuals(egui::Visuals::light());
            },
            move |ctx, setter, state| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        draw_panel(ui, &params, setter, state, &clip);
                    });
            },
        )
    }
}

impl ClapPlugin for SparklinDrive {
    const CLAP_ID: &'static str = CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Circuit-modelled drive: OP07 with feedback-loop diode clipping");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        #[cfg(not(feature = "mono"))]
        ClapFeature::Stereo,
        #[cfg(not(feature = "stereo"))]
        ClapFeature::Mono,
        ClapFeature::Distortion,
    ];
}

impl Vst3Plugin for SparklinDrive {
    const VST3_CLASS_ID: [u8; 16] = VST3_ID;
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(SparklinDrive);
nih_export_vst3!(SparklinDrive);

// ---------------------------------------------------------------- 界面
#[derive(Default)]
struct UiState {
    texture: Option<egui::TextureHandle>,
    sw_up: Option<egui::TextureHandle>,
    sw_down: Option<egui::TextureHandle>,
}

fn draw_panel(
    ui: &mut egui::Ui,
    params: &Arc<SparklinParams>,
    setter: &ParamSetter,
    state: &mut UiState,
    clip: &Arc<AtomicU32>,
) {
    let avail = ui.max_rect();
    // 保持面板宽高比（宿主若给了别的比例，就留边而不是拉伸）
    let scale = (avail.width() / ui::SVG_W).min(avail.height() / ui::SVG_H);
    let rect = egui::Rect::from_center_size(
        avail.center(),
        egui::vec2(ui::SVG_W * scale, ui::SVG_H * scale),
    );
    let to_screen = |p: (f32, f32)| -> egui::Pos2 {
        egui::pos2(rect.left() + p.0 * scale, rect.top() + p.1 * scale)
    };

    // 底图：按窗口像素尺寸生成一次，之后复用
    let want = [
        (rect.width() * ui.ctx().pixels_per_point()) as usize,
        (rect.height() * ui.ctx().pixels_per_point()) as usize,
    ];
    if state.texture.as_ref().map(|t| t.size() != want).unwrap_or(true) {
        if let Some(img) = decode_png(PANEL_PNG, want[0], want[1]) {
            state.texture = Some(ui.ctx().load_texture(
                "sparklin-panel",
                img,
                egui::TextureOptions::LINEAR,
            ));
        }
    }
    if state.sw_up.is_none() {
        let w = (ui::MODE_SW_W * scale * ui.ctx().pixels_per_point()) as usize;
        if let Some(img) = decode_png(SWITCH_UP_PNG, w, 0) {
            state.sw_up = Some(ui.ctx().load_texture("sw-up", img, egui::TextureOptions::LINEAR));
        }
        if let Some(img) = decode_png(SWITCH_DOWN_PNG, w, 0) {
            state.sw_down =
                Some(ui.ctx().load_texture("sw-down", img, egui::TextureOptions::LINEAR));
        }
    }
    let painter = ui.painter().clone();
    // 面板外的留边也涂成米白，视觉上浑然一体
    painter.rect_filled(avail, 0.0, egui::Color32::from_rgb(0xfa, 0xf1, 0xe0));
    if let Some(tex) = &state.texture {
        painter.image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    let white = egui::Color32::from_rgb(0xf5, 0xf5, 0xf5);
    let dark = egui::Color32::from_rgb(0x23, 0x18, 0x15);

    // ---- 削波模式开关：面板底图里已经删掉了原来画死的拨杆 ----
    let sym = params.symmetric.value();
    let sw_tex = if sym { &state.sw_down } else { &state.sw_up };
    let sw_rect = egui::Rect::from_min_size(
        to_screen(ui::MODE_SW_XY),
        egui::vec2(ui::MODE_SW_W * scale, ui::MODE_SW_H * scale),
    );
    // 按下反馈：半径缩到 0.9 倍（以开关中心为原点）
    let sw_pressed = ui
        .ctx()
        .is_pointer_over_area()
        && ui.input(|i| i.pointer.primary_down())
        && sw_rect_contains(&sw_rect, ui);
    let sw_draw = if sw_pressed {
        egui::Rect::from_center_size(sw_rect.center(), sw_rect.size() * 0.9)
    } else {
        sw_rect
    };
    if let Some(t) = sw_tex {
        painter.image(
            t.id(),
            sw_draw,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    // ---- LED（削波指示；bypass 时变灰）----
    let led_c = to_screen(ui::LED);
    let led_r = ui::LED_R * scale;
    let bypassed = params.bypass.value();
    let level = f32::from_bits(clip.load(Ordering::Relaxed)).clamp(0.0, 1.0);
    if bypassed {
        painter.circle_filled(led_c, led_r, egui::Color32::from_rgb(0x8a, 0x86, 0x80));
    } else {
        // 灭 = #a00000，亮 = #ff0000（线性插值）
        let t = level.clamp(0.0, 1.0);
        let base = egui::Color32::from_rgb(
            (0xa0 as f32 + (0xff - 0xa0) as f32 * t) as u8,
            0,
            0,
        );
        painter.circle_filled(led_c, led_r, base);
    }

    // ---- 旋钮指针 ----
    let mut knob = |center_svg: (f32, f32), value: f32| {
        let c = to_screen(center_svg);
        // −135°..+135°，0.5 = 正上方
        let ang = (-135.0 + 270.0 * value).to_radians();
        let dir = egui::vec2(ang.sin(), -ang.cos());
        painter.line_segment(
            [c, c + dir * (ui::POINTER_LEN * scale)],
            egui::Stroke::new((ui::POINTER_W * scale).max(2.0), white),
        );
        c
    };
    let level_c = knob(ui::LEVEL, params.level.value());
    let gain_c = knob(ui::GAIN, params.gain.value());

    // ---- 旋钮交互（拖动 + 滚轮 + 双击复位）----
    let mut do_knob = |ui: &mut egui::Ui, center: egui::Pos2, id: &str, param: &FloatParam| {
        let r = ui::KNOB_R * scale;
        let hit = egui::Rect::from_center_size(center, egui::vec2(r * 2.0, r * 2.0));
        let resp = ui.interact(hit, ui.id().with(id), egui::Sense::click_and_drag());
        let mut v = param.value();
        let before = v;
        if resp.dragged() {
            v -= resp.drag_delta().y / (260.0 * scale);
        }
        if resp.double_clicked() {
            v = 0.5;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                v += scroll / (1000.0 * scale);
            }
        }
        v = v.clamp(0.0, 1.0);
        if (v - before).abs() > f32::EPSILON {
            setter.set_parameter(param, v);
        }
        resp
    };
    do_knob(ui, level_c, "level", &params.level);
    do_knob(ui, gain_c, "gain", &params.gain);

    // ---- 削波模式开关：点击切换（视觉由上面的贴图负责）----
    let sw = ui.interact(sw_rect, ui.id().with("mode"), egui::Sense::click());
    if sw.clicked() {
        setter.set_parameter(&params.symmetric, !params.symmetric.value());
    }

    // ---- 大开关 = bypass ----
    let by_c = to_screen(ui::BYPASS_SW);
    let by_r = ui::BYPASS_SW_R * scale;
    let by = ui.interact(
        egui::Rect::from_center_size(by_c, egui::vec2(by_r * 2.0, by_r * 2.0)),
        ui.id().with("bypass"),
        egui::Sense::click(),
    );
    if by.clicked() {
        setter.set_parameter(&params.bypass, !bypassed);
    }
    // 开关外形不随 bypass 状态变化；只有**按住**时缩到 0.9 倍（松开即恢复）
    if by.is_pointer_button_down_on() {
        if let Some(t) = &state.texture {
            let r = by_r;
            let src = egui::Rect::from_min_max(
                egui::pos2(
                    (ui::BYPASS_SW.0 - r / scale) / ui::SVG_W,
                    (ui::BYPASS_SW.1 - r / scale) / ui::SVG_H,
                ),
                egui::pos2(
                    (ui::BYPASS_SW.0 + r / scale) / ui::SVG_W,
                    (ui::BYPASS_SW.1 + r / scale) / ui::SVG_H,
                ),
            );
            // 先用底色抹掉原位置，再把缩小的开关贴回去
            painter.rect_filled(
                egui::Rect::from_center_size(by_c, egui::vec2(r * 2.0, r * 2.0)),
                0.0,
                egui::Color32::from_rgb(0xfa, 0xf1, 0xe0),
            );
            painter.image(
                t.id(),
                egui::Rect::from_center_size(by_c, egui::vec2(r * 2.0 * 0.9, r * 2.0 * 0.9)),
                src,
                egui::Color32::WHITE,
            );
        }
    }
    let _ = dark;
}

/// 指针是否落在某个矩形内（用于按下反馈）
fn sw_rect_contains(rect: &egui::Rect, ui: &egui::Ui) -> bool {
    ui.ctx()
        .input(|i| i.pointer.hover_pos())
        .map(|p| rect.contains(p))
        .unwrap_or(false)
}

/// 解码内嵌 PNG 并缩放到目标像素尺寸（`h == 0` 时按原比例推算）。
fn decode_png(data: &'static [u8], w: usize, h: usize) -> Option<egui::ColorImage> {
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
    let dw = w.max(1);
    let dh = if h == 0 { (dw * sh / sw).max(1) } else { h.max(1) };
    let mut px = Vec::with_capacity(dw * dh);
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let i = (sy * sw + sx) * channels;
            let (r, g, b) = match channels {
                4 => (buf[i], buf[i + 1], buf[i + 2]),
                3 => (buf[i], buf[i + 1], buf[i + 2]),
                _ => (buf[i], buf[i], buf[i]),
            };
            px.push(egui::Color32::from_rgb(r, g, b));
        }
    }
    Some(egui::ColorImage {
        size: [dw, dh],
        pixels: px,
    })
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! **Sparklin Drive** —— HONEYPIE EFFECTS
//!
//! 电路级建模的失真/法兹（fakedist v1）：单电源 9V、OP07、反馈环内二极管削波。
//!
//! 本 crate 只是**插件外壳**：
//! - 电路在 `sparklin-drive-circuit`（框架的 `Circuit` 实现）
//! - DSP 通用机制在 `rustomp-dsp`（`Engine<C>`）
//! - 界面在 `rustomp-gui`（声明式面板 + 交互），素材与坐标来自 `rustomp.toml`
//!
//! 身份与界面声明由 `build.rs` 从 `plugins/sparklin-drive/rustomp.toml` 生成。

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};
use rustomp_dsp::Engine;
use sparklin_drive_circuit::{Fakedist, Params as DspParams};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

// ---------------------------------------------------------------- 由 build.rs 生成
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
include!(concat!(env!("OUT_DIR"), "/panel.rs"));

// 面板素材（框架不持有素材）
const PANEL_PNG: &[u8] = include_bytes!("../../assets/panel@2x.png");
const SWITCH_UP_PNG: &[u8] = include_bytes!("../../assets/switch_up@4x.png");
const SWITCH_DOWN_PNG: &[u8] = include_bytes!("../../assets/switch_down@4x.png");

static ASSETS: rustomp_gui::Assets = rustomp_gui::Assets {
    panel: PANEL_PNG,
    images: &[
        ("switch_up", SWITCH_UP_PNG),
        ("switch_down", SWITCH_DOWN_PNG),
    ],
};

// ---------------------------------------------------------------- 声道布局
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
            editor_state: EguiState::from_size(META_WIN_W, META_WIN_H),
        }
    }
}

// ---------------------------------------------------------------- 界面与参数的桥
/// 把 `rustomp-gui` 的通用 `ParamAccess` 接到 nih-plug 的具体参数上。
struct GuiAccess<'a> {
    params: &'a SparklinParams,
    setter: &'a ParamSetter<'a>,
    activity: f32,
}

impl rustomp_gui::ParamAccess for GuiAccess<'_> {
    fn get(&self, id: &str) -> Option<f32> {
        match id {
            "gain" => Some(self.params.gain.value()),
            "level" => Some(self.params.level.value()),
            "symmetric" => Some(if self.params.symmetric.value() { 1.0 } else { 0.0 }),
            "bypass" => Some(if self.params.bypass.value() { 1.0 } else { 0.0 }),
            // LED 由音频线程的活动指示驱动
            "clip_indicator" => Some(self.activity),
            _ => None,
        }
    }
    fn set(&self, id: &str, value: f32) {
        let b = value > 0.5;
        match id {
            "gain" => self.setter.set_parameter(&self.params.gain, value),
            "level" => self.setter.set_parameter(&self.params.level, value),
            "symmetric" => self.setter.set_parameter(&self.params.symmetric, b),
            "bypass" => self.setter.set_parameter(&self.params.bypass, b),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- 插件
const MAX_CHANNELS: usize = 2;

pub struct SparklinDrive {
    params: Arc<SparklinParams>,
    engines: [Engine<Fakedist>; MAX_CHANNELS],
    /// 活动指示（0..1），GUI 读；原子存 f32 位，音频线程不加锁
    activity: Arc<AtomicU32>,
}

impl Default for SparklinDrive {
    fn default() -> Self {
        Self {
            params: Arc::new(SparklinParams::default()),
            engines: std::array::from_fn(|_| Engine::<Fakedist>::new(48_000.0)),
            activity: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl Plugin for SparklinDrive {
    const NAME: &'static str = META_NAME;
    const VENDOR: &'static str = META_VENDOR;
    const URL: &'static str = META_URL;
    const EMAIL: &'static str = META_EMAIL;
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
        context.set_latency_samples(Engine::<Fakedist>::latency_samples());
        true
    }

    fn reset(&mut self) {
        for e in self.engines.iter_mut() {
            e.reset();
        }
        self.params.gain.smoothed.reset(self.params.gain.value());
        self.params.level.smoothed.reset(self.params.level.value());
        self.activity.store(0, Ordering::Relaxed);
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
                    peak = peak.max(e.activity_indicator());
                }
            }
        }

        self.activity.store(peak.to_bits(), Ordering::Relaxed);
        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        for e in self.engines.iter_mut() {
            e.reset();
        }
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let activity = self.activity.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            rustomp_gui::UiState::default(),
            |ctx, state| {
                ctx.set_visuals(egui::Visuals::light());
                // 新建 egui Context 后旧纹理句柄全部失效，必须清空重建
                rustomp_gui::reset_textures(state);
            },
            move |ctx, setter, state| {
                let level = f32::from_bits(activity.load(Ordering::Relaxed));
                let access = GuiAccess {
                    params: &params,
                    setter,
                    activity: level,
                };
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        rustomp_gui::draw(ui, &PANEL, &ASSETS, state, &access, level);
                    });
            },
        )
    }
}

impl ClapPlugin for SparklinDrive {
    const CLAP_ID: &'static str = META_CLAP_ID;
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Circuit-modelled drive: OP07 with feedback-loop diode clipping");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(META_URL);
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
    const VST3_CLASS_ID: [u8; 16] = META_VST3_ID;
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(SparklinDrive);
nih_export_vst3!(SparklinDrive);

// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 效果器插件的 nih-plug 外壳。
//!
//! 当前是 **fakedist v1**（`circuits/fakedist`）—— 单电源 9V、OP07、
//! 反馈环内非对称二极管削波的失真/法兹。
//!
//! 信号链：`宿主 f32 → f64 → 4x 过采样 → WDF 电路 → 4x 下采样 → 宿主 f32`
//!
//! * DSP 全部在 `rustomp-dsp`，与 ngspice 基线有对拍测试；
//! * 本文件只负责：参数、采样率、声道实例、延迟上报。
//!
//! 构建链：CLAP/VST3 由 nih-plug 直接导出；AUv2 由 `clap-wrapper` 包装
//! （见 `cmake/CMakeLists.txt` 与 `scripts/build-au.sh`）。

use nih_plug::prelude::*;
use rustomp_dsp::{Engine, Params as DspParams};
use std::sync::Arc;

/// 语音道数（按最大 2 声道分配实例）。
const MAX_CHANNELS: usize = 2;

struct RustompFuzz {
    params: Arc<FuzzParams>,
    engines: [Engine; MAX_CHANNELS],
    sample_rate: f64,
}

#[derive(Params)]
struct FuzzParams {
    /// GAIN 电位器（P1）位置 —— 线性 taper，对应 0..500kΩ
    #[id = "gain"]
    gain: FloatParam,
    /// LEVEL 电位器（P2）位置 —— 线性 taper
    #[id = "level"]
    level: FloatParam,
}

impl Default for FuzzParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            level: FloatParam::new(
                "Level",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
        }
    }
}

impl Default for RustompFuzz {
    fn default() -> Self {
        Self {
            params: Arc::new(FuzzParams::default()),
            engines: [Engine::new(48_000.0), Engine::new(48_000.0)],
            sample_rate: 48_000.0,
        }
    }
}

impl Plugin for RustompFuzz {
    const NAME: &'static str = "Rustomp Fuzz";
    const VENDOR: &'static str = "amonisachicken";
    const URL: &'static str = "https://github.com/amonisachicken/rustomp";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
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

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    // 参数逐样本平滑即可，不需要宿主切块
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
        self.sample_rate = buffer_config.sample_rate as f64;
        for e in self.engines.iter_mut() {
            e.set_sample_rate(self.sample_rate);
        }
        // 过采样滤波器带来的固定延迟，报给宿主做延迟补偿
        context.set_latency_samples(Engine::latency_samples());
        true
    }

    fn reset(&mut self) {
        for e in self.engines.iter_mut() {
            e.reset();
        }
        self.params.gain.smoothed.reset(self.params.gain.value());
        self.params.level.smoothed.reset(self.params.level.value());
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for mut channel_samples in buffer.iter_samples() {
            // 参数每样本平滑，避免自动化时的 zipper noise
            let gain = self.params.gain.smoothed.next();
            let level = self.params.level.smoothed.next();
            let p = DspParams {
                gain: gain as f64,
                level: level as f64,
            };
            for (ch, sample) in channel_samples.iter_mut().enumerate() {
                if let Some(e) = self.engines.get_mut(ch) {
                    e.set_params(p);
                    *sample = e.process(*sample as f64) as f32;
                }
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        for e in self.engines.iter_mut() {
            e.reset();
        }
    }
}

impl ClapPlugin for RustompFuzz {
    const CLAP_ID: &'static str = "com.amonisachicken.rustomp.fuzz";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("电路级建模的失真/法兹（OP07 + 非对称二极管削波）");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Distortion,
    ];
}

impl Vst3Plugin for RustompFuzz {
    // 16 字节唯一 ID，一旦发布**不可更改**
    const VST3_CLASS_ID: [u8; 16] = *b"RustompFuzzV1M0a";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(RustompFuzz);
nih_export_vst3!(RustompFuzz);

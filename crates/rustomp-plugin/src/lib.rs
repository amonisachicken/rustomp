// SPDX-License-Identifier: GPL-3.0-or-later

//! rustomp 插件外壳 —— **M0 骨架（空插件）**
//!
//! 当前阶段的目标**不是**做 DSP，而是把整条构建链打通：
//!
//! ```text
//! cargo build (x86_64 + aarch64) ──► lipo ──► universal dylib
//!        │
//!        ├─► .clap   (nih-plug 直接导出)
//!        └─► .vst3   (nih-plug 直接导出)
//!                │
//!                └─ clap-wrapper (CMake/ObjC++) ─► .component (AUv2)  ──► Logic
//! ```
//!
//! 本插件的音频处理只有一路输出增益（默认 0 dB = 直通），因此既是"空插件"，
//! 又能在 DAW 里用一个可听/可测的参数证明参数链路与音频链路都是通的。
//!
//! 注意：**不要**在这里写 DSP**。** M0-b 才会把 `rustomp-dsp` 的 WDF 削波级接进来。

use nih_plug::prelude::*;
use std::sync::Arc;

/// 骨架插件。
struct RustompSkeleton {
    params: Arc<SkeletonParams>,
}

#[derive(Params)]
struct SkeletonParams {
    /// 输出增益。存线性值，显示为 dB（与 nih-plug `gain` 示例一致）。
    #[id = "output"]
    output_gain: FloatParam,
}

impl Default for SkeletonParams {
    fn default() -> Self {
        Self {
            output_gain: FloatParam::new(
                "Output",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-24.0),
                    max: util::db_to_gain(24.0),
                    factor: FloatRange::gain_skew_factor(-24.0, 24.0),
                },
            )
            // 20 ms 对数平滑，避免自动化时的 zipper noise
            .with_smoother(SmoothingStyle::Logarithmic(20.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Default for RustompSkeleton {
    fn default() -> Self {
        Self {
            params: Arc::new(SkeletonParams::default()),
        }
    }
}

impl Plugin for RustompSkeleton {
    const NAME: &'static str = "Rustomp Skeleton";
    const VENDOR: &'static str = "amonisachicken";
    const URL: &'static str = "https://github.com/amonisachicken/rustomp";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // 第一个布局是默认布局；Logic 里可选 Stereo / Mono
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
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn reset(&mut self) {
        // 重置平滑器，避免初始化时从 0 渐变
        self.params.output_gain.smoothed.reset(util::db_to_gain(0.0));
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // M0-a：纯直通 + 输出增益。真正的 WDF 处理在 M0-b 接入。
        for channel_samples in buffer.iter_samples() {
            let gain = self.params.output_gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {}
}

impl ClapPlugin for RustompSkeleton {
    const CLAP_ID: &'static str = "com.amonisachicken.rustomp.skeleton";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("rustomp 构建链骨架（直通）");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for RustompSkeleton {
    // 16 字节唯一 ID，一旦发布**不可更改**（否则宿主无法识别同一插件）
    const VST3_CLASS_ID: [u8; 16] = *b"RustompSkeletonM";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(RustompSkeleton);
nih_export_vst3!(RustompSkeleton);

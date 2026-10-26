// SPDX-License-Identifier: GPL-3.0-or-later
//!
//! **面板声明的编译期/校验期示例**：这段 `PANEL` 由
//! `tools/meta/plugin_meta.py --panel-rust` 从 `rustomp.toml` 的 `[ui]` 生成，
//! 这里放一份样例用于 ① 编译检查字面量写法 ② 跑 `validate_panel`。
//!
//! 运行：`cargo run -p rustomp-gui --release --example panel_check`

use rustomp_gui::{Assets, PanelSpec};

pub const PANEL: rustomp_gui::PanelSpec = rustomp_gui::PanelSpec {
    svg_w: 104.0,
    svg_h: 54.0,
    window: (312, 162),
    background: (250, 241, 224),
    knobs: &[rustomp_gui::KnobSpec {
        param: "gain",
        center: (52.0, 40.0),
        radius: 9.5,
        image: "knob",
        image_size: (24.0, 24.0),
        render: rustomp_gui::KnobRender::Rotate,
        pointer_len: 0.0,
        pointer_width: 1.0,
        angle_min: -135.0,
        angle_max: 135.0,
        drag_range: 200.0,
    }],
    leds: &[rustomp_gui::LedSpec {
        param: "clip_indicator",
        center: (52.0, 8.0),
        radius: 2.5,
        off: (160, 0, 0),
        on: (255, 0, 0),
    }],
    toggles: &[rustomp_gui::ToggleSpec {
        param: "symmetric",
        image_up: "switch_up",
        image_down: "switch_down",
        x: 10.0,
        y_up: 14.0,
        y_down: 14.0,
        width: 16.0,
        height: 12.0,
        pivot: (18.0, 20.0),
        press_scale: 0.9,
    }],
    buttons: &[],
};

static IMAGES: [(&str, &[u8]); 3] = [("knob", &[]), ("switch_up", &[]), ("switch_down", &[])];
static ASSETS: Assets = Assets {
    panel: &[],
    images: &IMAGES,
};

fn main() {
    let errs = rustomp_gui::validate_panel(&PANEL, &ASSETS);
    if errs.is_empty() {
        println!(
            "✓ 面板声明通过校验：{} 个旋钮 / {} 个 LED / {} 个拨杆 / {} 个按钮（画布 {}×{} mm）",
            PANEL.knobs.len(),
            PANEL.leds.len(),
            PANEL.toggles.len(),
            PANEL.buttons.len(),
            PANEL.svg_w,
            PANEL.svg_h
        );
        println!("  固定绘制顺序：{}", rustomp_gui::DRAW_ORDER);
    } else {
        for e in &errs {
            println!("✗ {e}");
        }
        std::process::exit(1);
    }
}

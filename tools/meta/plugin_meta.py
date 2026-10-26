#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""
读取插件项目描述文件（rustomp.toml），输出给 shell / Rust 构建使用。

用法：
  python3 tools/meta/plugin_meta.py plugins/sparklin-drive/rustomp.toml --shell
  python3 tools/meta/plugin_meta.py plugins/sparklin-drive/rustomp.toml --json
  python3 tools/meta/plugin_meta.py plugins/sparklin-drive/rustomp.toml --get plugin.name
  python3 tools/meta/plugin_meta.py plugins/sparklin-drive/rustomp.toml --rust

`--shell` 输出形如 `PLUGIN_NAME='Sparklin Drive'` 的赋值，供 `eval`。
`--rust` 输出 `const` 定义，供插件 crate 的 build.rs 生成 identity.rs。

只用标准库（tomllib，Python ≥3.11），不引入额外依赖。
"""
from __future__ import annotations

import json
import shlex
import sys
from pathlib import Path

try:
    import tomllib  # Python ≥3.11
except ModuleNotFoundError:  # pragma: no cover
    sys.exit("plugin_meta.py 需要 Python ≥3.11（tomllib）。请检查 `python3 --version`。")


def load(path: str) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


def _asset(manifest_path, rel):
    """素材路径 → 绝对路径；未声明时返回空串（构建脚本自己判断）。"""
    if not rel:
        return ""
    return str(Path(manifest_path).resolve().parent / rel)


def get(data: dict, dotted: str):
    cur = data
    for part in dotted.split("."):
        cur = cur[part]
    return cur


def rust_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> None:
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)
    path, mode = sys.argv[1], sys.argv[2]
    data = load(path)
    variant = "dual"
    if "--variant" in sys.argv:
        variant = sys.argv[sys.argv.index("--variant") + 1]

    if mode == "--get":
        print(get(data, sys.argv[3]))
        return

    if mode == "--json":
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return

    # 无 GUI 阶段可以没有 [assets] / [ui]；缺失时给空字典，避免构建脚本直接崩
    p, au, b = data["plugin"], data["au"], data["build"]
    a = data.get("assets", {})
    ui = data.get("ui", {})

    if mode == "--shell":
        pairs = {
            # 目录
            "PLUGIN_DIR": str(Path(path).resolve().parent),
            "PLUGIN_PACKAGE": p["package"],
            # 身份
            "PLUGIN_NAME": p["name"],
            "PLUGIN_VENDOR": p["vendor"],
            "PLUGIN_URL": p.get("url", ""),
            "PLUGIN_CLAP_ID": p["clap_id"],
            "PLUGIN_VST3_ID": p["vst3_class_id"],
            "AU_MANUFACTURER": au["manufacturer_code"],
            "AU_MANUFACTURER_NAME": au["manufacturer_name"],
            "AU_BUNDLE_ID": au["bundle_id"],
            "AU_SUBTYPE_MONO": au["subtype_mono"],
            "AU_SUBTYPE_STEREO": au["subtype_stereo"],
            "AU_SUBTYPE_DUAL": au["subtype_dual"],
            "BUILD_VARIANTS": " ".join(b["variants"]),
            "BUILD_DUAL": "1" if b.get("dual_variant") else "0",
            # 素材（可能没有：无 GUI 阶段的插件不声明 [assets]）
            # 现在 [assets] 是自由映射（旋钮贴图等也放这里）→ 全部导出为 ASSET_<UPPER>
            "ASSET_PANEL": _asset(path, a.get("panel")),
            "ASSET_SWITCH_UP": _asset(path, a.get("switch_up")),
            "ASSET_SWITCH_DOWN": _asset(path, a.get("switch_down")),
            # 界面（同上，缺省 0）
            "UI_SVG_WIDTH": ui.get("svg_width", 0),
            "UI_SVG_HEIGHT": ui.get("svg_height", 0),
            "UI_WINDOW_WIDTH": ui.get("window_width", 0),
            "UI_WINDOW_HEIGHT": ui.get("window_height", 0),
            "UI_BACKGROUND": ui.get("background", "#000000"),
        }
        # 自由映射的额外素材：ASSET_<UPPER_KEY>
        for key, rel in a.items():
            k = "ASSET_" + key.upper()
            if k not in pairs:
                pairs[k] = _asset(path, rel)
        for k, v in pairs.items():
            print(f"{k}={shlex.quote(str(v))}")
        return

    if mode == "--rust":
        # 由插件的 build.rs 调用，写入 OUT_DIR/identity.rs。
        # 变体（mono/stereo/dual）会改变显示名、CLAP id、VST3 id。
        suffix = {"mono": " Mono", "stereo": " Stereo", "dual": ""}[variant]
        id_suffix = {"mono": ".mono", "stereo": ".stereo", "dual": ""}[variant]
        vst3 = p.get(f"vst3_class_id_{variant}", p["vst3_class_id"])
        if len(vst3) != 16:
            sys.exit(f"VST3 class id 必须 16 字节：{vst3!r}")
        print(f"pub const META_NAME: &str = {rust_str(p['name'] + suffix)};")
        print(f"pub const META_VENDOR: &str = {rust_str(p['vendor'])};")
        print(f"pub const META_URL: &str = {rust_str(p.get('url', ''))};")
        print(f"pub const META_EMAIL: &str = {rust_str(p.get('email', ''))};")
        print(f"pub const META_CLAP_ID: &str = {rust_str(p['clap_id'] + id_suffix)};")
        print(f"pub const META_VST3_ID: [u8; 16] = *b{rust_str(vst3)};")
        print(f"pub const META_WIN_W: u32 = {ui['window_width']};")
        print(f"pub const META_WIN_H: u32 = {ui['window_height']};")
        print(f"pub const META_SVG_W: f32 = {ui['svg_width']};")
        print(f"pub const META_SVG_H: f32 = {ui['svg_height']};")
        return

    if mode == "--panel-rust":
        # 生成 rustomp_gui::PanelSpec 的 Rust 字面量（供插件 build.rs 写入 panel.rs）。
        #
        # 与 rustomp-gui 的约定（rustomp-box-designer/docs/01-contract-panel-toml.md）：
        # - LED / 拨杆 / 按钮都是**数组**（数量不限）；
        # - 旋钮默认 `render = "rotate"`（整图旋转，素材指针朝上 ⇔ 参数 0.5，角度范围必须对称）；
        # - 绘制顺序固定：panel → leds → knobs → toggles → buttons。
        def rgb(hexstr: str):
            h = hexstr.lstrip("#")
            return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))

        out = []
        out.append("pub const PANEL: rustomp_gui::PanelSpec = rustomp_gui::PanelSpec {")
        out.append(f"    svg_w: {ui['svg_width']},")
        out.append(f"    svg_h: {ui['svg_height']},")
        out.append(f"    window: ({ui['window_width']}, {ui['window_height']}),")
        bg = rgb(ui["background"])
        out.append(f"    background: ({bg[0]}, {bg[1]}, {bg[2]}),")

        # ---- 旋钮 ----
        out.append("    knobs: &[")
        for k in ui.get("knobs", []):
            render = k.get("render", "rotate")
            travel = k.get("travel_deg")
            if travel is not None:
                # 设计器只暴露"总转角" → 导出对称范围
                angle_min, angle_max = -float(travel) / 2.0, float(travel) / 2.0
            else:
                angle_min = k.get("angle_min", -135.0)
                angle_max = k.get("angle_max", 135.0)
            size = k.get("image_size", [0.0, 0.0])
            out.append("        rustomp_gui::KnobSpec {")
            out.append(f'            param: {rust_str(k["param"])},')
            out.append(f'            center: ({k["center"][0]}, {k["center"][1]}),')
            out.append(f'            radius: {k["radius"]},')
            out.append(f'            image: {rust_str(k.get("asset", ""))},')
            out.append(f"            image_size: ({size[0]}, {size[1]}),")
            out.append(
                "            render: rustomp_gui::KnobRender::"
                + ("Rotate" if render == "rotate" else "Pointer")
                + ","
            )
            out.append(f'            pointer_len: {k.get("pointer_len", 0.0)},')
            out.append(f'            pointer_width: {k.get("pointer_width", 1.0)},')
            out.append(f"            angle_min: {angle_min},")
            out.append(f"            angle_max: {angle_max},")
            out.append(f'            drag_range: {k.get("drag_full_range_px", 260.0)},')
            out.append("        },")
        out.append("    ],")

        # ---- 指示灯（数组）----
        out.append("    leds: &[")
        for led in ui.get("leds", []):
            off, on = rgb(led["off_color"]), rgb(led["on_color"])
            out.append("        rustomp_gui::LedSpec {")
            out.append(f'            param: {rust_str(led["param"])},')
            out.append(f'            center: ({led["center"][0]}, {led["center"][1]}),')
            out.append(f'            radius: {led["radius"]},')
            out.append(f"            off: ({off[0]}, {off[1]}, {off[2]}),")
            out.append(f"            on: ({on[0]}, {on[1]}, {on[2]}),")
            out.append("        },")
        out.append("    ],")

        # ---- 拨杆（数组）----
        out.append("    toggles: &[")
        for tg in ui.get("toggles", []):
            out.append("        rustomp_gui::ToggleSpec {")
            out.append(f'            param: {rust_str(tg["param"])},')
            out.append(f'            image_up: {rust_str(tg["image_up"])},')
            out.append(f'            image_down: {rust_str(tg["image_down"])},')
            out.append(f'            x: {tg["x"]},')
            out.append(f'            y_up: {tg["y_up"]},')
            out.append(f'            y_down: {tg["y_down"]},')
            out.append(f'            width: {tg["width"]},')
            out.append(f'            height: {tg["height"]},')
            out.append(f'            pivot: ({tg["pivot"][0]}, {tg["pivot"][1]}),')
            out.append(f'            press_scale: {tg.get("press_scale", 0.9)},')
            out.append("        },")
        out.append("    ],")

        # ---- 按钮（数组）----
        out.append("    buttons: &[")
        for bt in ui.get("buttons", []):
            out.append("        rustomp_gui::ButtonSpec {")
            out.append(f'            param: {rust_str(bt["param"])},')
            out.append(f'            center: ({bt["center"][0]}, {bt["center"][1]}),')
            out.append(f'            radius: {bt["radius"]},')
            out.append(f'            press_scale: {bt.get("press_scale", 0.9)},')
            out.append("        },")
        out.append("    ],")

        out.append("};")
        print("\n".join(out))
        return

    print(f"未知模式：{mode}")
    sys.exit(1)


if __name__ == "__main__":
    main()

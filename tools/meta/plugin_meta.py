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
import tomllib
from pathlib import Path


def load(path: str) -> dict:
    with open(path, "rb") as f:
        return tomllib.load(f)


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

    p, au, b, a, ui = data["plugin"], data["au"], data["build"], data["assets"], data["ui"]

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
            # 素材
            "ASSET_PANEL": str(Path(path).resolve().parent / a["panel"]),
            "ASSET_SWITCH_UP": str(Path(path).resolve().parent / a["switch_up"]),
            "ASSET_SWITCH_DOWN": str(Path(path).resolve().parent / a["switch_down"]),
            # 界面
            "UI_SVG_WIDTH": ui["svg_width"],
            "UI_SVG_HEIGHT": ui["svg_height"],
            "UI_WINDOW_WIDTH": ui["window_width"],
            "UI_WINDOW_HEIGHT": ui["window_height"],
            "UI_BACKGROUND": ui["background"],
        }
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

    print(f"未知模式：{mode}")
    sys.exit(1)


if __name__ == "__main__":
    main()

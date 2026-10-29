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
`--get a.b.c` 取单个键；键不存在时输出空行、退出码 0（契约 A 的可选字段
—— 例如 `requires.packer` 没写就表示"不约束"）。

[au] / [build] 可以缺省：[au] 缺失时 AU_* 全部为空（Windows 插件不需要 AU），
[build] 缺失时按 `variants = ["mono", "stereo"]`、`dual_variant = false` 处理。
`[requires]` 会导出为 REQUIRES_FRAMEWORK / PACKER / PACKER_TAG / DESIGNER
（未声明则为空字串），供打包脚本与插件的 build.sh 使用。

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
        if len(sys.argv) < 4:
            sys.exit("用法: plugin_meta.py <rustomp.toml> --get <dotted.key>")
        try:
            print(get(data, sys.argv[3]))
        except KeyError:
            pass  # 未声明的可选项：输出空行，调用方用 `[ -n ... ]` 判断
        return

    if mode == "--json":
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return

    # 无 GUI 阶段可以没有 [assets] / [ui]；缺失时给空字典，避免构建脚本直接崩
    # [au] / [build] 也可以缺省：Windows 打包不需要 AU，[build] 全部有默认值
    p = data["plugin"]
    au = data.get("au", {})
    b = data.get("build", {})
    r = data.get("requires", {})
    a = data.get("assets", {})
    ui = data.get("ui", {})
    variants = b.get("variants") or ["mono", "stereo"]

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
            "AU_MANUFACTURER": au.get("manufacturer_code", ""),
            "AU_MANUFACTURER_NAME": au.get("manufacturer_name", ""),
            "AU_BUNDLE_ID": au.get("bundle_id", ""),
            "AU_SUBTYPE_MONO": au.get("subtype_mono", ""),
            "AU_SUBTYPE_STEREO": au.get("subtype_stereo", ""),
            "AU_SUBTYPE_DUAL": au.get("subtype_dual", ""),
            "BUILD_VARIANTS": " ".join(variants),
            "BUILD_DUAL": "1" if b.get("dual_variant") else "0",
            # 跨仓版本约束（契约 A；packer 自己也会再校验一次）
            "REQUIRES_FRAMEWORK": r.get("framework", ""),
            "REQUIRES_PACKER": r.get("packer", ""),
            "REQUIRES_PACKER_TAG": r.get("packer_tag", ""),
            "REQUIRES_DESIGNER": r.get("designer", ""),
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
        # 已迁移到 `rustomp-gen`（`cargo run -p rustomp-gen -- <项目目录>` 会写
        # `plugin/src/generated/panel.rs`）。这里保留一个明确报错，避免旧脚本静默产出第二份。
        sys.exit("--panel-rust 已迁移到 rustomp-gen：cargo run -q -p rustomp-gen -- <项目目录>")


if __name__ == "__main__":
    main()

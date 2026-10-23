#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""
把 LTspice 风格的器件模型（`.model` 语句 + `.asy` 符号块）清洗成 ngspice 可用形式，
并生成一份"我们这边可用的中间形式"。

做三件事：
  1. 按**每种器件类型的合法参数白名单**过滤 —— LTspice/厂商专用字段（MFG/Type/Vceo/Icrating...）
     ngspice 不认识，留着会报错；
  2. 标记 ngspice **不支持**的类型（LTspice 的 `VDMOS(...)`），给出转换建议；
  3. 产出：devices.lib（给 ngspice/oracle）、devices.json（给 Rust 侧生成常量）、REPORT.md（人读）。

用法：
  python3 tools/meta/ltspice_models.py devices/models.add -o devices
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

# ---------------------------------------------------------------- 参数白名单
# 参考 ngspice 手册；不在此列的参数一律丢弃并记录
ALLOWED = {
    "D": {  # 二极管
        "is", "n", "rs", "cjo", "vj", "m", "tt", "bv", "ibv", "eg", "xti",
        "fc", "tnom", "kf", "af", "area", "t_measured", "xtb", "pt",
    },
    "BJT": {
        "is", "bf", "nf", "vaf", "ikf", "ise", "ne", "br", "nr", "var", "ikr",
        "isc", "nc", "rb", "irb", "rbm", "re", "rc", "cje", "vje", "mje",
        "tf", "xtf", "vtf", "itf", "ptf", "cjc", "vjc", "mjc", "xcjc", "tr",
        "cjs", "vjs", "mjs", "xtb", "eg", "xti", "fc", "tnom", "kf", "af",
        "area", "t_measured", "pt",
    },
    "JFET": {
        "vto", "beta", "lambda", "rd", "rs", "is", "n", "cgd", "cgs", "pb",
        "fc", "xti", "kf", "af", "m", "betatce", "vtotc", "tnom", "area",
    },
    "MOS": {  # ngspice 的 MOS level 1/3 等
        "level", "vto", "kp", "gamma", "phi", "lambda", "rd", "rs", "cbd",
        "cbs", "is", "pb", "cgso", "cgdo", "cgbd", "tox", "nsub", "nss",
        "tpg", "ld", "wd", "u0", "fc", "mj", "cj", "cjsw", "mjsw", "js",
        "tnom", "kf", "af",
    },
}

# ngspice 完全不支持的类型（LTspice 专有）
UNSUPPORTED_TYPE = {"vdmos"}

# 器件类别（对应 docs/component-library.md 的 DeviceClass）
def classify(mtype: str, name: str) -> tuple[str, str]:
    """返回 (DeviceClass, 说明)。"""
    t = mtype.lower()
    if t in ("npn", "pnp"):
        germanium = name.upper() in ("AC127", "AC128", "AC126", "AC125", "OC71", "2N404")
        kind = "Ge" if germanium else "Si"
        return (f"{kind}{'Npn' if t == 'npn' else 'Pnp'}",
                f"{kind} BJT（{'锗' if germanium else '硅'}）")
    if t == "d":
        if name.upper() in ("1N34A", "OA90", "OA91", "1N60"):
            return "GeDiode", "锗二极管"
        if name.upper().startswith("1N58"):
            return "Schottky", "肖特基二极管"
        return "SiDiode", "硅二极管"
    if t in ("njf", "pjf"):
        return ("NJfet" if t == "njf" else "PJfet"), "JFET"
    if t in ("vdmos", "nmos"):
        return "NMos", "N 沟道 MOSFET"
    if t == "pmos":
        return "PMos", "P 沟道 MOSFET"
    return "Other", mtype


# 参考：docs/component-library.md §3
LEVEL_HINT = {
    "GeNpn": "D3 + C1（模型自带 CJC/CJE/TF/TR，可按需升 C2）",
    "GePnp": "D3 + C1（同上）",
    "GeDiode": "D2 + C0（模型含 CJO，可升 C1）",
    "SiDiode": "D2 + C1（含 CJO/TT）",
    "Schottky": "D2 + C1",
    "NMos": "D2 + C1（需先解决 VDMOS 兼容）",
    "NJfet": "D3 + C0",
    "SiNpn": "D3 + C0",
    "SiPnp": "D3 + C0",
}


def split_params(body: str) -> list[tuple[str, str]]:
    """`IS=5u ISC=5u ...` → [(is, 5u), ...]。值里允许括号与空格。"""
    out = []
    for m in re.finditer(r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([^=]+?)(?=\s+[A-Za-z_][A-Za-z0-9_]*\s*=|$)", body):
        key = m.group(1).strip().lower()
        val = m.group(2).strip().rstrip(",").strip()
        out.append((key, val))
    return out


def parse_models(text: str):
    """提取 `.model Name Type(params)`，支持 `+` 续行。返回 [(name, type, params, raw)]。"""
    lines = text.splitlines()
    joined = []
    for line in lines:
        if line.lstrip().startswith("+") and joined:
            joined[-1] += " " + line.lstrip()[1:].strip()
        else:
            joined.append(line)

    models = []
    for line in joined:
        s = line.strip()
        if not s.lower().startswith(".model"):
            continue
        # .model NAME TYPE(...)
        m = re.match(r"\.model\s+(\S+)\s+(\w+)\s*\((.*)\)\s*$", s, re.IGNORECASE)
        if not m:
            continue
        models.append((m.group(1), m.group(2), split_params(m.group(3)), s))
    return models


def parse_symbols(text: str):
    """提取 `###NAME###` ... `###NAME END###` 的 LTspice 符号块。"""
    out = []
    for m in re.finditer(r"###\s*(\S+)\s*###(.*?)###\s*\1\s+END\s*###", text, re.S):
        name, body = m.group(1), m.group(2)
        pins = re.findall(r"PINATTR\s+PinName\s+(\S+)", body)
        spicemodel = re.search(r"SYMATTR\s+SpiceModel\s+(\S+)", body)
        out.append({
            "name": name,
            "pins": pins,
            "spice_model_ref": spicemodel.group(1) if spicemodel else None,
        })
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("source", type=Path)
    ap.add_argument("-o", "--out", type=Path, default=Path("devices"))
    ap.add_argument("--skip", default="",
                    help="逗号分隔的型号名：这些型号由手写/覆盖版本提供，这里不输出")
    ap.add_argument("--vdmos-as-nmos1", action="store_true",
                    help="把 VDMOS 近似转换成 ngspice 的 MOS level 1（默认只报告不转换）")
    args = ap.parse_args()

    skip = {x.strip().upper() for x in args.skip.split(",") if x.strip()}
    text = args.source.read_text(encoding="utf-8", errors="replace")
    models = parse_models(text)
    symbols = parse_symbols(text)

    args.out.mkdir(parents=True, exist_ok=True)
    lib_lines = [
        "* 由 tools/meta/ltspice_models.py 从 %s 生成 —— 请勿手改" % args.source.name,
        "* 已按 ngspice 合法参数白名单过滤掉 LTspice/厂商专用字段",
    ]
    report = []
    json_out = {"source": args.source.name, "models": [], "symbols_only": []}

    report.append("# 器件模型清洗报告\n")
    report.append(f"来源：`{args.source.name}`\n")
    report.append(f"解析到 **{len(models)}** 个 `.MODEL`、**{len(symbols)}** 个 LTspice 符号块。\n")
    report.append("\n## 分立器件\n")
    report.append("| 型号 | 类型 | 类别 | 保留参数 | 丢弃参数 | 建议档位 | ngspice |")
    report.append("|---|---|---|---|---|---|---|")

    for name, mtype, params, _raw in models:
        if name.upper() in skip:
            report.append(f"| `{name}` | {mtype} | — | —（由 handwritten.lib 覆盖）| — | — | ⏭️ 跳过 |")
            continue
        t = mtype.lower()
        kind = ("BJT" if t in ("npn", "pnp") else
                "D" if t == "d" else
                "JFET" if t in ("njf", "pjf") else
                "MOS" if t in ("nmos", "pmos", "vdmos") else t)
        cls, cls_desc = classify(mtype, name)

        if t in UNSUPPORTED_TYPE:
            # LTspice 专有的 VDMOS：ngspice 无法解析
            kept, dropped = [], [f"{k}={v}" for k, v in params]
            if args.vdmos_as_nmos1:
                conv = {k: v for k, v in params if k in ("vto", "kp", "rd", "rs", "is", "cgs", "cgd")}
                conv["level"] = "1"
                kept = [f"{k}={v}" for k, v in conv.items()]
                lib_lines.append(f"* 注意：{name} 由 VDMOS 近似为 MOS level 1（仅供对拍参考）")
                lib_lines.append(f".model {name} NMOS({ ' '.join(f'{k}={v}' for k, v in conv.items()) })")
            status = "❌ 不支持 VDMOS" + ("（已近似转换）" if args.vdmos_as_nmos1 else "")
        else:
            allowed = ALLOWED.get(kind, set())
            kept = [f"{k}={v}" for k, v in params if k in allowed]
            dropped = [f"{k}={v}" for k, v in params if k not in allowed]
            lib_lines.append(f".model {name} {mtype.upper()}({' '.join(kept)})")
            status = "✅"

        report.append("| `{}` | {} | {} | {} | {} | {} | {} |".format(
            name, mtype, cls_desc,
            ", ".join(k.split("=")[0] for k in kept) or "—",
            ", ".join(k.split("=")[0] for k in dropped) or "—",
            LEVEL_HINT.get(cls, "待定"), status))

        json_out["models"].append({
            "name": name, "spice_type": mtype, "class": cls,
            "params": {k: v for k, v in (kv.split("=", 1) for kv in kept)} if kept else {},
            "dropped": [kv.split("=", 1)[0] for kv in dropped],
            "level_hint": LEVEL_HINT.get(cls, "待定"),
            "ngspice_ok": t not in UNSUPPORTED_TYPE or args.vdmos_as_nmos1,
        })

    # 只有符号、没有模型的（运放）
    if symbols:
        report.append("\n## 只有符号、没有模型的器件\n")
        report.append("| 名称 | 引脚 | 引用的外部模型 | 状态 |")
        report.append("|---|---|---|---|")
        for s in symbols:
            report.append("| `{}` | {} | `{}` | **缺模型本体** |".format(
                s["name"], "/".join(s["pins"]) or "—", s["spice_model_ref"] or "—"))
            json_out["symbols_only"].append(s)

    lib_lines.append("")
    (args.out / "devices.lib").write_text("\n".join(lib_lines), encoding="utf-8")
    (args.out / "devices.json").write_text(
        json.dumps(json_out, ensure_ascii=False, indent=2), encoding="utf-8")
    (args.out / "REPORT.md").write_text("\n".join(report) + "\n", encoding="utf-8")

    print(f"已生成 {args.out}/devices.lib、devices.json、REPORT.md")
    print(f"  .MODEL {len(models)} 个；符号块 {len(symbols)} 个")
    for m in json_out["models"]:
        flag = "ok " if m["ngspice_ok"] else "NG!"
        print(f"  [{flag}] {m['name']:10} {m['spice_type']:6} {m['class']:8} 丢弃 {len(m['dropped'])} 项")
    for s in symbols:
        print(f"  [符号] {s['name']:10} → 引用外部模型 {s['spice_model_ref']}")


if __name__ == "__main__":
    main()

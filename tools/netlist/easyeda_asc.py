#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""
把 EasyEDA Pro 导出的 PADS-PowerPCB ASCII（*.asc, PCB 版）解析成
一份规范化的「电路描述」，输出：
  <out>/netlist.json   机器可读（元件 + 每个引脚的网 + 值）
  <out>/NETLIST.md     人可读（BOM + 网表 + 器件连接摘要）

注意：这是 **PCB** 导出，引脚编号 = 封装焊盘号，不是符号引脚名。
TO-92 的 1/2/3 与 C/B/E 的对应关系必须人工确认（见 NETLIST.md 的警告）。

用法：
  python3 tools/netlist/easyeda_asc.py circuits/v1-fuzz/source.asc -o circuits/v1-fuzz
"""
from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path

# 每类器件需要知道哪些属性是「值」
VALUE_ATTRS = ("Value", "Name", "Comment")


def parse_sections(text: str) -> dict[str, list[str]]:
    """按 *XXX* 头切段。"""
    sections: dict[str, list[str]] = defaultdict(list)
    cur = "HEADER"
    for line in text.splitlines():
        if line.startswith("*"):
            m = re.match(r"\*([A-Za-z]+)\*", line)
            if m:
                cur = m.group(1).upper()
                sections[cur].append(line)
                continue
        sections[cur].append(line)
    return sections


def parse_parts(lines: list[str]) -> dict[str, dict]:
    """*PART* 段：`REFDES  DEVICE@FOOTPRINT`"""
    parts: dict[str, dict] = {}
    for line in lines:
        if line.startswith("*"):
            continue
        toks = line.split()
        if len(toks) < 2:
            continue
        ref = toks[0]
        dev_fp = toks[1]
        dev, _, fp = dev_fp.partition("@")
        parts[ref] = {"refdes": ref, "device": dev, "footprint": fp}
    return parts


def parse_nets(text: str) -> dict[str, list[tuple[str, str]]]:
    """*NET* 段：里面每条 `*SIGNAL* 网名` 后面跟若干行 `REF.PIN REF.PIN ...`

    注意：`*SIGNAL*` 是 *NET* 段内部的子标题，不能当成独立段落切开，
    所以要按 `*NET*` .. `*MISC*` 区间取原文。
    """
    start = text.find("*NET*")
    end = text.find("*MISC*")
    if start < 0:
        return {}
    region = text[start : end if end > 0 else len(text)]

    nets: dict[str, list[tuple[str, str]]] = {}
    cur: str | None = None
    for line in region.splitlines():
        if line.startswith("*SIGNAL*"):
            cur = line.split("*SIGNAL*", 1)[1].strip()
            nets.setdefault(cur, [])
            continue
        if line.startswith("*"):
            cur = None
            continue
        if cur is None:
            continue
        for tok in line.split():
            if "." in tok:
                ref, _, pin = tok.partition(".")
                nets[cur].append((ref, pin))
    return {k: v for k, v in nets.items() if v}


def parse_attributes(text: str) -> dict[str, dict[str, str]]:
    """ATTRIBUTE VALUES { PART REF { "key" value ... } }"""
    if "ATTRIBUTE VALUES" not in text:
        return {}
    body = text.split("ATTRIBUTE VALUES", 1)[1]
    out: dict[str, dict[str, str]] = {}
    for ref, block in re.findall(r"PART\s+(\S+)\s*\{(.*?)\n\}", body, re.S):
        attrs = {}
        for k, v in re.findall(r'"([^"]+)"\s+(.*)', block):
            v = v.strip()
            if len(v) >= 2 and v.startswith('"') and v.endswith('"'):
                v = v[1:-1]
            attrs[k.strip()] = v
        out[ref] = attrs
    return out


PLACEHOLDERS = {"", "yes", "no", "Description", "Reuse Block", "Add into BOM"}


def pick_value(attrs: dict[str, str], device: str) -> str:
    """属性里的值优先；占位符回退到器件库名。"""
    for k in VALUE_ATTRS:
        v = attrs.get(k, "").strip()
        if v and v not in PLACEHOLDERS:
            return v
    return device


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("asc", type=Path)
    ap.add_argument("-o", "--out", type=Path, required=True)
    args = ap.parse_args()

    text = args.asc.read_text(encoding="utf-8", errors="replace")
    sec = parse_sections(text)
    parts = parse_parts(sec.get("PART", []))
    nets = parse_nets(text)
    attrs = parse_attributes(text)

    # 把属性合进元件
    for ref, p in parts.items():
        a = attrs.get(ref, {})
        p["value"] = pick_value(a, p["device"])
        p["device_lib_value"] = p["device"]      # *PART* 里的 Device 名也常被当值
        p["attrs"] = {k: v for k, v in a.items()
                      if k in ("Name", "Value", "Comment", "Manufacturer Part", "Supplier Part")}

    # 引脚 -> 网
    pin_net: dict[tuple[str, str], str] = {}
    for net, pins in nets.items():
        for ref, pin in pins:
            pin_net[(ref, pin)] = net

    for ref, p in parts.items():
        pins = sorted({pin for (r, pin) in pin_net if r == ref})
        p["pins"] = {pin: pin_net[(ref, pin)] for pin in pins}

    # 一致性检查：值冲突
    warnings = []
    for ref, p in parts.items():
        name = p["attrs"].get("Name", "")
        dev = p["device"]
        generic = dev.startswith(("Res_", "HDR", "SW-", "LED_TH", "CAP-D"))
        if name and dev and not generic and name.replace(" ", "") != dev.replace(" ", ""):
            warnings.append(f"{ref}: *PART* 里 Device=`{dev}`，但属性 Name=`{name}` —— 以哪个为准？")
    # 每个网至少 2 个引脚
    for net, pins in nets.items():
        if len(pins) < 2:
            warnings.append(f"网 `{net}` 只有 1 个引脚（可能是测试点/连接器）：{pins}")

    data = {
        "source": args.asc.name,
        "format": "EasyEDA Pro / PADS-PowerPCB ASCII (PCB export)",
        "components": parts,
        "nets": {k: [f"{r}.{p}" for r, p in v] for k, v in nets.items()},
        "warnings": warnings,
    }
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "netlist.json").write_text(
        json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")

    # ---------------- Markdown ----------------
    L = []
    L.append(f"# 网表：{args.asc.name}\n")
    L.append("> 由 `tools/netlist/easyeda_asc.py` 自动生成，**不要手改**（改源文件后重新生成）。\n")
    L.append(f"- 格式：{data['format']}")
    L.append(f"- 元件数：{len(parts)}　网数：{len(nets)}\n")

    L.append("## ⚠️ 必须人工确认的点\n")
    L.append("1. **引脚编号是 PCB 焊盘号**，不是符号引脚名。TO-92 的 `1/2/3` 到底对应 C/B/E 需要确认")
    L.append("   （本网表的拓扑看起来是 `1=C, 2=B, 3=E`，与 BC547 / BC559 的常规符号一致，但要核过才算数）。")
    L.append("2. 下面是自动发现的值冲突：\n")
    if warnings:
        for w in warnings:
            L.append(f"   - {w}")
    else:
        L.append("   - （无）")
    L.append("")

    L.append("## BOM / 元件表\n")
    L.append("| RefDes | 值/型号 | 器件库 | 封装 | 引脚数 |")
    L.append("|---|---|---|---|---|")
    for ref in sorted(parts, key=lambda r: (re.sub(r"\d", "", r), len(r), r)):
        p = parts[ref]
        val = p["value"] or "**(未标注)**"
        L.append(f"| {ref} | {val} | {p['device']} | {p['footprint']} | {len(p['pins'])} |")
    L.append("")

    L.append("## 网表（节点 → 引脚）\n")
    L.append("| 网名 | 连接的引脚 |")
    L.append("|---|---|")
    for net in sorted(nets):
        L.append(f"| `{net}` | {' , '.join(f'`{r}.{p}`' for r, p in nets[net])} |")
    L.append("")

    L.append("## 器件视角（元件 → 每个引脚接到哪）\n")
    L.append("| RefDes | 连接 |")
    L.append("|---|---|")
    for ref in sorted(parts, key=lambda r: (re.sub(r"\d", "", r), len(r), r)):
        p = parts[ref]
        conns = " , ".join(f"{pin}→`{net}`" for pin, net in p["pins"].items())
        L.append(f"| {ref} | {conns} |")
    L.append("")
    (args.out / "NETLIST.md").write_text("\n".join(L), encoding="utf-8")

    print(f"已生成：{args.out/'netlist.json'}")
    print(f"已生成：{args.out/'NETLIST.md'}")
    print(f"元件 {len(parts)} 个，网 {len(nets)} 个，警告 {len(warnings)} 条")


if __name__ == "__main__":
    main()

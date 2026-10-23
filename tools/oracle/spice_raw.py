#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""
解析 ngspice 的 **ASCII rawfile**（`ngspice -b -r out.raw` + 工作目录 `.spiceinit`
里 `set filetype=ascii`），返回 {变量名: [浮点数组]}。

这是 oracle 的基础设施：M1 的对拍要拿这些波形去和 WDF 输出逐样本/频谱比对。

ASCII rawfile 格式（要点）：
```
Title: ...
Date: ...
Plotname: Transient Analysis
Flags: real
No. Variables: 4
No. Points: 20008
Variables:
    0  time      time
    1  v(out)    voltage
Values:
0   0.000000000000000e+00
    1   0.000000000000000e+00
    ...
1   1.000000000000000e-06
    ...
```
即：**每个数据点以「序号 + 第 0 个变量值」开头，后续变量行以 tab+序号 开头**。

用法：
  python3 tools/oracle/spice_raw.py out.raw            # 打印每个变量的统计
  python3 tools/oracle/spice_raw.py out.raw v(out)     # 只打印某个变量
"""
from __future__ import annotations

import math
import sys
from pathlib import Path


def parse_rawfile(path: str | Path) -> dict[str, list[float]]:
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()

    nvar = npts = 0
    vars_start = None
    vals_start = None
    for i, line in enumerate(lines):
        if line.startswith("No. Variables:"):
            nvar = int(line.split(":")[1])
        elif line.startswith("No. Points:"):
            npts = int(line.split(":")[1])
        elif line.startswith("Variables:"):
            vars_start = i + 1
        elif line.startswith("Values:"):
            vals_start = i + 1
            break
    if not (nvar and vals_start and vars_start):
        raise ValueError(f"不是可解析的 ASCII rawfile：{path}")

    names: list[str] = []
    for line in lines[vars_start : vals_start - 1][:nvar]:
        tok = line.split()
        if len(tok) >= 2:
            names.append(tok[1])

    out: dict[str, list[float]] = {n: [] for n in names}
    # ASCII rawfile 的坑：**只有每个数据点的第一行带序号**，其余变量只有值：
    #     0<TAB><TAB>2.000022737995739e-02
    #     <TAB><TAB>8.999994904969231e+00
    # 所以靠「行首是否是空白」来区分「新点」与「后续变量」；且点序号从 0 开始递增
    # （不是每个点都写 0），因此只能在遇到新点行时 flush。
    cur: list[float] = []

    def flush() -> None:
        if len(cur) == len(names):
            for n, v in zip(names, cur):
                out[n].append(v)
        cur.clear()

    for line in lines[vals_start:]:
        if not line.strip():
            continue
        if line[0] not in " \t":
            # 行首有内容 = 新数据点的第一行：`<点序号><TAB><TAB><var0 的值>`
            tok = line.split()
            try:
                float(tok[0])          # 点序号（0,1,2,...），仅用于校验
                val = float(tok[1])
            except (ValueError, IndexError):
                continue
            flush()
            cur.append(val)
        else:
            tok = line.split()
            if tok:
                try:
                    cur.append(float(tok[0]))
                except ValueError:
                    pass
    flush()

    # 点数可能因截断而少，按实际长度对齐
    return {n: v for n, v in out.items() if v}


def stats(xs: list[float]) -> dict[str, float]:
    n = len(xs)
    mx = max(xs)
    mn = min(xs)
    mean = sum(xs) / n
    rms = math.sqrt(sum(v * v for v in xs) / n)
    return {"n": n, "min": mn, "max": mx, "pp": mx - mn, "mean": mean, "rms": rms}


def thd(xs: list[float], fs: float, f0: float, nharm: int = 9) -> float:
    """用单点 DFT 估 THD（纯 Python，够用；正式基准用 ngspice 的 .four）。"""
    n = len(xs)
    def mag(f: float) -> float:
        re = im = 0.0
        for k, v in enumerate(xs):
            ang = 2 * math.pi * f * k / fs
            re += v * math.cos(ang)
            im -= v * math.sin(ang)
        return 2 * math.hypot(re, im) / n
    fund = mag(f0)
    if fund == 0:
        return float("nan")
    sq = sum(mag(f0 * h) ** 2 for h in range(2, nharm + 1))
    return math.sqrt(sq) / fund * 100.0


def main() -> None:
    if len(sys.argv) < 2:
        print(__doc__)
        return
    data = parse_rawfile(sys.argv[1])
    only = sys.argv[2] if len(sys.argv) > 2 else None
    print(f"变量 {len(data)} 个，点数 {len(next(iter(data.values())))}")
    for name, xs in data.items():
        if only and name != only:
            continue
        s = stats(xs)
        print(f"  {name:16s} min={s['min']:+.6g} max={s['max']:+.6g} "
              f"pp={s['pp']:.6g} mean={s['mean']:+.6g} rms={s['rms']:.6g}")


if __name__ == "__main__":
    main()

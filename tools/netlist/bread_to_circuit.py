#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""
把 breadspice 的工程文件（*.bread, format=breadspice-bread）转成
breadspice 后端消费的 `Circuit` JSON（domain.rs 的契约）。

为什么需要这一步：`.bread` 里只保存了「元件 + 引脚落在哪个插孔（node id）」，
而 `Circuit` 需要完整的 `breadboard`（nodes + nets）。面包板几何/连通规则由
前端 `frontend/src/layout/breadboardLayout.ts` 定义，这里按同样规则复刻：

- 端子排：30 行 × 左 5 列(a..e) / 右 5 列(f..j)
  - 左组横向连通 → net `t{row}L`
  - 右组横向连通 → net `t{row}R`
- 电源轨：4 条纵向 net（`rail_Lp` / `rail_Lm` / `rail_Rp` / `rail_Rm`），各 25 孔

用法：
  python3 tools/netlist/bread_to_circuit.py fakedist.bread -o build/fakedist.circuit.json
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

ROWS = 30
RAIL_HOLES = 25
LEFT_LETTERS = "abcde"
RIGHT_LETTERS = "fghij"
RAILS = ("rail_Lp", "rail_Lm", "rail_Rp", "rail_Rm")


def build_breadboard() -> dict:
    nodes: list[dict] = []
    nets: dict[str, list[str]] = {}

    def add(node_id: str, net_id: str, row: str, column: int) -> None:
        # x/y 只影响渲染，网表生成用不到；统一填 0，避免伪造几何数据。
        nodes.append({"id": node_id, "netId": net_id, "row": row, "column": column, "x": 0.0, "y": 0.0})
        nets.setdefault(net_id, []).append(node_id)

    for r in range(1, ROWS + 1):
        for col, letter in enumerate(LEFT_LETTERS):
            add(f"t{r}{letter}", f"t{r}L", letter, r)
        for col, letter in enumerate(RIGHT_LETTERS):
            add(f"t{r}{letter}", f"t{r}R", letter, r)

    for rail in RAILS:
        row = "+" if rail.endswith("p") else "-"
        for i in range(1, RAIL_HOLES + 1):
            add(f"{rail}_{i}", rail, row, i)

    return {
        "id": "mb102-rotated",
        "name": "面包板（SVG 资产）",
        "nodes": nodes,
        "nets": [{"id": k, "nodeIds": v} for k, v in nets.items()],
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("bread", type=Path)
    ap.add_argument("-o", "--out", type=Path, required=True)
    args = ap.parse_args()

    project = json.loads(args.bread.read_text(encoding="utf-8"))
    if project.get("format") != "breadspice-bread":
        raise SystemExit(f"不是 breadspice 工程文件：format={project.get('format')!r}")

    components = []
    for item in project["items"]:
        inst = dict(item["instance"])
        # 未连接的引脚显式置 null，与前端一致
        for pin in inst.get("pins", []):
            pin.setdefault("node", None)
        components.append(inst)

    circuit = {"breadboard": build_breadboard(), "components": components}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(circuit, ensure_ascii=False), encoding="utf-8")

    kinds = {}
    for c in components:
        kinds[c["kind"]] = kinds.get(c["kind"], 0) + 1
    print(f"已生成 {args.out}")
    print(f"元件 {len(components)} 个：" + ", ".join(f"{k}×{v}" for k, v in sorted(kinds.items())))
    print(f"仿真选项：{json.dumps(project.get('sim', {}), ensure_ascii=False)}")


if __name__ == "__main__":
    main()

#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 从 breadspice 面包板工程（*.bread）生成 ngspice 网表。
#
#   ./scripts/make-netlist.sh circuits/fakedist
#
# 产物（都在目标目录下）：
#   circuit.json   breadspice 后端的 Circuit 契约
#   netlist.cir    ngspice 网表（已注入 .MODEL / .SUBCKT）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DIR="${1:-plugins/sparklin-drive/circuit/netlist}"
BREAD=$(find "$DIR" -maxdepth 1 -name "*.bread" | head -1)
[ -n "$BREAD" ] || { echo "在 $DIR 里找不到 *.bread"; exit 1; }

echo "==> [1/3] $BREAD → $DIR/circuit.json"
python3 tools/netlist/bread_to_circuit.py "$BREAD" -o "$DIR/circuit.json"

echo "==> [2/3] 构建 nlgen（依赖 breadspice 后端）"
cargo build --release -q -p nlgen

echo "==> [3/3] circuit.json → $DIR/netlist.cir"
./target/release/nlgen "$DIR/circuit.json" > "$DIR/netlist.cir"

echo "完成：$DIR/netlist.cir（$(grep -cE '^[RVCDX]' "$DIR/netlist.cir") 个器件行）"

#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 从 breadspice 面包板工程（*.bread）生成 ngspice 网表。
#
#   ./scripts/make-netlist.sh plugins/my-drive/circuit/netlist
#
# 产物（都在目标目录下）：
#   circuit.json   breadspice 后端的 Circuit 契约
#   netlist.cir    ngspice 网表（已注入 .MODEL / .SUBCKT）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DIR="${1:?用法: $0 <插件项目>/circuit/netlist}"
BREAD=$(find "$DIR" -maxdepth 1 -name "*.bread" | head -1)
[ -n "$BREAD" ] || { echo "在 $DIR 里找不到 *.bread"; exit 1; }

echo "==> [1/3] $BREAD → 中间表示 → SPICE 元件行"
cargo build --release -q -p rustomp-import --bin rustomp-import
./target/release/rustomp-import "$BREAD" --spice "$DIR/_elements.cir"

echo "==> [2/3] 拼装网表（元件行 + models.lib + .end）"
[ -f "$DIR/models.lib" ] || { echo "缺少 $DIR/models.lib"; exit 1; }
{ cat "$DIR/_elements.cir"; echo; cat "$DIR/models.lib"; echo ".end"; } > "$DIR/netlist.cir"
rm -f "$DIR/_elements.cir"

echo "==> [3/3] 完成：$DIR/netlist.cir（顶层元件 $(grep -cE '^[RVCDX]' "$DIR/netlist.cir") 行，含模型库展开）"

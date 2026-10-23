#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 对一个 ngspice 网表跑基线仿真（M1 oracle 的起点）：
#   op    直流工作点
#   ac    频响（峰值增益 / -3dB 带宽 / 几个关键频点）
#   tran  瞬态 + THD（大信号、小信号各一次）
#   stats 波形统计（min/max/pp/rms）
#
#   ./scripts/sim-baseline.sh plugins/my-drive/circuit/netlist
#
# 输出：<dir>/baseline.txt（同时打印到终端）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DIR="${1:?用法: $0 <插件项目>/circuit/netlist}"
NET="$DIR/netlist.cir"
[ -f "$NET" ] || { echo "找不到 $NET"; exit 1; }

# 瞬态参数：这个电路的 C3(1µF) × 电位器(~500k) ≈ **0.5 s** 的直流恢复时间常数，
# 非对称削波会让 C3 充上直流（"blocking distortion"），必须跑足够久才能测到稳态。
# 早期版本只跑到 40ms，测出来的输出均值是 -0.089V（还在充电），是错的。
STEP="${STEP:-5u}"      # 5µs ≈ 每周期 200 点，配合 .four 的 Gridsize=200
STOP="${STOP:-3}"       # 总仿真时长
SAVE="${SAVE:-0.02}"    # 只保存最后 20ms（否则 rawfile 会到 GB 级）
TAIL=$(python3 -c "print(f'{$STOP-$SAVE:g}')")

# 用框架里的 Rust 工具解析 rawfile（替代已删除的 tools/oracle/spice_raw.py）
cargo build -q --release -p rustomp-oracle --bin spice-raw

WORK="build/sim/$(basename "$DIR")"
mkdir -p "$WORK"
printf 'set filetype=ascii\n' > "$WORK/.spiceinit"

# ngspice 输出里节点名一律小写，这里统一转小写再比对
OUT_NODE=$(grep -oE "^\* probe .*V\([A-Za-z0-9_]+\)" "$NET" | grep -oE "n_[A-Za-z0-9_]+" | head -1 | tr '[:upper:]' '[:lower:]' || true)
[ -n "$OUT_NODE" ] || { echo "网表里找不到 '* probe ... V(node)' 探针，无法确定输出节点"; exit 1; }
VSRC=$(grep -oE "^VS[0-9]+" "$NET" | head -1)
[ -n "$VSRC" ] || { echo "网表里找不到 VS* 正弦源"; exit 1; }

# 去掉 .end，后面拼分析语句
strip_end() { python3 -c "import sys;print(open(sys.argv[1]).read().rsplit('.end',1)[0],end='')" "$NET"; }

# 生成某个输入幅度下的网表主体（改 SIN 的幅度与 AC 幅度）
with_amp() {
    python3 - "$NET" "$1" <<'PY'
import re, sys
net, amp = sys.argv[1], sys.argv[2]
body = open(net).read().rsplit(".end", 1)[0]
body = re.sub(r"SIN\((\S+)\s+\S+", lambda m: f"SIN({m.group(1)} {amp}", body, count=1)
body = re.sub(r"\bAC\s+\S+", f"AC {amp}", body, count=1)
print(body, end="")
PY
}

{
  echo "# 基线仿真 · $(basename "$DIR")"
  echo "# 网表: $NET"
  echo "# 正弦源: $VSRC   输出节点: $OUT_NODE"
  echo "# 时间: $(date '+%Y-%m-%d %H:%M:%S')"
  echo

  # ---------------- OP ----------------
  { strip_end; echo ".op"; echo ".print op v($OUT_NODE) v(n_t19r) v(n_rail_rp)"; echo ".end"; } > "$WORK/op.cir"
  echo "## 直流工作点"
  ( cd "$WORK" && ngspice -b op.cir 2>&1 | sed -n '/Node *Voltage/,/^$/p' | sed 's/^/  /' )
  echo

  # ---------------- AC ----------------
  { strip_end; echo ".ac dec 30 10 20000"; echo ".print ac vm($OUT_NODE) vm(n_t21l)"; echo ".end"; } > "$WORK/ac.cir"
  ( cd "$WORK" && ngspice -b ac.cir 2>&1 | sed -n '/Index/,/^$/p' | tail -n +3 \
      | awk 'NF>=4 && $4+0>0 {printf "%s %s\n", $2, 20*log($3/$4)/log(10)}' > ac.txt )
  echo "## 频响（相对输入源，dB）"
  python3 - "$WORK/ac.txt" <<'PY'
import sys
rows = []
for line in open(sys.argv[1]):
    tok = line.split()
    if len(tok) == 2:
        try:
            rows.append((float(tok[0]), float(tok[1])))
        except ValueError:
            pass
if not rows:
    print("  (无数据)")
else:
    pf, pd = max(rows, key=lambda r: r[1])
    print(f"  峰值增益 : {pd:+.2f} dB @ {pf:.0f} Hz")
    band = [f for f, g in rows if g >= pd - 3.0]
    if band:
        print(f"-3dB 带宽 : {min(band):.0f} Hz ~ {max(band):.0f} Hz")
    for f0 in (50, 100, 500, 1000, 2000, 5000, 10000):
        near = min(rows, key=lambda r: abs(r[0] - f0))
        print(f"  {f0:>6} Hz : {near[1]:+7.2f} dB")
PY
  echo

  # ---------------- TRAN / THD ----------------
  echo "## 瞬态 + THD（1kHz 正弦）"
  for AMP in 0.2 0.001; do
    { with_amp "$AMP"; echo ".tran $STEP $STOP $TAIL"; echo ".four 1k v($OUT_NODE)"; echo ".end"; } \
        > "$WORK/tran_$AMP.cir"
    THD=$( cd "$WORK" && ngspice -b "tran_$AMP.cir" 2>&1 | grep -oE "THD: [0-9.]+ %" | head -1 )
    echo "  峰值 ${AMP} V → ${THD:-未知}"
  done
  echo

  # ---------------- 波形统计 ----------------
  { strip_end; echo ".tran $STEP $STOP $TAIL"; echo ".end"; } > "$WORK/tran.cir"
  ( cd "$WORK" && ngspice -b -r tran.raw tran.cir >/dev/null 2>&1 )
  echo "## 波形统计（稳态：$TAIL s 之后 ${SAVE} s）"
  python3 tools/oracle/spice_raw.py "$WORK/tran.raw" 2>/dev/null \
      | grep -E "v\($OUT_NODE\)|v\(n_t21l\)|v\(n_t1l\)|v\(n_t5l\)|v\(n_t10l\)" | sed 's/^/  /'
} | tee "$DIR/baseline.txt"

echo
echo "已写入 $DIR/baseline.txt"

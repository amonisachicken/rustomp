#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 由 template/ 生成一个与框架同构的下游插件项目。
#
#   ./scripts/new-plugin.sh "My Drive"
#   ./scripts/new-plugin.sh "My Drive" --vendor "SOME VENDOR"
#
# 生成到 plugins/<slug>/，并把两个 crate 注册进 workspace。
# 生成后仍需人工做的事会打印在最后。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NAME="${1:?用法: $0 \"插件显示名\" [--vendor \"厂商名\"]}"
shift || true
VENDOR="HONEYPIE EFFECTS"
while [ $# -gt 0 ]; do
    case "$1" in
        --vendor) VENDOR="${2:?--vendor 需要参数}"; shift 2 ;;
        *) echo "未知参数：$1"; exit 1 ;;
    esac
done

# 显示名 → 包名（小写、空格转连字符、去掉非字母数字）
SLUG="$(python3 -c "
import re,sys
s=sys.argv[1].strip().lower()
s=re.sub(r'[^a-z0-9]+','-',s).strip('-')
print(s or 'plugin')
" "$NAME")"
SLUG_U="${SLUG//-/_}"
SLUG_DOT="${SLUG//-/_}"

GUEST_ROOT="${RUSTOMP_GUEST_ROOT:-$(dirname "$ROOT")}"
DEST="${RUSTOMP_GUEST:-$GUEST_ROOT/$SLUG}"
[ -e "$DEST" ] && { echo "已存在：$DEST"; exit 1; }

# AU / VST3 码：由名字派生，之后可手改（发布后不可改）
CODES="$(python3 -c "
import sys,re
name=sys.argv[1]
letters=[c for c in name if c.isascii() and c.isalnum()]
stem=''.join(letters)
stem=(stem[:2] or 'Xx') if len(stem)>=2 else 'Xx'
au=[stem[:1].upper()+stem[1:].lower()+'%02d'%i for i in (1,2,3)]
vst=[ (stem[:13].ljust(13,'X'))+s for s in ('V01','M01','S01') ]
print(' '.join(au)); print(' '.join(vst))
" "$NAME")"
AU_DUAL=$(echo "$CODES" | sed -n 1p | awk '{print $1}')
AU_MONO=$(echo "$CODES" | sed -n 1p | awk '{print $2}')
AU_STEREO=$(echo "$CODES" | sed -n 1p | awk '{print $3}')
VST3_DUAL=$(echo "$CODES" | sed -n 2p | awk '{print $1}')
VST3_MONO=$(echo "$CODES" | sed -n 2p | awk '{print $2}')
VST3_STEREO=$(echo "$CODES" | sed -n 2p | awk '{print $3}')

# 插件在仓外 → 模板里的 rustomp 路径要按实际相对位置算
REL_RUSTOMP="$(python3 -c "import os,sys; print(os.path.relpath(sys.argv[1], sys.argv[2]))" "$ROOT" "$DEST")"
echo "==> 生成 ${DEST}（包名 ${SLUG}，AU $AU_DUAL/$AU_MONO/${AU_STEREO}）"
echo "    rustomp 相对路径：${REL_RUSTOMP}"
mkdir -p "$(dirname "$DEST")"
cp -R template "$DEST"
rm -rf "$DEST/circuit/netlist" "$DEST/plugin/target" 2>/dev/null || true
mkdir -p "$DEST/circuit/netlist"

# 替换占位符
python3 - "$DEST" "$NAME" "$SLUG" "$SLUG_U" "$SLUG_DOT" "$VENDOR" \
         "$AU_DUAL" "$AU_MONO" "$AU_STEREO" \
         "$VST3_DUAL" "$VST3_MONO" "$VST3_STEREO" "$REL_RUSTOMP" <<'PY'
import pathlib, sys
dest, name, slug, slug_u, slug_dot, vendor, au_d, au_m, au_s, v_d, v_m, v_s, rel = sys.argv[1:14]
subs = {
    "__RUSTOM_PATH__": rel,
    "__NAME__": name,
    "__SLUG_UNDERSCORE__": slug_u,
    "__SLUG_DOT__": slug_dot,
    "__SLUG__": slug,
    "__VENDOR__": vendor,
    "__AU_DUAL__": au_d, "__AU_MONO__": au_m, "__AU_STEREO__": au_s,
    "__VST3_DUAL__": v_d, "__VST3_MONO__": v_m, "__VST3_STEREO__": v_s,
}
for p in pathlib.Path(dest).rglob("*"):
    if not p.is_file() or p.suffix not in (".rs", ".toml", ".md", ".svg"):
        continue
    s = p.read_text(encoding="utf-8", errors="replace")
    o = s
    for k, v in subs.items():
        s = s.replace(k, v)
    if s != o:
        p.write_text(s, encoding="utf-8")
PY

# 厂商名替换（模板里写死了 HONEYPIE EFFECTS 的地方）
if [ "$VENDOR" != "HONEYPIE EFFECTS" ]; then
    python3 - "$DEST" "$VENDOR" <<'PY'
import pathlib, sys
dest, vendor = sys.argv[1], sys.argv[2]
for p in pathlib.Path(dest).rglob("*"):
    if p.is_file() and p.suffix in (".toml", ".rs"):
        s = p.read_text(encoding="utf-8")
        p.write_text(s.replace("HONEYPIE EFFECTS", vendor), encoding="utf-8")
PY
fi

# 生成插件侧代码（身份 / 参数表 / Plugin 实现）
echo "==> 生成插件代码（rustomp-gen）"
if ! cargo run -q -p rustomp-gen -- "$DEST" 2>&1 | sed 's/^/    /'; then
    echo "!! rustomp-gen 失败；修好 rustomp.toml 后重新运行：" >&2
    echo "   cargo run -q -p rustomp-gen -- $DEST" >&2
    exit 1
fi

# 插件项目自带 workspace（见 $DEST/Cargo.toml），**不**注册进 rustomp 的 workspace
echo "  插件自带 workspace：$DEST/Cargo.toml"

cat <<EOT

==> 完成：$DEST

已生成（${DEST}/plugin/src/generated/，入库）：
    identity.rs  params.rs  plugin_impl.rs  mod.rs

还需人工做：
  1. 编辑 ${DEST}/rustomp.toml：显示名、AU 四字符码（现为 ${AU_DUAL}/${AU_MONO}/${AU_STEREO}）、
     CLAP id、VST3 id；[[params]] 改成你的参数表。
     改完重新生成：cargo run -q -p rustomp-gen -- ${DEST}
  2. 把电路写进 ${DEST}/circuit/src/（现在是一个纯增益示例 Gain）。
  3. 电路写好后：
       ./scripts/make-netlist.sh ${DEST}/circuit/netlist      # 生成 SPICE 网表

构建（插件在仓外，用 RUSTOMP_GUEST 指路）：
       RUSTOMP_GUEST=${DEST} ./scripts/build-all.sh
       ./scripts/sim-baseline.sh ${DEST}/circuit/netlist      # ngspice 基线
       (cd ${DEST} && cargo test --release)                   # 对拍（插件自带 workspace）
EOT

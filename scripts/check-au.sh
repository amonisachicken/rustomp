#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 一条命令自检已安装的 AU：安装时间、通道能力、单/立体声渲染、Logic 缓存记录。
#
#   ./scripts/check-au.sh            # 默认检查 Mono 变体
#   ./scripts/check-au.sh stereo     # 检查 Stereo 变体
#   ./scripts/check-au.sh dual       # 检查双布局变体
#
# 用途：区分「插件本身不对」和「Logic 缓存没刷新」。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

source "$ROOT/scripts/plugin-env.sh"
case "${1:-mono}" in
    mono)   NAME="${PLUGIN_NAME} Mono";   SUBTYPE="${AU_SUBTYPE_MONO}";   CHANS="1" ;;
    stereo) NAME="${PLUGIN_NAME} Stereo"; SUBTYPE="${AU_SUBTYPE_STEREO}"; CHANS="2" ;;
    dual)   NAME="${PLUGIN_NAME}";        SUBTYPE="${AU_SUBTYPE_DUAL}";   CHANS="1 2" ;;
    *) echo "用法: $0 [mono|stereo|dual]"; exit 2 ;;
esac
MANU="${AU_MANUFACTURER}"
COMP="$HOME/Library/Audio/Plug-Ins/Components/$NAME.component"

if [ ! -d "$COMP" ]; then
    echo "没找到已安装的 AU：$COMP"
    exit 1
fi

echo "== 安装 ==="
echo "  路径: $COMP"
echo "  时间: $(date -r "$COMP" '+%Y-%m-%d %H:%M:%S')"
printf "  签名: "; codesign -dv "$COMP" 2>&1 | grep -E "^Signature" | sed 's/Signature=//' || echo "?"

echo
echo "== 通道能力 / 渲染（变体 ${1:-mono}，测 ${CHANS} 声道）=="
[ -x build/au_smoke ] || cc -O2 -o build/au_smoke tools/au-smoke/au_smoke.c \
    -framework AudioToolbox -framework CoreFoundation
for ch in $CHANS; do
    echo "  --- ${ch} 声道 ---"
    ./build/au_smoke "$COMP/Contents/MacOS/$NAME" wrapAsAUV2_inst0Factory \
        aufx "$SUBTYPE" "$MANU" 512 "$ch" 2>&1 \
        | grep -E "SupportedNumChannels|in=|渲染|音频通路|FAIL" | sed 's/^/  /'
done

echo
echo "== Logic 的 AU 缓存条目 =="
PLIST="$HOME/Library/Preferences/com.apple.logic10.plist"
KEY="aufx-$SUBTYPE-$MANU"
if plutil -convert xml1 -o /tmp/_rustomp_logic.plist "$PLIST" 2>/dev/null \
   && grep -q "<key>$KEY</key>" /tmp/_rustomp_logic.plist; then
    python3 - "$KEY" <<'PY'
import re, sys
key = sys.argv[1]
s = open('/tmp/_rustomp_logic.plist', encoding='utf-8', errors='replace').read()
i = s.find(f'<key>{key}</key>')
j = s.find('\n\t<key>', i + 10)
e = s[i:j]
midi = re.search(r'MIDIProtocol</key>\s*<integer>(\d+)</integer>', e)
chans = re.findall(r'<array>\s*<integer>(\d+)</integer>\s*<integer>(\d+)</integer>\s*</array>', e)
print(f"  条目存在: {key}")
print(f"  MIDIProtocol: {midi.group(1) if midi else '?'}   (0=纯效果器，1=会被当 MIDI 插件)")
print(f"  ChannelConfigurations: {['%s/%s' % c for c in chans] or '(无)'}")
print("  → 若这里没有 1/1，说明 Logic 还没重扫；退出 Logic 后执行：")
print(f'     plutil -remove "{key}" {sys.argv[0] and "~/Library/Preferences/com.apple.logic10.plist"}')
print("     killall cfprefsd")
PY
else
    echo "  没有缓存条目（Logic 还没扫过，或已清除）"
fi
rm -f /tmp/_rustomp_logic.plist

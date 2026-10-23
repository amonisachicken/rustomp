#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# 清掉 Logic 对某个 AU 的能力缓存（必须**先退出 Logic**，否则退出时会把内存里的旧值写回）。
set -euo pipefail
KEYS=("aufx-SPm1-HpEf" "aufx-SPs1-HpEf" "aufx-SPd1-HpEf" \
      "aufx-RFz1-Amch" "aufx-RFm1-Amch" "aufx-RFs1-Amch" "aufx-Rstp-Amch")
PLIST="$HOME/Library/Preferences/com.apple.logic10.plist"

if pgrep -qx "Logic Pro" 2>/dev/null; then
    echo "⚠️  Logic 正在运行。请先 ⌘Q 完全退出，再运行本脚本。"
    exit 1
fi
for k in "${KEYS[@]}"; do
    if plutil -remove "$k" "$PLIST" 2>/dev/null; then
        echo "  已删除 $k"
    else
        echo "  （无 $k）"
    fi
done
killall cfprefsd 2>/dev/null || true
echo "完成。现在打开 Logic，它会重新扫描全部变体。"

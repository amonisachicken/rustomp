#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 拉取构建 AUv2 所需的第三方源码（不入库，见 .gitignore 的 /third_party/）。
# 全部使用 **固定 commit / tag**，保证构建可复现。
#
# 用法：
#   source ~/vpn.env      # 若直连 github 受限
#   ./scripts/fetch-third-party.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TP="$ROOT/third_party"
mkdir -p "$TP"

CLAP_WRAPPER_REV="806bba0a5f792e3966b99c1561ab9952d3316d7c"
CLAP_TAG="1.2.9"                        # CLAP SDK（MIT）
AUSDK_TAG="AudioUnitSDK-1.1.0"          # Apple AudioUnitSDK（Apache-2.0）

clone_pinned() {
    local url="$1" dir="$2" ref="$3"
    if [ -d "$TP/$dir/.git" ]; then
        echo "[skip] $dir 已存在"
        return
    fi
    echo "[clone] $dir @ $ref"
    git clone --depth 1 --branch "$ref" "$url" "$TP/$dir" 2>/dev/null \
        || { git clone "$url" "$TP/$dir" && git -C "$TP/$dir" checkout "$ref"; }
}

clone_pinned https://github.com/free-audio/clap-wrapper.git clap-wrapper "$CLAP_WRAPPER_REV"
clone_pinned https://github.com/free-audio/clap.git         clap         "$CLAP_TAG"
clone_pinned https://github.com/apple/AudioUnitSDK.git      AudioUnitSDK "$AUSDK_TAG"

echo
echo "第三方源码就绪："
for d in clap-wrapper clap AudioUnitSDK; do
    printf "  %-14s %s\n" "$d" "$(git -C "$TP/$d" rev-parse --short HEAD 2>/dev/null || echo '?')"
done

# 便捷链接：clap-wrapper 也支持在自身目录下找 AudioUnitSDK
ln -sfn "$TP/AudioUnitSDK" "$TP/clap-wrapper/AudioUnitSDK" 2>/dev/null || true
echo "完成。"

#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# **标准构建入口**：出「单声道」和「立体声」两个独立插件（Waves 式）。
#
#   ./scripts/build-all.sh                # Mono + Stereo（默认，推荐）
#   ./scripts/build-all.sh --with-dual    # 额外再出一个同时支持两种布局的版本
#
# 为什么要分开发：见 DEVLOG ADR-008 / E14。
# 宿主（Logic）在插入菜单里按通道条声道数过滤 AU；一个"同时支持 mono+stereo"
# 的插件能不能被正确识别，取决于宿主对多布局的探测是否可靠 —— 实测不可靠。
# 拆成两个单一布局的独立身份最稳，而且各自有独立 AU 码与缓存条目。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WITH_DUAL=0
for a in "$@"; do
    case "$a" in
        --with-dual) WITH_DUAL=1 ;;
        *) echo "未知参数：$a"; exit 1 ;;
    esac
done

echo "############ 1/2  Rustomp Fuzz Mono ############"
./scripts/build-variant.sh mono

echo
echo "############ 2/2  Rustomp Fuzz Stereo ############"
./scripts/build-variant.sh stereo

if [ "$WITH_DUAL" = "1" ]; then
    echo
    echo "############ 附加：双布局版本（过渡用）############"
    ./scripts/build-au.sh --skip-auval
fi

echo
echo "==================== 安装结果 ===================="
for d in "$HOME"/Library/Audio/Plug-Ins/Components/*.component; do
    f="$d/Contents/Info.plist"
    [ -f "$f" ] || continue
    printf "  %-34s %s\n" "$(basename "$d" .component)" \
        "$(plutil -extract AudioComponents.0.subtype raw "$f" 2>/dev/null)/Amch"
done
cat <<'EOF'

用法：
  单声道音轨 → Rustomp Fuzz Mono
  立体声音轨 → Rustomp Fuzz Stereo
若 Logic 里仍不出现，先完全退出 Logic 再执行：
  ./scripts/reset-logic-cache.sh
EOF

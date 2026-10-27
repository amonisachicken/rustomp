#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 从插件的 rustomp.toml 载入身份/素材/界面配置。
# 用法（必须 source）：
#   PLUGIN_DIR=<插件目录> source scripts/plugin-env.sh
#
# 查找顺序：
#   1) 显式 $PLUGIN_DIR
#   2) $RUSTOMP_GUEST（插件项目的默认位置，推荐放**仓外**，例如 ../sparklin-drive）
#   3) plugins/ 下恰好一个项目则自动用它（历史位置；注意 cargo-nih-plug 会往上找
#      最顶层的 Cargo.toml，插件嵌在本仓里会导致"packages outside of workspace"）
# 注意：不要用位置参数——source 时 $1 会指向调用者的参数。
set -euo pipefail
_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -n "${PLUGIN_DIR:-}" ]; then
    _PLUGIN_DIR="$PLUGIN_DIR"
elif [ -n "${RUSTOMP_GUEST:-}" ] && [ -f "${RUSTOMP_GUEST}/rustomp.toml" ]; then
    _PLUGIN_DIR="$RUSTOMP_GUEST"
else
    # 未指定时：若 plugins/ 下恰好有一个插件项目就自动用它，否则报错
    _cands=()
    for d in "$_ROOT"/plugins/*/; do
        [ -f "$d/rustomp.toml" ] && _cands+=("${d%/}")
    done
    if [ ${#_cands[@]} -eq 1 ]; then
        _PLUGIN_DIR="${_cands[0]}"
    else
        {
            echo "请用 PLUGIN_DIR=<插件目录> 或 RUSTOMP_GUEST=<插件目录> 指定要操作的插件项目。"
            echo "已找到：${_cands[*]:-（无）}"
            echo "插件项目应当放在**本仓之外**（例如 ../sparklin-drive）："
            echo "  RUSTOMP_GUEST=../sparklin-drive ./scripts/build-all.sh"
            echo "原因：cargo-nih-plug 会向上找最顶层的 Cargo.toml，插件嵌在本仓里会报"
            echo "      \"cannot specify features for packages outside of workspace\"。"
        } >&2
        exit 1
    fi
fi
[ -f "$_PLUGIN_DIR/rustomp.toml" ] || { echo "找不到 $_PLUGIN_DIR/rustomp.toml"; exit 1; }

# 供脚本探测：`plugin-env.sh --print-dir` 只打印目录
if [ "${1:-}" = "--print-dir" ]; then
    echo "$_PLUGIN_DIR"
    exit 0
fi
eval "$(python3 "$_ROOT/tools/meta/plugin_meta.py" "$_PLUGIN_DIR/rustomp.toml" --shell)"
ROOT="$_ROOT"

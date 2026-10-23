#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 从插件的 rustomp.toml 载入身份/素材/界面配置。
# 用法（必须 source）：
#   PLUGIN_DIR=<插件目录> source scripts/plugin-env.sh
# 未指定时：若 plugins/ 下恰好有一个插件项目则自动使用它
# 注意：不要用位置参数——source 时 $1 会指向调用者的参数。
set -euo pipefail
_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -n "${PLUGIN_DIR:-}" ]; then
    _PLUGIN_DIR="$PLUGIN_DIR"
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
            echo "请用 PLUGIN_DIR=<插件目录> 指定要操作的插件项目。"
            echo "已找到：${_cands[*]:-（无）}"
            echo "新建插件：./scripts/new-plugin.sh \"My Drive\""
        } >&2
        exit 1
    fi
fi
[ -f "$_PLUGIN_DIR/rustomp.toml" ] || { echo "找不到 $_PLUGIN_DIR/rustomp.toml"; exit 1; }
eval "$(python3 "$_ROOT/tools/meta/plugin_meta.py" "$_PLUGIN_DIR/rustomp.toml" --shell)"
ROOT="$_ROOT"

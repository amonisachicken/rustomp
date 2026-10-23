#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 从插件的 rustomp.toml 载入身份/素材/界面配置。
# 用法（必须 source）：
#   PLUGIN_DIR=<插件目录> source scripts/plugin-env.sh
# 默认插件目录：$ROOT/plugins/sparklin-drive
# 注意：不要用位置参数——source 时 $1 会指向调用者的参数。
set -euo pipefail
_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_PLUGIN_DIR="${PLUGIN_DIR:-$_ROOT/plugins/sparklin-drive}"
[ -f "$_PLUGIN_DIR/rustomp.toml" ] || { echo "找不到 $_PLUGIN_DIR/rustomp.toml"; exit 1; }
eval "$(python3 "$_ROOT/tools/meta/plugin_meta.py" "$_PLUGIN_DIR/rustomp.toml" --shell)"
ROOT="$_ROOT"

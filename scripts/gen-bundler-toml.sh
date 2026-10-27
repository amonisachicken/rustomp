#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# 由 rustomp.toml 生成 nih-plug 的 bundler.toml（bundle 名 = 插件名）
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -n "${1:-}" ] && export PLUGIN_DIR="$1"
source "$ROOT/scripts/plugin-env.sh"
# cargo-nih-plug 会 chdir 到**插件项目的 workspace 根**再读 bundler.toml
OUT="$_PLUGIN_DIR/bundler.toml"
cat > "$OUT" <<EOT
# 由 scripts/gen-bundler-toml.sh 从 $PLUGIN_DIR/rustomp.toml 生成，请勿手改。

["$PLUGIN_PACKAGE"]
name = "$PLUGIN_NAME"
EOT
echo "bundler.toml → ${OUT}（${PLUGIN_NAME}）"  # 中文括号紧跟变量必须加花括号

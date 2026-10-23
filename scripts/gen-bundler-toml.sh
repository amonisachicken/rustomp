#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# 由 rustomp.toml 生成 nih-plug 的 bundler.toml（bundle 名 = 插件名）
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/plugin-env.sh" "${1:-}"
cat > "$ROOT/bundler.toml" <<EOT
# 由 scripts/gen-bundler-toml.sh 从 $PLUGIN_DIR/rustomp.toml 生成，请勿手改。

[rustomp-plugin]
name = "$PLUGIN_NAME"
EOT
echo "bundler.toml → \"$PLUGIN_NAME\""

#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# **构建 macOS 插件 = 调用外部打包工具**（本仓不再包含任何平台打包代码）。
#
#   ./scripts/build-all.sh [插件目录] [--variant mono|stereo|dual]... [--format au|vst3|clap]...
#
# 为什么：打包与宿主兼容（clap-wrapper / CMake / 签名 / 缓存）是平台专属的，
# 放进来会让框架被迫知道 Logic 的行为（DEVLOG E70、ADR-015、docs/ecosystem.md）。
#
# 需要的打包工具：
#   git clone git@github.com:amonisachicken/rustomp-mac-plugin.git
#   export RUSTOMP_MAC_PLUGIN=$HOME/develop/rustomp-mac-plugin
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

PACKER="${RUSTOMP_MAC_PLUGIN:-$HOME/develop/rustomp-mac-plugin}"
if [ ! -x "$PACKER/pack.sh" ]; then
    cat >&2 <<EOF
✗ 找不到 macOS 打包工具（$PACKER/pack.sh）

框架只做 DSP / 代码生成 / 面板，打包在独立仓库里。装一次即可：

  git clone git@github.com:amonisachicken/rustomp-mac-plugin.git ~/develop/rustomp-mac-plugin
  $PACKER/scripts/fetch-third-party.sh        # 首次：拉打包所需的第三方源码
  export RUSTOMP_MAC_PLUGIN=~/develop/rustomp-mac-plugin

然后重新运行：
  ./scripts/build-all.sh [插件目录]
EOF
    exit 1
fi

# 插件目录：显式参数 > $RUSTOMP_GUEST > plugins/ 下唯一项目
PASS_ARGS=("$@")
if [ $# -gt 0 ] && [ -f "${1}/rustomp.toml" ]; then
    PLUGIN_DIR="$1"
    PASS_ARGS=("${@:2}")     # 第一个参数是插件目录，不要再往 pack.sh 传
fi
PLUGIN_DIR="${PLUGIN_DIR:-}"
if [ -z "$PLUGIN_DIR" ] && [ -n "${RUSTOMP_GUEST:-}" ]; then PLUGIN_DIR="$RUSTOMP_GUEST"; fi
if [ -z "$PLUGIN_DIR" ]; then
    PLUGIN_DIR="$(RUSTOMP_ROOT="$ROOT" "$ROOT/scripts/plugin-env.sh" --print-dir 2>/dev/null || true)"
fi
[ -n "$PLUGIN_DIR" ] || { echo "✗ 请指定插件目录：./scripts/build-all.sh <插件目录>" >&2; exit 1; }

export RUSTOMP_ROOT="$ROOT"
exec "$PACKER/pack.sh" "$PLUGIN_DIR" ${PASS_ARGS[@]+"${PASS_ARGS[@]}"}

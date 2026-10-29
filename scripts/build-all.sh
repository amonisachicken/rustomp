#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# **构建插件 = 调用对应平台的打包工具**（本仓不再包含任何平台打包代码）。
#
#   ./scripts/build-all.sh [插件目录] [--variant ...] [--format ...] [--arch ...] [--no-install]
#
# 平台分派：
#   macOS            → rustomp-mac-plugin/pack.sh
#   Windows（MSYS）  → rustomp-win-plugin/pack.sh
#   其他平台         → 明确报错；可用 RUSTOMP_PACKER 指定任意实现契约 B 的脚本
#
# 为什么：打包与宿主兼容是平台专属的，放进来会让框架被迫知道各平台的细节
# （DEVLOG E70、ADR-015、docs/ecosystem.md）。
#
# 打包工具按**同级目录**自动查找，也可显式指定：
#   RUSTOMP_PACKER       任意实现契约 B 的 pack.sh（优先级最高）
#   RUSTOMP_MAC_PLUGIN   macOS 工具目录（默认 <框架同级>/rustomp-mac-plugin）
#   RUSTOMP_WIN_PLUGIN   Windows 工具目录（默认 <框架同级>/rustomp-win-plugin）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PARENT="$(dirname "$ROOT")"
PLATFORM="$(uname -s)"

# ---------------------------------------------------------------- 1. 选打包工具
PACKER="${RUSTOMP_PACKER:-}"
if [ -z "$PACKER" ]; then
    case "$PLATFORM" in
        Darwin)
            if [ -n "${RUSTOMP_MAC_PLUGIN:-}" ]; then
                PACKER="$RUSTOMP_MAC_PLUGIN"   # 显式指定：不再回退到其它位置
            else
                for c in "$PARENT/rustomp-mac-plugin" "$HOME/develop/rustomp-mac-plugin"; do
                    if [ -x "$c/pack.sh" ]; then PACKER="$c"; break; fi
                done
                [ -n "$PACKER" ] || PACKER="$PARENT/rustomp-mac-plugin"
            fi
            ;;
        MINGW*|MSYS*|CYGWIN*)
            if [ -n "${RUSTOMP_WIN_PLUGIN:-}" ]; then
                PACKER="$RUSTOMP_WIN_PLUGIN"   # 显式指定：不再回退到其它位置
            else
                for c in "$PARENT/rustomp-win-plugin"; do
                    if [ -x "$c/pack.sh" ]; then PACKER="$c"; break; fi
                done
                [ -n "$PACKER" ] || PACKER="$PARENT/rustomp-win-plugin"
            fi
            ;;
        *)
            echo "✗ 暂不支持这个平台（$PLATFORM）：框架目前只提供 macOS / Windows 的打包工具。" >&2
            echo "  新平台 = 写一个实现契约 B 的 pack.sh，然后用它显式指定：" >&2
            echo "    RUSTOMP_PACKER=/path/to/pack.sh $0 <插件目录>" >&2
            echo "  契约见 docs/ecosystem.md §2（stdout 只出 ARTIFACT: 行）。" >&2
            exit 1
            ;;
    esac
fi

if [ ! -x "$PACKER/pack.sh" ]; then
    if [ -n "${RUSTOMP_PACKER:-}" ]; then
        cat >&2 <<EOF
✗ 显式指定的打包工具不存在或不可执行：$RUSTOMP_PACKER/pack.sh

去掉 RUSTOMP_PACKER 让框架按平台自动查找，或指向一个实现契约 B 的 pack.sh：
  RUSTOMP_PACKER=/path/to/pack.sh ./scripts/build-all.sh <插件目录>
EOF
        exit 1
    fi
    case "$PLATFORM" in
        Darwin)
            cat >&2 <<EOF
✗ 找不到 macOS 打包工具（$PACKER/pack.sh）

框架只做 DSP / 代码生成 / 面板，打包在独立仓库里。装一次即可：

  cd "$PARENT"
  git clone git@github.com:amonisachicken/rustomp-mac-plugin.git
  ./rustomp-mac-plugin/scripts/fetch-third-party.sh      # 首次：拉打包所需的第三方源码
  export RUSTOMP_MAC_PLUGIN="$PARENT/rustomp-mac-plugin"  # 放在同级目录时可略过

然后重新运行：
  ./scripts/build-all.sh [插件目录]
EOF
            ;;
        MINGW*|MSYS*|CYGWIN*)
            cat >&2 <<EOF
✗ 找不到 Windows 打包工具（$PACKER/pack.sh）

需要 bash（Git Bash / MSYS2）+ MSVC 工具链 + Python ≥3.11。装一次即可：

  cd "$PARENT"
  git clone git@github.com:amonisachicken/rustomp-win-plugin.git
  ./rustomp-win-plugin/scripts/doctor.sh                 # 体检：缺什么、怎么装
  export RUSTOMP_WIN_PLUGIN="$PARENT/rustomp-win-plugin"  # 放在同级目录时可略过

然后重新运行：
  ./scripts/build-all.sh [插件目录]
EOF
            ;;
        *)
            cat >&2 <<EOF
✗ 找不到打包工具：$PACKER/pack.sh

用一个实现契约 B 的脚本启动：
  RUSTOMP_PACKER=/path/to/pack.sh ./scripts/build-all.sh <插件目录>
EOF
            ;;
    esac
    exit 1
fi

# ---------------------------------------------------------------- 2. 插件目录
# 显式参数 > $RUSTOMP_GUEST > plugins/ 下唯一项目
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

# 打包工具需要框架读 rustomp.toml（契约 A）
export RUSTOMP_ROOT="$ROOT"
exec "$PACKER/pack.sh" "$PLUGIN_DIR" ${PASS_ARGS[@]+"${PASS_ARGS[@]}"}

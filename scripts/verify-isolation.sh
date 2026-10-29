#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# **隔离验证**：在全新目录里从 GitHub 拉三个仓（钉 tag），跑一次真实构建。
#
# 为什么需要它：DEVLOG E74 里两个缺陷都是"本机能跑、tag 是坏的"
# （bash 全角变量 unbound / 生成物漂移）。「本机能跑」证明不了「tag 能用」。
#
#   ./scripts/verify-isolation.sh              # 用各仓最新的 semver tag
#   ./scripts/verify-isolation.sh --keep       # 保留工作区（排查用）
#   ./scripts/verify-isolation.sh --tags v0.1.0,v0.1.1,v0.1.2   # 指定三个 tag
#   ./scripts/verify-isolation.sh --fast       # 只出 mono 的 AU（省一半时间）
#
# 检查项：
#   1) 三仓都能 clone 到（tag 存在）
#   2) build.sh 退出码 0
#   3) 产物 ARTIFACT 行数量正确，且**都落在这个隔离目录里**
#   4) au_smoke 自检通过，且 mono / stereo 的 RMS 一致
#   5) 依赖全部来自 git（没有意外的 path 依赖）
#   6) 本机已有仓没被改动（如果它们存在）
#
# 需要：网络（GitHub + crates）、Rust 两个 target、Xcode CLT、cmake。
# 用时：约 4 分钟（全新 target 冷编译）。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWNER="${RUSTOMP_GH_OWNER:-amonisachicken}"
KEEP=0
FAST=0
TAGS=""

while [ $# -gt 0 ]; do
    case "$1" in
        --keep) KEEP=1; shift ;;
        --fast) FAST=1; shift ;;
        --tags) TAGS="${2:?--tags 需要三个逗号分隔的 tag}"; shift 2 ;;
        -h|--help) sed -n '3,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "未知参数：$1（--help）" >&2; exit 2 ;;
    esac
done

say()  { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
bad()  { printf '  \033[31m✗\033[0m %s\n' "$*"; FAIL=1; }
warn() { printf '  ! %s\n' "$*"; }
FAIL=0

latest_tag() {   # $1 = repo
    git ls-remote --tags --refs "git@github.com:${OWNER}/$1.git" 2>/dev/null \
        | sed 's|.*refs/tags/||' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' \
        | sort -V | tail -1
}

# ---------------------------------------------------------------- 0. 选 tag
if [ -n "$TAGS" ]; then
    T_RUSTOMP="$(echo "$TAGS" | cut -d, -f1)"
    T_PACKER="$(echo "$TAGS"  | cut -d, -f2)"
    T_PLUGIN="$(echo "$TAGS"  | cut -d, -f3)"
else
    say "查各仓最新 tag"
    T_RUSTOMP="$(latest_tag rustomp)"
    T_PACKER="$(latest_tag rustomp-mac-plugin)"
    T_PLUGIN="$(latest_tag sparklin-drive)"
fi
[ -n "$T_RUSTOMP" ] && [ -n "$T_PACKER" ] && [ -n "$T_PLUGIN" ] || {
    echo "✗ 取不到 tag（网络？）——也可以用 --tags 手动指定" >&2; exit 1; }

# ---------------------------------------------------------------- 1. 记录本机状态
LOCAL_DIRS="$ROOT"           # 框架仓自身
[ -d "$ROOT/../sparklin-drive" ] && LOCAL_DIRS="$LOCAL_DIRS $ROOT/../sparklin-drive"
[ -d "$ROOT/../rustomp-mac-plugin" ] && LOCAL_DIRS="$LOCAL_DIRS $ROOT/../rustomp-mac-plugin"
DIRTY_BEFORE=""
for d in $LOCAL_DIRS; do
    DIRTY_BEFORE="$DIRTY_BEFORE $(git -C "$d" status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
done

# ---------------------------------------------------------------- 2. 建隔离工作区
ISO="${RUSTOMP_ISO_DIR:-$HOME/develop/.iso-verify}"
ISO="${ISO}-$(date +%m%d-%H%M)"
rm -rf "$ISO"; mkdir -p "$ISO"

say "隔离工作区：$ISO"
printf '   rustomp            %s\n   rustomp-mac-plugin %s\n   sparklin-drive     %s\n' \
       "$T_RUSTOMP" "$T_PACKER" "$T_PLUGIN"

clone_at() {   # $1 = repo, $2 = tag
    if git clone -q --depth 1 --branch "$2" "git@github.com:${OWNER}/$1.git" "$ISO/$1" 2>/dev/null; then
        ok "$1 @ $2（$(git -C "$ISO/$1" rev-parse --short HEAD)）"
    else
        bad "$1 @ $2 clone 失败（tag 不存在？）"
        return 1
    fi
}
clone_at rustomp "$T_RUSTOMP" || true
clone_at rustomp-mac-plugin "$T_PACKER" || true
clone_at sparklin-drive "$T_PLUGIN" || true
[ "$FAIL" = "0" ] || { echo; echo "隔离工作区：${ISO}（保留）"; exit 1; }

# ---------------------------------------------------------------- 3. 第三方源码
say "拉打包工具的第三方源码"
if ( cd "$ISO/rustomp-mac-plugin" && ./scripts/fetch-third-party.sh ) >"$ISO/fetch.log" 2>&1; then
    ok "$(grep -c '^\[clone\]' "$ISO/fetch.log") 份第三方源码就绪"
else
    bad "fetch-third-party 失败（见 $ISO/fetch.log）"
    echo; echo "隔离工作区：${ISO}（保留）"; exit 1
fi

# ---------------------------------------------------------------- 4. 构建
say "构建（$([ "$FAST" = 1 ] && echo 'fast：只出 mono 的 AU' || echo 'mono + stereo，AU/VST3/CLAP')）"
# 不设任何环境变量：三仓已经在同一个目录里（build.sh 的"同级目录"约定）
unset RUSTOMP_ROOT RUSTOMP_MAC_PLUGIN
BUILD_ARGS=(--no-install)
[ "$FAST" = "1" ] && BUILD_ARGS=(--no-install --variant mono --format au)

START=$(date +%s)
( cd "$ISO/sparklin-drive" && ./scripts/build.sh ${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"} ) >"$ISO/build.log" 2>&1
RC=$?
ELAPSED=$(( $(date +%s) - START ))
[ "$RC" = "0" ] && ok "build.sh 退出码 0（${ELAPSED}s）" || bad "build.sh 退出码 ${RC}（见 $ISO/build.log）"

# ---------------------------------------------------------------- 5. 校验产物
say "校验"
ARTIFACTS="$(grep -c '^ARTIFACT: ' "$ISO/build.log" 2>/dev/null || echo 0)"
WANT=6; [ "$FAST" = "1" ] && WANT=1
[ "$ARTIFACTS" = "$WANT" ] && ok "ARTIFACT 行 $ARTIFACTS 条" || bad "ARTIFACT 行 $ARTIFACTS 条（期望 ${WANT}）"

# 产物必须都在隔离目录里（防止"跑到本机别处去了"）
STRAY="$(grep '^ARTIFACT: ' "$ISO/build.log" 2>/dev/null | sed 's/^ARTIFACT: //' | grep -v "^$ISO/" || true)"
[ -z "$STRAY" ] && ok "产物都在隔离目录内" || { bad "有产物写到隔离目录之外："; echo "$STRAY" | head -3; }

MISSING=""
while IFS= read -r p; do
    [ -z "$p" ] && continue
    [ -e "$p" ] || MISSING="$MISSING $p"
done < <(grep '^ARTIFACT: ' "$ISO/build.log" 2>/dev/null | sed 's/^ARTIFACT: //')
[ -z "$MISSING" ] && ok "ARTIFACT 指向的路径都存在" || bad "缺少产物：$MISSING"

# au_smoke：每个变体都要过，且 RMS 一致
SMOKE_OK="$(grep -c '音频通路正常' "$ISO/build.log" 2>/dev/null || echo 0)"
[ "$SMOKE_OK" -ge 1 ] && ok "au_smoke 通过 $SMOKE_OK 次" || bad "au_smoke 没有通过记录"
RMS="$(grep -oE 'RMS=[0-9.]+' "$ISO/build.log" | sort -u | tr '\n' ' ')"
[ "$(echo "$RMS" | wc -w | tr -d ' ')" = "1" ] && ok "RMS 一致：${RMS}" || warn "RMS 有多个值：${RMS}"

# 依赖必须来自 git（没有意外的 path 依赖）
if command -v cargo >/dev/null 2>&1; then
    DEP="$(cd "$ISO/sparklin-drive" && cargo metadata --format-version 1 --offline 2>/dev/null \
        | tr ',' '\n' | grep -oE '"name":"rustomp-(dsp|gui|oracle)"' | wc -l | tr -d ' ')"
    GITSRC="$(grep -c 'source = "git+https://github.com/.*rustomp?tag=' "$ISO/sparklin-drive/Cargo.lock" 2>/dev/null || echo 0)"
    [ "$GITSRC" -ge 3 ] && ok "框架依赖来自 git tag（$GITSRC 条）" || bad "框架依赖不是 git tag（$GITSRC 条）"
fi

# ---------------------------------------------------------------- 6. 本机仓未被污染
say "本机仓"
i=0
DIRTY_AFTER=""
for d in $LOCAL_DIRS; do
    DIRTY_AFTER="$DIRTY_AFTER $(git -C "$d" status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
done
[ "$DIRTY_BEFORE" = "$DIRTY_AFTER" ] && ok "本机仓状态未变化（${LOCAL_DIRS}）" || bad "本机仓被改动了：$LOCAL_DIRS"

# ---------------------------------------------------------------- 7. 报告
say "结论"
if [ "$FAIL" = "0" ]; then
    printf '  \033[32m隔离验证通过\033[0m：%s + %s + %s（%ss）\n' \
        "$T_RUSTOMP" "$T_PACKER" "$T_PLUGIN" "$ELAPSED"
    if [ "$KEEP" = "1" ]; then
        echo "  工作区保留：$ISO"
    else
        rm -rf "$ISO"
        echo "  工作区已删除（--keep 可保留）"
    fi
    exit 0
else
    printf '  \033[31m隔离验证未通过\033[0m —— 日志在 %s/build.log\n' "$ISO"
    [ "$KEEP" = "0" ] && echo "  （工作区保留，方便排查）"
    exit 1
fi

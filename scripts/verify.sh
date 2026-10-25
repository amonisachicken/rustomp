#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 一键验证：把"能不能发"的判断收敛到一个命令里。本地与 CI 共用。
#
#   ./scripts/verify.sh                     # 全部（含 oracle 活体对拍）
#   ./scripts/verify.sh --only fast         # 只跑不需要 ngspice 的部分
#   ./scripts/verify.sh --only oracle       # 只跑对拍（缺 ngspice 直接失败）
#   ./scripts/verify.sh --report out.md     # 额外产出 markdown 报告（CI 用）
#   ./scripts/verify.sh --no-fmt            # 跳过格式检查
#
# 退出码：0 全过；非 0 = 有层失败。
#
# 注意 bash 3.2（macOS 自带）兼容：不用关联数组 / mapfile / [[ -v ]]。
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

ONLY="all"
REPORT=""
RUN_FMT=1
while [ $# -gt 0 ]; do
  case "$1" in
    --only) ONLY="${2:-all}"; shift 2 ;;
    --report) REPORT="${2:-}"; shift 2 ;;
    --no-fmt) RUN_FMT=0; shift ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "未知参数：$1" >&2; exit 2 ;;
  esac
done

case "$ONLY" in
  all|fast|oracle) ;;
  *) echo "--only 只能是 fast / oracle / all" >&2; exit 2 ;;
esac

TMP="$(mktemp -d "${TMPDIR:-/tmp}/rustomp-verify.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

FAILED=0
RESULTS="$TMP/results.tsv"
: >"$RESULTS"

# run_tier <slug> <显示名> <命令...>
# slug 只用于文件名（ASCII，无空格）；显示名可以带中文和标点。
# 捕获输出、记录状态；失败不中断（要跑完才能看清全貌）。
run_tier() {
  local slug="$1"; shift
  local label="$1"; shift
  local log="$TMP/$slug.log"
  printf '\n\033[1m== %s ==\033[0m\n' "$label"
  if "$@" >"$log" 2>&1; then
    printf '   \033[32m✓ %s\033[0m\n' "$label"
    printf '%s\t%s\tok\n' "$slug" "$label" >>"$RESULTS"
  else
    printf '   \033[31m✗ %s\033[0m（日志：%s）\n' "$label" "$log"
    printf '%s\t%s\tfail\n' "$slug" "$label" >>"$RESULTS"
    FAILED=1
    tail -n 20 "$log" | sed 's/^/     /'
  fi
}

# ---------------------------------------------------------------- 快速层（不需要 ngspice）
if [ "$ONLY" = "all" ] || [ "$ONLY" = "fast" ]; then
  if [ "$RUN_FMT" = "1" ]; then
    # 格式检查**不阻塞**（仓库尚未统一跑过 rustfmt，见 DEVLOG）。只记录状态，不计入失败。
    if cargo fmt --all --check >"$TMP/fmt.log" 2>&1; then
      printf '\n\033[1m== 格式检查 ==\033[0m\n   \033[32m✓ 通过\033[0m\n'
      printf 'fmt\t格式检查（不阻塞）\tok\n' >>"$RESULTS"
    else
      printf '\n\033[1m== 格式检查 ==\033[0m\n   \033[33m! 有差异（不阻塞）\033[0m\n'
      printf 'fmt\t格式检查（不阻塞，有差异）\twarn\n' >>"$RESULTS"
    fi
  fi

  run_tier workspace "工作区测试（无 ngspice 也能跑）" cargo test --workspace --release

  # 生成物漂移守卫：显式再跑一次 CLI 的 --check（与测试里的守卫互为双保险）
  run_tier drift-sparklin "生成物漂移（sparklin_drive）" \
    cargo run -q -p rustomp-import --release -- \
      crates/rustomp-import/tests/fixtures/sparklin_drive.cir \
      --mna crates/rustomp-dsp/tests/generated/sparklin_drive.rs --check
  run_tier drift-bjt "生成物漂移（bjt_stage）" \
    cargo run -q -p rustomp-import --release -- \
      crates/rustomp-import/tests/fixtures/bjt_stage.cir \
      --mna crates/rustomp-dsp/tests/generated/bjt_stage.rs --check
fi

# ---------------------------------------------------------------- 对拍层（必须有 ngspice）
if [ "$ONLY" = "all" ] || [ "$ONLY" = "oracle" ]; then
  ORACLE_BIN="${NGSPICE:-}"
  if [ -z "$ORACLE_BIN" ]; then
    ORACLE_BIN="$(command -v ngspice 2>/dev/null || true)"
  fi
  if [ -z "$ORACLE_BIN" ] || ! "$ORACLE_BIN" --version >/dev/null 2>&1; then
    printf '\n\033[1m== 对拍层 ==\033[0m\n'
    printf '   \033[31m✗ 找不到可用的 ngspice\033[0m（%s）\n' "${ORACLE_BIN:-未找到}"
    printf '     macOS 安装：brew install ngspice\n'
    printf '     或指定路径：NGSPICE=/path/to/ngspice ./scripts/verify.sh\n'
    FAILED=1
    printf 'oracle\t对拍层（需要 ngspice）\tfail\n' >>"$RESULTS"
  else
    # RUSTOMP_REQUIRE_NGSPICE=1：让"找不到 ngspice"变成失败而不是静默跳过
    export RUSTOMP_REQUIRE_NGSPICE=1
    run_tier devices "器件级 V-I（二极管 / LED / BJT / JFET / MOS）" \
      cargo test -q -p rustomp-dsp --release --test devices -- --nocapture
    run_tier mna "装配器级（削波 / BJT / JFET / RC 瞬态 / 运放）" \
      cargo test -q -p rustomp-dsp --release --test mna -- --nocapture
    run_tier sparklin "整机级（Sparklin Drive，手写网表）" \
      cargo test -q -p rustomp-dsp --release --test sparklin_mna -- --nocapture
    run_tier generated "生成电路（S1a / S1b）" \
      cargo test -q -p rustomp-dsp --release --test generated_bjt -- --nocapture
  fi
fi

# ---------------------------------------------------------------- 报告
status_word() { if [ "$FAILED" = "0" ]; then echo "✅ 全过"; else echo "❌ 有失败"; fi; }

printf '\n\033[1m== 汇总 ==\033[0m\n'
if [ "$FAILED" = "0" ]; then
  printf '   \033[32m全部通过\033[0m\n'
else
  printf '   \033[31m有失败\033[0m（见上）\n'
fi
while IFS="$(printf '\t')" read -r slug label status; do
  [ -z "$slug" ] && continue
  case "$status" in
    ok)   printf '   \033[32m✓\033[0m %s\n' "$label" ;;
    warn) printf '   \033[33m!\033[0m %s\n' "$label" ;;
    *)    printf '   \033[31m✗\033[0m %s\n' "$label" ;;
  esac
done <"$RESULTS"

if [ -n "$REPORT" ]; then
  {
    echo "## rustomp 验证报告"
    echo ""
    echo "**结果：$(status_word)**　层级：\`$ONLY\`"
    echo ""
    echo "| 层 | 状态 |"
    echo "|---|---|"
    while IFS="$(printf '\t')" read -r slug label status; do
      [ -z "$slug" ] && continue
      case "$status" in
        ok)   echo "| $label | ✅ |" ;;
        warn) echo "| $label | ⚠️ |" ;;
        *)    echo "| $label | ❌ |" ;;
      esac
    done <"$RESULTS"
    echo ""
    echo "<details><summary>对拍数字（关键输出）</summary>"
    echo ""
    echo '```'
    while IFS="$(printf '\t')" read -r slug label status; do
      [ -z "$slug" ] && continue
      case "$slug" in
        devices|mna|sparklin|generated)
          echo "--- $label ---"
          grep -E '^(===|  )' "$TMP/$slug.log" 2>/dev/null | head -n 36
          ;;
      esac
    done <"$RESULTS"
    echo '```'
    echo ""
    echo "</details>"
  } >>"$REPORT"
  printf '   报告已写入 %s\n' "$REPORT"
fi

exit "$FAILED"

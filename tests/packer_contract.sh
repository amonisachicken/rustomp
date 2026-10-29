#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# **契约 A/B 的可执行测试**（docs/ecosystem.md §2 / §6）。
#
# 验证五件事：
#   1. `scripts/build-all.sh` 会把「插件目录 + 其余参数」原样交给打包工具，并导出 RUSTOMP_ROOT；
#   2. 打包工具的 `ARTIFACT: <路径>` 行会原样透出（框架不认识平台细节）；
#   3. 缺打包工具时给出**当前平台**可执行的提示（clone 命令）；
#   4. 契约 A 的最小可用性：模板带 `[requires]`，`plugin_meta.py --get` 对未声明的键返回空；
#   5. **框架源码里不许出现平台专属词**（clap-wrapper / codesign / CMake / Logic / .component）。
#
# 用一个假插件 + 假 packer，不需要 Xcode/MSVC、不需要网络。
#
#   ./tests/packer_contract.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() { printf '✗ %s\n' "$*" >&2; exit 1; }
ok()   { printf '  ✓ %s\n' "$*"; }

echo "== 1/5 假插件 + 假 packer：参数透传、RUSTOMP_ROOT 与 ARTIFACT"
mkdir -p "$TMP/fake-plugin" "$TMP/fake-packer"
cat > "$TMP/fake-plugin/rustomp.toml" <<'EOF'
[plugin]
package = "fake"
name = "Fake"
vendor = "V"
clap_id = "x"
vst3_class_id = "0123456789abcdef"
[au]
manufacturer_code = "Fake"
manufacturer_name = "V"
bundle_id = "x.y"
subtype_mono = "Fa01"
subtype_stereo = "Fa02"
subtype_dual = "Fa03"
[build]
variants = ["mono", "stereo"]
gui = false
[requires]
framework = ">=0.1"
packer = ">=0.1"
EOF

# 假 packer：把收到的参数与环境记下来，并按契约打印 ARTIFACT
cat > "$TMP/fake-packer/pack.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" > "$(dirname "$0")/argv.txt"
printf 'RUSTOMP_ROOT=%s\n' "${RUSTOMP_ROOT:-}" > "$(dirname "$0")/env.txt"
[ "${1:-}" != "" ] || { echo "packer 没收到插件目录" >&2; exit 1; }
echo "ARTIFACT: /tmp/fake/Fake.component"
echo "ARTIFACT: /tmp/fake/Fake.vst3"
EOF
chmod +x "$TMP/fake-packer/pack.sh"

OUT="$(RUSTOMP_PACKER="$TMP/fake-packer" "$ROOT/scripts/build-all.sh" \
        "$TMP/fake-plugin" --variant stereo --arch both --no-install 2>&1)" \
    || fail "build-all.sh 调用假 packer 失败：$OUT"

grep -q '^ARTIFACT: /tmp/fake/Fake.component$' <<<"$OUT" || fail "ARTIFACT 行没有透出：$OUT"
ok "ARTIFACT 行原样透出"

ARGV=()
while IFS= read -r line; do ARGV+=("$line"); done < "$TMP/fake-packer/argv.txt"   # bash 3.2 没有 mapfile
[ "${ARGV[0]}" = "$TMP/fake-plugin" ] || fail "第一个参数不是插件目录：${ARGV[0]}"
[ "${ARGV[1]}" = "--variant" ] && [ "${ARGV[2]}" = "stereo" ] || fail "参数没透传：${ARGV[*]}"
[ "${ARGV[3]}" = "--arch" ] && [ "${ARGV[4]}" = "both" ] || fail "--arch 没透传：${ARGV[*]}"
[ "${ARGV[5]}" = "--no-install" ] || fail "--no-install 没透传：${ARGV[*]}"
ok "插件目录与其余参数正确透传（${ARGV[*]}）"

grep -qx "RUSTOMP_ROOT=$ROOT" "$TMP/fake-packer/env.txt" \
    || fail "RUSTOMP_ROOT 没有导出给打包工具：$(cat "$TMP/fake-packer/env.txt")"
ok "RUSTOMP_ROOT 已导出"

# 假 uname：让三个平台分派在任何开发机上都能被测到（不依赖真机）
mkdir -p "$TMP/bin"
cat > "$TMP/bin/uname" <<'UNAME_EOF'
#!/bin/sh
# 只被 build-all.sh 的 `uname -s` 调用；FAKE_UNAME 没设时退回真实 uname
if [ -n "${FAKE_UNAME:-}" ]; then echo "$FAKE_UNAME"; else exec /usr/bin/uname "$@"; fi
UNAME_EOF
chmod +x "$TMP/bin/uname"

echo "== 2/5 三个平台的环境变量回退仍然有效"
test_env_fallback() {
    local plat="$1" var="$2" label="$3" out
    out="$(PATH="$TMP/bin:$PATH" FAKE_UNAME="$plat" env "$var=$TMP/fake-packer" \
            "$ROOT/scripts/build-all.sh" "$TMP/fake-plugin" 2>&1)" \
        || fail "$var 回退失效（${label}）：$out"
    grep -q '^ARTIFACT: ' <<<"$out" || fail "$var 回退没有产物（${label}）：$out"
    ok "$var 可指定打包工具（${label}）"
}
test_env_fallback Darwin RUSTOMP_MAC_PLUGIN macOS
test_env_fallback Linux RUSTOMP_LINUX_PLUGIN Linux
test_env_fallback MINGW64_NT-10.0 RUSTOMP_WIN_PLUGIN Windows

echo "== 3/5 找不到打包工具时给出可执行的提示"
test_missing_packer() {
    local plat="$1" var="$2" repo="$3" out
    out="$(PATH="$TMP/bin:$PATH" FAKE_UNAME="$plat" env "$var=$TMP/nope" \
            "$ROOT/scripts/build-all.sh" "$TMP/fake-plugin" 2>&1 || true)"
    grep -q "pack.sh" <<<"$out" || fail "缺 packer 时没有说明缺什么（${plat}）：$out"
    grep -q "git clone" <<<"$out" || fail "缺 packer 时没有给出 clone 命令（${plat}）：$out"
    grep -q "$repo" <<<"$out" || fail "缺 packer 时没指向 ${repo}（${plat}）：$out"
    ok "缺 packer → 明确指引（${plat}）"
}
test_missing_packer Darwin RUSTOMP_MAC_PLUGIN rustomp-mac-plugin
test_missing_packer Linux RUSTOMP_LINUX_PLUGIN rustomp-linux-plugin
test_missing_packer MINGW64_NT-10.0 RUSTOMP_WIN_PLUGIN rustomp-win-plugin

OUT="$(PATH="$TMP/bin:$PATH" FAKE_UNAME=FreeBSD "$ROOT/scripts/build-all.sh" "$TMP/fake-plugin" 2>&1 || true)"
grep -q "暂不支持这个平台" <<<"$OUT" || fail "未知平台没有明确报错：$OUT"
ok "未知平台 → 明确报错"

# 显式指定了一个不存在的路径：要说清是 RUSTOMP_PACKER 的问题，而不是乱提示
OUT="$(RUSTOMP_PACKER="$TMP/nope" "$ROOT/scripts/build-all.sh" "$TMP/fake-plugin" 2>&1 || true)"
grep -q "RUSTOMP_PACKER" <<<"$OUT" || fail "显式路径错了却没有提到 RUSTOMP_PACKER：$OUT"
ok "显式 RUSTOMP_PACKER 错了 → 指出该变量"

echo "== 4/5 契约 A：模板带 [requires]，--get 对未声明键返回空"
TPL="$ROOT/template/rustomp.toml"
REQ_PACKER="$(python3 "$ROOT/tools/meta/plugin_meta.py" "$TPL" --get requires.packer)"
[ -n "$REQ_PACKER" ] || fail "模板没有 [requires] packer（打包器的版本校验会永远跳过）"
REQ_FRAMEWORK="$(python3 "$ROOT/tools/meta/plugin_meta.py" "$TPL" --get requires.framework)"
[ -n "$REQ_FRAMEWORK" ] || fail "模板没有 [requires] framework（生成器无法校验框架版本）"
[ -z "$(python3 "$ROOT/tools/meta/plugin_meta.py" "$TPL" --get requires.does_not_exist)" ] \
    || fail "--get 对未声明的键应输出空"
SHELL_REQ="$(python3 "$ROOT/tools/meta/plugin_meta.py" "$TPL" --shell \
    | sed -n 's/^REQUIRES_PACKER=//p' | tr -d "'\"")"
[ "$SHELL_REQ" = "$REQ_PACKER" ] \
    || fail "--shell 导出的 REQUIRES_PACKER 不对：${SHELL_REQ:-（空）}"
ok "模板声明了 framework/packer，且 plugin_meta 导出 REQUIRES_*"

echo "== 5/5 框架源码里不许出现平台专属词"
# 只扫「代码与脚本」，文档里出现是正常的（讲背景/历史）
# 排除：文档、以及 shell 的注释行（讲"为什么这么拆"必须能提到这些词）
hits="$(grep -rnE "clap-wrapper|codesign|CMakeLists|\.component|Logic" \
        "$ROOT/crates" "$ROOT/tools/meta" "$ROOT/scripts" 2>/dev/null \
        | grep -vE "^Binary|\.md:" \
        | grep -vE "^[^:]+:[0-9]+: *#" || true)"
if [ -n "$hits" ]; then
    printf '%s\n' "$hits" >&2
    fail "框架里还有平台专属词（应当搬去 rustomp-mac-plugin）"
fi
ok "crates/ scripts/ tools/meta 里没有平台专属词"

echo
echo "== 契约 A/B 通过"

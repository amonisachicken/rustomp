#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# **契约 B 的可执行测试**（docs/ecosystem.md §2 / §6）。
#
# 验证三件事：
#   1. `scripts/build-all.sh` 会把「插件目录 + 其余参数」原样交给打包工具；
#   2. 打包工具的 `ARTIFACT: <路径>` 行会原样透出（框架不认识平台细节）；
#   3. **框架源码里不许出现平台专属词**（clap-wrapper / codesign / CMake / Logic / .component）。
#
# 用一个假插件 + 假 packer，不需要 Xcode、不需要网络。
#
#   ./tests/packer_contract.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() { printf '✗ %s\n' "$*" >&2; exit 1; }
ok()   { printf '  ✓ %s\n' "$*"; }

echo "== 1/3 假插件 + 假 packer：参数透传与 ARTIFACT"
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
gui = false
EOF

# 假 packer：把收到的参数记下来，并按契约打印 ARTIFACT
cat > "$TMP/fake-packer/pack.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" > "$(dirname "$0")/argv.txt"
[ "${1:-}" != "" ] || { echo "packer 没收到插件目录" >&2; exit 1; }
echo "ARTIFACT: /tmp/fake/Fake.component"
echo "ARTIFACT: /tmp/fake/Fake.vst3"
EOF
chmod +x "$TMP/fake-packer/pack.sh"

OUT="$(RUSTOMP_MAC_PLUGIN="$TMP/fake-packer" "$ROOT/scripts/build-all.sh" \
        "$TMP/fake-plugin" --variant stereo --no-install 2>&1)" \
    || fail "build-all.sh 调用假 packer 失败：$OUT"

grep -q '^ARTIFACT: /tmp/fake/Fake.component$' <<<"$OUT" || fail "ARTIFACT 行没有透出：$OUT"
ok "ARTIFACT 行原样透出"

ARGV=()
while IFS= read -r line; do ARGV+=("$line"); done < "$TMP/fake-packer/argv.txt"   # bash 3.2 没有 mapfile
[ "${ARGV[0]}" = "$TMP/fake-plugin" ] || fail "第一个参数不是插件目录：${ARGV[0]}"
[ "${ARGV[1]}" = "--variant" ] && [ "${ARGV[2]}" = "stereo" ] || fail "参数没透传：${ARGV[*]}"
[ "${ARGV[3]}" = "--no-install" ] || fail "--no-install 没透传：${ARGV[*]}"
ok "插件目录与其余参数正确透传（${ARGV[*]}）"

echo "== 2/3 找不到打包工具时给出可执行的提示"
OUT="$(RUSTOMP_MAC_PLUGIN="$TMP/nope" "$ROOT/scripts/build-all.sh" "$TMP/fake-plugin" 2>&1 || true)"
grep -q "rustomp-mac-plugin" <<<"$OUT" || fail "缺 packer 时没有给出安装指引：$OUT"
grep -q "git clone" <<<"$OUT" || fail "缺 packer 时没有给出 clone 命令：$OUT"
ok "缺 packer → 明确指引"

echo "== 3/3 框架源码里不许出现平台专属词"
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
echo "== 契约 B 通过"

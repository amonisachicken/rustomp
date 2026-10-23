#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 完整构建链（一条命令从源码到 Logic 可加载）：
#
#   Rust 插件 ──► 通用二进制 CLAP + VST3 ──► clap-wrapper ──► AUv2 ──► 签名 ──► 安装 ──► 验证
#
# 用法：
#   ./scripts/build-au.sh                 # 构建 + 安装 + 验证
#   ./scripts/build-au.sh --no-install    # 只构建
#   ./scripts/build-au.sh --skip-auval    # 跳过 auval（本机 registrar 缓存坏了时用）
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PKG="rustomp-plugin"
PLUGIN_NAME="${RUSTOMP_PLUGIN_NAME:-Rustomp Skeleton}"
AU_TYPE="aufx"
AU_SUBTYPE="${RUSTOMP_AU_SUBTYPE:-Rstp}"
AU_MANUFACTURER="${RUSTOMP_AU_MANUFACTURER_CODE:-Amch}"
BUNDLE_ID="${RUSTOMP_BUNDLE_ID:-com.amonisachicken.rustomp.skeleton}"

INSTALL=1
RUN_AUVAL=1
for a in "$@"; do
    case "$a" in
        --no-install) INSTALL=0 ;;
        --skip-auval) RUN_AUVAL=0 ;;
        *) echo "未知参数：$a"; exit 1 ;;
    esac
done

# ---------------------------------------------------------------- 0. 依赖
echo "==> [0/7] 检查第三方源码"
if [ ! -d third_party/clap-wrapper ] || [ ! -d third_party/clap ] || [ ! -d third_party/AudioUnitSDK ]; then
    ./scripts/fetch-third-party.sh
fi

# ---------------------------------------------------------------- 1. Rust
echo "==> [1/7] 构建通用二进制 CLAP + VST3 (x86_64 + arm64)"
cargo nih-plug bundle-universal "$PKG" --release

CLAP="target/bundled/${PLUGIN_NAME}.clap"
VST3="target/bundled/${PLUGIN_NAME}.vst3"
for f in "$CLAP" "$VST3"; do
    [ -e "$f" ] || { echo "构建产物缺失：$f"; exit 1; }
done

# nih-plug 的 bundler 把 CFBundleIdentifier 硬编码成 com.nih-plug.<package>，
# 这里改成我们自己的（改了 plist 必须重新签名，签名在下面统一做）。
echo "==> [2/7] 修正 Info.plist 的 bundle id"
for b in "$CLAP/Contents/Info.plist" "$VST3/Contents/Info.plist"; do
    plutil -replace CFBundleIdentifier -string "$BUNDLE_ID" "$b"
    plutil -replace CFBundleShortVersionString -string "0.1.0" "$b" 2>/dev/null || true
    plutil -replace CFBundleVersion -string "0.1.0" "$b" 2>/dev/null || true
done

# ---------------------------------------------------------------- 2. AUv2
echo "==> [3/7] clap-wrapper → AUv2"
cmake -B build/au -S cmake \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_OSX_ARCHITECTURES="x86_64;arm64" \
      -DCLAP_BUNDLE="$ROOT/$CLAP" \
      -DRUSTOMP_AU_SUBTYPE="$AU_SUBTYPE" \
      -DRUSTOMP_AU_MANUFACTURER_CODE="$AU_MANUFACTURER" \
      -DRUSTOMP_BUNDLE_ID="$BUNDLE_ID" \
      -DRUSTOMP_PLUGIN_NAME="$PLUGIN_NAME" > build/au/configure.log 2>&1 \
    || { tail -30 build/au/configure.log; exit 1; }
cmake --build build/au --config Release -j"$(sysctl -n hw.ncpu)" > build/au/build.log 2>&1 \
    || { grep -nE "error:" build/au/build.log | head -10; exit 1; }

COMPONENT="build/au/${PLUGIN_NAME}.component"
[ -d "$COMPONENT" ] || { echo "AU 构建产物缺失：$COMPONENT"; exit 1; }
lipo -info "$COMPONENT/Contents/MacOS/${PLUGIN_NAME}"
lipo -info "$COMPONENT/Contents/PlugIns/${PLUGIN_NAME}.clap/Contents/MacOS/${PLUGIN_NAME}"

# ---------------------------------------------------------------- 3. 签名
echo "==> [4/7] ad-hoc 签名（先内层 clap，再外层 bundle）"
codesign --force --sign - "$COMPONENT/Contents/PlugIns/${PLUGIN_NAME}.clap" 2>/dev/null || true
codesign --force --sign - "$COMPONENT" 2>/dev/null || true
codesign --force --sign - "$VST3" 2>/dev/null || true
codesign --force --sign - "$CLAP" 2>/dev/null || true

# ---------------------------------------------------------------- 4. 冒烟测试
echo "==> [5/7] AU 进程内冒烟测试（绕过系统注册表）"
mkdir -p build
cc -O2 -o build/au_smoke tools/au-smoke/au_smoke.c \
   -framework AudioToolbox -framework CoreFoundation
set +e
./build/au_smoke "$COMPONENT/Contents/MacOS/${PLUGIN_NAME}" \
                 wrapAsAUV2_inst0Factory \
                 "$AU_TYPE" "$AU_SUBTYPE" "$AU_MANUFACTURER" 512 2
SMOKE=$?
set -e
[ $SMOKE -eq 0 ] || { echo "AU 冒烟测试失败（退出码 $SMOKE）"; exit $SMOKE; }

if [ "$INSTALL" = "0" ]; then
    echo "==> 跳过安装 (--no-install)"
    exit 0
fi

# ---------------------------------------------------------------- 5. 安装
echo "==> [6/7] 安装到用户目录"
COMP_DIR="$HOME/Library/Audio/Plug-Ins/Components"
VST3_DIR="$HOME/Library/Audio/Plug-Ins/VST3"
CLAP_DIR="$HOME/Library/Audio/Plug-Ins/CLAP"
mkdir -p "$COMP_DIR" "$VST3_DIR" "$CLAP_DIR"
rm -rf "${COMP_DIR:?}/${PLUGIN_NAME}.component" \
       "${VST3_DIR:?}/${PLUGIN_NAME}.vst3" \
       "${CLAP_DIR:?}/${PLUGIN_NAME}.clap"
cp -R "$COMPONENT" "$COMP_DIR/"
cp -R "$VST3" "$VST3_DIR/"
cp -R "$CLAP" "$CLAP_DIR/"
killall -9 AudioComponentRegistrar 2>/dev/null \
    || echo "    (提示: 无法重启 AudioComponentRegistrar，需要 sudo 或重启系统才能刷新 AU 列表)"
sleep 1

# ---------------------------------------------------------------- 6. auval
if [ "$RUN_AUVAL" = "0" ]; then
    echo "==> 跳过 auval (--skip-auval)"
    exit 0
fi
echo "==> [7/7] auval 校验"
set +e
OUT=$(auval -v "$AU_TYPE" "$AU_SUBTYPE" "$AU_MANUFACTURER" 2>&1)
echo "$OUT" | tail -25
if echo "$OUT" | grep -q "AU VALIDATION SUCCEEDED"; then
    echo "==> auval: 通过"
else
    cat <<'HINT'

==> auval 未通过。
    若日志里是 "didn't find the component"，先确认这是不是本机环境问题：
        auval -v aufx Dqu0 Chow       # 系统里已安装的第三方 AU (BYOD)
    如果 BYOD 也找不到，说明 AudioComponentRegistrar 的缓存坏了，
    与我们构建的插件无关。修复方式（任一）：
        1) 重启系统
        2) sudo killall -9 AudioComponentRegistrar
    在修复前可用 `--skip-auval`，并以 AU 冒烟测试（上面已通过）为准。
HINT
    exit 1
fi

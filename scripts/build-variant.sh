#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 构建「只支持单声道 / 只支持立体声」的独立插件变体（Waves 式）。
#
#   ./scripts/build-variant.sh mono
#   ./scripts/build-variant.sh stereo
#
# 为什么这样做：
#   Logic 的插入菜单按通道条的声道数过滤 AU。一个"同时支持 mono+stereo"的插件
#   依赖宿主正确读出两个布局；而单独的 mono / stereo 变体各自只有一个声道布局，
#   不存在歧义。而且新变体是**新的 AU 身份**（新 subtype），宿主会当成全新插件
#   扫描，天然绕开任何陈旧缓存。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VARIANT="${1:?用法: $0 <mono|stereo>}"
source "$ROOT/scripts/plugin-env.sh"

case "$VARIANT" in
    mono)   SUFFIX="Mono";   SUBTYPE="$AU_SUBTYPE_MONO";   BID="$AU_BUNDLE_ID.mono" ;;
    stereo) SUFFIX="Stereo"; SUBTYPE="$AU_SUBTYPE_STEREO"; BID="$AU_BUNDLE_ID.stereo" ;;
    *) echo "未知变体：$VARIANT（只支持 mono / stereo）"; exit 1 ;;
esac
NAME="$PLUGIN_NAME $SUFFIX"
BUNDLE_ID="$BID"
VENDOR="$PLUGIN_VENDOR"
MANUFACTURER="$AU_MANUFACTURER"
MANUFACTURER_NAME="$AU_MANUFACTURER_NAME"

# bundle 名由 bundler.toml 决定，从 rustomp.toml 生成一次
./scripts/gen-bundler-toml.sh >/dev/null
SRC_BASE="$PLUGIN_NAME"

echo "==> [1/5] 构建 ${NAME}（通用二进制，feature=${VARIANT}）"
# 插件项目自带 workspace（不在 rustomp 的 workspace 里），所以要在插件目录里构建：
#   - target/ 落在插件项目根下（cargo-nih-plug 会 chdir 到 workspace 根）
#   - rustomp 本身 clone 下来就能 cargo test（不依赖 plugins/ 是否存在）
PLUGIN_BUILD_DIR="$_PLUGIN_DIR"
(
    cd "$PLUGIN_BUILD_DIR"
    cargo nih-plug bundle-universal "$PLUGIN_PACKAGE" --release --features "$VARIANT"
)

SRC_CLAP="$PLUGIN_BUILD_DIR/target/bundled/${SRC_BASE}.clap"
SRC_VST3="$PLUGIN_BUILD_DIR/target/bundled/${SRC_BASE}.vst3"
DST_CLAP="$PLUGIN_BUILD_DIR/target/bundled/${NAME}.clap"
DST_VST3="$PLUGIN_BUILD_DIR/target/bundled/${NAME}.vst3"

echo "==> [2/5] 重命名 bundle 并修正 plist"
rm -rf "$DST_CLAP" "$DST_VST3"
mv "$SRC_CLAP" "$DST_CLAP"
mv "$SRC_VST3" "$DST_VST3"
for b in "$DST_CLAP" "$DST_VST3"; do
    rm -rf "$b/Contents/_CodeSignature"
    mv "$b/Contents/MacOS/${SRC_BASE}" "$b/Contents/MacOS/$NAME"
    plutil -replace CFBundleExecutable -string "$NAME" "$b/Contents/Info.plist"
    plutil -replace CFBundleName -string "$NAME" "$b/Contents/Info.plist"
    plutil -replace CFBundleDisplayName -string "$NAME" "$b/Contents/Info.plist" 2>/dev/null || true
    plutil -replace CFBundleIdentifier -string "$BUNDLE_ID" "$b/Contents/Info.plist"
done

echo "==> [3/5] clap-wrapper → AUv2（${SUBTYPE}）"
BUILD_DIR="build/au-${VARIANT}"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
cmake -B "$BUILD_DIR" -S cmake \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_OSX_ARCHITECTURES="x86_64;arm64" \
      -DCLAP_BUNDLE="$DST_CLAP" \
      -DRUSTOMP_PLUGIN_NAME="$NAME" \
      -DRUSTOMP_AU_SUBTYPE="$SUBTYPE" \
      -DRUSTOMP_AU_MANUFACTURER_CODE="$MANUFACTURER" \
      -DRUSTOMP_AU_MANUFACTURER_NAME="$MANUFACTURER_NAME" \
      -DRUSTOMP_BUNDLE_ID="$BUNDLE_ID" > "$BUILD_DIR/configure.log" 2>&1 \
    || { tail -30 "$BUILD_DIR/configure.log"; exit 1; }
cmake --build "$BUILD_DIR" --config Release -j"$(sysctl -n hw.ncpu)" > "$BUILD_DIR/build.log" 2>&1 \
    || { grep -nE "error:" "$BUILD_DIR/build.log" | head -10; exit 1; }

COMPONENT="$BUILD_DIR/${NAME}.component"
[ -d "$COMPONENT" ] || { echo "AU 产物缺失：$COMPONENT"; exit 1; }

# Info.plist 归位（同 build-au.sh 的原因：增量构建时 clap-wrapper 的 POST_BUILD 可能不执行）
cp "$BUILD_DIR/rustomp_as_auv2-build-helper-output/auv2_Info.plist" "$COMPONENT/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "$BUNDLE_ID" "$COMPONENT/Contents/Info.plist"
plutil -replace CFBundlePackageType -string "BNDL" "$COMPONENT/Contents/Info.plist"
plutil -insert LSMinimumSystemVersion -string "10.13" "$COMPONENT/Contents/Info.plist" 2>/dev/null || true
plutil -insert CFBundleSupportedPlatforms -json '["MacOSX"]' "$COMPONENT/Contents/Info.plist" 2>/dev/null || true
plutil -replace AudioComponents.0.sandboxSafe -bool NO "$COMPONENT/Contents/Info.plist" 2>/dev/null || true
got=$(plutil -extract AudioComponents.0.subtype raw "$COMPONENT/Contents/Info.plist")
[ "$got" = "$SUBTYPE" ] || { echo "subtype 校验失败：$got"; exit 1; }

echo "==> [4/5] 签名"
codesign --force --sign - "$COMPONENT/Contents/PlugIns/${NAME}.clap" 2>/dev/null || true
codesign --force --sign - "$COMPONENT" 2>/dev/null || true
codesign --force --sign - "$DST_VST3" 2>/dev/null || true
codesign --force --sign - "$DST_CLAP" 2>/dev/null || true

echo "==> [5/5] 安装 + 自检"
for d in Components VST3 CLAP; do mkdir -p "$HOME/Library/Audio/Plug-Ins/$d"; done
rm -rf "$HOME/Library/Audio/Plug-Ins/Components/${NAME}.component" \
       "$HOME/Library/Audio/Plug-Ins/VST3/${NAME}.vst3" \
       "$HOME/Library/Audio/Plug-Ins/CLAP/${NAME}.clap"
cp -R "$COMPONENT" "$HOME/Library/Audio/Plug-Ins/Components/"
cp -R "$DST_VST3" "$HOME/Library/Audio/Plug-Ins/VST3/"
cp -R "$DST_CLAP" "$HOME/Library/Audio/Plug-Ins/CLAP/"

CH=1; [ "$VARIANT" = "stereo" ] && CH=2
[ -x build/au_smoke ] || cc -O2 -o build/au_smoke tools/au-smoke/au_smoke.c \
    -framework AudioToolbox -framework CoreFoundation
./build/au_smoke "$COMPONENT/Contents/MacOS/${NAME}" wrapAsAUV2_inst0Factory \
    aufx "$SUBTYPE" "$MANUFACTURER" 512 "$CH" 2>&1 \
    | grep -E "SupportedNumChannels|in=|渲染|音频通路|FAIL" | sed 's/^/    /'

echo
echo "完成：${NAME}  →  aufx / ${SUBTYPE} / ${MANUFACTURER}"

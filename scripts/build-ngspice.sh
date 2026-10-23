#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# 从源码编译并安装 ngspice（仅开发期 oracle 用，不进插件二进制）。
#
# 为什么不用 brew：本机 Intel + macOS 13 在 Homebrew 属 Tier 3，**无预编译 bottle**，
# 而整棵依赖树（libomp/libpng/...）比 ngspice 本身还重。这里只取必需项。
#
# 用法：
#   source ~/vpn.env      # 若直连 sourceforge 受限
#   ./scripts/build-ngspice.sh
#
# 结果：$PREFIX/bin/ngspice（默认 ~/.local/ngspice）
set -euo pipefail

NGSPICE_VER="${NGSPICE_VER:-47}"
PREFIX="${PREFIX:-$HOME/.local/ngspice}"
BUILD="${BUILD_DIR:-/tmp/ngbuild}"
# Homebrew 的 readline 8.3：系统自带的是 readline 4.2(libedit 模拟)，
# 缺 ngspice-47 需要的 rl_reset_after_signal()，会导致 signal_handler.c 编译失败。
READLINE="$(brew --prefix readline 2>/dev/null || echo /usr/local/opt/readline)"
# 系统 bison 2.3 太旧，ngspice 的解析器生成需要 >= 2.6
export PATH="$(brew --prefix bison 2>/dev/null)/bin:$PATH"

URL="https://sourceforge.net/projects/ngspice/files/ng-spice-rework/${NGSPICE_VER}/ngspice-${NGSPICE_VER}.tar.gz"
SRC="$BUILD/ngspice-${NGSPICE_VER}"

mkdir -p "$BUILD"
cd "$BUILD"

if [ ! -d "$SRC" ]; then
    echo "==> 下载 ngspice-${NGSPICE_VER}"
    [ -s "ngspice-${NGSPICE_VER}.tar.gz" ] || \
        curl -L --retry 5 --retry-delay 3 --max-time 3600 \
             -o "ngspice-${NGSPICE_VER}.tar.gz" "$URL"
    tar xzf "ngspice-${NGSPICE_VER}.tar.gz"
fi

cd "$SRC"

echo "==> configure (prefix=$PREFIX, readline=$READLINE)"
./configure \
    --prefix="$PREFIX" \
    --without-x \
    --disable-openmp \
    --disable-debug \
    --disable-tcl \
    CFLAGS="-O2 -I${READLINE}/include" \
    LDFLAGS="-L${READLINE}/lib" > configure.log 2>&1 \
    || { tail -40 configure.log; exit 1; }
grep -iE "readline" configure.log | tail -3

echo "==> make -j$(sysctl -n hw.ncpu)"
make -j"$(sysctl -n hw.ncpu)" > make.log 2>&1 || { grep -nE "error:" make.log | head -10; exit 1; }

echo "==> make install"
make install > install.log 2>&1 || { tail -30 install.log; exit 1; }

echo "==> 版本"
"$PREFIX/bin/ngspice" --version 2>&1 | head -3

echo "==> 冒烟测试（RC 低通 tran）"
cat > /tmp/ng_smoke_$$.cir <<'EOF'
* RC lowpass smoke test
V1 in 0 PULSE(0 1 0 1n 1n 1m 2m)
R1 in out 1k
C1 out 0 100n
.tran 1u 3m
.print tran v(out)
.end
EOF
"$PREFIX/bin/ngspice" -b /tmp/ng_smoke_$$.cir 2>&1 | grep -A3 "Index" | head -5
rm -f /tmp/ng_smoke_$$.cir

echo
echo "完成。可执行文件：$PREFIX/bin/ngspice"
echo "如需加入 PATH：ln -sfn $PREFIX/bin/ngspice /usr/local/bin/ngspice"

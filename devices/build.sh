#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# 重新生成 devices/devices.lib：
#   parsed.lib（从 models.add 清洗，跳过被手写模型覆盖的项）
#   + handwritten.lib（手写/覆盖）
#   → devices.lib（最终交给 ngspice/oracle）
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# 被 handwritten.lib 覆盖的型号（避免重复 .model 定义）
SKIP="1N4007,2N7000"

python3 tools/meta/ltspice_models.py devices/models.add -o devices --skip "$SKIP"
mv devices/devices.lib devices/parsed.lib

{
  echo "* ===== devices.lib：自动生成，请勿手改 ====="
  echo "* 组成：parsed.lib + handwritten.lib"
  echo
  echo "* ----- parsed.lib（来自 models.add）-----"
  cat devices/parsed.lib
  echo
  echo "* ----- handwritten.lib（手写 / 覆盖）-----"
  cat devices/handwritten.lib
  echo
  echo "* ----- reference.lib（从 breadspice 迁入，供对拍）-----"
  cat devices/reference.lib
} > devices/devices.lib
echo "已生成 devices/devices.lib（$(grep -c '^\.model\|^\.subckt\|^\.MODEL\|^\.SUBCKT' devices/devices.lib) 个 model/subckt）"

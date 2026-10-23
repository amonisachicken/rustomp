# SPDX-License-Identifier: GPL-3.0-or-later
#
# 用 clap-wrapper 把 nih-plug 产出的 .clap 包装成 AUv2（Logic 用）和可选的 VST3。
#
# 为什么需要这个文件：nih-plug 只导出 VST3/CLAP，没有 AU。
# 见 DEVLOG.md §2.7 / ADR-006。
#
# 用法（见 scripts/build-au.sh）：
#   cmake -B build/au -S cmake \
#         -DCLAP_BUNDLE="target/bundled/Rustomp Skeleton.clap" \
#         -DCMAKE_OSX_ARCHITECTURES="x86_64;arm64"
#   cmake --build build/au
#
cmake_minimum_required(VERSION 3.20)
project(rustomp-au LANGUAGES C CXX OBJCXX VERSION 0.1.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# ---- 我们的插件身份（AU 的四字符码，一旦发布不可更改）----
set(RUSTOMP_PLUGIN_NAME   "Rustomp Skeleton"                     CACHE STRING "插件显示名")
set(RUSTOMP_BUNDLE_ID     "com.amonisachicken.rustomp.skeleton"  CACHE STRING "AU bundle id")
set(RUSTOMP_AU_SUBTYPE    "Rstp"                                 CACHE STRING "AU subtype (4 chars)")
set(RUSTOMP_AU_MANUFACTURER_CODE "Amch"                          CACHE STRING "AU manufacturer (4 chars)")
set(RUSTOMP_AU_MANUFACTURER_NAME "amonisachicken"                CACHE STRING "AU manufacturer name")
# aufx = 效果器；aumu = 乐器。我们是效果器。
set(RUSTOMP_AU_TYPE       "aufx"                                 CACHE STRING "AU component type")

# 由 nih-plug `cargo nih-plug bundle` 产出的 .clap 包路径（会被嵌入 .component 内）
set(CLAP_BUNDLE "${CMAKE_CURRENT_SOURCE_DIR}/../target/bundled/${RUSTOMP_PLUGIN_NAME}.clap"
    CACHE PATH "要包装的 .clap 包")
if (NOT EXISTS "${CLAP_BUNDLE}")
    message(FATAL_ERROR
        "找不到 CLAP 包：${CLAP_BUNDLE}\n"
        "请先运行：cargo nih-plug bundle-universal rustomp-plugin --release")
endif()

# ---- 第三方 SDK（scripts/fetch-third-party.sh 拉取）----
set(THIRD_PARTY "${CMAKE_CURRENT_SOURCE_DIR}/../third_party")
set(CLAP_SDK_ROOT        "${THIRD_PARTY}/clap"         CACHE PATH "CLAP SDK")
set(AUDIOUNIT_SDK_ROOT   "${THIRD_PARTY}/AudioUnitSDK" CACHE PATH "Apple AudioUnitSDK")
set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES OFF CACHE BOOL "禁止构建期联网下载 SDK")

add_subdirectory("${THIRD_PARTY}/clap-wrapper" "${CMAKE_BINARY_DIR}/clap-wrapper" EXCLUDE_FROM_ALL)

# ---- AUv2 ----
add_library(rustomp_as_auv2 MODULE)
target_add_auv2_wrapper(
    TARGET              rustomp_as_auv2
    OUTPUT_NAME         "${RUSTOMP_PLUGIN_NAME}"
    BUNDLE_IDENTIFIER   "${RUSTOMP_BUNDLE_ID}"
    BUNDLE_VERSION      "${PROJECT_VERSION}"
    MANUFACTURER_NAME   "${RUSTOMP_AU_MANUFACTURER_NAME}"
    MANUFACTURER_CODE   "${RUSTOMP_AU_MANUFACTURER_CODE}"
    SUBTYPE_CODE        "${RUSTOMP_AU_SUBTYPE}"
    INSTRUMENT_TYPE     "${RUSTOMP_AU_TYPE}"
    # 关键：把 .clap 嵌进 .component/Contents/PlugIns/，用户只需装 AU 一个文件
    MACOS_EMBEDDED_CLAP_LOCATION "${CLAP_BUNDLE}"
)

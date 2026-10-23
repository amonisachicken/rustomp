// SPDX-License-Identifier: GPL-3.0-or-later
//
// au_smoke —— 绕过 AudioComponentRegistrar 的 AUv2 冒烟测试
//
// 背景：本机 ~/Library/Caches/AudioUnitCache 被破坏后，AudioComponentRegistrar
// 无法在不重启的情况下重建缓存，导致 auval 连已安装的第三方 AU（如 BYOD）都找不到。
// 本工具用 AudioComponentRegister() 直接在进程内注册 bundle 的工厂函数，
// 从而验证「AU 本体是否能打开 / 初始化 / 渲染」，与系统注册表无关。
//
// 用法：
//   au_smoke <component Mach-O 路径> <工厂符号名> <type> <subtype> <manu> [帧数]
// 例：
//   au_smoke "Rustomp Skeleton.component/Contents/MacOS/Rustomp Skeleton" \
//            wrapAsAUV2_inst0Factory aufx Rstp Amch
//
// 退出码：0 = 通过；非 0 = 失败（并打印失败步骤）

#include <AudioToolbox/AudioToolbox.h>
#include <dlfcn.h>
#include <math.h>
#include <stdio.h>
#include <string.h>

typedef void *(*FactoryFn)(const AudioComponentDescription *);

#define CHECK(expr, msg)                                                      \
    do {                                                                      \
        OSStatus _s = (expr);                                                 \
        if (_s != noErr) {                                                    \
            printf("[FAIL] %-52s OSStatus=%d (%c%c%c%c)\n", msg, (int)_s,     \
                   (int)(_s >> 24) & 0xff, (int)(_s >> 16) & 0xff,            \
                   (int)(_s >> 8) & 0xff, (int)_s & 0xff);                    \
            return 10;                                                        \
        }                                                                     \
        printf("[ ok ] %s\n", msg);                                           \
    } while (0)

static OSStatus input_callback(void *inRefCon, AudioUnitRenderActionFlags *flags,
                               const AudioTimeStamp *ts, UInt32 bus,
                               UInt32 nframes, AudioBufferList *io) {
    (void)inRefCon;
    (void)flags;
    (void)ts;
    (void)bus;
    // 直接往输入缓冲写 0.25 直流 + 正弦，便于观察增益
    for (UInt32 c = 0; c < io->mNumberBuffers; c++) {
        float *p = (float *)io->mBuffers[c].mData;
        for (UInt32 i = 0; i < nframes; i++) {
            p[i] = 0.25f;
        }
    }
    return noErr;
}

int main(int argc, char **argv) {
    if (argc < 6) {
        fprintf(stderr,
                "用法: %s <Mach-O 路径> <工厂符号> <type> <subtype> <manu> [帧数] [声道数]\n",
                argv[0]);
        return 1;
    }
    const char *macho = argv[1];
    const char *sym = argv[2];
    UInt32 nframes = (argc > 6) ? (UInt32)atoi(argv[6]) : 512;
    UInt32 nch = (argc > 7) ? (UInt32)atoi(argv[7]) : 2;

    AudioComponentDescription desc;
    memset(&desc, 0, sizeof(desc));
    desc.componentType = (UInt32)(((uint32_t)argv[3][0] << 24) | ((uint32_t)argv[3][1] << 16) |
                                 ((uint32_t)argv[3][2] << 8) | (uint32_t)argv[3][3]);
    desc.componentSubType = (UInt32)(((uint32_t)argv[4][0] << 24) | ((uint32_t)argv[4][1] << 16) |
                                    ((uint32_t)argv[4][2] << 8) | (uint32_t)argv[4][3]);
    desc.componentManufacturer = (UInt32)(((uint32_t)argv[5][0] << 24) | ((uint32_t)argv[5][1] << 16) |
                                         ((uint32_t)argv[5][2] << 8) | (uint32_t)argv[5][3]);

    printf("=== AU 冒烟测试 ===\n  bundle: %s\n  工厂: %s\n", macho, sym);

    void *h = dlopen(macho, RTLD_LAZY | RTLD_LOCAL);
    if (!h) {
        printf("[FAIL] dlopen: %s\n", dlerror());
        return 2;
    }
    printf("[ ok ] dlopen\n");

    FactoryFn factory = (FactoryFn)dlsym(h, sym);
    if (!factory) {
        printf("[FAIL] dlsym(%s): %s\n", sym, dlerror());
        return 3;
    }
    printf("[ ok ] dlsym 工厂函数\n");

    printf("  host 架构: ");
#if defined(__arm64__) || defined(__aarch64__)
    printf("arm64\n");
#else
    printf("x86_64\n");
#endif

    AudioComponent comp = AudioComponentRegister(&desc, CFSTR("Rustomp Smoke"),
                                                 0x00010000,
                                                 (AudioComponentFactoryFunction)factory);
    if (!comp) {
        printf("[FAIL] AudioComponentRegister\n");
        return 4;
    }
    printf("[ ok ] AudioComponentRegister (绕过系统注册表)\n");

    AudioComponentInstance inst = NULL;
    CHECK(AudioComponentInstanceNew(comp, &inst), "AudioComponentInstanceNew");

    AudioStreamBasicDescription fmt;
    memset(&fmt, 0, sizeof(fmt));
    fmt.mSampleRate = 48000.0;
    fmt.mFormatID = kAudioFormatLinearPCM;
    fmt.mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked |
                       kAudioFormatFlagIsNonInterleaved;
    fmt.mBytesPerPacket = 4;
    fmt.mFramesPerPacket = 1;
    fmt.mBytesPerFrame = 4;
    fmt.mChannelsPerFrame = nch;
    fmt.mBitsPerChannel = 32;

    CHECK(AudioUnitSetProperty(inst, kAudioUnitProperty_StreamFormat,
                               kAudioUnitScope_Input, 0, &fmt, sizeof(fmt)),
          "设置输入格式 (48k f32, element 0)");
    CHECK(AudioUnitSetProperty(inst, kAudioUnitProperty_StreamFormat,
                               kAudioUnitScope_Output, 0, &fmt, sizeof(fmt)),
          "设置输出格式");

    AURenderCallbackStruct in_cb = {input_callback, NULL};
    CHECK(AudioUnitSetProperty(inst, kAudioUnitProperty_SetRenderCallback,
                               kAudioUnitScope_Input, 0, &in_cb, sizeof(in_cb)),
          "挂输入总线回调");

    CHECK(AudioUnitInitialize(inst), "AudioUnitInitialize（会真正加载内嵌 CLAP）");

    // 读取参数数量，确认插件身份与参数链路
    AudioUnitParameterID ids[64];
    UInt32 sz = sizeof(ids);
    if (AudioUnitGetProperty(inst, kAudioUnitProperty_ParameterList,
                             kAudioUnitScope_Global, 0, ids, &sz) == noErr) {
        UInt32 nparams = sz / sizeof(AudioUnitParameterID);
        printf("[info] 参数数量: %u\n", (unsigned)nparams);
        for (UInt32 i = 0; i < nparams && i < 8; i++) {
            AudioUnitParameterID id = ids[i];
            AudioUnitParameterInfo pi;
            UInt32 psz = sizeof(pi);
            if (AudioUnitGetProperty(inst, kAudioUnitProperty_ParameterInfo,
                                     kAudioUnitScope_Global, id, &pi, &psz) == noErr) {
                // 注意：clap-wrapper 故意把 name (char[52]) 清零，改用 cfNameString。
                // 宿主（Logic / AULab）优先读 cfNameString，所以这是正常的。
                char cfname[128] = "";
                if (pi.cfNameString) {
                    CFStringGetCString(pi.cfNameString, cfname, sizeof(cfname),
                                       kCFStringEncodingUTF8);
                    CFRelease(pi.cfNameString);
                }
                printf("        [%u] id=%u name=\"%s\" cfName=\"%s\" range=%g..%g default=%g unit=%u\n",
                       (unsigned)i, (unsigned)id, pi.name, cfname, pi.minValue,
                       pi.maxValue, pi.defaultValue, (unsigned)pi.unit);
            }
        }
    }

    // 渲染：输入恒 0.25，输出应为 0.25 * gain（默认 0 dB = 0.25）
    printf("[info] 声道数: %u\n", (unsigned)nch);
    float **chan = (float **)calloc(nch, sizeof(float *));
    AudioBufferList *abl = (AudioBufferList *)calloc(
        1, sizeof(AudioBufferList) + (nch - 1) * sizeof(AudioBuffer));
    abl->mNumberBuffers = nch;
    for (UInt32 c = 0; c < nch; c++) {
        chan[c] = (float *)calloc(nframes, sizeof(float));
        abl->mBuffers[c].mNumberChannels = 1;
        abl->mBuffers[c].mDataByteSize = nframes * sizeof(float);
        abl->mBuffers[c].mData = chan[c];
    }

    AudioTimeStamp ts;
    memset(&ts, 0, sizeof(ts));
    ts.mSampleTime = 0;
    ts.mFlags = kAudioTimeStampSampleTimeValid;
    AudioUnitRenderActionFlags flags = 0;

    CHECK(AudioUnitRender(inst, &flags, &ts, 0, nframes, abl), "AudioUnitRender");

    double sum = 0.0;
    int bad = 0;
    for (UInt32 c = 0; c < nch; c++) {
        for (UInt32 i = 0; i < nframes; i++) {
            if (!isfinite(chan[c][i])) bad++;
            sum += chan[c][i];
        }
    }
    double mean = sum / (double)(nframes * nch);
    printf("[info] 渲染 %u 帧: 非有限值=%d, 输出均值=%.6f (期望 ≈0.250000)\n",
           (unsigned)nframes, bad, mean);

    int rc = 0;
    if (bad > 0) { printf("[FAIL] 输出含 NaN/Inf\n"); rc = 20; }
    else if (fabs(mean - 0.25) > 0.02) { printf("[FAIL] 直通/增益电平不符\n"); rc = 21; }
    else printf("[ ok ] 音频通路与增益参数均正常\n");

    AudioUnitUninitialize(inst);
    AudioComponentInstanceDispose(inst);
    for (UInt32 c = 0; c < nch; c++) free(chan[c]);
    free(chan);
    free(abl);

    printf(rc == 0 ? "=== 通过 ===\n" : "=== 失败 ===\n");
    return rc;
}

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

// 输入：1 kHz 正弦（幅度 0.2）。
// 注意：不能喂直流 —— 这个电路有 C1/C3 隔直，直流输入下输出本来就是 0。
static double g_phase = 0.0;

static OSStatus input_callback(void *inRefCon, AudioUnitRenderActionFlags *flags,
                               const AudioTimeStamp *ts, UInt32 bus,
                               UInt32 nframes, AudioBufferList *io) {
    (void)inRefCon;
    (void)flags;
    (void)ts;
    (void)bus;
    const double step = 2.0 * 3.14159265358979 * 1000.0 / 48000.0;
    for (UInt32 i = 0; i < nframes; i++) {
        float v = (float)(0.2 * sin(g_phase));
        g_phase += step;
        if (g_phase > 2.0 * 3.14159265358979) g_phase -= 2.0 * 3.14159265358979;
        for (UInt32 c = 0; c < io->mNumberBuffers; c++) {
            ((float *)io->mBuffers[c].mData)[i] = v;
        }
    }
    return noErr;
}


// 从 Mach-O 路径推出 .component 包路径：<bundle>/Contents/MacOS/<name>
static int bundle_path_from_macho(const char *macho, char *out, size_t outlen) {
    char tmp[4096];
    snprintf(tmp, sizeof(tmp), "%s", macho);
    for (int i = 0; i < 3; i++) {
        char *slash = strrchr(tmp, '/');
        if (!slash) return -1;
        *slash = '\0';
    }
    snprintf(out, outlen, "%s", tmp);
    return 0;
}

static void cfstr_to_c(CFStringRef s, char *buf, size_t n) {
    if (!s) { buf[0] = 0; return; }
    CFStringGetCString(s, buf, (CFIndex)n, kCFStringEncodingUTF8);
}

// 校验 bundle 的 Info.plist 里确实声明了我们要的那一个 AudioComponent。
// 这能抓住「AU 代码没问题、但 plist 漏了 AudioComponents」这类致命且难查的错误。
static int verify_plist_declaration(const char *bundle, const char *want_type,
                                    const char *want_sub, const char *want_manu,
                                    const char *want_factory) {
    CFURLRef url = CFURLCreateFromFileSystemRepresentation(
        NULL, (const UInt8 *)bundle, (CFIndex)strlen(bundle), true);
    if (!url) { printf("[FAIL] 无法构造 bundle URL\n"); return -1; }
    CFBundleRef b = CFBundleCreate(NULL, url);
    CFRelease(url);
    if (!b) { printf("[FAIL] CFBundleCreate 失败: %s\n", bundle); return -1; }

    CFTypeRef ac = CFBundleGetValueForInfoDictionaryKey(b, CFSTR("AudioComponents"));
    if (!ac || CFGetTypeID(ac) != CFArrayGetTypeID()) {
        printf("[FAIL] Info.plist 里没有 AudioComponents（系统将完全看不见这个 AU）\n");
        CFRelease(b);
        return -1;
    }
    int ok = 0;
    CFIndex n = CFArrayGetCount((CFArrayRef)ac);
    printf("[info] Info.plist 声明了 %ld 个 AudioComponent\n", (long)n);
    for (CFIndex i = 0; i < n; i++) {
        CFTypeRef e = CFArrayGetValueAtIndex((CFArrayRef)ac, i);
        if (!e || CFGetTypeID(e) != CFDictionaryGetTypeID()) continue;
        CFDictionaryRef d = (CFDictionaryRef)e;
        char t[16] = "", st[16] = "", mn[64] = "", fn[128] = "";
        cfstr_to_c(CFDictionaryGetValue(d, CFSTR("type")), t, sizeof(t));
        cfstr_to_c(CFDictionaryGetValue(d, CFSTR("subtype")), st, sizeof(st));
        cfstr_to_c(CFDictionaryGetValue(d, CFSTR("manufacturer")), mn, sizeof(mn));
        cfstr_to_c(CFDictionaryGetValue(d, CFSTR("factoryFunction")), fn, sizeof(fn));
        printf("        [%ld] %s/%s/%s factory=%s\n", (long)i, t, st, mn, fn);
        if (!strcmp(t, want_type) && !strcmp(st, want_sub) &&
            !strcmp(mn, want_manu) && !strcmp(fn, want_factory))
            ok = 1;
    }
    CFRelease(b);
    if (!ok) {
        printf("[FAIL] plist 声明与期望不符（期望 %s/%s/%s factory=%s）\n",
               want_type, want_sub, want_manu, want_factory);
        return -1;
    }
    printf("[ ok ] plist 中的 AudioComponent 声明正确\n");
    return 0;
}


// 打印 AU 的通道/总线配置 —— 决定宿主把它当"效果器"还是"乐器"
static void print_bus_info(AudioUnit inst) {
    AudioComponentDescription desc;
    AudioComponentGetDescription(AudioComponentInstanceGetComponent(inst), &desc);
    printf("[info] 注册的组件类型: %c%c%c%c / %c%c%c%c / %c%c%c%c\n",
           (int)(desc.componentType >> 24) & 0xff, (int)(desc.componentType >> 16) & 0xff,
           (int)(desc.componentType >> 8) & 0xff, (int)desc.componentType & 0xff,
           (int)(desc.componentSubType >> 24) & 0xff, (int)(desc.componentSubType >> 16) & 0xff,
           (int)(desc.componentSubType >> 8) & 0xff, (int)desc.componentSubType & 0xff,
           (int)(desc.componentManufacturer >> 24) & 0xff, (int)(desc.componentManufacturer >> 16) & 0xff,
           (int)(desc.componentManufacturer >> 8) & 0xff, (int)desc.componentManufacturer & 0xff);

    UInt32 n = 0, sz = sizeof(n);
    if (AudioUnitGetProperty(inst, kAudioUnitProperty_BusCount, kAudioUnitScope_Input, 0, &n, &sz) == noErr)
        printf("[info] 输入总线数: %u\n", (unsigned)n);
    sz = sizeof(n);
    if (AudioUnitGetProperty(inst, kAudioUnitProperty_BusCount, kAudioUnitScope_Output, 0, &n, &sz) == noErr)
        printf("[info] 输出总线数: %u\n", (unsigned)n);

    // 支持的通道组合
    AUChannelInfo info[32];
    UInt32 isz = sizeof(info);
    if (AudioUnitGetProperty(inst, kAudioUnitProperty_SupportedNumChannels,
                             kAudioUnitScope_Global, 0, info, &isz) == noErr) {
        UInt32 cnt = isz / sizeof(AUChannelInfo);
        printf("[info] SupportedNumChannels: %u 组\n", (unsigned)cnt);
        for (UInt32 i = 0; i < cnt && i < 8; i++)
            printf("        in=%d out=%d\n", info[i].inChannels, info[i].outChannels);
    } else {
        printf("[warn] 读不到 SupportedNumChannels\n");
    }
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

    // 先校验 bundle 声明（这一步能抓住「plist 缺 AudioComponents」的致命错误）
    char bundle[4096];
    if (bundle_path_from_macho(macho, bundle, sizeof(bundle)) == 0) {
        printf("  bundle: %s\n", bundle);
        if (verify_plist_declaration(bundle, argv[3], argv[4], argv[5], sym) != 0)
            return 5;
    } else {
        printf("[warn] 无法从路径推导 bundle，跳过 plist 校验\n");
    }

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
    print_bus_info(inst);

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

    // 乐器专属能力检查：效果器**不应**宣称这些（否则 Logic 归到软件乐器插槽）
    {
        struct { AudioUnitPropertyID id; const char *name; } props[] = {
            {kMusicDeviceProperty_InstrumentCount, "kMusicDeviceProperty_InstrumentCount"},
            {kMusicDeviceProperty_SupportsStartStopNote, "kMusicDeviceProperty_SupportsStartStopNote"},
        };
        for (unsigned i = 0; i < sizeof(props) / sizeof(props[0]); i++) {
            UInt32 v = 0, vsz = sizeof(v);
            OSStatus st = AudioUnitGetProperty(inst, props[i].id, kAudioUnitScope_Global, 0, &v, &vsz);
            if (st == noErr)
                printf("[warn] 宣称支持 %s → Logic 会把插件当乐器！\n", props[i].name);
            else
                printf("[ ok ] 不支持 %s（效果器应有行为）\n", props[i].name);
        }
    }

    // MIDI 能力检查：纯效果器**不应**宣称支持 MIDI 属性
    // （否则 Logic 会把它归类成 "MIDI 控制效果器"，只出现在软件乐器插槽）
    {
        SInt32 proto = 0;
        UInt32 psz = sizeof(proto);
        OSStatus st = AudioUnitGetProperty(inst, kAudioUnitProperty_AudioUnitMIDIProtocol,
                                           kAudioUnitScope_Global, 0, &proto, &psz);
        if (st == noErr) {
            printf("[warn] 宣称支持 MIDI (protocol=%d) → Logic 会归类为 MIDI 控制效果器\n", (int)proto);
        } else {
            printf("[ ok ] 未宣称支持 MIDI（纯音频效果器）→ Logic 会归到 Audio FX\n");
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

    double sum = 0.0, sq = 0.0;
    int bad = 0;
    for (UInt32 c = 0; c < nch; c++) {
        for (UInt32 i = 0; i < nframes; i++) {
            if (!isfinite(chan[c][i])) bad++;
            sum += chan[c][i];
            sq += (double)chan[c][i] * chan[c][i];
        }
    }
    double n = (double)(nframes * nch);
    double mean = sum / n;
    double rms = sqrt(sq / n);
    printf("[info] 渲染 %u 帧: 非有限值=%d, 输出均值=%.6f, RMS=%.6f\n",
           (unsigned)nframes, bad, mean, rms);

    int rc = 0;
    if (bad > 0) { printf("[FAIL] 输出含 NaN/Inf\n"); rc = 20; }
    else if (rms < 1e-4) { printf("[FAIL] 输出静音（音频通路可能断了）\n"); rc = 21; }
    else if (rms > 2.0) { printf("[FAIL] 输出电平异常\n"); rc = 22; }
    else printf("[ ok ] 音频通路正常（1kHz 正弦已通过电路）\n");

    AudioUnitUninitialize(inst);
    AudioComponentInstanceDispose(inst);
    for (UInt32 c = 0; c < nch; c++) free(chan[c]);
    free(chan);
    free(abl);

    printf(rc == 0 ? "=== 通过 ===\n" : "=== 失败 ===\n");
    return rc;
}

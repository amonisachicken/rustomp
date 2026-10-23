# 第三方补丁

`third_party/` 下的代码由 `scripts/fetch-third-party.sh` 拉取（不入库）。
这里存放必须改动的补丁，拉取后自动应用，保证可复现。

## 0001-auv2-advertise-midi-only-when-present.patch

**问题**：clap-wrapper 的 AUv2 外壳**无条件**对外宣称支持
`kAudioUnitProperty_AudioUnitMIDIProtocol` / `HostMIDIProtocol` /
`MIDIOutputCallback(Info)`，即使被托管的 CLAP 根本没有 note port。

**后果**：Logic 会把一个纯音频效果器标记成 `MIDIProtocol = 1`
（见 `~/Library/Preferences/com.apple.logic10.plist` 里的
`aufx-XXXX-YYYY` 缓存条目），于是把它归到 **"MIDI 控制效果器"**，
在 Logic 里就**只出现在软件乐器插槽，音频轨的 Audio FX 里找不到**。

对照：BYOD / Waves 等原生 AUv2 效果器的缓存条目都是 `MIDIProtocol = 0`。

**修复**：只在 `hasMIDI()`（有 MIDI 输入 note port 或有 MIDI 输出）时才返回这些属性，
否则返回 `kAudioUnitErr_InvalidProperty`。

**验证**：`tools/au-smoke` 会读 `kAudioUnitProperty_AudioUnitMIDIProtocol`，
修复后应返回 `kAudioUnitErr_InvalidProperty`。

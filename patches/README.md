# 第三方补丁

`third_party/` 下的代码由 `scripts/fetch-third-party.sh` 拉取（不入库）。
这里存放必须改动的补丁，拉取后自动应用，保证可复现。

## 0001-auv2-do-not-advertise-instrument-midi-only-capabilities.patch

**问题**：clap-wrapper 的 AUv2 外壳**无条件**对外宣称支持一批
「乐器 / MIDI 专属」属性，即使被托管的 CLAP 是纯音频效果器：

- `kMusicDeviceProperty_InstrumentCount`（MusicDevice 专属）
- `kMusicDeviceProperty_SupportsStartStopNote`
- `kMusicDeviceProperty_DualSchedulingMode`
- `kAudioUnitProperty_AudioUnitMIDIProtocol` / `HostMIDIProtocol`
- `kAudioUnitProperty_MIDIOutputCallback(Info)` / `MIDIOutputEventListCallback`

**后果**：Logic 会把一个纯音频效果器标记成 `MIDIProtocol = 1`
（见 `~/Library/Preferences/com.apple.logic10.plist` 里的
`aufx-XXXX-YYYY` 缓存条目），于是把它归到 **"MIDI 控制效果器"**，
在 Logic 里就**只出现在软件乐器插槽，音频轨的 Audio FX 里找不到**。

对照：BYOD / Waves 等原生 AUv2 效果器的缓存条目都是 `MIDIProtocol = 0`。

**修复**：

- `kMusicDeviceProperty_*` 只在 `_autype == aumu_musicdevice`（即 plist 里声明为乐器）时才宣称；
- MIDI 相关属性只在 `hasMIDI()`（有 MIDI note 输入端口或有 MIDI 输出）时才宣称；
- 否则一律返回 `kAudioUnitErr_InvalidProperty`。

**验证**：`tools/au-smoke` 会逐个查询上面这些属性，修复后全部返回
`kAudioUnitErr_InvalidProperty`：

```
[ ok ] 不支持 kMusicDeviceProperty_InstrumentCount（效果器应有行为）
[ ok ] 不支持 kMusicDeviceProperty_SupportsStartStopNote（效果器应有行为）
[ ok ] 未宣称支持 MIDI（纯音频效果器）→ Logic 会归到 Audio FX
```

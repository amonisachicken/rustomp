# ngspice 冒烟测试（oracle 基线）

用来确认「本机 ngspice 可用，且能产出我们需要的分析结果」。

```bash
# 1) 直接看 THD（Fourier 分析）
ngspice -b clipper_1n4148.cir | sed -n '/Fourier analysis/,/^ *9/p'

# 2) 产出 ASCII rawfile（oracle 驱动实际消费的格式）
printf 'set filetype=ascii\n' > .spiceinit
ngspice -b -r out.raw clipper_1n4148.cir
head -14 out.raw
```

## 基线数据（2026-10-23，ngspice-47）

电路：`Vin(1kHz, 1V) — Rs 10k — [C 47nF ∥ D 1N4148] — GND`

| 指标 | 值 |
|---|---|
| 输出基波幅度 | 0.319904 V |
| **THD** | **0.353802 %** |
| 2 次谐波（归一） | 0.00314395 |
| 3 次谐波（归一） | 0.00145341 |
| tran 数据点 | 20008（`.tran 1u 20m`） |
| ASCII rawfile 变量 | time, v(in), v(out), i(v1) |

1N4148 模型取自 `breadspice/backend/assets/models.lib`：
`.model 1N4148 D(Is=2.52n Rs=.568 N=1.752 Cjo=4p M=.4 tt=20n)`

→ 后续 WDF 实现的 THD/频响要与这组数字对得上（M1 验收标准：误差 < 0.5dB）。

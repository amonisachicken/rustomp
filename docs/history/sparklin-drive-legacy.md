# 早期示例插件 Sparklin Drive（归档）

> 归档：2026-10-29。原因：示例插件已独立为 **[sparklin-drive](https://github.com/amonisachicken/sparklin-drive)**
> （当前 v0.1.9，三平台 prerelease）；更早的手推实现冻结在 `sparklin-drive-legacy`。
> 当前验证数字见 [`../verification.md`](../verification.md)。

本仓早期把完整插件（电路 + 基线 + 界面 + 打包）放在 `plugins/` 下，后来拆出独立仓库。
下面保留当时的电路说明，作为"怎么用框架"的历史样例。

单电源 9V、OP07、**反馈环内非对称二极管削波**的失真/法兹：

```text
IN ─C1─► IN+ ─ OP07 ─ OUT ─C3─► LEVEL ─► 出
          │      │
   R5+C2 ─┴      └─► GAIN（可变电阻）
          │
         IN- ◄── 削波环：D1(1 只) ∥ (D2+D3 串联反向)
```

| 控件 | 功能 |
|---|---|
| GAIN / LEVEL | 线性电位器，0–100% |
| Clip Mode | 上 = 3 只二极管非对称（原机）；下 = 2 只对称（短接 D3） |
| 大开关 | true-bypass（15 样本延迟对齐 + 5 ms 交叉淡化，电路持续运行） |
| LED | 削波指示（`abs(u)` 阈值 + 150 ms 释放） |

AU 码：`aufx / SPm1 / SPs1 / HpEf`（Mono / Stereo）。

当时的验证数据（ngspice / MNA / legacy WDF 的 THD、频响、h2–h9）已整理到
[`../verification.md`](../verification.md)；AB 测试复盘见
[`ab-test-sparklin-v2.md`](ab-test-sparklin-v2.md)。


## 早期 fakedist v1 的整机对拍（电位器 50%）

第一版 Sparklin Drive 电路用"一维牛顿 + WDF"实现，已拆到 `sparklin-drive-legacy`：

| 项目 | WDF 实现 | ngspice | 误差 |
|---|---|---|---|
| 直流工作点 | 4.49999 V | 4.50000 V | 0.01 mV |
| 频响 50 Hz – 10 kHz | — | — | **≤0.31 dB** |
| THD @0.2 V（2–9 次） | 27.650 % | 27.616 % | +0.034 pp |
| h2–h9 | 逐次 ≤0.03 pp | | |

三条教训仍然有效，且已被现在的器件库 / MNA 继承：

1. **两只二极管串联时发射系数也要翻倍**（`i = Is·(exp(u/(2·N·Vt)) − 1)`），只翻 `Rs` 会让串联支路提前导通；
2. **不能用 ngspice 的 `.four` 当基准**（只取 1 个周期 + 线性插值，高次谐波偏低）——一律自己算 DFT；
3. **直接以 48 kHz 跑电路会失效**：1× 不可用；2× 起步，4× 默认更稳（8 kHz 处 2× 明显偏低）。

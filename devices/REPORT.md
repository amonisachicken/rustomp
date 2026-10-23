# 器件模型清洗报告

来源：`models.add`

解析到 **5** 个 `.MODEL`、**2** 个 LTspice 符号块。


## 分立器件

| 型号 | 类型 | 类别 | 保留参数 | 丢弃参数 | 建议档位 | ngspice |
|---|---|---|---|---|---|---|
| `AC127` | NPN | Ge BJT（锗） | is, isc, ise, ikf, itf, nc, ne, bf, br, rb, rc, vaf, var, cjc, cje, tr, tf, fc, eg, vjc, vje, vtf, mjc, mje, xtb, xtf, xti | vceo, icrating, mfg | D3 + C1（模型自带 CJC/CJE/TF/TR，可按需升 C2） | ✅ |
| `AC128` | PNP | Ge BJT（锗） | is, isc, ise, ikf, itf, nc, ne, bf, br, rb, rc, re, vaf, var, cjc, cje, tr, tf, fc, eg, vjc, vje, vtf, mjc, mje, xtb, xtf, xti | vceo, icrating, mfg | D3 + C1（同上） | ✅ |
| `1N34A` | D | 锗二极管 | is, rs, n, cjo, eg, bv, ibv | type | D2 + C0（模型含 CJO，可升 C1） | ✅ |
| `1N4007` | D | 硅二极管 | is, rs, n, eg, xti, bv, ibv, cjo, vj, m, fc, tt | mfg, type | D2 + C1（含 CJO/TT） | ✅ |
| `2N7000` | VDMOS | N 沟道 MOSFET | — | rg, vto, rd, rs, rb, kp, mtriode, cgdmax, cgdmin, cgs, cjo, is, mfg, vds, ron, qg | D2 + C1（需先解决 VDMOS 兼容） | ❌ 不支持 VDMOS |

## 只有符号、没有模型的器件

| 名称 | 引脚 | 引用的外部模型 | 状态 |
|---|---|---|---|
| `RC4558` | In+/In-/V+/V-/OUT | `..\sym\EXTRA\AudioOpamp\TI\RC4558.301` | **缺模型本体** |
| `TL072` | In+/In-/V+/V-/OUT | `..\sym\EXTRA\AudioOpamp\TI\TL072.301` | **缺模型本体** |

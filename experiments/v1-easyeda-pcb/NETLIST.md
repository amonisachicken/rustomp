# 网表：source.asc

> 由 `tools/netlist/easyeda_asc.py` 自动生成，**不要手改**（改源文件后重新生成）。

- 格式：EasyEDA Pro / PADS-PowerPCB ASCII (PCB export)
- 元件数：36　网数：21

## ⚠️ 必须人工确认的点

1. **引脚编号是 PCB 焊盘号**，不是符号引脚名。TO-92 的 `1/2/3` 到底对应 C/B/E 需要确认
   （本网表的拓扑看起来是 `1=C, 2=B, 3=E`，与 BC547 / BC559 的常规符号一致，但要核过才算数）。
2. 下面是自动发现的值冲突：

   - Q1: *PART* 里 Device=`BC547B`，但属性 Name=`BC549C` —— 以哪个为准？
   - C9: *PART* 里 Device=`10u`，但属性 Name=`100uF` —— 以哪个为准？
   - D1: *PART* 里 Device=`D_1N4007A`，但属性 Name=`Description` —— 以哪个为准？
   - C2: *PART* 里 Device=`4n7`，但属性 Name=`47nF` —— 以哪个为准？
   - C1: *PART* 里 Device=`4n7`，但属性 Name=`10nF` —— 以哪个为准？
   - C3: *PART* 里 Device=`4n7`，但属性 Name=`1uF` —— 以哪个为准？
   - D2: *PART* 里 Device=`D_1N4007A`，但属性 Name=`Description` —— 以哪个为准？
   - C4: *PART* 里 Device=`4n7`，但属性 Name=`1nF` —— 以哪个为准？
   - ADJST: *PART* 里 Device=`卧式/蓝白色`，但属性 Name=`1M` —— 以哪个为准？
   - Q2: *PART* 里 Device=`BC547B`，但属性 Name=`BC549C` —— 以哪个为准？
   - Q3: *PART* 里 Device=`BC547B`，但属性 Name=`BC549C` —— 以哪个为准？
   - Q4: *PART* 里 Device=`BC559CTA`，但属性 Name=`BC559C` —— 以哪个为准？

## BOM / 元件表

| RefDes | 值/型号 | 器件库 | 封装 | 引脚数 |
|---|---|---|---|---|
| ADJST | 1M | 卧式/蓝白色 | RES-ADJ-SMD_RM065-V1 | 3 |
| C1 | 10nF | 4n7 | HDR-M-2.54_1X2 | 2 |
| C2 | 47nF | 4n7 | HDR-M-2.54_1X2 | 2 |
| C3 | 1uF | 4n7 | HDR-M-2.54_1X2 | 2 |
| C4 | 1nF | 4n7 | HDR-M-2.54_1X2 | 2 |
| C9 | 100uF | 10u | CAP-D5.0×F2.0 | 2 |
| D1 | D_1N4007A | D_1N4007A | DO_1N4007 | 2 |
| D2 | D_1N4007A | D_1N4007A | DO_1N4007 | 2 |
| H1 | FUZZ | HDR-M_2.54_1x3P | POT_5.04 | 3 |
| H2 | LEVEL | HDR-M_2.54_1x3P | POT_5.04 | 3 |
| IG | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |
| IL | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |
| IR | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |
| LED1 | LED_TH-R_5mm | LED_TH-R_5mm | LED-TH_5mm | 2 |
| LED2 | LED_TH-R_5mm | LED_TH-R_5mm | LED-TH_5mm | 2 |
| OG | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |
| OR | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |
| Q1 | BC549C | BC547B | TO-92(TO-92-3)-2.54 | 3 |
| Q2 | BC549C | BC547B | TO-92(TO-92-3)-2.54 | 3 |
| Q3 | BC549C | BC547B | TO-92(TO-92-3)-2.54 | 3 |
| Q4 | BC559C | BC559CTA | TO-92-3_L4.9-W3.7-P2.54-L | 3 |
| R1 | LDR | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R2 | 10kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R4 | 4,7kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R5 | 10kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R6 | 10kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R7 | 10kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R8 | 10kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R9 | 4,7kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R10 | 4,7kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R11 | 10kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| R12 | 2,2kΩ | Res_AXIAL-1/4W | Res_1/4W | 2 |
| SWI | HDR-F_2.54_1x2P | HDR-F_2.54_1x2P | SW-2.54 | 2 |
| SWO | HDR-F_2.54_1x2P | HDR-F_2.54_1x2P | SW-2.54 | 2 |
| V+ | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |
| V- | HDR-M-2.54_1x1 | HDR-M-2.54_1x1 | HDR-M-2.54_1X1 | 1 |

## 网表（节点 → 引脚）

| 网名 | 连接的引脚 |
|---|---|
| `$1N3213` | `V-.1` , `IL.1` |
| `$1N3222` | `IR.1` , `SWI.1` |
| `$1N3249` | `R9.2` , `R10.1` , `C9.1` |
| `$1N3252` | `LED1.1` , `R9.1` |
| `$1N3254` | `V+.1` , `D1.2` |
| `$1N3264` | `Q1.2` , `C1.2` , `ADJST.2` |
| `$1N3265` | `C2.1` , `R4.2` |
| `$1N3269` | `Q1.3` , `R2.2` , `R5.1` |
| `$1N3272` | `C3.2` , `R6.2` |
| `$1N3277` | `H2.3` , `R6.1` , `D2.2` , `LED2.2` , `C4.2` |
| `$1N3281` | `Q2.1` , `R7.1` , `Q4.2` |
| `$1N3283` | `Q3.1` , `R8.1` |
| `$1N3289` | `Q2.3` , `Q3.3` , `R11.2` |
| `$1N3295` | `C1.1` , `SWI.2` |
| `FXOUT` | `H2.2` , `R1.1` , `SWO.2` |
| `GND` | `LED1.2` , `C9.2` , `IG.1` , `H2.1` , `OG.1` , `R2.1` , `R4.1` , `D2.1` , `LED2.1` , `C4.1` , `ADJST.1` , `R1.2` , `R11.1` , `R12.1` |
| `OPI+` | `R5.2` , `Q2.2` |
| `OPI-` | `H1.1` , `C2.2` , `Q3.2` |
| `OPO` | `H1.2` , `H1.3` , `C3.1` , `Q4.1` , `R12.2` |
| `OUT` | `OR.1` , `SWO.1` |
| `VCC` | `Q1.1` , `R10.2` , `D1.1` , `ADJST.3` , `R7.2` , `R8.2` , `Q4.3` |

## 器件视角（元件 → 每个引脚接到哪）

| RefDes | 连接 |
|---|---|
| ADJST | 1→`GND` , 2→`$1N3264` , 3→`VCC` |
| C1 | 1→`$1N3295` , 2→`$1N3264` |
| C2 | 1→`$1N3265` , 2→`OPI-` |
| C3 | 1→`OPO` , 2→`$1N3272` |
| C4 | 1→`GND` , 2→`$1N3277` |
| C9 | 1→`$1N3249` , 2→`GND` |
| D1 | 1→`VCC` , 2→`$1N3254` |
| D2 | 1→`GND` , 2→`$1N3277` |
| H1 | 1→`OPI-` , 2→`OPO` , 3→`OPO` |
| H2 | 1→`GND` , 2→`FXOUT` , 3→`$1N3277` |
| IG | 1→`GND` |
| IL | 1→`$1N3213` |
| IR | 1→`$1N3222` |
| LED1 | 1→`$1N3252` , 2→`GND` |
| LED2 | 1→`GND` , 2→`$1N3277` |
| OG | 1→`GND` |
| OR | 1→`OUT` |
| Q1 | 1→`VCC` , 2→`$1N3264` , 3→`$1N3269` |
| Q2 | 1→`$1N3281` , 2→`OPI+` , 3→`$1N3289` |
| Q3 | 1→`$1N3283` , 2→`OPI-` , 3→`$1N3289` |
| Q4 | 1→`OPO` , 2→`$1N3281` , 3→`VCC` |
| R1 | 1→`FXOUT` , 2→`GND` |
| R2 | 1→`GND` , 2→`$1N3269` |
| R4 | 1→`GND` , 2→`$1N3265` |
| R5 | 1→`$1N3269` , 2→`OPI+` |
| R6 | 1→`$1N3277` , 2→`$1N3272` |
| R7 | 1→`$1N3281` , 2→`VCC` |
| R8 | 1→`$1N3283` , 2→`VCC` |
| R9 | 1→`$1N3252` , 2→`$1N3249` |
| R10 | 1→`$1N3249` , 2→`VCC` |
| R11 | 1→`GND` , 2→`$1N3289` |
| R12 | 1→`GND` , 2→`OPO` |
| SWI | 1→`$1N3222` , 2→`$1N3295` |
| SWO | 1→`OUT` , 2→`FXOUT` |
| V+ | 1→`$1N3254` |
| V- | 1→`$1N3213` |

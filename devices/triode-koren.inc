* =====================================================================
* TRIODE —— Koren 三极管本体（ngspice 版）
*
* 为什么需要：tubes.txt 里 4 个 .SUBCKT（12AX7/12AT7/12AY7/12AU7）只是**调用层**，
* 它们写的是 `X1 P G C TRIODE MU=… EX=… KG1=… KP=… KVB=… VCT=…`，
* 而 `TRIODE` 本体不在文件里 → ngspice 报 `unknown subckt: x1.x1 … triode`。
*
* 本体方程 = **Koren 1996（Tube.lib）的三极管式**，只多一个可选的 VCT
* （VCT 是 Koren 1997-12 修订版引入的栅偏置偏移；VCT=0 时与 1996 完全一致）：
*
*   E1 = (Va/KP)·log( 1 + exp( KP·(1/MU + (Vg + VCT)/sqrt(KVB + Va²)) ) )
*   Ia = 2·E1^EX / KG1                       （E1 > 0；等价于 (PWR+PWRS)/KG1）
*
* ⚠️ 常见误解：`ATAN(Va/KVB)` 的"屏压膝"**不是三极管本体**，
*    它只出现在 Koren 的五极管模型（EL34/6550 的帘栅支路）里。
*    给三极管加 ATAN 会把 12AX7 的静态电流压低 2.2 倍（0.53 mA vs 1.2 mA）。
*
* 参数来源与差异（Va=250 V，实测本文件 + tubes.txt）：
*   型号   手册 Ia    Koren96 Tube.lib        Koren97 Tube97.lib      tubes.txt（拟合值）
*   12AX7  1.2 mA     0.95 mA (MU100/KP600)   0.87 mA (MU107.5/KP549) 1.21 mA (MU100.26/KP17950)
*   12AT7  10  mA     12.84 mA (MU60/KP300)   —（97 未改）            10.77 mA (MU72.8/KP233)
*   12AU7  10.5 mA    10.39 mA (MU21.5/KP84)  10.03 mA (MU22.98/KP80) 10.95 mA (MU19.41/KP105)
*   12AY7  ~3 mA      **Koren 库里没有**      **没有**                7.12 mA (MU44.16/KP410)
*
*   → `tubes.txt` 是"Koren 1996 公式 + VCT"的**重新拟合值**（社区拟合，含 12AY7），
*     与官方库同源但数值不同；两组参数都配本文件用（公式相同）。
*
* ngspice 注意：没有 `pwrs`（PSpice 专有），`pwr(x,y)` = |x|^y，
* 所以 `PWR+PWRS` 用三目 `x>0 ? 2*pwr(x,EX) : 0` 展开。
* =====================================================================
.MODEL DKOREN D(IS=1N RS=1 CJO=10PF TT=1N)

.SUBCKT TRIODE P G C PARAMS: MU=100 EX=1.4 KG1=1060 KP=600 KVB=300 VCT=0
+ RGI=2000 CCG=0 CGP=0 CCP=0
E1  7 0 VALUE = { (V(P,C)/KP)*log(1+exp(KP*(1/MU + (V(G,C)+VCT)/sqrt(KVB + V(P,C)*V(P,C))))) }
RE1 7 0 1G
G1  P C VALUE = { V(7) > 0 ? 2*pwr(V(7),EX)/KG1 : 0 }
RCP P C 1G
C1  G C {CCG}
C2  G P {CGP}
C3  P C {CCP}
R1  G 5 {RGI}
D3  5 C DKOREN
.ENDS TRIODE

* ==== Koren 12/97 本体（Tube97.lib 的 12AX7 / 12AU7 / 6SN7 用这个）====
*   E1 = Va/KP·log(1+exp(KP·(1/MU + (Vg+VCT)/sqrt(KVB²+Va²))))
*   Ia = 2·(E1/KG1)^EX            （E1>0）
.SUBCKT TRIODE97 P G C PARAMS: MU=100 EX=1.4 KG1=1060 KP=600 KVB=300 VCT=0
+ RGI=2000 CCG=0 CGP=0 CCP=0
E1  7 0 VALUE = { (V(P,C)/KP)*log(1+exp(KP*(1/MU + (V(G,C)+VCT)/sqrt(KVB*KVB + V(P,C)*V(P,C))))) }
RE1 7 0 1G
G1  P C VALUE = { V(7)/KG1 > 0 ? 2*pwr(V(7)/KG1,EX) : 0 }
RCP P C 1G
C1  G C {CCG}
C2  G P {CGP}
C3  P C {CCP}
R1  G 5 {RGI}
D3  5 C DKOREN
.ENDS TRIODE97

# 电路导入与元件映射规范

> 状态：**S1a/S1b/S1c 已实现**（2026-10-26）—— `SpiceNetlist` 导入器 + 闭集映射 +
> 网表→装配代码生成 + 多分支（开关=两份完整电路），并已在真插件上跑通（Sparklin Drive v2）。
> 映射分层里目前只落地了「精确命中 / 报错」两级，别名、项目映射、类别兜底、参数直通仍是待做项（§3）。
> **2026-10-27：首选设计器改为 `rustomp-circuit-designer`** —— 它的工程文件 `.design.json`
> 可以直接导入（`CircuitDesigner` 导入器），breadspice 的 `.bread` 退为次选（§1/§2/§10）。
> 相关：`DEVLOG.md` E32 / ADR-013、E44 / E45；`docs/component-library.md`、`docs/declarative-build.md`
> 上游：用户提供的 LTspice 风格器件模型；**rustomp-circuit-designer**（首选，见 §10）；`plugins/` 下的下游插件项目

---

## 1. 导入模块的可插拔性

`crates/rustomp-import` 已具备插件化结构：

```rust
pub trait Importer {
    fn id(&self) -> &'static str;                            // 例如 "circuit-designer"
    fn matches(&self, path: &Path, content: &str) -> bool;   // 扩展名 + 内容嗅探
    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError>;
}

pub fn registry() -> Vec<Box<dyn Importer>> {   // 顺序即优先级
    vec![
        Box::new(CircuitDesigner),   // 首选设计器的工程文件（.design.json）
        Box::new(BreadSpice),        // 次选：面包板工程（.bread）
        Box::new(SpiceNetlist),      // 兜底：任何 SPICE 网表（.cir/.net/.sp）
    ]
}
```

新增一种格式 = **实现一个结构体 + 在 `registry()` 加一行**。框架其余部分不需要改动。

现成的三个实现：

| 导入器 | 处理 | 状态 |
|---|---|---|
| **`CircuitDesigner`** | **`.design.json`**（rustomp-circuit-designer 工程）—— 首选 | ✅（2026-10-27） |
| `BreadSpice` | `.bread`（面包板）—— 次选 | ✅ |
| `SpiceNetlist` | `.cir` / `.net` / `.sp` / `.spice`（含内容嗅探）—— 兜底 | ✅（E44） |

> `CircuitDesigner` 的**引脚几何不在本仓**：它运行时读设计器仓库的 `symbols/symbols.json`
> （`RUSTOM_DESIGNER_SYMBOLS=<path>` 可指定；默认从工程文件向上找、再看当前目录、
> 最后找同级的 `../rustomp-circuit-designer/`）。找不到就**报错并列出找过的路径**，不猜。
> 这样“同一个量只有一个来源”——设计器改了符号几何，本仓不需要跟着改代码。

### 1.1 每种导入器必须产出什么

统一的中间表示（IR）：**元件（类别 + 节点 + 值 + 型号）+ 接地网 + 提示**。
导入器**不做**以下事情：

- 不做电路求解（那是 `rustomp-dsp`）；
- 不写 DSP 代码（那是插件项目的事）；
- **不决定元件映射的最终结果**（映射是独立的一层，见 §3）。

---

## 2. 格式优先级：先网表，后原理图

| 格式 | 本质 | 工作量 | 说明 |
|---|---|---|---|
| **`CircuitDesigner`**（`.design.json`） | 首选设计器（rustomp-circuit-designer）的**工程文件**：元件 + 导线（mm 坐标）+ 符号名 | 已完成（2026-10-27） | 几何规则照搬设计器（引脚变换 / T 型接点 / 接网络的最小坐标命名）；引脚几何**运行时读设计器的 `symbols.json`**（唯一事实来源）；与本仓对拍：读工程文件得到的网表必须与设计器导出的 `.cir` **逐行相同** |
| **`SpiceNetlist`**（`.net` / `.cir` / `.sp`） | 网表：一行一个器件 + 节点 | 小 | ✅ **已实现**（E44）。覆盖 LTspice（*View → SPICE Netlist*）、KiCad、ngspice、Qucs 等导出；设计器的导出物也是这个格式 |
| `LtspiceAsc`（`.asc`） | 原理图：SYMBOL / WIRE / FLAG + 坐标 | 大 | 连通性要靠**导线坐标重合**推断，还要处理旋转/镜像与 `.lib` 引用；**优先级低**（已有首选设计器） |
| `BreadSpice`（`.bread`） | 面包板：元件 + 插孔 id | 已完成 | 次选：几何规则归并 net；旧工程继续可用 |
| 其它（EasyEDA / KiCad .kicad_sch / …） | — | 各自 | 按同一 trait 接入 |

> 结论：**首选设计器直接交工程文件**（`.design.json`），其余一概可先变成 `.cir` 再进来 ——
> 网表层永远是兜底通道。

---

## 3. 元件映射：分层流水线

外部电路可能写任意型号（`2N5088`、`1N34A`、`MMBT4148`），而库内是**闭集**。
解析按下面的顺序执行，**第一个命中即停**：

| 级 | 手段 | 结果标记 | 说明 |
|---|---|---|---|
| **1** | 精确命中库内型号 | `Exact` | `1N4148` → `1N4148` |
| **2** | 归一化 + 别名 | `Alias` | 去大小写/包装前缀后缀：`MMBT4148`、`1N4148W`、`D_1N4148` → `1N4148` |
| **3** | 项目级 `mapping.toml` | `Project` | 项目自己指定替换，如 `"2N5088" = "BC549C"` |
| **4** | **类别默认（防呆兜底）** | `ClassDefault` | 只知道类别时用该类默认型号，**必须写进 notes** |
| **5** | 参数直通（**power user 档**） | `Params` | 用外部 `.model` 参数构造器件实例，**仅限实验，不进发布路径** |
| **6** | 仍失败 | `Unresolved` | 按 `[strict]` 决定：`error` / 仅警告 / 用默认 |

> **当前实现**：只落地了 **1（精确命中）** 与 **6（报错）**。
> 未命中时会打印"不在闭集表里"+ 列出网表里出现的所有型号 + 已有型号清单（`mna_codegen::unknown_model_error`）。
> 2/3/4/5 级（别名、项目 `mapping.toml`、类别兜底、参数直通）**尚未实现** ——
> 在实现之前，未命中一律**硬失败**，不会静默替换（这比"防呆兜底做错"安全）。

对应的策略选择（用户已确认）：

- **B（别名）+ E（项目映射）** 应对绝大多数情况；
- **C（类别默认）** 作为**防呆兜底**，保证导入不中断，但**替换必须可见**；
- **D（参数直通）** 保留开发接口给 power user，默认关闭。

### 3.1 必须坚持：替换可见、可测

导入时把 `外部型号 → 库内型号 → 原因` 写进 `notes`，并由 `report()` 打印：

```
Q1  2N5088  SiNpn   → BC549C   (类别默认：hFE 略低)
D1  1N34A   GeDiode → GeDiode_1N34A  (项目映射)
D2  1N4007  SiDiode → 1N4007  (精确命中)
R3  10k     Resistor→ (无型号)
```

**oracle 基线必须与映射结果来自同一份配置**，否则会出现"为什么和原图不一样"却无法排查的局面。

---

## 4. 器件类别（映射的第一层依据）

```rust
pub enum DeviceClass {
    SiNpn, SiPnp, GeNpn, GePnp,
    SiDiode, GeDiode, Schottky, Led,
    NJfet, PJfet, NMos, PMos,
    OpAmp,
    Resistor, Capacitor, Inductor, VoltageSource, Ground, Other,
}
```

类别决定两件事：

1. 映射的兜底选择（每个类别一个默认型号）；
2. **引脚命名规则**（见 §5）。

---

## 5. 引脚绑定：按**引脚名**，不按位置

沿用当前规则（breadspice 导入器已在用）：

| 器件 | 规范的引脚名（顺序即 SPICE 节点顺序） |
|---|---|
| 电阻 / 电容 / 电感 | `1` `2` |
| 二极管 / LED | `a`（阳极）`k`（阴极） |
| BJT（NPN/PNP） | `C` `B` `E` |
| JFET / MOSFET | `D` `G` `S` |
| 运放 | `IN+` `IN-` `V+` `V-` `OUT` |
| 电压源 | `+` `-` |
| 电位器 | `1` `2`（抽头）`3` |

外部引脚名不一致时，在导入器的别名表里补映射（例如某些库用 `anode/cathode`、`b/c/e`、`inp/inn`）。
**任何按位置猜引脚的行为都不允许** —— 这是最容易被静默搞错的地方。

---

## 6. 项目级映射文件（`<project>/mapping.toml`）

```toml
# 显式替换（优先级高于别名，低于精确命中）
[aliases]
"2N5088" = "BC549C"
"1N34A"  = "GeDiode_1N34A"
"OA90"   = "GeDiode_1N34A"

# 每个类别的兜底型号
[defaults]
SiNpn   = "BC549C"
SiPnp   = "BC559C"
GeNpn   = "GeBjt_AC128"
GePnp   = "GeBjt_AC128_P"
SiDiode = "1N4148"
GeDiode = "GeDiode_1N34A"
NJfet   = "J201"
NMos    = "2N7000"
OpAmp   = "OP07"

# 未解析时的行为
[strict]
unmapped = "warn"      # error | warn | class-default
params_passthrough = false   # power user 档，默认关闭
```

放在插件项目里（随项目走、可版本化），由 `rustomp-import` 在导入时读取。

---

## 6.5 网表 → 装配代码（S1a/S1b/S1c，已实现）

导入的终点不是"生成一份 SPICE"，而是**生成能跑的 Rust 电路**：

```bash
# 首选：直接吃设计器的工程文件（2026-10-27 起）
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- <工程.design.json> --mna <输出.rs> [--check] [--input 名] [--output-node N]

# 通用：任何 SPICE 网表（含设计器导出的 .cir）
cargo run -q -p rustomp-import -- <网表.cir> --mna <输出.rs> [--check] [--input 名] [--output-node N]
```

网表里用注释写指令（导入器把 `* @rustomp …` 收进 `notes`，由 codegen 解析）：

```text
* @rustomp in=VS1 out=n_t27R
* @rustomp param RP1A gain pot max=500k              # R = max·gain
* @rustomp param RP2B level pot max=500k invert      # R = max·(1−level)
* @rustomp switch symmetric n_t10L n_t5L closed=0.001 open=1e12   # legacy，见下
* @rustomp newton=8                                 # 每样本牛顿迭代次数**上限**（默认 3，残差够了就提前停）
* @rustomp activity_i=50u                           # LED 活动量的参考电流（**只驱动 GUI**，不影响 DSP）
```

**`in=` 可以是元件名，也可以是节点名**（两者都有真实用户）：

| 写法 | 谁在用 | 含义 |
|---|---|---|
| `in=VS1` | breadspice 时代的网表 | 那个电压源的正端节点就是输入 |
| `in=IN` | **rustomp-circuit-designer** | 网表是**纯拓扑**（激励不画进网表），`IN` 节点由 `process(x)` 每拍写入 |

> 早期实现只认元件名，于是设计器的图会退化成 "无输入端口（纯直流电路）" ——
> 生成的 `Ckt` **直接忽略输入**（`process(x)` 里 `let _ = x;`）。这是一个沉默错误：
> 编译、运行、都不报错，只是没声音。现在两种都认，且在 notes 里明确写出结论。

**`param` 的元素名必须是网表里的元件名**（`RP1A`/`RP1B`），不是位号（`P1A`）。
设计器 2026-10-27 之前写的是位号，本仓为了不断旧网表，加了一层兼容 ——
找不到 `P1A` 时会去试 `RP1A` 并在 notes 里提醒。

> **`switch` 已是 legacy**（设计器决策 D-005）：开关改成"每个状态一份**完整网表**"，
> 由插件层的 `rustomp_dsp::Branch<A,B>` 选择 + 预热 5 ms / 淡化 10 ms。Sparklin Drive 现在是
> `sparklin_drive_asym.cir` + `sparklin_drive_sym.cir` 两份完整网表（对称那份用一根 ≤0.01 Ω 导线短接 D3）。
>
> **`newton=` 不是性能旋钮，是正确性旋钮**（DEVLOG E50）：次数不够时求解器会进入
> 次谐波极限环 —— 正弦输入完全看不出来，方波/快速边沿才会暴露（低频"噗噗"、高频发毛）。
>
> 它现在是**常规预算**而不是硬天花板：某拍确实没收敛时会继续迭代，最多 `4×预算`（≤24），
> 否则会把一个离谱的解留给下一拍。锗管（AC127，`Is≈5µA`）正是靠这条才跑得动（DEVLOG E81）。

产出（入库，`// @generated` 头）：

| 内容 | 说明 |
|---|---|
| `pub const N` / `pub mod node` | 未知量个数、节点编号（地 = 0，其余按名字排序 → diff 稳定） |
| `pub fn build(fs, &Params) -> Netlist<N>` | 装配函数 |
| `pub struct Params` + `Default` | 电位器（`f64`，默认 0.5）、开关（`bool`，默认 `false`） |
| `pub struct Ckt` + `impl rustomp_dsp::Circuit` | **直接能塞进 `Engine`**；`set_params` 只改电阻值 |
| `pub const IDX_*` / `FIXED_IN` / `IN` / `OUT` | 参数元件下标与端口 |

codegen 顺手做的拓扑处理（都在 `mna_codegen.rs` 里，**有测试**）：

1. **导线合并**：`≤ 0.01 Ω` 视为理想导线（breadspice 的连线是 `0.001 Ω`），并查集并节点；
2. **自环跳过**：电位器"短接的那一半"（`RP1B a a 250k`）对电路无影响；
3. **激励分类**：纯 DC 源 → 固定电位节点（不占未知量）；`SIN/AC/PULSE` 源 → 输入端口；
   显式 `in=<节点>` 时把该节点当成输入端口（标为固定电位，排在未知量之后）；
4. **闭集型号映射**：`1N4148/1N34A/1N4007/1N5817/LedRed/LedGreen/LedBLUE/BC549C/BC559C/AC127/AC128/J201/2N7000/OP07A`。

### 6.5.1 漂移守卫

生成物必须与"现在重新生成"逐字节一致，否则测试失败并打印重生成命令：

```
crates/rustomp-import/tests/codegen.rs::all_generated_files_are_up_to_date
crates/rustomp-import/tests/codegen.rs::designer_project_is_up_to_date   ← 设计器工程 → 生成物
crates/rustomp-import/tests/designer.rs::与设计器导出的网表逐行一致
```

配套 `--check`（只比对不写），可直接进 CI。

> 生成物**必须做行为级验证**（DC 工作点 + THD），光比文本不够 ——
> E44/E45 里各有一个"编译通过、不报错、结果却是错的"的例子。

## 7. IR 需要的扩展

```rust
pub struct ImportedElement {
    pub name: String,
    pub class: DeviceClass,
    pub nodes: Vec<String>,              // 已按 §5 的引脚名排序
    pub value: String,                   // 10k / 47n / 9
    pub source_model: Option<String>,    // 外部文件里的型号
    pub resolved: Resolved,              // 映射结果
}

pub enum Resolved {
    Exact(&'static str),
    Alias { to: &'static str, via: String },
    Project(&'static str),
    ClassDefault { to: &'static str, reason: String },
    Params(DeviceParams),          // power user 档
    Unresolved,
}
```

`ImportedCircuit::to_spice()` 用**解析后的型号**写模型名；`to_skeleton()` 用解析后的 Rust `const` 名。

---

## 8. 待定项

| 项 | 说明 |
|---|---|
| 锗器件型号与参数 | 等用户提供 LTspice 模型；缺参数时用实测 V-I 拟合 |
| 别名表的维护主体 | 建议：通用别名进框架（`rustomp-import/aliases.toml`），项目特例进项目 `mapping.toml` |
| `params_passthrough` 的产物形态 | 生成 `DeviceParams` 数据 + 运行期构造器件；不生成代码，避免测试矩阵爆炸 |

---

## 10. 与外部设计器的接口（**首选设计器已就位**，2026-10-27）

### 10.1 首选：`rustomp-circuit-designer`

本仓生态里原理图就由它画。它的**工程文件 `.design.json` 直接进 `rustomp-import`**：

```
 rustomp-circuit-designer            rustomp-import                插件
 ────────────────────────            ──────────────                ────
 画图 → 保存 .design.json  ───────►  CircuitDesigner 导入器
                                     （读设计器的 symbols.json）
                                          │
                                     mna_codegen
                                          ▼
                                    src/generated/circuit.rs
                                          │
                                          ▼
                                    Engine<Ckt> 出声

（导出物 `.cir` 也仍然可用：走 SpiceNetlist 导入器，结果与本路**逐行相同**）
```

契约要点（两边同时记档：本文件 ↔ 设计器 `docs/05-netlist-contract.md`）：

| 项 | 约定 |
|---|---|
| 端口 | `* @rustomp in=<节点> out=<节点>`（**节点名**，不是元件名） |
| 电位器 | `* @rustomp param RP1A <字段> pot max=<值> [invert]`（**网表元件名**，不是位号 `P1A`） |
| 网表形态 | 一张图一份 `.cir`（开关不进网表，D-007）；导出的网表是**纯拓扑**，无激励源 |
| 型号 | 落在闭集表里，否则导入即报错（不静默替换） |
| 几何 | 引脚坐标以设计器的 `symbols/symbols.json` 为准，本仓不复制一份 |
| 漂移守卫 | `tests/designer.rs`：工程文件 → 网表必须与设计器自己导出的 `.cir` **逐行相同** |

### 10.2 次选：breadspice（`.bread`）

面包板工程仍可导入（`BreadSpice` 导入器），但不再推荐作为“画原理图”的默认路径：
它的产物是面包板布局，跨项目复用/评审都不如一张原理图清楚。

### 10.3 插件 GUI 设计器

输出 `rustomp.toml` 的 `[ui]` 段（`rustomp_gui::PanelSpec`）+ 素材 PNG：
设计器 → `rustomp.toml` → `build.rs` 生成面板。

### 10.4 三条硬要求（对任何设计器）

- **端口必须显式标注**（`in=` / `out=`）：我们不猜"哪个节点是输出"；
- **可调元件必须显式声明**（`param`）：我们不从"250k+250k"里反推电位器；
- 型号落在**闭集表**里；不在表里会在导入时就报错，不会悄悄替换。

这三条是刻意的：**生成的电路会直接进插件发布路径**，任何靠猜的默认值都可能变成噪音。

## 9. 变更记录

| 日期 | 内容 |
|---|---|
| 2026-10-23 | 建立本文档：确认可插拔导入器、格式优先级（先网表后 `.asc`）、映射分层流水线（B+E 常规、C 防呆、D 为 power user）、引脚按名绑定 |
| 2026-10-23 | **S1a/S1b 落地**：`SpiceNetlist` 导入器、闭集型号映射（未命中硬失败）、`* @rustomp` 指令（端口/电位器/开关）、网表 → `Netlist`+`Params`+`impl Circuit` 的代码生成、漂移守卫；映射分层只做 1+6 级；新增 §10 外部设计器接口 |
| 2026-10-27 | **首选设计器切换**：新增 `CircuitDesigner` 导入器（`.design.json` 直读，几何规则与设计器一致，引脚几何读设计器的 `symbols.json`）；`registry()` 优先级 = 工程文件 > `.bread` > `.cir`；`in=` 支持**节点名**（设计器写法）并修掉“无输入端口”退化；`param` 元素名修正为网表元件名（`RP1A`）；新增 `--input`；新增对拍测试 `tests/designer.rs` 与漂移守卫 `designer_project_is_up_to_date`；§2/§6.5/§10 重写 |

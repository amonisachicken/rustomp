# 电路导入与元件映射规范

> 状态：**原则已定，待实现**（2026-10-23）
> 相关：`DEVLOG.md` E32 / ADR-013；`docs/component-library.md`
> 上游：用户提供的 LTspice 风格器件模型；`plugins/` 下的下游插件项目

---

## 1. 导入模块的可插拔性

`crates/rustomp-import` 已具备插件化结构：

```rust
pub trait Importer {
    fn id(&self) -> &'static str;                            // 例如 "breadspice"
    fn matches(&self, path: &Path, content: &str) -> bool;   // 扩展名 + 内容嗅探
    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError>;
}

pub fn registry() -> &'static [&'static dyn Importer] { &[&BreadSpice] }
```

新增一种格式 = **实现一个结构体 + 在 `registry()` 加一行**。框架其余部分不需要改动。

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
| **`SpiceNetlist`**（`.net` / `.cir` / `.sp`） | 网表：一行一个器件 + 节点 | 小 | **优先实现**。覆盖 LTspice（*View → SPICE Netlist*）、KiCad、ngspice、Qucs 等导出 |
| `LtspiceAsc`（`.asc`） | 原理图：SYMBOL / WIRE / FLAG + 坐标 | 大 | 连通性要靠**导线坐标重合**推断，还要处理旋转/镜像与 `.lib` 引用；作为后续便利格式 |
| `BreadSpice`（`.bread`） | 面包板：元件 + 插孔 id | 已完成 | 几何规则归并 net |
| 其它（EasyEDA / KiCad .kicad_sch / …） | — | 各自 | 按同一 trait 接入 |

> 结论：**先把 `SpiceNetlist` 做掉**，它覆盖的软件最多、歧义最少。

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

## 9. 变更记录

| 日期 | 内容 |
|---|---|
| 2026-10-23 | 建立本文档：确认可插拔导入器、格式优先级（先网表后 `.asc`）、映射分层流水线（B+E 常规、C 防呆、D 为 power user）、引脚按名绑定 |

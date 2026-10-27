# rustomp-import

**外部电路导入 + 网表 → 可运行电路代码的生成器。**

```text
 .design.json ─┐   （rustomp-circuit-designer，首选）
 .bread       ─┼─► Importer ─► ImportedCircuit（IR）─┬─► to_spice()      ngspice 网表（喂给 oracle 对拍）
 .cir 网表     ─┘                                    ├─► to_json()       给其它工具消费
 其它格式                                            └─► mna_codegen     Rust 装配代码（可直接进插件）
```

设计原则：**导入器只负责"读"，语义（型号映射、端口、参数）交给 codegen。**
这样同一份 IR 既能回写 SPICE、又能生成代码，还能被设计器/工具复用。

---

## 1. 导入器（`registry()` 顺序即优先级）

| 导入器 | 格式 | 说明 |
|---|---|---|
| **`CircuitDesigner`** | `.design.json` | **首选**：rustomp-circuit-designer 的工程文件。几何规则（引脚变换 / T 型接点 / 接网络的最小坐标命名）与该设计器一致；引脚几何**运行时读它的 `symbols.json`**（不复制一份）；产物与本路径对拍：`tests/designer.rs` |
| `BreadSpice` | `.bread` | 次选：面包板工程；按插孔 id 归并 net |
| `SpiceNetlist` | `.cir` / `.net` / `.sp` / `.spice` | 兜底：SPICE 网表；含内容嗅探。支持 `R C L D V I Q J M X`、续行 `+`、`.subckt` 整段跳过（设计器导出的 `.cir` 也走这里） |

> `CircuitDesigner` 需要设计器的符号表：`RUSTOM_DESIGNER_SYMBOLS=<设计器>/symbols/symbols.json`，
> 或把设计器仓库放在本仓同级（`../rustomp-circuit-designer/`）。找不到会报错并列出找过的路径。

新增格式 = 实现一个 `Importer` + 在 `registry()` 加一行。

```rust
pub trait Importer {
    fn id(&self) -> &'static str;
    fn matches(&self, path: &Path, content: &str) -> bool;
    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError>;
}
```

---

## 2. 工程/网表 → MNA 装配代码（S1a/S1b）

```bash
# 首选：直接吃 rustomp-circuit-designer 的工程文件（不用先导出 .cir）
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- <工程.design.json> --mna <输出.rs> [--check]

# 通用：任何 SPICE 网表
cargo run -q -p rustomp-import -- <网表.cir> --mna <输出.rs> [--check] [--input 名] [--output-node N]
```

两条路的产物**逐行相同**（`tests/designer.rs` 用设计器自己导出的 `.cir` 做裁判）。

### 2.1 网表里的指令（写在注释里，ngspice 会忽略）

```text
* @rustomp in=VS1 out=n_t27R                        # in= 可以写元件名，也可以写节点名（设计器用后者：in=IN）
* @rustomp param RP1A gain pot max=500k              # R = max·gain（元素名 = 网表元件名，不是位号）
* @rustomp param RP2B level pot max=500k invert      # R = max·(1−level)
* @rustomp switch symmetric n_t10L n_t5L closed=0.001 open=1e12
* @rustomp newton=4                                  # 每样本牛顿迭代次数（默认 3）
```

- `in=` / `out=`：**必须显式给**，我们不猜哪个节点是输出；
  `in=<节点名>`（如设计器的 `in=IN`）表示该节点是**纯输入端口**，
  没有激励源，由 `process(x)` 每拍写入；
- `param`：电位器 → 生成 `Params.gain: f64`（默认 0.5）；
- `switch`：开关 → 生成 `Params.symmetric: bool`（默认 `false`），实现方式是"新建一只可切换电阻"。
- `newton=`：实时路径每样本的牛顿迭代**常规预算**。**这是正确性参数，不是性能参数** ——
  2 次在方波/快速边沿下会欠收敛、产生次谐波极限环（DEVLOG E50）。默认 3，有振荡器类
  方波输入的电路建议 4。
  预算不是硬天花板：某个样本确实没收敛时允许继续，最多 `4×预算`（且 ≤24）——
  锗管（`Is≈5µA`，结斜率 ~0.7 S）的陡沿需要更多次，硬停在预算上会把烂解留给下一拍
  （DEVLOG E81）。稳态仍是 2~3 次，成本不变。

### 2.1b 指令补充（2026-10-26）

```text
* @rustomp newton=8          # 每样本牛顿迭代**上限**（默认 3；残差够了提前退出）
* @rustomp activity_i=50u    # LED 活动量的参考电流（默认 50µA，只驱动 GUI，不影响 DSP）
```

`switch` 指令保留但已是 **legacy**（D-005）：开关拆成多份完整网表，由 `Branch<A,B>` 选择
（Sparklin Drive 现在就是 asym / sym 两份）。生成的 `impl Circuit` 多了 `activity()`：

```rust
pub fn activity(&self) -> f32 {
    // 网表内所有二极管正向电流的最大值 / ACTIVITY_I_REF，clamp 到 0..1
}
```

### 2.2 生成的产物

| 内容 | 说明 |
|---|---|
| `pub const N`、`pub mod node` | 未知量个数、节点编号（地 = 0，其余按名字排序 → diff 稳定） |
| `pub fn build(fs, &Params) -> Netlist<N>` | 装配函数 |
| `pub struct Params` + `Default` | 见上 |
| `pub struct Ckt` + `impl rustomp_dsp::Circuit` | **可直接塞进 `Engine<Ckt>`**；`set_params` 只改电阻值 |
| `pub const IDX_*` / `FIXED_IN` / `IN` / `OUT` | 参数元件下标与端口 |

### 2.3 codegen 顺手处理的拓扑问题

| 规则 | 为什么 |
|---|---|
| **导线合并**：`≤ 0.01 Ω` 视为理想导线，并查集并节点 | breadspice 把连线生成成 `0.001 Ω`；不合并会让矩阵出现 1000 S 的对角项 |
| **自环跳过**：`RP1B n_t1L n_t1L 250k` | 电位器"短接的那一半"对电路无影响 |
| **激励分类**：纯 DC 源 → 固定电位节点（不占未知量）；`SIN/AC/PULSE` → 输入端口 | 少两个未知量，LU 省一半 |
| **模型映射是闭集**，未命中**报错** | 替换必须可见、可测（ADR-013）；不静默兜底 |

型号表：`1N4148` `1N34A` `1N4007` `1N5817` `LedRed` `LedGreen` `LedBLUE`
`BC549C` `BC559C` `AC127` `AC128` `J201` `2N7000` `OP07A`。

---

## 3. 验证（`cargo test -p rustomp-import`）

| 测试 | 守什么 |
|---|---|
| `mna_codegen::tests::*` | SPICE 数值后缀（`4.7k`/`1meg`/`47n`）、指令解析（含错误语法）、节点常量命名 |
| `spice::tests::*` | 网表解析、`.subckt` 体跳过、扩展名嗅探 |
| `tests/codegen.rs::all_generated_files_are_up_to_date` | **漂移守卫**：生成物必须等于"现在重新生成"的结果 |
| `tests/codegen.rs::designer_project_is_up_to_date` | 漂移守卫（设计器工程 `.design.json` → 生成物） |
| `tests/designer.rs::与设计器导出的网表逐行一致` | 读工程文件的网表 == 设计器自己导出的 `.cir`（几何/网名/指令逐行对拍） |
| `tests/codegen.rs::unknown_model_is_rejected_with_help` | 未知型号必须报错并点名 |
| `tests/codegen.rs::wire_resistors_are_merged` | `0.001 Ω` 导线合并 |

生成物的**行为**验证在 `crates/rustomp-dsp/tests/`：

- `sparklin_generated.rs`：THD 27.677%（离线 27.579%、ngspice 27.5682%）、对称开关让 h2 → 0%、GAIN 扫描单调
- `generated_bjt.rs`：`Q` 卡片映射，Vb 与 ngspice 差 2.2 µV、Ic 差 0.26%（未建模 `Nk`）
- `designer_generated.rs`：**设计器工程（`.design.json`）→ 代码 → 10k/100n 低通 1 kHz 衰减**
  与理论 `1/√(1+(f/fc)²)` 一致（顺带守住"输入端口不能退化成 0"）

> 生成物**只比文本是不够的**。E44 里有一个 `FIXED_IN` 下标写死 0、
> 把音频写进 9V 轨的例子：编译通过、不 panic，只是 THD 只有 0.237%。

---

## 4. 更新生成物的流程

```bash
# 1) 改网表（或改夹具）
$EDITOR crates/rustomp-import/tests/fixtures/sparklin_drive.cir

# 2) 重新生成（会打印"导线合并/自环跳过"等信息）
cargo run -q -p rustomp-import -- crates/rustomp-import/tests/fixtures/sparklin_drive.cir \
    --mna crates/rustomp-dsp/tests/generated/sparklin_drive.rs

# 3) 跑测试（行为级 + 漂移）
cargo test -p rustomp-import -p rustomp-dsp --release

# CI 里用 --check 只比对不写入
cargo run -q -p rustomp-import -- <网表> --mna <生成物> --check
```

---

## 5. 明确不做

| 不做 | 原因 / 后续 |
|---|---|
| 写 DSP 代码（手推实现） | 那是插件项目的事；这里只生成**通用装配代码** |
| 决定型号映射的兜底 | 目前只做"精确命中 / 报错"；别名、项目 `mapping.toml`、类别兜底见 `docs/circuit-import.md` §3 |
| 从网表反推电位器/开关 | 必须显式 `param`/`switch` 声明 |
| `.asc` 原理图 | 首选设计器直接交工程文件，不需要 LTspice 的 `.asc`（`docs/circuit-import.md` §10） |
| 复制设计器的符号几何 | 运行时读它的 `symbols.json`：同一个量只有一个来源，改几何不用改本仓代码 |

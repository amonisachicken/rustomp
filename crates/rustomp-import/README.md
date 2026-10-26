# rustomp-import

**外部电路导入 + 网表 → 可运行电路代码的生成器。**

```text
 .cir 网表 ─┐
 .bread   ─┼─► Importer ─► ImportedCircuit（IR）─┬─► to_spice()      ngspice 网表（喂给 oracle 对拍）
 其它格式 ─┘                                     ├─► to_json()       给其它工具消费
                                                 └─► mna_codegen     Rust 装配代码（可直接进插件）
```

设计原则：**导入器只负责"读"，语义（型号映射、端口、参数）交给 codegen。**
这样同一份 IR 既能回写 SPICE、又能生成代码，还能被设计器/工具复用。

---

## 1. 导入器（`registry()` 顺序即优先级）

| 导入器 | 格式 | 说明 |
|---|---|---|
| `BreadSpice` | `.bread` | 面包板工程；按插孔 id 归并 net |
| `SpiceNetlist` | `.cir` / `.net` / `.sp` / `.spice` | SPICE 网表；含内容嗅探。支持 `R C L D V I Q J M X`、续行 `+`、`.subckt` 整段跳过 |

新增格式 = 实现一个 `Importer` + 在 `registry()` 加一行。

```rust
pub trait Importer {
    fn id(&self) -> &'static str;
    fn matches(&self, path: &Path, content: &str) -> bool;
    fn import(&self, path: &Path, content: &str) -> Result<ImportedCircuit, ImportError>;
}
```

---

## 2. 网表 → MNA 装配代码（S1a/S1b）

```bash
cargo run -q -p rustomp-import -- <网表.cir> --mna <输出.rs> [--check] [--output-node N]
```

### 2.1 网表里的指令（写在注释里，ngspice 会忽略）

```text
* @rustomp in=VS1 out=n_t27R
* @rustomp param RP1A gain pot max=500k              # R = max·gain
* @rustomp param RP2B level pot max=500k invert      # R = max·(1−level)
* @rustomp switch symmetric n_t10L n_t5L closed=0.001 open=1e12
* @rustomp newton=4                                  # 每样本牛顿迭代次数（默认 3）
```

- `in=` / `out=`：**必须显式给**，我们不猜哪个节点是输出；
- `param`：电位器 → 生成 `Params.gain: f64`（默认 0.5）；
- `switch`：开关 → 生成 `Params.symmetric: bool`（默认 `false`），实现方式是"新建一只可切换电阻"。
- `newton=`：实时路径每样本的牛顿迭代次数。**这是正确性参数，不是性能参数** ——
  2 次在方波/快速边沿下会欠收敛、产生次谐波极限环（DEVLOG E50）。默认 3，有振荡器类
  方波输入的电路建议 4。

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
| `tests/codegen.rs::unknown_model_is_rejected_with_help` | 未知型号必须报错并点名 |
| `tests/codegen.rs::wire_resistors_are_merged` | `0.001 Ω` 导线合并 |

生成物的**行为**验证在 `crates/rustomp-dsp/tests/`：

- `sparklin_generated.rs`：THD 27.677%（离线 27.579%、ngspice 27.5682%）、对称开关让 h2 → 0%、GAIN 扫描单调
- `generated_bjt.rs`：`Q` 卡片映射，Vb 与 ngspice 差 2.2 µV、Ic 差 0.26%（未建模 `Nk`）

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
| `.asc` 原理图 | 让设计器直接输出网表更干净（`docs/circuit-import.md` §10） |

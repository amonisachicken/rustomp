# 架构

> 当前实现的结构说明。器件精度、导入映射、声明式构建的规范分别在
> [`component-library.md`](component-library.md)、[`circuit-import.md`](circuit-import.md)、
> [`declarative-build.md`](declarative-build.md)；这里只讲"东西怎么拼起来"。

---

## 1. 目标与分层

把「一块模拟效果器」变成可复用、可验证的工程流程：

```text
原理图 / 面包板 → SPICE 网表 → 与 ngspice 对拍的 DSP → 插件 → 三平台产物
```

| 层 | 位置 | 作用 |
|---|---|---|
| **器件库** | `crates/rustomp-dsp/src/devices/` | 闭集器件模型（二极管/BJT/JFET/MOSFET/运放），参数与 SPICE 对齐 |
| **电路装配** | `crates/rustomp-dsp/src/mna.rs` | 通用 MNA：器件 + 拓扑 → 可求解电路（牛顿、gmin 步进、伴随模型） |
| **实时机制** | `crates/rustomp-dsp/src/{engine,oversample,circuit,branch}.rs` | `Circuit` 契约、`Engine<C>`、2×/4× 过采样、真旁通、NaN 熔断、分支切换 |
| **界面层** | `crates/rustomp-gui` | 声明式面板（`PanelSpec`）、参数桥、贴图生命周期、面板校验 |
| **工具链** | `crates/rustomp-{import,gen,oracle}` | 网表导入 / 代码生成 / ngspice 对拍 |
| **插件外壳** | `template/` + `scripts/new-plugin.sh` | nih-plug 外壳；单/立体声独立变体；`gui = true` 时含面板模块 |

**打包不在这里**：三个平台各有一个独立打包仓，通过契约 B（`ARTIFACT:`）接入。

---

## 2. 数据流

```text
 rustomp-circuit-designer ──(.design.json / .cir)──┐
 breadspice（次选）────────(.bread)────────────────┤
                                                   ▼
                                      rustomp-import --mna
                                                   │
                                                   ▼
                                 circuit/src/generated/circuit.rs
                                 （Netlist + Params + impl Circuit）
                                                   │
 rustomp-box-designer ──([ui] + PNG)──► rustomp.toml ──► rustomp-gen
                                                   │
                                                   ▼
                        plugin/src/generated/{identity,params,plugin_impl,panel,…}.rs
                                                   │
                                                   ▼
                             nih-plug 插件 crate（手写只有 editor.rs）
                                                   │
                                                   ▼
                     rustomp-{mac,win,linux}-plugin/pack.sh ──► ARTIFACT:
```

两条链都**以可验证的中间格式为界**：

- 电路侧：网表可被 ngspice 直接跑，所以 DSP 与基线能逐点对拍；
- 面板侧：`rustomp.toml` 的 `[ui]`/`[assets]` 是设计器与生成器的契约，素材缺失在编译期报错。

---

## 3. Crates

| crate | 职责 | 关键入口 |
|---|---|---|
| `rustomp-dsp` | 器件库、MNA 装配、`Engine`、过采样、分支、旁通 | `Circuit`、`Netlist`、`Engine<C>` |
| `rustomp-gui` | 声明式面板 + 交互 + 纹理生命周期（不含素材） | `PanelSpec`、`ParamAccess`、`draw` |
| `rustomp-oracle` | ngspice 驱动、rawfile 解析（变步长重采样）、统一测量 | `Oracle`、`RawFile` |
| `rustomp-import` | 外部电路导入（可插拔 Importer）+ 网表 → 装配代码 | `--spice` / `--mna` / `.design.json` |
| `rustomp-gen` | `rustomp.toml` → 插件侧生成物 + `--check` 漂移守卫 | `rustomp-gen <项目> [--check]` |

各 crate 的细节见 `crates/*/README.md`。

---

## 4. 仓库结构

```text
devices/                器件库（SPICE 侧）：models.add → devices.lib、手写子电路、对拍报告
crates/                 框架 crates（见上表）
template/               插件脚手架模板（new-plugin.sh 复制后替换占位符）
scripts/                new-plugin / build-all / verify / 网表 / 基线 / 隔离验证
tools/
  meta/                 rustomp.toml 读取器（契约 A）与 LTspice 模型清洗
  oracle/               ngspice 冒烟测试用的最小电路
tests/                  packer_contract.sh（契约 A/B 的可执行测试）
docs/                   当前规范与状态（本目录）+ history/（归档）
experiments/            归档的一次性验证代码（不参与构建）
.github/workflows/      框架 CI（macOS：测试 + ngspice 对拍）
DEVLOG.md               历史：决策、实验数据、踩坑（只追加/归档）
```

> 插件项目**不在本仓**：`cargo-nih-plug` 会向上找最顶层 `Cargo.toml`，插件必须住在框架之外。
> 约定：框架、本平台打包工具、插件项目三个目录同级。

---

## 5. 关键机制

### 5.1 两套求解器，共用器件模型

- **MNA 装配器**：任意拓扑/多管电路。器件只描述"端电压 → 流入器件的电流"，
  雅可比由装配器得到（二极管有解析雅可比）；加新器件不需要手写整矩阵。
- **一维牛顿 / WDF**：简单电路（如单级削波）更快，用于性能敏感路径或历史基线。

### 5.2 实时路径

- `step_fixed`：固定步长，固定电位节点（电源/输入）不参与 LU；
- **自适应牛顿**：迭代有上限，残差足够小提前退出（实测平均 ~2 次）；
- **大跳变子步进**：固定节点单拍变化过大时拆 2–8 个子步，失败按 k 重试，最后 DC 恢复；
- **过采样**：2×/4× 共用多相 FIR，上报延迟都是 15 样本；
- **分支/旁通**：`Branch<A,B>` 两份完整电路 + 预热/交叉淡化；true-bypass 对齐延迟；
- **数值兜底**：阻尼、失败不提交坏解、状态限幅、NaN 熔断、首口参数到达时重解 DC。

### 5.3 代码生成

- **电路**：`rustomp-import --mna` 从网表生成 `Netlist`/`Params`/`impl Circuit`；
  参数与开关来自网表指令（`* @rustomp param/switch`）；
- **插件**：`rustomp-gen` 从 `rustomp.toml` 生成身份、参数表、`Plugin` 实现、面板模块；
  `--check` 做逐字节漂移守卫；
- **确定性**：生成物入库、可 review、可 diff；手写代码放在 `local.rs` / `editor.rs`。

### 5.4 界面

`[ui]`（毫米坐标、12 px/mm）→ `PanelSpec`（const）→ `rustomp_gui::draw`。
旋钮整图旋转、拨杆/LED/脚踏；控件 id 不写死数量。契约见 `crates/rustomp-gui/README.md`。

---

## 6. 框架 / 插件 / 打包器的边界

| 谁 | 负责 | 不负责 |
|---|---|---|
| **本仓（rustomp）** | DSP、器件、MNA、实时、代码生成、面板契约、对拍 | 平台打包、具体插件、素材美术 |
| **插件仓（如 sparklin-drive）** | 电路值、网表、生成物、面板素材、产品身份 | 框架代码（git tag 依赖） |
| **打包仓（mac/win/linux）** | 各平台构建、安装包、宿主验证 | DSP、代码生成、插件源码 |

契约与版本约束见 [`ecosystem.md`](ecosystem.md)；打包平面见 [`declarative-build.md`](declarative-build.md)。

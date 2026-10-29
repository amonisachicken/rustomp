# 验证与质量基线

> 当前验证体系与最近一次记录的数字。历史实验过程（AB 测试、逐次优化）见
> [`history/`](history/README.md) 与 [`../DEVLOG.md`](../DEVLOG.md)。
>
> 数字会随代码变化；**发布前以 `./scripts/verify.sh` 的实际输出为准**。

---

## 1. 一条命令判断"能不能发"

```bash
./scripts/verify.sh                 # 全部：fmt + 测试 + 漂移守卫 + 四层对拍
./scripts/verify.sh --only fast     # 不需要 ngspice 的部分（改代码时用）
./scripts/verify.sh --only oracle   # 只跑对拍（缺 ngspice 直接失败，不静默跳过）
./scripts/verify.sh --report out.md # 额外产出 markdown 报告（CI 写进 job summary）
./scripts/verify.sh --logs vlogs    # 保留每层日志
```

`verify.sh` 的 9 层（fmt 不阻塞）：

| 层 | 内容 | 需要 ngspice |
|---|---|---|
| fmt | `cargo fmt --all --check` | 否（不阻塞） |
| workspace | `cargo test --workspace --release` | 否 |
| packer-contract | `tests/packer_contract.sh`（契约 A/B） | 否 |
| drift × 2 | 生成电路重新生成必须逐字节一致 | 否（需要框架自身） |
| oracle × 4 | 器件级 / 装配器级 / 整机级 / 生成电路 | **是** |

> 缺 ngspice 在本地会"打印原因并跳过"；CI 设 `RUSTOMP_REQUIRE_NGSPICE=1`，
> 同一函数变成失败 —— 不允许"忘了装 ngspice、CI 却是绿的"。

---

## 2. 器件级（`tests/devices.rs`）

每个型号扫一段 V-I 曲线（覆盖多个数量级），与 `devices/devices.lib` 的 SPICE 模型逐点比较：

| 器件 | 比较量 | 误差 |
|---|---|---|
| `1N4148` / `1N34A` / `1N4007` / `1N5817` | 1 µA ~ 10 mA 的 `Vf` | **0.00%** |
| 红 / 绿 / 蓝 LED | 1 µA ~ 10 mA 的 `Vf` | **0.00%** |
| `BC549C` / `BC559C` / `AC127` / `AC128` | `Ic` 与 `Ib`（PNP 也测） | **≤0.09%**（BC549C 在 4.6 mA 处 0.67%） |
| `J201` | 转移特性 + 输出特性（含三极管区） | **0.00%** |
| `2N7000` | 沟道 8 点 + 体二极管 3 点 | **≤0.01%** |
| `OP07` | 闭环 -3 dB（4× 过采样） | **<2%** |

---

## 3. 装配器级（`tests/mna.rs`）

同一电路分别用 `Netlist` 和 ngspice 求解：

| 电路 | 量 | 偏差 |
|---|---|---|
| 二极管削波（6 个 DC 点） | `v(out)` | **≤1.6 µV** |
| BJT 共射偏置 | `v(b)` / `v(c)` | **≤123 µV** |
| JFET 共源 | `v(d)` | **34 µV** |
| RC + 削波瞬态（48 kHz，240 点） | 波形 | **≤17 mV**（后向欧拉 vs 梯形积分） |
| 运放非反相 10× | DC 增益 / 饱和 / -3 dB | 10.998 / 钉在 7.8 V / 40.5 kHz |

---

## 4. 生成电路与整机

### 4.1 由网表自动生成的电路

| 项目 | 生成电路 | 手写/ngspice | 差 |
|---|---|---|---|
| Sparklin Drive THD @0.2 V | **27.677 %** | 27.579 % / 27.5682 % | +0.10 pp |
| 对称开关（短接 D3） | h2 **0.000 %** | 非对称 4.002 % | ✓ 开关真的接对了 |
| GAIN 0.15 / 0.5 / 1.0 | THD 14.20 / 25.87 / 28.62 % | — | 单调 ✓ |
| BJT 前级 Vb / Ic | 0.679100 V / 2.0432 mA | 0.679098 V / 2.0379 mA | 2.2 µV / 0.26 %（未建模 `Nk`） |
| BJT 前级小信号增益 | 143× | — | 共射反相 ✓ |

生成物有**漂移守卫**：重新生成必须逐字节一致，`--check` 已进 CI。

### 4.2 整机（Sparklin Drive）

| 实现 | THD @0.2 V | 说明 |
|---|---|---|
| ngspice（完整 OP07A 子电路） | 27.5682 % | 基线 |
| **MNA 装配器**（本仓库，E42） | **27.579 %** | +0.011 pp |
| legacy WDF（一维牛顿，已归档） | 27.650 % | +0.034 pp |

legacy 实现的其余实测（电位器 50%，历史记录）：

| 项目 | WDF 实现 | ngspice | 误差 |
|---|---|---|---|
| 直流工作点 | 4.49999 V | 4.50000 V | 0.01 mV |
| 频响 50 Hz – 10 kHz | — | — | **≤ 0.31 dB** |
| THD @0.2 V（2–9 次） | 27.650 % | 27.616 % | +0.034 pp |
| 对称模式 THD | 25.595 % | 25.516 % | +0.08 pp |
| h2–h9（非对称） | 3.915 / 22.488 / 2.428 / 11.981 / 1.836 / 7.700 / 1.484 / 5.400 % | 3.920 / 22.470 / 2.430 / 11.960 / 1.837 / 7.674 / 1.483 / 5.374 % | ≤0.03 pp |
| 活体对拍（每次重跑 ngspice） | 同上 | 3.921 / 22.457 / … / 5.344 % | **≤0.056 pp** |

### 4.3 两套测试互补

| 测试 | 是否跑 ngspice | 用途 |
|---|---|---|
| 插件项目里 `tests/baseline.rs` | 否（期望值是抄下来的常量） | 日常回归，快 |
| 插件项目里 `tests/oracle.rs` | **是**（每次重跑） | 抓网表/电路改动，无 ngspice 时自动跳过 |

> 基准一律从 **ngspice 原始波形**自己算 DFT，不用 `.four`（只取 1 个周期 + 线性插值，
> 高次谐波偏低）。ngspice 的 `.tran` 是**变步长**，对拍前必须重采样（`RawFile::resample`）。

---

## 5. 性能基线（只做同机配对比较）

> 实测数字强依赖**电路、采样率、过采样倍数、机器状态**；不要跨实验直接对比。
> 以下是代表性记录，完整过程见 DEVLOG。

| 场景 | 数字 | 出处 |
|---|---|---|
| Sparklin Drive MNA（4×，192 kHz，单核） | 约 2.07 µs/样本，单核约 40% | E42/E43 |
| 自适应牛顿（对比固定 8 次） | 平均 **2 次**；快 3.0×；输出差 −227 dB | E57 |
| BJT 电路热路径（4×） | 单声道 **9.7 µs** / 立体声 **19.4 µs** | E89 |
| BJT 电路改 2× | 立体声 **11.9 µs（约 57%）**；2×/4× 延迟都 15 样本 | E91/E92 |
| 大跳变子步进 | 单次迭代约 1.05–1.41 µs（release） | E95 |

测量纪律：必须 **release** 构建、同机配对、避开 Spotlight/编译等后台负载；本机绝对性能
一小时内可能漂移数倍（E57）。

---

## 6. CI 与发布验证

| 环节 | 内容 |
|---|---|
| **框架 CI** | macOS：`verify.sh --only fast`（workspace + 契约 + 漂移）与 `--only oracle`（四层对拍）；产物/报告进 job summary |
| **契约测试** | `tests/packer_contract.sh`：三平台分派、`--arch`/`RUSTOMP_ROOT` 透传、`[requires]`、无平台词 |
| **三平台 Release** | 参考插件 `sparklin-drive`：tag `v*` → macOS/Windows/Linux 构建 → GitHub Release（未签名 prerelease） |
| **打包器自检** | mac `au_smoke`；win `clap-probe + pluginval + PE`；linux `clap-probe + pluginval + ELF/ldd` |

本轮发布核对过：`SHA256SUMS.txt` 与 GitHub API 的 asset digest 全部一致。

历史 AB 测试（v2 生成路径 vs legacy 手推实现、FFT 判读、13 条经验）见
[`history/ab-test-sparklin-v2.md`](history/ab-test-sparklin-v2.md)。

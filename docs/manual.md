# 全流程手动操作手册：从画电路图到装进宿主

> 面向**人**的一步步操作说明。每个阶段都给出：**做什么 → 敲什么 → 应该看到什么 → 卡住了查哪里**。
>
> 想先知道"框架整体长什么样"，读 [`../README.md`](../README.md)；
> 想知道"每条规范为什么这样定"，读 [`architecture.md`](architecture.md) 与
> [`../DEVLOG.md`](../DEVLOG.md)。
>
> 本文按 **2026-10 的现状**写：框架 `rustomp v0.1.3`、macOS 打包器 `v0.1.8`、
> Windows `v0.3.0`、Linux `v0.1.0`。命令里的 tag 一律以插件仓自己声明为准。

---

## 0. 全流程一览

### 0.1 一条流水线，四个交付物

```text
①  画图                    ②  变成代码                ③  做面板            ④  出插件
┌──────────────────┐   ┌────────────────────┐   ┌─────────────────┐   ┌──────────────────┐
│ circuit-designer │   │ rustomp-import     │   │ box-designer    │   │ rustomp-gen      │
│  原理图 .design  │──►│  .design/.cir      │   │  面板 + 素材     │──►│ + 平台打包器      │
│  仿真 / FFT      │   │  → generated/*.rs  │   │  → [ui]/[assets]│   │  → AU/VST3/CLAP  │
└──────────────────┘   └────────────────────┘   └─────────────────┘   └──────────────────┘
        │                        │                      │                       │
   电路对不对                能实时出声            面板长什么样            宿主里能弹
```

| 阶段 | 你要做的事 | 用的工具 | 落盘的交付物 | 验收判据 |
|---|---|---|---|---|
| 0 | 装环境、拉仓库 | git / rustup / 打包器 | 同级目录里四个仓 | `verify.sh --only fast` 全绿 |
| 1 | 画原理图 + 仿真 | `rustomp-circuit-designer` | `*.design.json`（首选）/ `*.cir` | 设计器里 lint 无 error、能出声 |
| 2 | 生成电路代码 | `rustomp-import --mna` | `circuit/src/generated/*.rs` | 单元测试的 THD/谐波落在预期区间 |
| 3 | 画面板、导素材 | `rustomp-box-designer` | `assets/*.png` + `[ui]` 段 | `validate_panel` 无错 |
| 4 | 生成插件外壳 | `rustomp-gen` | `plugin/src/generated/*.rs` | `--check` 无漂移、能 `cargo build` |
| 5 | 打包安装 | `build-all.sh` / `pack.sh` | AU / VST3 / CLAP（+ DMG/MSI/tar.gz） | 宿主扫到、有声音、延迟 15 样本 |
| 6 | 发布（可选） | CI + tag | GitHub Release | 三平台产物能下载 |

### 0.2 三条路线，挑一条

| 路线 | 适合谁 | 电路从哪来 | 本文要读的章节 |
|---|---|---|---|
| **A：设计器全流程**（推荐） | 有电路图、想画出来让工具算 | 设计器画的原理图 | 0 → 1 → 2 → 3 → 4 → 5 |
| **B：手写 / 现成网表** | 已有 LTspice、面包板或别人给的 `.cir` | 文本网表 | 0 → 2 → 3 → 4 → 5（跳过第 1 章） |
| **C：纯 Rust 手写电路** | 电路很简单，或想完全自己控制数值方法 | 自己实现 `Circuit` trait | 0 → 2.5 → 3 → 4 → 5 |

路线 C 不适合"运放 + 非线性"这类电路：手推 WDF/隐式积分很容易在**快速边沿**上欠收敛
（历史上真踩过，见 `history/ab-test-sparklin-v2.md`）。能画成网表，就走 A 或 B。

### 0.3 先看清目录布局

框架、打包器、**你自己的插件项目**必须**同级**。脚本按这个约定自动找东西，
不需要设任何环境变量：

```text
some-dir/
  rustomp/                    ← 框架（本手册所在仓）
  rustomp-circuit-designer/   ← 原理图设计器（路线 A 需要）
  rustomp-box-designer/       ← 面板设计器（第 3 章需要）
  rustomp-mac-plugin/         ← 当前平台的打包器（mac / win / linux 三选一）
  my-drive/                   ← 你的插件项目（new-plugin.sh 生成在这里）
```

插件**不能**放进 `rustomp/` 里面：`cargo-nih-plug` 会把**最顶层**的 `Cargo.toml` 当成
workspace 根，然后报 `cannot specify features for packages outside of workspace`。

### 0.4 时间预算（第一次）

| 阶段 | 纯操作时间 | 说明 |
|---|---|---|
| 0 环境 | 30–60 min | 一半时间在等 rustup / cargo 编译，网络受限会更久 |
| 1 画图 | 30 min – 数小时 | 取决于电路复杂度；仿真调参最花时间 |
| 2 生成 + 测试 | 15 min | 生成是秒级；跑测试是分钟级 |
| 3 面板 | 30 min | 设计器里画 + 对齐坐标 |
| 4 生成外壳 | 5 min | 一条命令 |
| 5 打包 | 3–10 min（首次更久） | 首次要编 nih-plug + egui + AU 外壳 |

---

## 1. 准备环境（一次性）

### 1.1 拉齐仓库

```bash
mkdir -p ~/develop && cd ~/develop

# 框架（tag 与你插件项目 Cargo.toml 里钉的一致；当前 v0.1.3）
git clone --depth 1 --branch v0.1.3 https://github.com/amonisachicken/rustomp.git

# 两个设计器（只走路线 B 时可以不拉）
git clone https://github.com/amonisachicken/rustomp-circuit-designer.git
git clone https://github.com/amonisachicken/rustomp-box-designer.git

# 打包器：按平台取一个（tag 见各仓 Releases）
git clone -b v0.1.7 https://github.com/amonisachicken/rustomp-mac-plugin.git      # macOS
git clone -b v0.3.0 https://github.com/amonisachicken/rustomp-win-plugin.git      # Windows
git clone -b v0.1.0 https://github.com/amonisachicken/rustomp-linux-plugin.git    # Linux
```

> 网络受限时先 `source ~/vpn.env`（作者本机的习惯，换成你自己的代理方式）；
> 打包器还支持 `RUSTOMP_GIT_SSH=1` 把 https 依赖重写成 SSH。

### 1.2 装工具

**三平台都要**：

| 依赖 | 为什么需要 | 检查 | 装法 |
|---|---|---|---|
| Rust stable | 编译一切 | `cargo -V` | https://rustup.rs |
| Python ≥ 3.11 | 读 `rustomp.toml`（`tomllib`） | `python3 -V` | 系统自带 / `brew install python` / `apt install python3` |
| `cargo-nih-plug`（rev 钉死） | 把插件打成 AU/VST3/CLAP | `cargo --list \| grep nih-plug` | `cargo install --git https://github.com/robbert-vdh/nih-plug.git --rev de42101 cargo-nih-plug --locked` |
| ngspice（推荐） | 与 SPICE 对拍 | `ngspice -v` | `brew install ngspice` / `apt install ngspice` |
| Node.js ≥ 18 | 两个设计器的前端 | `node -v` | https://nodejs.org |

`cargo-nih-plug` 的 rev **必须**和打包器 `deps.lock` 里的一致（当前 `de42101`）；
不一致会出现"能编但打不出包"。这是整条链上最容易忽略的一处版本耦合。

**macOS 额外**：

| 依赖 | 检查 | 装法 |
|---|---|---|
| Xcode 命令行工具 | `xcode-select -p` | `xcode-select --install` |
| cmake ≥ 3.20 | `cmake --version` | `brew install cmake`（AUv2 外壳靠 clap-wrapper 的 CMake） |
| 两个 rust target | `rustup target list --installed` | `rustup target add aarch64-apple-darwin x86_64-apple-darwin` |

**Windows**：MSVC 工具链（**不要**用 `-gnu`）+ MSYS2/Git Bash。**Linux**：`g++`、X11/GL 开发库
（打包器 README 里有整行 `apt install`）。

### 1.3 打包器的一次性准备 + 体检

```bash
./rustomp-mac-plugin/scripts/fetch-third-party.sh            # macOS：clap-wrapper / CLAP / AudioUnitSDK
./rustomp-win-plugin/scripts/fetch-third-party.sh --all      # Windows
./rustomp-linux-plugin/scripts/fetch-third-party.sh --all    # Linux

./rustomp-mac-plugin/scripts/doctor.sh                       # 缺什么、怎么装，逐条说清
```

> `fetch-third-party.sh` 只跑一次。忘了跑，AU 构建会在 CMake 阶段报
> `CPMAddPackage` 之类的网络相关错误（旧版打包器还会偷偷联网下载）。

### 1.4 冒烟：确认环境是好的

在**框架仓**里：

```bash
cargo test --workspace            # 框架自检（不需要 ngspice、不需要插件）
./scripts/verify.sh --only fast   # fmt + 测试 + 契约 + 漂移守卫
./scripts/verify.sh --only oracle # 四层 ngspice 对拍（需要 ngspice）
```

**应该看到**：`fast` 各层 `ok`，最后一行类似 `全部通过`。`oracle` 缺 ngspice 时会明确报错
（本地默认跳过并打印原因；CI 里设 `RUSTOMP_REQUIRE_NGSPICE=1` 后直接失败）。

这一步没过，后面所有问题都会变成"到底是环境坏了还是我改坏了"。

### 1.5 这一步的常见失败

| 现象 | 原因 | 处理 |
|---|---|---|
| `no such command: nih-plug` | 没装 `cargo-nih-plug` | 按上表装，注意 `--rev de42101` |
| `unknown target aarch64-apple-darwin` | 少 rust target | `rustup target add …` |
| AU 构建报 CMake / `CPMAddPackage` | 没跑 `fetch-third-party.sh`，或 `MACOS_DEPLOYMENT_TARGET` 为空 | 补跑；`export MACOS_DEPLOYMENT_TARGET=11.0` |
| `error: failed to load manifest … rustomp-dsp` | 首次要拉框架的 git tag，网络被墙 | 开代理后重试 |

---

## 2. 画出电路（路线 A）

> 路线 B（已有网表）可整章跳过，直接读第 3 章；但第 2.2 与 2.6 的"端口/参数约定"仍然适用于你的网表。

### 2.1 启动设计器

两个终端，都在 `rustomp-circuit-designer/` 下：

```bash
# 终端 1：后端（默认 127.0.0.1:3600）
cd backend && cargo run

# 终端 2：前端
cd frontend && npm install && npm run dev
```

浏览器打开前端打印的地址（默认 `http://localhost:5173`）。后端没起时页面能开但保存/仿真会失败。

| 环境变量 | 默认 | 什么时候改 |
|---|---|---|
| `CD_PORT` | `3600` | 端口冲突 |
| `NGSPICE` | `ngspice` | ngspice 不在 PATH |
| `RUSTOMP_ROOT` | `../rustomp` | 框架不在同级（影响顶栏「生成代码」） |
| `CD_DATA` | `<设计器仓>/.cache/designs` | 想换「保存」的落盘位置 |

### 2.2 先想清楚三个"接口符号"

生成代码时要靠它们认出端口，**画图时就必须用对**：

| 符号 | 网表里的名字 | 作用 |
|---|---|---|
| `node-in`（In） | 节点 `IN` | 插件的输入。**网表里没有激励源**，每样本由 `process(x)` 注入 |
| `node-out`（Out） | 节点 `OUT` | 插件的输出 |
| `接地` | `0` | 地。**每张图必须有一个**，否则 lint 直接报错 |
| `Vcc` / `Vee` | `VCC` / `VEE` | 电源轨。单电源 9V 就画 `Vcc` + 一个 9V 电压源 |
| 测试点 | 自动命名 | 只在仿真里当探针用，不进插件 |

### 2.3 画图：全部靠键盘模式（没有拖放）

主界面**不提供拖放式元件库**。所有编辑都在"模式"里完成，`esc` 永远回到查看模式。

| 键 | 进入什么模式 | 怎么用 |
|---|---|---|
| `空格` | 元件库 | 弹出全部 18 个符号，点一个就进入该类放置模式 |
| `r` | 放电阻 | 左键连点连续放，`esc` 结束 |
| `c` | 放电容 | 同上 |
| `g` | 放地 | 同上（接地必备，给单键） |
| `w` | 画导线 | 点一下定起点 → 只能走横竖 → 再点定终点，继续点继续画 |
| `q` | 删除 | 点元件删元件；点导线**删整段直线** |
| `m` | 移动 | 点起 → 点落；手持时 `esc` 放回原处 |
| `d` | 复制 | 点一个元件 → 进入同类放置，**数值/型号/别名一起带过去** |
| `ctrl+r` / `ctrl+e` | 旋转 90° / 左右镜像 | 放置或移动中可用 |
| `双击` | 属性 / 示波器 | 普通元件 → 属性面板；`In` → 幅值/频率；`Out`/测试点 → 示波器 + FFT |
| `⌘/Ctrl+Z` / `⌘/Ctrl+Y` | 撤销 / 重做 | — |
| `h` 或 `?` | 快捷键手册 | 忘了就按 |

视图：`滚轮` 缩放、`中键拖动` 平移（查看模式下**左键拖动**也能平移，移动超过 2 px 才算拖）。

**栅格 2.5 mm，坐标单位毫米。** 元件引脚落在导线的端点或线上就算连上。

### 2.4 连线规则（会咬人的三条）

1. **T 型才是连接**：三根线交汇会画黑点（junction）；十字交叉**不**并网。
   想让十字也接上，就从 T 形交点继续画（归一化不会丢掉那个连接点）。
2. **导线只走横竖**：斜线是非法的（lint 报错）。画折线不需要整理，
   同一直线上的重合/首尾相接线段会自动合并成一条。
3. **引脚落在导线上会自动剪线**：例如先拉一根长线、再把 BJT 的 C–E 插上去，
   两脚之间那段会被剪掉（否则等于短路）。所以"先画骨架、再放器件"是合法工作流。

移动元件**不会**带走导线 —— 引脚可能悬空，lint 会用红圈标出来（warn，不阻断导出）。

### 2.5 填数值与型号

双击元件打开属性面板。数值是"**数字框 + 单位下拉**"，不用自己拼 `10k`：

| 元件 | 默认单位 | 可切 |
|---|---|---|
| 电阻 / 电位器 | `k` | `k`、`Ω` |
| 电容 | `n` | `n`、`p`、`u` |
| 电感 | `m` | `m`（mH）、`u`、不写（H）；**不要写 `meg`** |
| 电压源 | `V` | — |

**型号必须是闭集里的**（写错会在导入阶段报错，不会静默替换）：

```text
二极管  1N4148  1N34A  1N4007  1N5817  LedRed  LedGreen  LedBLUE
BJT     BC549C  BC559C  AC127  AC128
JFET    J201        MOSFET  2N7000        运放  OP07A
```

想加型号 = 去框架 `devices/` 标定并写对拍测试，不是在本项目里改字符串。

> ⚠️ **`M` 是 milli，不是 mega**：`1M` = 0.001 Ω，1 MΩ 要写 `1meg`（或 `1000k`）。
> 这是 SPICE 的通用约定，也是最容易吃的一次亏：`1M` 的偏置电位器会把整块电路短接。
> 现在两侧都有护栏 —— 导入器对 `pot max < 1 Ω` 直接报错，设计器会把电位器上的 `M`
> 规范成 `meg` 并给提示；普通电阻写 `1M` 会打印一条 warning。

> 🔌 **电感（M1）**：网表里写 `L1 in mid 10m`，**绕线电阻必须写成串联电阻**
> （`RDCR mid out 0.1`）—— ngspice 的 `L` 没有 DCR 参数，多写一个数会直接报错。
> 电感在 DC 用 1 mΩ 代替（理想 L 在 DC 是短路），所以纯 LC 电路要留意直流回路。
> **互感 `K`（变压器）还没有**，写了会报错（不会静默忽略）。

### 2.6 电位器 → 可调参数（关键一步）

画一个电位器（3 脚），**导出时会自动展开**成两只电阻 + 一条 `param` 指令：

```spice
* @rustomp param RP1A gain pot max=500k
* @rustomp param RP2B level pot max=500k invert
```

含义与三条铁律：

| 项 | 规则 |
|---|---|
| 字段名 | `gain` / `level` 这种**合法 Rust 标识符**，它会变成 `Params.gain: f64` |
| 元素名 | `RP1A`/`RP2B` 是**展开后的网表元件名**，不是位号 `P1A`。写错 → 参数被**静默丢弃**（老坑） |
| 方向 | `invert` ⇒ `R = max·(1−t)`，即"旋到底 = 0 Ω"。音量电位器一般要 `invert` |
| 数值 | 0..1，默认 0.5。插件参数表里的 `range = [0,1]` 与之对应 |

> 电位器的**两个**抽头电阻会各自生成一条 `param` 指令（`RP1A` 和 `RP1B` 都指向同一字段，
> 一个正接一个反接），这是正常现象。

### 2.7 先在设计器里听一遍（tran / ac / FFT / THD）

顶栏「仿真」：

| 分析 | 用途 |
|---|---|
| `tran` | 时域波形。`In` 元件的属性里有**波形发生器（幅值 / 频率）**，当信号源用 |
| `ac` | 频响，看通带与 -3 dB |
| 示波器 / FFT | 双击 `Out` 或测试点，看频谱与 THD |

**先在这里把削波量、频响、谐波看清楚**，比后面在宿主里边听边猜省时间得多。
经验值：正弦 THD 对得上不代表没问题，**方波输入**能暴露求解器欠收敛（次谐波）。

### 2.8 交付：导出什么，放哪

| 交付物 | 怎么拿 | 用它做什么 |
|---|---|---|
| **`<名字>.design.json`** | 顶栏「导出布局」 | **首选**：直接喂给 `rustomp-import`，不用先转 `.cir` |
| `<名字>.cir` | 顶栏「生成网表」 | 通用：`ngspice` 能跑、任何 SPICE 工具能用；也便于 review/diff |
| 截图 PNG | 顶栏「预览」 | 写文档、留档 |

两个文件都放进插件项目的 `circuit/netlist/` 里一起入库（`.design.json` 是源，`.cir` 是
可被 SPICE 验证的中间格式）。文件名用**小写下划线**，别用空格。

> 顶栏「生成代码」等价于第 3 章的 `rustomp-import --mna`，适合快速试；**正式流程建议用命令行**，
> 因为要写进脚本、要能 `--check`。

### 2.9 导出前自查（30 秒）

- [ ] 有 `In`、有 `Out`、有地（`0`），端点在正确的网络上
- [ ] 没有悬空引脚红圈；没有斜线；没有十字交叉却以为连上了
- [ ] 每个非地网络至少两个连接点（不要出现孤立元件）
- [ ] 电位器的字段名是最终要用的名字（发布后改名 = 参数 id 变了，老工程会失效）
- [ ] 型号都在闭集里
- [ ] 仿真能出声，波形不是"钉在电源轨"的死样子

### 2.10 设计器**不做**的三件事（别在这里纠结）

| 不做 | 为什么 | 那怎么做 |
|---|---|---|
| **开关 / 多档拓扑** | 决策 D-007：开关是插件层的分支，不是电路拓扑；图纸永远是**单份**网表 | 见第 7 章：两份网表 + `Branch<A,B>` |
| 层次图 / 多页 / `.subckt` | 未做 | 用平铺；或手写 `.subckt` 走路线 B（导入器会整段跳过子电路） |
| 极性元件（电解电容） | 器件库没有极性模型 | 用普通电容；需要极性行为就自己扩展器件库 |

---

## 3. 网表 → 能跑的 Rust 电路

### 3.1 先建个项目（如果是新插件）

```bash
cd ~/develop/rustomp
./scripts/new-plugin.sh "My Drive"                       # 生成 ../my-drive/
./scripts/new-plugin.sh "My Drive" --vendor "ACME AUDIO" # 指定厂商
RUSTOMP_GUEST=/path/to/my-drive ./scripts/new-plugin.sh "My Drive"   # 指定生成位置
```

它会：复制 `template/` → 替换占位符（包名、AU 码、VST3 id、框架相对路径）→ 跑一次
`rustomp-gen` 生成外壳 → 打印**还需人工做的 3 件事**。

生成后的结构（**只有这几处要你动**）：

```text
my-drive/
  rustomp.toml              身份 / 参数 / 素材 / [ui] / [requires]
  Cargo.toml                自带 workspace，框架依赖钉在 tag 上
  circuit/
    netlist/                放 .design.json 与 .cir
    src/lib.rs              电路 crate 的入口（你要改）
    src/generated/*.rs      由网表生成（不手改）
    tests/engine.rs         电路行为测试（你要改）
  plugin/
    src/lib.rs              手写入口（很短，只 re-export 生成物）
    src/editor.rs           手写界面层（模板里**没有**，第 4.6 节教你自己建）
    src/generated/*.rs      生成物（不手改）
  assets/                   面板与控件贴图
```

> 模板自带一个"纯增益"示例电路（`circuit/src/gain.rs`），它的作用是**让项目一开始就能跑**。
> 你的真实电路会替换它。

### 3.2 生成电路代码

**方式 A（推荐，可脚本化）**：在框架仓执行

```bash
# 首选：直接吃设计器工程文件
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- \
    ../my-drive/circuit/netlist/my_drive.design.json \
    --mna ../my-drive/circuit/src/generated/main.rs

# 或者：喂导出的 .cir（产物与上面逐行相同）
cargo run -q -p rustomp-import -- \
    ../my-drive/circuit/netlist/my_drive.cir \
    --mna ../my-drive/circuit/src/generated/main.rs
```

| 参数 | 作用 |
|---|---|
| `--mna <文件>` | 生成 MNA 装配代码 |
| `--check` | **只比对不写入**：产物和现有文件不一致就非零退出（放进 CI / 构建脚本防漂移） |
| `--input <名>` / `--output-node <名>` | 网表里没写 `in=/out=` 时手动指定（设计器导出的不需要） |

**方式 B**：设计器顶栏「生成代码」按钮（走同一套 `rustomp-import`，需要 `RUSTOMP_ROOT` 指对）。

> `RUSTOM_DESIGNER_SYMBOLS` 少了 `P`（历史拼写），**是 `RUSTOM_`**，别写成 `RUSTOMP_`。
> 设计器仓放在框架同级时可以省略这个变量（导入器会自动找 `../rustomp-circuit-designer/`）。

### 3.3 生成物长什么样

`circuit/src/generated/main.rs`（自动生成、**不要手改**）：

| 项 | 含义 |
|---|---|
| `N` / `UNKNOWNS` | MNA 未知量个数（节点 + 电压源支路；地不占未知量） |
| `NEWTON_ITERS` | 每样本牛顿迭代上限（来自网表 `* @rustomp newton=`，默认 3） |
| `struct Params` | 每个 `param` 指令一个 `f64` 字段，默认 0.5 |
| `IDX_RP1A` … | 参数化元件在 `Netlist::elems` 里的下标（调试用） |
| `IN` / `OUT` / `FIXED_IN` | 端口下标 |
| `fn build(fs, &Params) -> Netlist<N>` | 按参数装配网表 |
| `struct Ckt` + `impl Circuit` | **能直接塞进 `Engine`**；`set_params` 只改电阻值 |
| `ACTIVITY_I_REF` | LED 活动量的参考电流（只驱动界面，不影响声音） |

### 3.4 接进电路 crate

把 `circuit/src/lib.rs` 改成（以单份网表为例）：

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod main_circuit {          // 名字随意，与文件名对应即可
    include!("generated/main.rs");
}

/// 插件与 `rustomp.toml` 的 `[circuit]` 用它
pub type Ckt = main_circuit::Ckt;
pub type Params = main_circuit::Params;

/// 参数顺序必须与 `rustomp.toml` 里 `[[params]]` 的声明顺序一致
pub fn make_params(gain: f64, level: f64) -> Params {
    Params { gain, level }
}
```

然后改 `rustomp.toml` 的 `[circuit]`：

```toml
[circuit]
plugin_type = "Ckt"          # 或 "Branch"（第 7 章）
params_type = "Params"
params_builder = "make_params"   # 参数不是平坦结构体时用它按顺序装配
```

> `params_builder` 只在"参数需要转换"时才需要（例如 `Branch` 两路各一份）。
> 参数结构与 `Params` 一一对应时可以省略，由 `rustomp-gen` 直接构造。

### 3.5 第一次让它出声

```bash
cd ../my-drive
cargo test -p my-drive-circuit --release          # 电路 crate 的测试
```

**应该看到**：模板自带的冒烟测试通过（`gain_passes_signal`）。换成生成电路后，
把 `circuit/tests/engine.rs` 改成这颗电路的测试：

```rust
use rustomp_dsp::{Circuit, Engine};
use my_drive_circuit::{make_params, Ckt};
use std::f64::consts::TAU;

#[test]
fn clips_like_the_baseline() {
    const FS: f64 = 48_000.0;
    let mut e = Engine::<Ckt>::new(FS);
    e.set_params(make_params(0.5, 0.5));

    // 预热：跳过过采样滤波器的建立过程
    let mut ph = 0.0;
    for _ in 0..(0.2 * FS) as usize {
        e.process(0.2 * ph.sin());
        ph += TAU * 1_000.0 / FS;
    }

    // 取稳态后若干个周期算 THD / h2..h9（用 rustomp-oracle 的工具函数）
    let mut out = Vec::new();
    for _ in 0..(0.02 * FS) as usize {
        out.push(e.process(0.2 * ph.sin()));
        ph += TAU * 1_000.0 / FS;
    }
    let thd = rustomp_oracle::thd_percent(&out, FS, 1_000.0, 9);
    println!("THD = {thd:.2} %");
    assert!((thd - 20.0).abs() < 5.0, "THD {thd:.2}% 偏离预期");
}
```

三个容易忘的点：

1. **必须预热**（几百毫秒到几秒）：过采样滤波器有延迟，电位器/耦合电容有直流恢复时间常数。
   一个 1 µF × 500 k 的节点要 0.5 s 才稳。
2. **必须用 `--release`**：debug 下电路慢两个数量级，测试会超时。
3. **基线是回归线，不是装饰**：数字变了要么是电路变了，要么是求解器出问题了，两者都必须解释清楚。
4. **DC 工作点要真的收敛**：`net.solve_dc()` 之后顺手看一眼 `net.last_dc_resid`（应 ≤1e-6）
   与 `net.last_dc_retried`（正常应为 `false`）。只看波形很容易漏掉"仿真能跑、工作点在伪解上"
   —— 这类错误以前真的发生过（见 `history/downstream-report-freedist.md` 的 P0）。

### 3.6 与 ngspice 对拍（什么时候值得）

设计器里已经仿真过一轮；要在**代码侧**再对一次，用：

```bash
cd ~/develop/rustomp
./scripts/verify.sh --only oracle        # 框架自己的四层对拍
```

对**你的插件**做整机对拍，最省事的做法是照抄 `sparklin-drive` 的证据链：
把 `ngspice` 跑出的 THD/谐波写进测试注释，再用 `rustomp-oracle` 在 Rust 侧算同一组数字，
断言偏差在 1% 以内。历史数据供参考：同一电路 ngspice 27.5682 % / MNA 27.579 %（差 0.011 pp）。

> `scripts/sim-baseline.sh <目录>` 是**面包板（breadspice）网表**的基线工具，它要求目录里有
> `netlist.cir` 且含 `* probe … V(节点)` 探针。设计器导出的 `.cir` 没有这个注释，
> 用设计器内置仿真即可，不要硬套这个脚本。

### 3.7 漂移守卫：什么时候必须重新生成

| 你改了什么 | 必须重跑 |
|---|---|
| 网表 / 设计图 | `rustomp-import --mna` |
| `rustomp.toml`（身份、参数表、`[ui]`、`[assets]`） | `rustomp-gen` |
| `circuit/src/lib.rs`（手写） | 不用（但可能要改 `[circuit]` 里的类型名，然后再跑 `rustomp-gen`） |

生成物**入库**，`--check` 进 CI/构建脚本：

```bash
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- ../my-drive/circuit/netlist/x.design.json \
    --mna ../my-drive/circuit/src/generated/main.rs --check
cargo run -q -p rustomp-gen -- ../my-drive --check
```

---

## 4. 做面板（可选，但没人愿意用没有界面的插件）

### 4.1 启动面板设计器

```bash
cd ~/develop/rustomp-box-designer
./scripts/dev.sh          # 或按该仓 README：本地服务 + 浏览器
```

### 4.2 三条硬约定（照做就不会糊、不会跳）

| 约定 | 值 | 为什么 |
|---|---|---|
| 坐标 | **毫米**，原点左上，y 向下 | 与 `[ui]` 一致，导出永不缩放 |
| 素材密度 | **12 px/mm**（= 304.8 dpi） | 60 mm 宽的面板 → 720 px；PNG 里的 DPI 元数据是假的 |
| 旋钮行程 | 270°，指针朝上 ⇔ 0.5 | 运行时按 `angle_min/angle_max` 整图旋转 |

面板尺寸随你（Sparklin Drive 是 60 × 112 mm，1590B 竖放）。逻辑窗口 = 毫米 × 6。

### 4.3 导出素材包

顶栏「下载素材包（zip）」，拿到：

```text
panel.svg   panel.png
knob_*.png  toggle_*_up.png  toggle_*_down.png  3pdt_*_hold.png
ui.toml     panel.box.json
```

| 文件 | 去处 |
|---|---|
| `*.png` | 插件项目的 `assets/`（**名字不要改**，`[assets]` 要点名） |
| `panel.svg` | `assets/`（留档；想用 Inkscape 重导 PNG 时用） |
| `panel.box.json` | `design/panel.box.json`（源文件，可回设计器改） |
| `ui.toml` | 覆盖 `rustomp.toml` 的 `[assets]` + `[ui]` 两段 |

**只替这两段**，别动 `[plugin] [au] [build] [circuit] [[params]] [requires]`。

### 4.4 生成面板代码

```bash
cd ~/develop/rustomp
cargo run -q -p rustomp-gen -- ../my-drive
```

多生成三个文件：

| 文件 | 内容 |
|---|---|
| `plugin/src/generated/panel.rs` | `pub const PANEL: PanelSpec`（毫米坐标的旋钮/拨杆/LED/脚踏） |
| `plugin/src/generated/bridge.rs` | `[[params]]` → `ParamAccess`（界面 ↔ 参数的桥） |
| `plugin/src/generated/assets.rs` | `PANEL_PNG` + `IMAGES`（`include_bytes!`） |

缺素材文件在**生成阶段**就报错，不用等 `cargo build` 报 include 路径。

### 4.5 打开 GUI 开关

`rustomp.toml`：

```toml
[build]
gui = true
```

`plugin/Cargo.toml`：

```toml
[dependencies]
nih_plug_egui = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "de42101" }
rustomp-gui = { workspace = true }     # 或按你项目里的路径/git 依赖写法

[features]
default = ["gui"]
gui = []
```

再重跑一次 `rustomp-gen`（它会为 `gui = true` 多生成 `editor_state` 与面板模块）。

### 4.6 手写 `plugin/src/editor.rs`（约 90 行）

这是**唯一必须手写的界面代码**。最小版本（结构与 `sparklin-drive` 一致）：

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui};
use std::sync::{atomic::{AtomicU32, Ordering}, Arc};

use crate::generated::assets::{IMAGES, PANEL_PNG};
use crate::generated::bridge::ParamBridge;
use crate::generated::panel::PANEL;
use crate::generated::params::PluginParams;

static ASSETS: rustomp_gui::Assets = rustomp_gui::Assets { panel: PANEL_PNG, images: IMAGES };

pub fn make(params: Arc<PluginParams>, activity: Arc<AtomicU32>) -> Option<Box<dyn Editor>> {
    create_egui_editor(
        params.editor_state.clone(),
        rustomp_gui::UiState::default(),
        |ctx, state| {
            ctx.set_visuals(egui::Visuals::light());
            rustomp_gui::reset_textures(state);   // 新 Context 后旧纹理句柄失效
        },
        move |ctx, setter, state| {
            let level = f32::from_bits(activity.load(Ordering::Relaxed));
            let access = ParamBridge { params: &params, setter, activity: level };
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                rustomp_gui::draw(ui, &PANEL, &ASSETS, state, &access, level);
            });
        },
    )
}
```

要点：**不写死素材名**（由 `assets.rs` 提供）、**不写参数 match**（由 `bridge.rs` 提供）。
历史上这两处各漏过一次，症状分别是"设计器一改名就编译不过"和"点了没反应"。

### 4.7 面板自检

```bash
cd ../my-drive
cargo test -p my-drive --features gui --release
```

加一条"面板自洽"测试（照抄 `sparklin-drive` 的 `generated_panel_is_valid`）：

```rust
let errs = rustomp_gui::validate_panel(&PANEL, &ASSETS);
assert!(errs.is_empty(), "面板校验失败：{errs:#?}");
```

它检查：参数 id 存在且类型匹配、贴图名都在、坐标不越界、旋钮角度对称。
**编译期 + 单测**两道关卡，比在宿主里一个个点过去快得多。

---

## 5. 生成插件外壳（`rustomp-gen`）

### 5.1 先改 `rustomp.toml` 的这几段

```toml
[plugin]
package = "my-drive"                      # cargo 包名：小写短横线
name = "My Drive"                         # 宿主里显示的名字
vendor = "ACME AUDIO"
url = "https://github.com/you/my-drive"
clap_id = "com.acme.my_drive"             # 变体会追加 .mono / .stereo
vst3_class_id_mono = "MyDriveXXXXXM01"    # 恰好 16 字节，发布后不可改
vst3_class_id_stereo = "MyDriveXXXXXS01"

[au]
manufacturer_code = "Acme"                # AU 四字符制造商码，发布后不可改
subtype_dual = "Md01"                     # 三个 subtype 必须互不相同
subtype_mono = "Md02"
subtype_stereo = "Md03"
bundle_id = "com.acme.my_drive"

[build]
variants = ["mono", "stereo"]
dual_variant = false
gui = true                                # 第 4 章做完面板再打开

[requires]
framework  = ">=0.1,<0.2"                 # 必须包含当前框架版本
packer     = ">=0.1"
packer_tag = "v0.1.7"                     # 本版用过的打包器 tag（提示里会引用）

[[params]]                                # 顺序 = make_params 的位置参数顺序
id = "gain"
name = "Gain"
type = "float"                            # 旋钮用 float，拨杆/脚踏用 bool，LED 用 indicator
default = 0.5
range = [0.0, 1.0]
format = "percent"
circuit = "gain"                          # 对应 Params 的字段名
```

`rustomp-gen` 会在生成**之前**一次报出全部问题（不是只报第一个）：

| 校验 | 规则 |
|---|---|
| `package` | kebab-case |
| VST3 id | 每个变体**恰好 16 字节** |
| 参数 id | 唯一、合法 Rust 标识符；`bypass` 是保留 id |
| `circuit` 字段 | 必须存在，且不被两个参数映射 |
| `gui = true` | 必须有 `[ui]` |
| `[requires] framework` | 必须包含当前框架版本 |

### 5.2 生成 + 防漂移

```bash
cd ~/develop/rustomp
cargo run -q -p rustomp-gen -- ../my-drive            # 写文件
cargo run -q -p rustomp-gen -- ../my-drive --check    # 只比对（CI / 构建脚本用）
```

### 5.3 生成物的分工（**决定你能不能改**）

| 文件 | 来源 | 能手改吗 |
|---|---|---|
| `plugin/src/generated/identity.rs` | `[plugin] [au]` | ❌ 改 `rustomp.toml` 后重生成 |
| `plugin/src/generated/params.rs` | `[[params]]` | ❌ 同上 |
| `plugin/src/generated/plugin_impl.rs` | 全部 | ❌ 同上 |
| `plugin/src/generated/panel.rs` / `bridge.rs` / `assets.rs` | `[ui]` / `[[params]]` / `[assets]` | ❌ 同上 |
| `plugin/src/lib.rs` | 模板 | ✅ 手写（很短，只 re-export） |
| `plugin/src/editor.rs` | 你新建（第 4.6 节） | ✅ 手写 |
| `circuit/src/generated/*.rs` | 网表 | ❌ 改网表后重生成 |
| `circuit/src/lib.rs`、`tests/*` | 你 | ✅ 手写 |

> 一句话：**`generated/` 里的一切都由"上游文件"决定**，上游是 `rustomp.toml` 或网表。

---

## 6. 编译、打包、安装

### 6.1 三种粒度，从快到全

| 目的 | 命令 | 出什么 |
|---|---|---|
| 只检查能不能编过 | `cd ../my-drive && cargo build --release -p my-drive` | Rust 库（没有插件包） |
| 出 VST3/CLAP 并安装 | `cd ../rustomp && ./scripts/build-all.sh ../my-drive` | 本平台默认格式 + 装到用户目录 |
| 出指定格式/变体/安装包 | `./scripts/build-all.sh ../my-drive --variant stereo --format au --no-install` | 只出你要的 |

> **`build-all.sh` 不会替你重新生成代码**，也不会跑 `--check`。
> 想让它"顺手校验"，把两条 `--check` 写进你项目的 `build.sh`（`sparklin-drive/scripts/build.sh`
> 就是这么做的），或者养成"改上游文件后先重生成"的习惯。

### 6.2 打包参数（三平台同形，契约 B）

```bash
./pack.sh <plugin_dir> [--variant mono|stereo|dual] [--format au|vst3|clap|dmg|msi|targz] [--no-install]
```

| 平台 | 入口 | 默认格式 | 默认安装到 | 安装包 |
|---|---|---|---|---|
| macOS | `rustomp-mac-plugin/pack.sh` | AU + VST3 + CLAP（universal） | `~/Library/Audio/Plug-Ins/` | `--format dmg` → `<Name> Installer.dmg` |
| Windows | `rustomp-win-plugin/pack.sh` | VST3 + CLAP（x64） | `%LOCALAPPDATA%\Programs\Common\` | `--format msi` → 系统级 + 用户级两个 MSI |
| Linux | `rustomp-linux-plugin/pack.sh` | VST3 + CLAP | `~/.vst3` `~/.clap` | `--format targz` |

macOS 两个环境变量值得知道：

| 变量 | 默认 | 说明 |
|---|---|---|
| `MACOS_DEPLOYMENT_TARGET` | `11.0` | **不要留空**：留空会触发 clap-wrapper 的 CPM 联网下载，并写进 AU 的最低系统版本 |
| `RUSTOMP_BUILD_JOBS` | `4` | `cmake --build -j` 并行度。机器弱/有过热史就调到 `2` |

### 6.3 看产物：只认 `ARTIFACT:`

成功时 stdout **只有** `ARTIFACT:` 行（日志全走 stderr，便于脚本抓取）：

```text
ARTIFACT: /Users/me/develop/rustomp-mac-plugin/build/au-mono/My Drive Mono.component
ARTIFACT: /Users/me/develop/my-drive/target/bundled/My Drive Mono.vst3
ARTIFACT: /Users/me/develop/my-drive/target/bundled/My Drive Mono.clap
ARTIFACT: /Users/me/develop/my-drive/target/bundled/My Drive Stereo.clap
```

看不到 `ARTIFACT:` = 没成功，别去猜路径。

### 6.4 macOS 的 AU 自检（Logic 用户重点看）

```bash
cd ~/develop
./rustomp-mac-plugin/scripts/check-au.sh mono      # 安装时间 / 签名 / 声道 / 渲染 / Logic 缓存
auval -v aufx <subtype> <manufacturer>             # 直接问系统：这个 AU 合不合规
./rustomp-mac-plugin/scripts/reset-logic-cache.sh  # Logic 菜单不对时（先 ⌘Q 退出 Logic）
```

`check-au.sh` 是**只读**的：先告诉你插件本身能不能渲染，再决定要不要清缓存。
典型症状与原因：

| 症状 | 原因 |
|---|---|
| Logic 里看不到插件 | 身份四字符码写错 / 缓存太旧 → 先 `auval` 再清缓存 |
| 显示了旧名字 | 改过 `rustomp.toml` 但没重生成，或缓存 |
| 能扫到但没声音 | 用了 Mono 插件在立体声通道（或反之） |

### 6.5 机器负载：别把构建拉满

插件构建会同时跑 `cargo`（nih-plug + egui）和 `cmake`（AU 外壳）。并发拉满在弱机/过热机上
很容易触发内存与 IO 峰值：

```bash
nice -n 19 ./scripts/build-all.sh ../my-drive          # 降优先级
RUSTOMP_BUILD_JOBS=2 ./scripts/build-all.sh ../my-drive # 降 cmake 并行
CARGO_BUILD_JOBS=2 ./scripts/build-all.sh ../my-drive   # 降 cargo 并行
```

不要 `cargo clean` / `rm -rf target` 来解决构建问题 —— 增量重编通常就够了。

> **实时 CPU 太大怎么办**（第 3 章生成的电路）：先按 2× 估开销（`oversample-2x` feature），
> 再考虑 `newton=` 预算与 `strip_bjt_body_resistance()`。经验：BJT 数量与未知量个数是主导项，
> 4× 过采样直接翻倍。细节见 `crates/rustomp-dsp/README.md` §5.2。

### 6.6 无界面构建（排障用）

界面出问题、想确认"是不是 GUI 的锅"：

```bash
cd ../my-drive
cargo build --release -p my-drive --no-default-features
```

如果无界面版能出声、带界面版不出声，问题几乎一定在 `editor.rs` 或素材（而不是 DSP）。

---

## 7. 进阶：一个开关（多档 / 多拓扑）

**设计器不画开关**（D-007）。做法是"**两份完整网表**"，插件层做切换：

1. 在设计器里画两张图（或复制工程后改元件），导出 `asym.design.json` / `sym.design.json`；
2. 各自生成代码：

```bash
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- ../my-drive/circuit/netlist/asym.design.json \
    --mna ../my-drive/circuit/src/generated/asym.rs
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- ../my-drive/circuit/netlist/sym.design.json \
    --mna ../my-drive/circuit/src/generated/sym.rs
```

3. `circuit/src/lib.rs` 组装：

```rust
pub mod asym { include!("generated/asym.rs"); }
pub mod sym  { include!("generated/sym.rs"); }

pub type Branch = rustomp_dsp::Branch<asym::Ckt, sym::Ckt>;
pub type Params = rustomp_dsp::BranchParams<asym::Params, sym::Params>;

/// 顺序必须与 [[params]] 声明一致：gain, symmetric, level
pub fn make_params(gain: f64, symmetric: bool, level: f64) -> Params {
    Params {
        a: asym::Params { gain, level },
        b: sym::Params  { gain, level },
        select_b: symmetric,
    }
}
```

4. `rustomp.toml` 加一个 bool 参数（面板上绑拨杆）：

```toml
[[params]]
id = "symmetric"
name = "Clip Mode"
type = "bool"
default = false
```

并设 `[circuit] plugin_type = "Branch"`、`params_type = "Params"`、`params_builder = "make_params"`。

`Branch<A,B>` 自带**预热 + 交叉淡化**（约 10 ms），两路电路在后台都持续运行，所以切换不咔哒、
也没有"冷启动"的直流跳变。代价是 CPU ≈ 两倍。

---

## 8. 发布（可选）

```bash
# 1. 先在本机把该平台产物跑通（见第 6 章）
# 2. 打 tag 并推
git tag v0.1.0 && git push origin v0.1.0
```

三平台发布流水线（参考 `sparklin-drive/.github/workflows/release.yml`）：
tag `v*` → macOS / Windows / Linux 各自构建 → 汇总改名 + `SHA256SUMS.txt` → GitHub Release
（**prerelease / 未签名**）。`workflow_dispatch` 可 dry-run，也能对已有 tag 补发。

签名现状：mac 只做 ad-hoc（`codesign -s -`）、Windows 无 Authenticode。
**对外正式分发需要 Developer ID + 公证**，否则 macOS 会拦（Gatekeeper）、Windows 会弹 SmartScreen。

细节见 [`declarative-build.md`](declarative-build.md) §4 与三个打包仓的 README。

---

## 9. 全流程检查清单

**画图**

- [ ] 有 `In` / `Out` / 地；端口在正确的网络上
- [ ] 无悬空引脚、无斜线、十字交叉处确认是否**需要**连接
- [ ] 电位器字段名 = 最终参数 id；`invert` 方向对
- [ ] 型号全部在闭集里
- [ ] `tran` 能出声，`ac` 通带合理，方波不出现"基频以下隆起"

**生成电路**

- [ ] `--mna` 生成无警告；`--check` 通过
- [ ] `circuit/src/lib.rs` 里 `Ckt`/`Params`/`make_params` 与 `rustomp.toml [circuit]` 对得上
- [ ] 参数顺序 = `[[params]]` 声明顺序
- [ ] `cargo test -p <slug>-circuit --release` 通过，THD/谐波在预期区间
- [ ] 预热时间足够（耦合电容 × 电位器的时间常数）

**面板**

- [ ] `assets/` 里 PNG 齐全，`[assets]` 点名一致
- [ ] `[ui]` 坐标是毫米，12 px/mm，旋钮 270°、指针朝上 ⇔ 0.5
- [ ] `validate_panel` 单测通过
- [ ] `editor.rs` 不写死素材名、不写参数 match

**外壳**

- [ ] VST3 id 恰好 16 字节；AU 三个 subtype 互不相同
- [ ] `rustomp-gen --check` 通过
- [ ] `gui = true` 时确有 `[ui]`，且 `plugin/Cargo.toml` 打开 `gui` feature

**打包 / 宿主**

- [ ] `build-all.sh` 打出 `ARTIFACT:` 列表
- [ ] 宿主能扫到 **Mono 与 Stereo 两个身份**
- [ ] 能出声；旁通是真旁通；切档不咔哒；LED 跟着演奏
- [ ] 延迟 15 样本（`latency_is_reported`）
- [ ] CPU 可接受（4× 过采样 + 可能的两路 Branch）

---

## 10. 排错：症状 → 原因 → 处理

| 症状 | 最可能的原因 | 处理 |
|---|---|---|
| `cannot specify features for packages outside of workspace` | 插件项目放进了别的 Rust 目录树 | 移到框架同级 |
| `找不到 rustomp 框架` / `找不到打包器` | 目录不同级 | 按提示 clone 到同级；或 `RUSTOMP_PACKER` / `RUSTOMP_<平台>_PLUGIN` 指定 |
| `RUSTOM_DESIGNER_SYMBOLS` 找不到 | 变量名拼成 `RUSTOMP_…`，或路径不对 | 用 `RUSTOM_DESIGNER_SYMBOLS=<设计器>/symbols/symbols.json` |
| 导入报"未知型号" | 型号不在闭集 | 换闭集型号，或去框架 `devices/` 标定并加对拍 |
| 参数拖动没反应 | `param` 的元素名写错（写了位号 `P1A`） | 改成展开后的元件名 `RP1A`，重生成 |
| 生成的 `Ckt` 无视输入（输出是纯直流） | 网表缺 `in=`，或 `in=` 写成了不存在的节点 | 补/改 `* @rustomp in=IN out=OUT` |
| 方波听起来"嗡嗡"、频谱基频以下有隆起 | 牛顿迭代不足（欠收敛） | 网表加 `* @rustomp newton=8`，重生成 |
| 音量旋钮拧到头静音 | 电位器端点算出 0 Ω → 导纳无穷大 | 框架已夹到 1 mΩ；确认用的是 `v0.1.3` 及以上 |
| `rustomp-gen` 报 `[assets] … 指向的文件不存在` | 只贴了 `ui.toml`，没放 PNG | 从素材包补齐 `assets/` |
| 生成物与 manifest 不一致 | 改了 `rustomp.toml`/网表没重生成 | 重跑 `rustomp-gen` / `rustomp-import --mna` |
| `requires.packer` 不满足 | 打包器版本旧 | checkout 到插件 `[requires] packer_tag` |
| 面板糊 | `panel.png` 尺寸小于 面板宽 mm × 12 | 用设计器的素材包，别手工缩图 |
| 拨杆切换时图形跳 | 手改成 `y_up == y_down` | 两态 y 差是六边形基线补偿，别"对齐" |
| 界面不出现 | `gui = false`、缺 `[ui]`、或没开 feature | 三处对齐（第 4.5 节） |
| 构建报 CMake `CPMAddPackage` | 没跑 `fetch-third-party.sh` / `MACOS_DEPLOYMENT_TARGET` 为空 | 补跑并 export 11.0 |
| `no such command: nih-plug` | 没装或 rev 不对 | `--rev de42101` 重装 |
| Logic 里看不到 / 旧名字 | 宿主持久化缓存 | `check-au.sh` → `reset-logic-cache.sh`（先退出 Logic） |
| 插件能扫到但没声音 | Mono/Stereo 与通道不匹配 | 换对应的变体 |
| 偏置网络"被钉死"、工作点完全不对 | 电位器写成 `1M`（= 1 mΩ） | 改成 `1meg`；导入器现在会直接报错 |
| DC 工作点可疑、想确认求解是否收敛 | 需要看残差 | `RUSTOMP_DC_DEBUG=1` 打印 `resid/iters/retried`；或读 `Netlist::last_dc_resid` |
| 偶发"丢拍"、输出突然静一下 | `Engine` 的 NaN 熔断在工作 | 读 `Engine::guard_trips()` / `last_guard_value()`；`RUSTOMP_GUARD_LOG=1` 直接打印 |
| `doctor` 全绿但构建报 `E0463: can't find crate for core` | PATH 里有多个 rust（Homebrew + rustup） | `export PATH="$HOME/.cargo/bin:$PATH"` 统一工具链；mac 打包器新版会直接指出这一点 |

---

## 11. 速查

### 11.1 命令清单（在框架仓执行，`P=../my-drive`）

```bash
# 环境
cargo test --workspace
./scripts/verify.sh --only fast | --only oracle

# 建项目
./scripts/new-plugin.sh "My Drive" [--vendor "ACME"]

# 电路
RUSTOM_DESIGNER_SYMBOLS=../rustomp-circuit-designer/symbols/symbols.json \
  cargo run -q -p rustomp-import -- $P/circuit/netlist/x.design.json --mna $P/circuit/src/generated/x.rs [--check]

# 外壳
cargo run -q -p rustomp-gen -- $P [--check]

# 打包
./scripts/build-all.sh $P [--variant mono|stereo] [--format au|vst3|clap|dmg] [--no-install]

# 设计器
(cd ../rustomp-circuit-designer/backend && cargo run)
(cd ../rustomp-circuit-designer/frontend && npm install && npm run dev)
```

### 11.2 每一层"谁说了算"

| 事实 | 唯一来源 | 别在别处改 |
|---|---|---|
| 插件身份（名字/AU 码/VST3 id） | `rustomp.toml` 的 `[plugin] [au]` | `generated/identity.rs` |
| 参数表 | `rustomp.toml` 的 `[[params]]` | `generated/params.rs`、`bridge.rs` |
| 面板布局 | `rustomp.toml` 的 `[ui]`（由设计器产出） | `generated/panel.rs` |
| 素材 | `assets/*.png` + `[assets]` | `generated/assets.rs` |
| 电路拓扑 | `.design.json` / `.cir` | `circuit/src/generated/*.rs` |
| 器件模型 | 框架 `devices/`（闭集） | 任何地方的型号字符串 |
| 数值方法（牛顿/过采样/旁通） | 框架 `rustomp-dsp` | 插件里的手写 DSP |

### 11.3 还想读什么

| 想知道 | 去哪 |
|---|---|
| 三个跨仓契约、8 个仓的边界 | [`ecosystem.md`](ecosystem.md) |
| 网表指令的完整语义与映射规则 | [`circuit-import.md`](circuit-import.md) |
| 器件精度档位、怎么加新型号 | [`component-library.md`](component-library.md) |
| 生成平面 vs 打包平面、平台矩阵 | [`declarative-build.md`](declarative-build.md) |
| 验证体系与实测数字 | [`verification.md`](verification.md) |
| 面板 `[ui]` 每个字段 | `rustomp-box-designer/docs/01-contract-panel-toml.md` |
| 导出 → 装进插件的原始 runbook | `rustomp-box-designer/docs/06-export-runbook.md` |
| 设计器的交互模型与决策 | `rustomp-circuit-designer/docs/00-decisions.md`、`03-editor-model.md` |
| 一个完整可跑的样板项目 | [`sparklin-drive`](https://github.com/amonisachicken/sparklin-drive) |
| **真实踩坑案例（P0–P6）与处置** | [`history/downstream-report-freedist.md`](history/downstream-report-freedist.md) |

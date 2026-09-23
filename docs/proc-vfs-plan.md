# /proc 虚拟文件系统最小仿真计划（Windows）

> 立项：2026-09-23。定位：userland 增值层，**不属 GNU 兼容契约**（GNU bash 在 Windows
> 上同样没有 /proc），**不进 83 套件账本**。
> 设计原则：复刻 `/dev` 虚拟路径体系的既有四层架构，不新造机制。
> 任务板条目：TASKBOARD Q11。

## 0. 目标场景（验收即这些命令工作）

```bash
cat /proc/stat                      # 含 cat 被 alias 到第三方（bat/node）的场景
read line < /proc/loadavg           # 重定向
while read cpu; do ...; done < /proc/stat
mapfile -t rows < /proc/meminfo
grep MemTotal /proc/meminfo         # 引擎内工具族（external_file_builtins）
sed -n 1p /proc/cpuinfo
python script.py < /proc/cpuinfo    # 第三方子进程经重定向继承句柄
nproc / free / uptime               # winuxcmd 工具共用同一数据源（后续）
```

范围外：`/proc/<pid>/...` 动态进程树（无 killer use case 不做）；
第三方程序**自发** open（`python -c "open('/proc/stat')"`，P3 ProjFS 调研项）。

## 1. 现状盘点（/dev 先例，代码证据）

| 层 | 现有实现 | /proc 对应工作 |
|---|---|---|
| fd 别名 | `execution_misc.rs:243-249`（`/dev/stdin\|stdout\|stderr`、`/dev/fd/N` → fd 端点）| A1：补 `/proc/self/fd/N`、`/proc/self/fd/0-2`；`source.rs:150`、`test.rs:551` 已把 `/proc/self/fd/0` 认作 `/dev/stdin` 别名，需收编到统一入口 |
| 语义合成 | `/dev/null`（空）、`/dev/tty`（`external_file_builtins.rs:49` 终端探测）| B：新内容合成模块（见 §2） |
| 参数翻译谓词 | `path.rs:718-762 windows_external_absolute_argument_needs_translation`：`/dev`、`/tmp`、`/var/tmp`、`/home`、`/mnt/X`、`bin\|etc\|usr\|var...` 各有分支 | C：**当前无 `/proc` 分支** → `/proc/stat` 原样传给外部子进程 → `os error 3`（bat 报错根因）|
| 引擎内工具族 | `external_file_builtins.rs:13-38`：cat/sed/mkdir/touch/chmod/cp/rm/rmdir/mkfifo/tty/pwd/printf 原生执行 | B2：文件操作数读取共用合成模块 |

## 2. 架构：一个合成模块，三个挂接点（全部在引擎内）

```
                    proc_file_content(path) -> Option<Vec<u8>>
   cpuinfo ← GetLogicalProcessorInformation      meminfo ← GlobalMemoryStatusEx
   stat    ← GetSystemTimes                      loadavg ← processor queue length
   uptime  ← GetTickCount64                      version ← 合成
   格式纪律：字段名/行格式对齐 Linux 下游 grep 习惯
   （"MemTotal:"、"cpu0 ..."、"procs_running"、nproc 数 processor 行）

  挂接点 A（fd 别名）：execution_misc redirect_target_fd + source/test 收编
  挂接点 B（open 咽喉）：FdTable::open_read (fd/mod.rs:163)
                        + external_file_builtins 文件操作数读取
  挂接点 C（参数经纪人）：path.rs 谓词加 /proc 分支
                        → 翻译目标 <NIU_SHELL_ROOT>\proc\<name>
                        → 翻译时物化（原子写：tmp+rename）
```

挂接点 C 是覆盖第三方的关键：rubash 路径模型本来就是外部命令参数的
**路径经纪人**（POSIX 形态 → Win32 路径）。`bat /proc/stat` 在翻译瞬间把
合成内容物化成真实文件，bat 收到的是一个真实存在的 Win32 路径——
**第三方程序零感知，无需 ProjFS**。物化目录 `<root>\proc` 在 shell 初始化时创建。

## 3. 阶段

| 阶段 | 内容 | 验收 |
|---|---|---|
| **P1 引擎内语义** | 合成模块 + 挂接点 A、B | 每文件字段格式单测（对齐 Linux 惯例字段名）；`read/mapfile/重定向/external_cat/sed` 读 /proc 的 cli tests；`test -r /proc/cpuinfo` 语义 |
| **P2 第三方参数面** | 挂接点 C：谓词分支 + 物化目录 | cli test：真实外部 exe 以 `/proc/stat` 为参数能读出内容（物化路径）；手工探针：bat /proc/stat、node script.js /proc/meminfo |
| **P3 全透明（可选）** | ProjFS（cldflt.sys）把 `<root>\proc` 投影为系统级虚拟目录 | 仅覆盖"第三方自发 open"（python open('/proc/stat')）；独立立项，P2 落地后按需求评估 |

依赖关系：P1 可立即（合成模块与 /dev 层同域，注意与 Q1/Q2 准入守卫批次防冲突）；
P2 依赖 P1；P3 独立。

## 4. 边界与纪律

### 4.0 平台门禁纪律（硬性）

- 合成模块 `proc_file_content` **整体 `#[cfg(windows)]`**；Unix 侧 fallback 为
  `return None`（路径原样放行到真实文件系统）
- **Unix 上禁止拦截 /proc**——Linux/macOS 有真实内核 procfs，引擎合成层一旦
  生效就是**遮蔽真文件系统**，是语义正确性问题而非编译卫生问题
- 挂接点 B（`FdTable::open_read`）所在的 fd 模块本就是 `#[cfg(windows)]` 门内
  模块，P1 挂接天然 Windows-only；Unix 的重定向走 std fs 路径，无需合成
- 反面教材（现状，2026-09-23 实测）：`crate::fd` 在 lib.rs 是 windows 门内模块，
  但 11 个消费者文件 **67 处无条件 `use crate::fd`** → Linux 交叉编译 80 错、
  CI ubuntu job 连红。**引用 gated 模块的消费者必须同步 gated**
- 新代码纪律：触碰 Win32 FFI 的模块必须模块级或项级 cfg；CI 加
  `cargo check --target {x86_64-unknown-linux-gnu, aarch64-apple-darwin}` 门禁
  （host-contract §3 已计划），Q12 跟踪修复

### 4.1 其余边界

- **不进 83 账本**：GNU 兼容契约不含 /proc；这是 niu/winuxcmd userland 的增值层
- 合成逻辑**集中一个模块**，禁止在工具内散落特判（对齐 C 类桩禁令）
- `/proc/self` P2 内只做 fd 子集（fd 别名层）；pid 目录语义不在范围
- 物化写入用原子写防并发 shell 竞态；`NIU_SHELL_ROOT` 不可写时 P2 优雅降级
  （跳过物化，仅引擎内挂接点可用，不报错）
- 附带红利（后续独立任务）：`nproc`/`free`/`uptime` 与合成模块共用数据源；
  niu 侧不再需要为 alias 劫持（cat→bat）做任何事——P2 本身就是答案

## 5. 风险

| 风险 | 处置 |
|---|---|
| 下游解析器假设 per-cpu 行存在 | stat/cpuinfo 产出 per-processor 行（Toolhelp/GetLogicalProcessorInformation 枚举）|
| loadavg 无内核对应物 | 用 processor queue length 合成，文档标注近似值 |
| 物化路径与用户真实 `<root>\proc` 冲突 | 初始化时检测，冲突则 P2 降级并告警一次 |
| `test -s /proc/*` 语义 | 合成内容非空 → -s 为真，与 Linux 一致；纳入 P1 单测 |

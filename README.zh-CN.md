# Rubash

使用 Rust 从零实现的 GNU Bash 兼容 Shell。

[English](README.md)

[![CI](https://github.com/unixwin/rubash/actions/workflows/ci.yml/badge.svg)](https://github.com/unixwin/rubash/actions/workflows/ci.yml)
[![Rust Version](https://img.shields.io/badge/rust-1.70+-blue)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

## 什么是 Rubash

Rubash 是用 Rust 从零实现的 GNU Bash —— 词法分析、解析器、展开引擎、执行器、内建命令，全部重写。目标是与 GNU Bash 5.3.0 逐字节兼容，原生运行在 Windows 上。

**当前状态**：83 个 GNU Bash 上游测试套件中 43 个零差异通过。全部 83 套件总差异 2702 行 —— 其中 `intl` 单套件占 1209 行（ANSI-C `$'...'` 载体字节架构缺口，见下文）。排除 `intl` 后余 39 套件共 1493 行，7 天内从 3427 行下降 57%。完整详情见 [`docs/COMPATIBILITY-STATUS.md`](docs/COMPATIBILITY-STATUS.md)。

## 兼容性一览

```
GNU Bash 5.3.0 测试套件 — 83 个文件，true-baseline 实测
（台账：2026-09-16 全量复核）

  零差通过：      43 套件  █████████████████░░░░░░░░░░░░░░  52%
  小差异(1-50)：  28 套件  ███████████░░░░░░░░░░░░░░░░░░░░  34%
  中差异(51-250)：11 套件  ████░░░░░░░░░░░░░░░░░░░░░░░░░░  13%
  大差异(251+)：   1 套件  █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   1%
  ────────────────────────────────────────────────────────────────
  总差异：        2702 行（intl=1209，排除 intl 后 1493 行）
  9月9日为 3427 行 → 排除 intl 后 7 天内 −57%
```

### 完全通过的套件（零差异）

`appendop` `arith-for` `attr` `builtins` `case` `casemod` `comsub-eof` `complete` `cprint` `dbg-support` `dbg-support2` `dstack` `dstack2` `dynvar` `exportfunc` `extglob2` `extglob3` `func` `getopts` `glob-bracket` `heredoc` `herestr` `ifs` `invert` `lastpipe` `mapfile` `nquote1` `nquote2` `nquote3` `nquote4` `nquote5` `parser` `posixexp2` `posixpat` `precedence` `printf` `quote` `rhs-exp` `rsh` `strip` `tilde` `tilde2` `trap`

### 近期重大修复（2026 年 9 月）

| 领域 | 修复前 → 修复后 | 改了什么 |
|------|----------------|---------|
| **dbg-support** | 635 → 0 | AND 列表双触发、source-scope trap 继承、`{` 回归 |
| **rsh** | 194 → 0 | `set +o restricted` 静默解除修复、受限 shell 全链路 |
| **invocation** | 14 → 0 | `BASH_ARGV0`、长选项表、`--pretty-print`、`-o`/`-O` 启动报错 |
| **trap** | 3 → 0 | ERR 行号绑定、SIGCHLD 排队、后台子进程 trap 隔离 |
| **func** | 58 → 0 | POSIX funcname 规则、AST printer、special-builtin 优先级 |
| **complete** | 115 → 0 | 多操作数 compspec 注册 |
| **history** | 190 → 127 | `history -d start-end` 范围删除（GNU 5.3 特性） |
| **globstar** | 182 → 101 | 多重性修复、相邻 `**` 折叠、尾斜杠语义 |
| **array/assoc** | 444+358 → 148+187 | 复合赋值引号分组、`"$@"`/`$0` 展开、算术下标副作用（`count++`） |
| **信号表** | BSD 表 → Linux 表 | USR1=10、CHLD=17、RTMIN=34，与 GNU 5.3.0 WSL 契约一致 |

### 本轮修复（2026-09-16 — PR #111 + 本地批次合入 master）

| 领域 | 修复前 → 修复后 | 改了什么 |
|------|----------------|---------|
| **CRLF 脚本（niubash #106）** | v1.1.2 回归 → 已修 | 词法器行切分时把 `\r\n` 作为整体行终止符剥掉（主循环 + heredoc body，`<<EOF` 分隔符恢复匹配）；孤立 `\r`（后不跟 `\n`）仍保留为词文本，GNU 保真场景不丢 |
| **`-c` 选项解析（niubash #107）** | 损坏 → GNU 一致 | `-c` 取「第一个非选项参数」作为命令串；`bash -c -l 'script'` 可用，AI agent/调用方不再被挡；裸 `bash -c` 保持 GNU 用法报错（rc 2） |
| **`type` 输出捕获（niubash #108）** | 泄漏 → 捕获 | `$(type -t ls)` 现在正确返回 `file`，不再打到进程 stdout 并赋空串 |
| **nameref** | 558 → 226 diff 行（run-83 check） | 间接展开、unset 传播、作用域修复（本地批次） |
| **history** | 323 → 250 diff 行（run-83 check） | 同 shell 嵌套脚本输出顺序、IFS 隔离修复（本地批次） |
| **`$( )`/`printf` 退出路径的 trap** | 调试残留清除 | 合入前剥掉 WIP 遗留的 `[DEBUG]` eprintln 插桩 |

### Rubash 已经能跑什么

- **bashdb** — 核心调试闭环（list、step、next、where、continue、quit）在 rubash 下工作
- **复杂 Bash 脚本** — 数组、关联数组、算术、条件、nameref、命令替换、花括号展开、进程替换、coproc、`eval`、`trap`、`source`
- **GNU Bash 测试套件** — 83 个上游测试文件，自动化 diff 测量

## 快速开始

### 从源码构建

```bash
git clone https://github.com/unixwin/rubash.git
cd rubash
cargo build
target/debug/rubash --version
```

### 运行脚本

```bash
target/debug/rubash path/to/script.sh
target/debug/rubash -c 'echo hello from rubash'
```

### 运行兼容性测试套件

```bash
# 全量 83 套件测量（需要 WSL + GNU Bash 5.3.0）
MSYS_NO_PATHCONV=1 wsl bash scripts/true-baseline.sh

# 单个套件
MSYS_NO_PATHCONV=1 wsl bash scripts/true-baseline.sh array
```

## 架构

```
src/
├── lexer/           词法分析器（引号、转义、heredoc、续行）
├── parser/          递归下降（简单命令、管道、case、arith-for、[[ ]]）
├── executor/        命令执行、内建命令、展开、glob、数组、trap
├── builtins/        40+ 内建命令实现（declare、read、printf、kill、...）
└── lib.rs           核心类型和错误处理
```

- **词法分析器**：Bash 风格引号、转义、注释、变量、命令替换、算术展开、here-doc/here-string token、常见重定向。
- **解析器**：简单命令、管道、AND/OR 列表、函数、花括号/子 shell 组、`if`、`for`、算术 `for`、`while`、`until`、`case`、`select`、`[[ ... ]]`、`coproc`、`time` 前缀。
- **执行器**：外部命令、管道、重定向、临时赋值、函数调用、`source`/`.`、`eval`、无 shebang 脚本回退、Windows/Git Bash 路径桥接。
- **展开系统**：变量、位置参数、索引/关联数组、命令替换、算术展开、花括号展开、tilde 展开、路径名 glob、`${parameter...}` 操作符、大小写/替换变换。
- **内建命令**：`alias`、`cd`、`declare`/`typeset`/`local`、`echo`、`eval`、`exec`、`export`/`readonly`、`getopts`、`hash`、`jobs`、`kill`、`let`、`mapfile`、`printf`、`pushd`/`popd`/`dirs`、`read`、`return`、`set`、`shopt`、`source`、`test`/`[`、`trap`、`type`、`ulimit`、`umask`、`unset`、`wait` 等。

## 测试

```bash
# 单元 + 集成测试
cargo test --lib

# bashdb 兼容性
cargo test --test cli_tests bashdb_compat -- --nocapture

# source 展开
cargo test --test cli_tests source_expands -- --nocapture
```

## 文档

- [`docs/COMPATIBILITY-STATUS.md`](docs/COMPATIBILITY-STATUS.md) — **唯一权威来源**，Rubash ↔ GNU Bash 兼容性状态
- [`docs/builtins.md`](docs/builtins.md) — 内建命令清单和分发模型
- [`docs/bashdb-debugging-rubash.md`](docs/bashdb-debugging-rubash.md) — bashdb fixture 设置和 smoke test
- [`docs/bash-upstream-tests.md`](docs/bash-upstream-tests.md) — 如何运行 GNU Bash 上游测试

## 开发原则

- 按 Bash 语义的 root cause 修 Rubash 子系统，不按单条 expected output 打补丁。
- bashdb 保持外部 clean 工具；临时 instrumentation 仅用于诊断。
- 每个失败的 bashdb 命令都是发现和修复 Rubash 兼容性缺口的机会。
- 兼容性基线为 GNU Bash 5.3.0（业主编译于 `/usr/local/bin/bash`）。

## 许可证

MIT — 详见 [`LICENSE`](LICENSE)。

## 贡献

欢迎提交 issue、兼容性复现、focused regression tests 和实现补丁。贡献前请阅读 [`AGENTS.md`](AGENTS.md)。

## 致谢

- GNU Bash 团队 — 被重新实现的原始实现
- Trepan-Debuggers/bashdb — 外部调试器和兼容性压力测试
- Rust 社区 — 语言和工具链

---

*最后更新：2026-09-11*

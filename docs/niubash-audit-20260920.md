# niubash × rubash 引擎使用审计（2026-09-20）

> 审计对象：`D:/repo/niubash-multiline`（最新 master 4289da5，锁 rubash `0fd42049`）、
> `D:/repo/niubash-issues`（`fix/niu-issues` e88f19a，落后 origin/master 7 提交，
> 且工作树有未提交的 Cargo.lock 回退 5eaae06c→8b81c750）。
> 结论先行：**薄壳纪律执行不佳**——`shell.rs` 8600 行，执行入口堆了 5 个预处理
> pass、3 条 simple-AST 快路径、至少 4 份手写引号状态机。

## A. 问题清单（按严重度）

| # | 严重度 | 位置 | 问题 | 去向 |
|---|---|---|---|---|
| A1 | **严重**（**引擎根因已于 2026-09-22 解决**：rubash#117 白名单正解 2d7c973a，补偿 pass 不再有存在必要；niu 侧删除见 G3） | `crates/niubash-runtime/src/shell.rs:685-696 / 2650-2663 / 2398-2412` | 5 连预处理 pass：`protect_parameter_pattern_removal_equals`（tokenize 前手写引号状态机**改源文本**）、`normalize_parameter_pattern_operator_order`（parse 后直接改引擎 `ParameterExpansion`）——两者是引擎 bug 的产品层补偿（rubash#117 反模式的翻版）；另有 3 个 Windows drive/virtual-root AST 归一化（合法产品特性但应走引擎 hook） | 补偿类下沉引擎；归一化走 hook |
| A2 | **严重**（**引擎侧已于 2026-09-22 解决**：同上 2d7c973a；H 节转义模式实锤 bug 亦由此消解——niu 侧须删 fast path 见处置清单） | `shell.rs:2762 / 2821` | 产品自实现 `${var#pat}` 展开：白名单 fast path，含 `strip_prefix('\x1d')` **解读引擎私有标记字节** | GNU 无 word-level 展开捷径 → 引擎修复后整段删除 |
| A3 | 高 | `shell.rs:290` | **#129 修复（68ecc7b，expand_aliases）不在 `fix/niu-issues` 工作树**——该克隆构建会复发 | 合并 origin/master 即消 |
| A4 | 高 | `src/main.rs:69/99` | 后台 `&` stdio 剥离在产品层：约 180 行 Windows 句柄手术 + 自解析 source，依赖 `__RUBASH_SHELL_PID` 私有协议 | 下沉 rubash |
| A5 | 中（**引擎侧前置条件已于 2026-09-22 满足**：markers.rs 公开注册表已建成（f8124f8b 起 M1-M5）、FdTable 已落地（7e56967d 起 M1-M5）、Phase 0 decode API 已入（3de4e165）；niu 侧迁移待排期） | `shell.rs:5191` 等 | 产品层复刻引擎内部标记协议（heredoc `\x1f`、`__RUBASH_HD1__`、`\x1d`、PUA 区间，注释自认绕开非公开 API） | rubash 补公开 API（对接治理文档 markers.rs） |
| A6 | 中 | `shell.rs:549 / 585` | 进程内 THIS_SH 子 shell 手工 save/restore 仅 4 个 env，函数/alias/shopt/trap 泄漏 | 复用引擎 fresh-init（rubash 01029f1b/41a3308c 已有） |
| A7-A9 | 低 | `completion/runtime.rs:211`、`main.rs:532`、`shell.rs:628` | completion 自写 `split_shell_words`（不识 `$'...'`）；`--dump-strings`/`--pretty-print` 手写扫描（引擎缺 AST serializer）；用 `parse+execute_ast("unset HISTFILE")` unset 变量属绕路 | 引擎补 API 后替换 |

## B. 版本漂移

master 克隆锁定 rubash `0fd42049`，距 rubash HEAD（57952602）**36 commits ≈ 25 个
语义修复**未吃到，关键包括：varenv 大批次、assoc/array 大批次、#83 算术、#109
ANSI-C carrier、**cfaa9125（quoted name=$(...) 非 assignment——恰好消解 A1/A2
补偿存在的必要）**、c5c97683（ExitCode 不降级——niubash `shell.rs:2704` 的
`ExecuteError` match 需同步）。`fix/niu-issues` 口径落后 91 commits。

## C. 交互式错误格式 "line 1:" —— 分歧定位（已实测 GNU 5.3.0）

> **更新（2026-09-22）：引擎侧已修**——交互错误前缀 `diagnostic_prefix` interactive
> 分支已落地（提交 `534bc2e8`）。以下定位分析保留作历史记录；niu 侧
> `enter_interactive()` 改设 `__RUBASH_SHELL_NAME` 仍待产品层跟进。

GNU 三种模式（脚本文件探针实测）：
- 脚本：`/tmp/t1.sh: line 1: nosuchcmd: command not found`
- `bash -c`：`/usr/local/bin/bash: line 1: ...`
- **交互（`bash -i`）：`bash: nosuchcmd: command not found`（无 line 段）**

rubash/niubash 的分歧是**两层叠加**：
1. 引擎根因：`public_accessors.rs:627 diagnostic_prefix()` 与
   `builtins/set.rs:54 builtin_error_prefix()` 没有 interactive 分支（GNU
   `error.c error_prolog` 仅非交互打印 line 段）；且每次执行都 set_current_line，
   交互单行 line=1 → 出现 "line 1:"。rubash 自家 REPL 已设 `__RUBASH_INTERACTIVE=1`
   但 prefix 未消费它。
2. 产品放大：niubash `enter_interactive()` 把 `niu` 塞进 `__RUBASH_SCRIPT_NAME`
   （脚本名语义误用作 shell 名），交互诊断变成 `niu: line 1: ...`。GNU 交互的
   `$0` 语义是 shell 名。另 CommandNotFound 打品牌名 `niubash:` 而非调用名（GNU
   用 `$0`/shell_name；rubash `main.rs:722-729` 的 `__RUBASH_SHELL_NAME` 回退链
   已正确实现，niubash 未用）。

**修法**：引擎修（`diagnostic_prefix` 加 `__RUBASH_INTERACTIVE` 分支，引 GNU
error.c report_prolog）；niubash `enter_interactive()` 改设 `__RUBASH_SHELL_NAME`。

## D. pty/交互测试归属

- **niubash 侧**：reedline 0.50 行为、按键、补全 UI、PS1/prompt 渲染（999 行
  `prompt.rs` 是产品特性）、`-C` REPL 命令。pty 端到端只放这一层。
- **rubash 侧**：错误前缀格式（C 节，`diagnostic_prefix` 是纯函数可无 pty 单测）、
  `bind`/readline 函数语义（`src/input/readline`）、PS1 变量展开语义、交互语义
  开关（`shopt -s extglob` 等）。
- GNU 上游 `tests/` 没有 interactive/readline/PS1 类 `.tests` 文件——该域本来
  就没有官方可跑套件，需自建（对 GNU 用 `bash -i` 探针脚本对拍）。

## E. 产品层允许 / 不允许的 diff 边界

**允许（UI/品牌/产品特性区）**：提示符样式与默认值（但 **PS1 变量展开语义必须
引擎内与 GNU 一致**）、AI/产品命令（`plugin`/`setup`/`--self-update`/`-C` 等，
不与 bash 保留名冲突）、品牌输出（banner/更新提示）、交互 UI、文档化的 env 注入
（BASH/SHELL/STARSHIP_SHELL/按调用名进 posix/sudo 默认禁用）、Windows 路径
归一化（须走引擎 hook）。

**绝不允许（语义区，diff 即 bug）**：错误消息格式（前缀结构、line 段、措辞、
shell 名取值）、退出码（`ExecuteError` 映射须随 rubash c5c97683 同步）、
expand_aliases/shopt 默认随交互性的取值（#129）、stdout/stderr 内容与顺序、
展开/分词/trap/作业语义（产品层不得有第二条实现路径）。

**边界一句话**：用户看得到的 bash 协议面（stderr/exit code/env/输出流）零分叉；
bash 未定义的 UI 面自由分叉，但消费的引擎原语必须经公开 API。

## F. 参数/选项 parity

- CLI：GNU 16 个 long options + short 全集，rubash `invocation.rs` **全齐**。
- `shopt` 56 项、`set -o` 27 项：与 GNU 5.3.0 逐名一致。
- 内建选项 spot check（getopts/read/declare/printf/mapfile/complete/shopt）：
  脚本文件 diff **零差异**。
- 真实分叉只有两处：① C 节交互前缀；② niubash `main.rs:299` 对无效参数报
  `unknown argument '-Z' (not a script file)` 而 GNU 是 `bash: -Z: invalid option`
  + rc 2（EX_BADUSAGE）——低危，建议对齐。`-V` 是产品自加参数（GNU 只有
  `--version`），可接受但需文档标注。

## G. 行动项（按优先级）

1. niubash 各克隆对齐 origin/master（消 A3 #129 复发风险 + Cargo.lock 回退）。
2. 引擎修 `diagnostic_prefix` interactive 分支 + niubash 改设 SHELL_NAME（C 节）。——**引擎半已修（2026-09-22，534bc2e8）**；niu 侧待办。
3. rubash 升版后 niubash 刷新依赖（消 B 节 25 个修复的漂移），**删除 A1/A2
   补偿 pass**（cfaa9125 已使引擎原生正确），跑双侧基线确认。
4. A4/A5/A6 按治理文档 3.6 分层纪律排期下沉；markers.rs 建成后 A5 自然消解。——**markers.rs 已建成（2026-09-22，f8124f8b 起 M1-M5；FdTable 7e56967d；decode API 3de4e165）**，A5 迁移可启动。
5. `niu -Z` 措辞/退出码对齐 GNU EX_BADUSAGE。

## H. 追加审计：产品层手写 marker/heredoc 逻辑（2026-09-20 第二轮，含三方实测）

对象：niubash-multiline @4289da5 对照引擎 HEAD。逐点核对了 10 处手写逻辑。

### 实锤 bug（niu 正在产生错误输出）

**`x=${v#pat}` fast path 转义模式错误**（shell.rs:2753-2872 全家，约 180 行）：
`t='a?b'; x=${t#\?}` → GNU/rubash 引擎均输出 `a?b`，**niu 输出 `?b`**；
`w='*b'; x=${w#\*}` → GNU/引擎 `b`，niu `*b`。根因：`decode_simple_parameter_pattern`
把 `\*`/`\?` 剥成 glob 元字符、`simple_glob_matches` 只支持裸 `*`/`?`。引擎 HEAD
已原生正确——整个 fast path 应**立即删除**。

### 处置清单

- **立即删**：`execute_parameter_pattern_assignment_simple_ast` 全家 + 三个辅助函数。
- **合并后删**（先 eprintln 证伪"曾实际修正过"）：`normalize_parameter_pattern_operator_order`
  （shell.rs:2892-2997，未找到引擎切错的输入，且缺 `$'...'` 感知有反向污染风险）；
  `shell.rs:3559` 的 `\x11` 全量剥除（引擎已在 argv 前剥，提前剥反而销毁保护信息）。
- **下沉排期**（换引擎公开 API）：repl.rs:833 的 `__RUBASH_HD1__`/裸 `\x1f` heredoc
  完整性判定（引擎导出公开函数）；shell.rs:4135 的 `\x1c` alias 镜像（改引擎
  accessor，废"重解析自己刚敲的行"）；stdin bridge（shell.rs:2709，疑似死路径，
  若保留必须先修定界符碰撞/未引号展开注入/非 UTF-8 三个炸点）；completion 的
  自写分词换 `rubash::lexer::tokenize`。
- **正当保留**：fast-path 判废字段、env 注入、产品 UI。

### 已验证安全

- repl.rs heredoc 完整性判定与引擎逐字节一致（`\x05` 是 executor 层产物，不在
  tokenize 输出，与 repl pass 无交集——B1 碰撞是引擎内部问题，产品层不背）。
- 后台 `&` 句柄手术已在当前 master 下沉引擎（rubash f0703c0a），旧审计的
  main.rs:69/99 定位过期。

### 引擎版漂移补充

niu 锁定的 0fd42049 距引擎 HEAD 36 commits：**cfaa9125（quoted name=$(...）
非 assignment）正是niu 这些补偿 pass 失效的原因**——升级引擎后立即删补偿即可。

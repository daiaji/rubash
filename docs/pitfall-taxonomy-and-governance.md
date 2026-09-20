# 坑分类学与治理机制（Pitfall Taxonomy & Governance）

> 生成日期：2026-09-20。数据来源：全量 git log（2566 条提交）主题聚类、
> 热点文件统计、docs/COMPATIBILITY-STATUS.md、docs/issue-suite-diff-analysis.md、
> GitHub issues（gnu-compat G1–G26、niubash #100–#130）。
> 本文档的目的：**让"同一类坑反复踩"变得可见、可防、可测**。
> 新 agent 进场必读；修改词展开/标记/重定向相关代码前先查第五节的禁令。

## 一、事故家族总表（按根因分组）

### 甲类：标记/载体字节（带内哨兵机制）

rubash 用带内哨兵（C0 字节 \x11–\x1f、PUA 码点 E000–E10C、命名串
`__RUBASH_CA1__`/`__RUBASH_HD1__`/`__RUBASH_CSB1__`）在词展开管线里承载
引号/来源信息，对标 GNU 的 CTLESC/CTLNUL。

| # | 根因 | 症状 | 证据（示例） |
|---|------|------|------------|
| M1 | **编码后消费点漏解码（主模式）** | 每新增一种标记/一条路径需人工补 5–10 处 decode，漏一处即泄漏 | eda3a734 系列五连补；613e7089(recho)、cb39127a(echo)、5eaae06c(eval)、f3108992(declare)、d45619ee |
| M2 | 标记与用户数据碰撞 | 用户数据含控制字节/PUA 被误认成标记 | 049d6d9d、118c5ee3、0a4ba289、f39669b3(E000 碰撞) |
| M3 | 同字节双语义 / 词表不统一 | ANSI-C 解码出的真实字节被载体占用；常量多处重复定义 | d45619ee、358c6a21、4c8f543f、c282a850 整编；**现存 E10A 双占用**（SQ_DOLLAR_DATA @assignment_expansion.rs:182 vs FAILED_SUBSCRIPT_SENTINEL @types.rs:53） |
| M4 | restore 站点反向漏调 | 有解码要求但漏调用 → 标记被剥/泄漏 | 649b2ac1、1d405365、80ee05a3、af987f35 |
| M5 | merge/rescue 丢载体修复 | 合并静默吞掉已修语义 | ba2e7c19(#103)、0e44b432、907d2d99、8c9a4ee8 |
| M6 | 标记泄漏到用户可见输出 | echo/xtrace/declare -p/recho 出现内部标记 | COMPAT-STATUS:640/661/1074、:147、:471 |
| M7 | 快路径白名单漏载体 | 词级 fast path 不认识新载体形态 | 3f23f8dc(niubash#103)、7ab91ffd、2e149678 |

当前词表热点面：`\x1f` 解码散布 **50+ 文件**、`\x1d` **39 文件**——
这是 M1 持续发生的结构性原因。

### 乙类：展开/解析语义（非标记）

| # | 家族 | 规模 | 热点文件 | 深层根因 |
|---|------|------|---------|---------|
| S1 | **词层快速路径引号/转义失真** | ~242 提交（#116/#117、niubash#103…） | executor/mod.rs、assignment_expansion.rs | fast path 在文本层重实现词法语义，准入靠 contains() 黑名单（#117 已定性） |
| S2 | **数组复合赋值/下标展开** | 223+ 提交，两次 merge 丢修复 | declare.rs、executor/mod.rs | 引号去除→转义解码→字段拆分的阶段顺序无唯一实现；declare/read/alias/直接赋值四入口各一份 |
| S3 | 重定向/fd 模型 | ~206 提交（#118/#122） | command_prepare.rs | Windows 无真实 fd；内建输出通路 N 条各自打补丁 |
| S4 | heredoc 收集 | ~101 提交（heredoc3/7/9/10 变体各自失败） | lexer/mod.rs | 独立扫描器而非 parser 状态机（GNU PST_EOFTOKEN 模型） |
| S5 | 命令替换 body 再解析 | ~129 提交 | command_substitution.rs | 文本切片+再解析替代 parse_and_execute 一体化 |
| S6 | 子壳隔离 | 20+ 提交（#100 两修） | ast_exec.rs | 非 fork，靠手工快照/恢复清单，新增状态即新泄漏点 |
| S7 | 作用域/varenv/nameref | 100+ 提交 | declare.rs、varenv | 与 variables.c frame/tempenv 不同构，正成批补语义 |
| S8 | errexit/status 传播 | ~30 提交 | executor/mod.rs | `$?` 曾是裸整数，c5c97683 才统一 ExitCode |
| S9 | trap/job/signal | 45+ 提交，coproc 三次 revert | trap_exec.rs、pipeline_exec.rs | 无子进程所有权模型 |
| S10 | Windows 进程/路径 | ~104 提交 | path.rs、main.rs | 平台基建债，收敛中 |
| S11 | stderr 顺序 | AGENTS.md 已记录 | 多处 eprintln | line-buffered vs 逐写 flush |
| S12 | 字段拆分/IFS | ~44 提交 | executor | 拆分入口不唯一（read/declare/array/comsub 各有路径） |
| S13 | 流程性：merge 丢修复 + 假绿基线 | 10+ 条 | 全仓 | 双线开发共享树；历史上用 5.2.21/Git Bash/仿真层判定 PASS |

## 二、结构性结论

1. **两个根因喂养了大半个事故库**：S1（词层 fast path）同时喂养 S2/S5/S12；
   S2（复合赋值展开管线不唯一）是 #103→回归→#109→revert→"four lost fixes
   re-implemented" 循环的源头。
2. **标记机制的症状（甲类）大多也是 S1/S2 的投影**：fast path 不认识载体（M7）、
   多入口展开各自带解码（M1/M4）。
3. **平台债（S10/S11）是一次性的**，已在收敛，不再新增。
4. **流程漏洞（M5/S13）用规则+CI 补**：merge 前聚焦回归、PASS 只认 WSL 5.3.0
   script-file 基线——规则已存在，需强制化。

## 三、治理机制（新代码必须遵守）

### 3.1 标记注册表（markers.rs，待建）

- 所有哨兵字节/PUA 码点/命名标记串**只允许**在 `src/executor/markers.rs`
  声明一次；业务代码禁止裸写 `\x14`、`E10A` 等字面量，一律 `use markers::*`。
- 每个标记声明必须包含：字节值、含义、**编码函数**、**解码函数**（同文件相邻、
  成对导出）、消费者边界清单（输出/存储/重解析三类边界各写明谁负责还原）。
- 新增标记 = 新增一对 encode/decode + 一条"标记值不得出现在 stdout、
  `declare -p`、xtrace 输出"的金标断言测试。
- **立即项**：拆分 E10A 双占用（FAILED_SUBSCRIPT_SENTINEL 换空闲码点），
  这是现存碰撞，一行改动，优先级最高。

### 3.2 解码收口

- 按三类边界收口解码：**输出边界**（echo/declare/-p/xtrace 前统一还原）、
  **存储边界**（变量/数组赋值前统一还原）、**重解析边界**（eval/alias/comsub
  再解析前统一还原）。目标是把 50 文件的散布 decode 收敛到每边界一个入口。
- 新路径接入时只允许调用边界入口，不允许自带 decode。

### 3.3 语义唯一入口（按杠杆排序）

1. **消灭词层 fast path**（S1）：准入改白名单（纯字面量才短路）；
   CI 静态检查禁止词层函数新增 `contains(`/`starts_with(` 准入条件（#117 裁决落地）。
2. **复合赋值唯一管线**（S2）：抽 `expand_compound_assignment_word()`，
   对齐 GNU expand_word_internal 阶段顺序，declare/read/alias/直接赋值四入口强制走它；
   属性测试覆盖 赋值×引号×转义×IFS 矩阵。
3. **重定向单一 RedirectionEngine**（S3）：内建禁止直写 std handle。
4. **heredoc 进 parser 状态**（S4）：对齐 PST_EOFTOKEN，所有解析入口免费获得正确行为。
5. **子壳隔离 ShellState 化**（S6）：可变状态全收进一个结构，子壳=克隆整结构，
   新字段自动被隔离。
6. **ExitCode 贯通**（S8）、**子进程所有权注册表**（S9）、**stderr 单通道逐写 flush**（S11）。

### 3.4 流程防线

- merge 前跑聚焦回归；rescue merge 后必须重跑相关族的探针（M5 四次事故的教训）。
- PASS 断言只认 `true-baseline.sh` + WSL GNU 5.3.0 script-file 口径；
  建议 CI 强制而非靠纪律（S13）。
- 合并跨 agent 分支时：先通读对方分叉点以来的全部提交再解冲突；
  同一 GNU 行为两边都实现的，保留更贴 C 源码、更少特判的一边。

## 四、热点文件提示（改动需extra谨慎）

`executor/mod.rs`(677 次)、`command_prepare.rs`(123)、`command_substitution.rs`(98)、
`compound_exec.rs`(92)、`embedded_mutations.rs`(81)、`lexer/mod.rs`(79)、
`trap_exec.rs`(77)——这些是历史事故密集区，改动前先查本文件第一节对应家族，
改完跑对应族探针。

## 五、快速禁令（code review 自查）

1. 禁止新增词级 `contains()`/`starts_with()` 语义准入（#117）。
2. 禁止裸写哨兵字节/码点字面量；必须走 markers.rs。
3. 禁止在业务文件自定义标记常量副本（历史上 E102/E107/E108 曾 4 处重定义）。
4. 禁止新增"文本切片+再解析"式 comsub 捷径。
5. 禁止内建绕过重定向引擎直写 std handle。
6. 新增可变 shell 状态必须进 ShellState（或明确记入子壳隔离清单）。
7. 新增 POSIX 分支必须集中在唯一的 posix 判定点，不许散落。

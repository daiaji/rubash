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

- **修 issue 必须固化回归测试（资产化，硬规则）**：每个 issue 修复提交必须同时
  包含一个能在旧代码上失败、在新代码上通过的测试（`tests/` 下，命名引用 issue
  号，如 `issue120_*.rs`）。判定标准：revert 修复提交后该测试必须红。多次
  "丢失的修复重做"（ba2e7c19、0e44b432、907d2d99、8c9a4ee8）证明仅修代码不留
  钉子的修复等于没修。
- merge 前跑聚焦回归；rescue merge 后必须重跑相关族的探针（M5 四次事故的教训）。
- PASS 断言只认 `true-baseline.sh` + WSL GNU 5.3.0 script-file 口径；
  建议 CI 强制而非靠纪律（S13）。
- 合并跨 agent 分支时：先通读对方分叉点以来的全部提交再解冲突；
  同一 GNU 行为两边都实现的，保留更贴 C 源码、更少特判的一边。

## 3.5 八三套件与"百分百兼容"的关系（口径声明）

**83 套件零差 ≠ 100% 兼容。** 它是必要条件，不是充分条件。覆盖盲区：

1. **测试面**：83 个 `.tests` 是 GNU 自带的回归测试（约几千个断言），针对
   历史 bug，不是穷尽一致性规范。GNU bash 的内建选项矩阵、组合语义空间
   远大于此。
2. **观测面**：true-baseline 主要比对 stdout（stderr 单独捕获但不进主台账），
   exit code、时序、信号投递时机、内存/资源行为均不在字面 diff 内。
3. **域盲区**：交互/readline/PS1/completion 基本无自动覆盖；多字节/ locale
   组合稀疏；`bashdb`、大型真实脚本（bash-completion 等）是抽样而非系统覆盖；
   POSIX conformance test suite 未接入。
4. **环境面**：部分剩余 diff 是 Windows 平台噪声（AGENTS.md 口径单独记账），
   反之 GNU 的 Linux 特有行为（如真实 fork、信号语义）在 Windows 上只能模拟，
   零差也可能只是该场景恰好没被触发。

**因此补强方向**（全部只对齐唯一权威 WSL GNU bash 5.3.0，不引入第二口径——
POSIX conformance suite 等一律不用，POSIX 与 GNU 存在分歧，对齐标准会偏离 GNU）：
① 修 issue 固化回归测试（上文硬规则）；② 差分模糊测试（随机脚本对 WSL GNU
5.3.0 逐字节比对，自动发现 83 套件外的分歧）；③ 真实世界脚本语料作为更高强度
的 GNU 对照（autotools `./configure` 双侧跑 diff 产物、bash-completion、发行版
脚本——判定标准仍是与 GNU 5.3.0 输出一致，不是"能跑通"）；④ 补齐第四节数的
套件缺口（解析器族无专属覆盖）。

### 3.6 POSIX fork/exec 模型的实现策略（架构决策记录，2026-09-20）

**决策**：不追 fork 的机制，只复刻 fork 的语义。GNU 用 fork 是因为 Unix 内核
给了这个原语；fork 机制本身在 Windows 上的模拟（Cygwin/MSYS 的内存循环拷贝）
又慢又脆，不值得模仿。要对齐的语义只有四条：①子壳状态完整隔离复制；②独立
执行流（后台 `&`、coproc）；③exit status/信号投递回传；④fd 表按序复制。

**三层混合实现**（分场景，不追单一机制）：

| 场景 | 机制 | 说明 |
|---|---|---|
| 普通 `( )` 子壳 | 串行模拟（现状保留）+ **ShellState 结构化**：全部可变 shell 状态收进一个结构，子壳 = 克隆整个结构 | 类型系统保证新增字段自动被隔离，消灭 S6 的手工清单漏项 |
| 后台 `&` / coproc | **线程池 + 子进程所有权注册表**（谁 spawn、谁 reap、trap 归谁） | 唯一语义上必须并发的场景；即 AGENTS.md 挂账的"后台任务线程化"深水区，值得立项 |
| fd 表复制/排序 | **Win32 精确句柄继承**（`PROC_THREAD_ATTRIBUTE_HANDLE_LIST` + Job Objects） | 修 `read v <&3 3<<EOF` 排序、exec fd 中毒等 redir/read 族架构项，不动执行模型 |

**明确排除**：全量线程化 fork（引擎全量线程安全化代价大且普通子壳不需要）、
子进程序列化 fork（引擎状态不可整体序列化）、WSL 桥接（破坏 Windows 原生定位）。

**验收口径**：每层独立验证——子壳快照完整性用属性测试（随机状态组合下克隆
隔离性）、并发用 ownership registry 单测、fd 用定向探针对拍 WSL GNU 5.3.0。

**实测校准（2026-09-20，`exp/fork-hybrid` @74bf11dc，产物在
`D:/repo/rubash-exp-fork/experiments/fork-hybrid/`）**：

1. **fd 语义现状**：12 探针 6 个真实分歧，单一根因——fd 层是"逐条重定向重开
   模拟"而非 open file description 表受控复制（典型：fd 偏移量不共享、子壳
   `exec 3<&-` 穿透父壳、后台不继承 fd3）。`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`
   POC 通过（白名单继承、rogue 句柄不泄漏）。**校准**：HANDLE_LIST 只修继承面；
   偏移量共享/关闭隔离需把 FdTable 改为记录真实 HANDLE——规模是 FdTable 重构
   （中等工程），大于本节原估的"修排序"。
2. **并发 registry POC 通过**（spawn <100ms 不阻塞、wait 回收 exit code、
   shutdown 不挂死，5/5 测试）。**校准**：引擎 `&mut self` 单线程借用模型是
   真正成本，建议先 registry 化记账、后评估状态共享；且与第 3 层有顺序依赖
   ——必须先 HANDLE_LIST 化再线程化，否则 stdio 手术补偿的单线程 spawn 安全
   论证失效。
3. **ShellState 量化**：Executor 88 字段；`( )` 子壳 save/restore 清单仅 7 项、
   命令替换 fresh-Executor 手工清单约 55 项——**两份清单已漂移**；实测打穿：
   **alias 和 function 从子壳泄漏到父壳（GNU 干净）**，S6 家族现行证据。
   工作量约 60 项语义状态 + 28 项瞬态分离，估 3–5 天机械重构 + 83 全量回归
   （载体字节/$'...' 重点）。

**优先级修订**：第一层（ShellState）提升为最高——alias/function 泄漏是现行
bug 且不依赖其他层；fd 层规模上修；线程化必须在句柄层之后。

### 3.7 双层测试口径（引擎层 + 产品层）

**真正的 shell 层是 niubash**（`D:/repo/niubash-*`，crate `niubash`，依赖
`rubash = { git = ".../rubash.git", branch = "master" }` + winuxcmd），用户
摸到的是它。因此：

1. **引擎层**：rubash 二进制跑 83 套件 true-baseline（现行口径）。
2. **产品层**：niubash 二进制跑同一 83 套件 + niubash 专属回归（其 issue 系列的
   固化测试）。发布前产品层必须过，因为 CLI 解析、readline、默认值、AI 粘合层
   完全可能在引擎零差之上引入自己的分歧（实例：niubash#129 脚本模式无条件
   expand_aliases 是产品层语义 bug，不是引擎的）。
3. **分层纪律**：任何语义修复落在 rubash 引擎，niubash 保持薄壳——niubash
   里出现语义补丁即是架构异味，应下沉引擎。
4. 注意：AGENTS.md "不要用 niubash 做测量"指的是**不要拿它当工具 shell 跑
   harness**（它 mangle glob/引号），不是"不测它"——它本身是被测对象。
5. 时序约束：niubash 依赖 rubash master git 分支，因此产品层基线只能在引擎
   合并后刷新；分支开发期以引擎层基线为准，合并后补产品层。

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

## 六、问题热点索引（issue/PR 交叉版，2026-09-20）

数据来源：rubash 全量 118 issues + 28 PRs、WinuxCmd/niubash 关联 issue、
git log 交叉核对。按"反复出问题"频次排序。

| # | 热点主题 | issue/PR 证据 | 家族 | 套件 | 主要源文件 |
|---|---|---|---|---|---|
| 1 | 词层 fast path / comsub 捷径漏语义（黑名单守卫不断被打穿） | #68 #69 #70 #116 #117、PR#101/102/116；niubash#119 | S1+S5 | 全套 | command_substitution*.rs、parameter_core.rs |
| 2 | 数组/关联数组复合赋值、下标、declare 回显 | #24 #77(open) #79 #109 #111；niubash#72 #78 | S2 | assoc/array/quotearray | builtins/declare*、arrays |
| 3 | 载体字节泄漏到用户可见层 | #64 #95 #96 #97 #109、PR#104/114；niubash#92 #103 #124 | M 类 | quotearray/posixexp/varenv | lexer/quotes.rs、eval_source_for_reparse |
| 4 | comsub 捕获/重解析 | #69 #70、PR#6/7/9/102/104/115；niubash#76 #120 | S5 | heredoc/comsub、bashdb | command_substitution.rs |
| 5 | varenv/nameref/tempenv | #24 #78 #86；audit C2–C15 批次 | S7 | nameref11/varenv | varenv、nameref |
| 6 | 解析器：同行 `#` 尾注释 / CRLF / `{` 未闭合（三仓库各报一次） | #118(PR#119)、#31；niubash#106 #130 | S1 邻域 | **无专属套件（缺口）** | lexer/（continuation.rs captain-exclusive） |
| 7 | 重定向/fd/dev 别名 | #89(G17)；niubash#118 #122 | S3+S9 | redir | spawn/redir |
| 8 | Windows 路径/argv 修辞 | #31 #59 #60、PR#103(#1/#5)；niubash#61 #62 #83 #88 | S10 | 环境绑定 | external_argument_path |
| 9 | 子壳隔离（同一问题两半分两次修） | niubash#70 #100 | S6 | 靠 niubash 回归 | compound_exec.rs、ast_exec.rs |
| 10 | 算术/错误消息/errexit | #67 #73 #74 #83 #88、PR#110 | S8+S11 | arith | arith、expr 错误模型 |
| 11 | trap/shopt/nullglob | #85 #91(G19)、niubash#121 | S9 | trap | exec/trap |

## 七、13 家族未覆盖的新问题域（issue 证据）

1. **CLI/调用选项**：bundled 短选项、`-c -l`（niubash#107、PR#112）——提了两次。
2. **位置参数暴露**：`$0/$1/$@`、`${arr[@]+"${arr[@]}"}` 解析（niubash#15 #73）。
3. **alias 语义**：脚本模式 expand_aliases 无条件开启、alias 覆盖函数（niubash#129、#22/#23）。
4. **交互/readline/PS1/completion**：niubash#53 #54 #75 #91 #117；bashdb 集成同域。
5. **后台任务/进程生命周期**：`&` 无法脱离、stdout 早关 panic（niubash#122 #125）——比 S9 更具体的"进程脱离/句柄释放"主题。
6. **性能/冷启动**：负 lookup 60–85ms、`-c` 550ms 固定开销（#71、niubash#79、PR#10–14）。
7. **多字节/编码边界**：中文路径 char_boundary panic、ACP 解码（niubash#84 #86 #88 #92）。
8. **环境注入/隔离**：THIS_SH 子进程 env scrub、temp env 不可见于嵌套（niubash#38）。

## 八、未关闭与映射缺口（需复核）

**open：** #62（gnu-baseline 归因账本）、#77（declare -A/-ai 回显，assoc.tests 409/361）、
#117（词级捷径白名单化母 issue）。niubash#104（winget，特性请求）。

**issue→提交映射缺口（建议复验）：**
- **#66 嵌套花括号展开**：已关闭但找不到修复提交——最明确缺口，需复验是否真修。
- **#64 awk -F '\t'**：评论称 "fixed in working tree" 但无对应提交；确认回归测试
  `external_pipeline_preserves_quoted_awk_field_separator_argument` 在 master。
- **#98 (G26)**：关闭留言不给哈希，无法追溯。
- **niubash#118 只修 1/3**：ce89fa85 只覆盖 /dev/std* 别名；内部 cd 改道、ssh 无输出
  两项无 rubash 提交记录。
- **niubash#92 multibyte panic**：PR#104 是 follow-up，本体修复无独立提交。
- 反向缺口为零：所有声称修复的提交，issue 侧均已关闭。

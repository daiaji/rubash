# Harness 归因清扫 — 2026-09-22

数据源：`target/issue-suites/results/true-baseline/`（83 套件，55 零差 / 28 有 diff，共 683 diff 行，`diff gnu.out rb.out | grep -c '^[<>]'` 口径）。
方法：逐套件抽样 diff gnu.out/rb.out，交叉核对 gnu.err/rb.err，并与
`docs/COMPATIBILITY-STATUS.md`、`docs/pitfall-taxonomy-and-governance.md` 既有归因对照。
约束：本轮只读分析，未跑任何新基线（WSL 被 T5 迁移验证占用）。

> 口径更正：实际有 diff 套件为 **28 个**（非任务清单中的约 34 个）；attr、shopt、
> heredoc、lastpipe、comsub-eof、posixpipe、quotearray 等本轮已零差（与
> COMPATIBILITY-STATUS 2026-09-19 后的收敛记录一致）。

## A. 归因总表

| 套件 | 行数 | 分类 | 证据（一句话） |
| --- | --- | --- | --- |
| history | 173 | 半环境 | 双侧 40s 超时截断（沿用既有归因）叠加真实缺口：rb 侧 `history` 输出 `0`、缺 33-40/142-149 行历史条目、`!!` 未回放 |
| histexp | 74 | 真账 | `!!`/`!str` 历史展开原样透传（rb 输出字面 `!!`），GNU 侧展开为 `echo a` 等 |
| alias | 69 | 真账 | rb 在 `eval`/alias 路径上把脚本文本直接落进 stdout（GNU 输出 `ok 1/ok 2/text`，rb 输出脚本源码行），alias 展开失效 |
| glob | 67 | 半环境 | 文件名排序两组互为倒序（collation/环境）为噪声；`\**.c`、`[qwe/qwe]`、`argv <a*b/*>` 等为真实分歧 |
| redir | 41 | 真账 | rb 多打 `to c`×9、缺 `this is redir2.sub`、`from stdin: aa` 变空、六行 stdin 内容丢失 |
| extglob | 32 | 环境噪声 | 全部差异集中在 `a:b`：Windows `touch a:b` 报 "系统找不到指定的路径"（rb.err 实证），冒号非法文件名；另 2 行 dotfile 排序噪声 |
| jobs | 31 | 环境噪声 | rb 侧 `/bin/sh: command not found` → job rc=127（Windows 无 /bin/sh，PATH/环境注入）；与既有"timeout 截断伪影"归因一致方向 |
| vredir | 27 | 真账 | fd→readonly 变量赋值语义：多出 `foo 1/2/3`、计数 10→11 偏移；rb.err 自证 `v: cannot assign fd to variable` |
| read | 25 | 半环境 | `/dev/tty: No such file or directory`、mkfifo 不支持为平台噪声；`unset or null` vs `1/4` 输出为真实 read -n/-t 语义差 |
| cond | 19 | 真账 | 缺 `matches 8`/`ok 4a`/`ok 11/12`，多 `bad 5`；cond-regexp2.sub 三处 invalid-regexp 诊断缺失 |
| comsub | 17 | 真账 | comsub5/6 子脚本语法错误点不同（rb 在 comsub5.sub:26 报错），`\/tmp\/foo\/bar` 转义未还原 |
| procsub | 14 | 半环境 | 临时路径反斜杠被吃（`C:UsersADMINI~1...`，rb.err 实证）为噪声；缺 `test5`/`extern`/计数行为真账 |
| nquote | 14 | 真账 | `$'\t'` 未解释为字面 tab、`od` 输出 `del  nl` 多空格、ESC/FS/GS 缺失 — 引号删除/载体缺口 |
| comsub-posix | 14 | 真账 | POSIX 形态 `) )` 收尾行缺失、`we should not see this` 泄漏进输出 |
| test | 13 | 真账 | test/`[` 内建整数比较退出码 0 vs 1 系统性翻转；rb.err 多 `line 118: No such file or directory` |
| intl | 12 | 真账 | C0 控制符 `$'\030'` 等 4/1318 编码失败（已知 ANSI-C 载体架构缺口）；unicode3.sub cd 报错缺 `$'...'` 引用格式 |
| type | 6 | 环境噪声 | `bash is hashed (/tmp/bash)` vs `rubash.exe is hashed (/tmp/rubash.exe)` — 被测 shell 身份注入，非语义 |
| coproc | 6 | 环境噪声 | `$0` 命名（`bash:` vs `./coproc.tests:`）+ Windows 无 `/etc/passwd`（`cat: /etc/passwd: No such file`） |
| set-x | 5 | 真账 | rb 缺 `+ echo 1..4` 及 `+ unset BASH_XTRACEFD` xtrace 行 |
| iquote | 4 | 真账 | DEL 字节 `^?` 参数在 rb 侧整组消失（载体对 DEL 的处理缺口） |
| globstar | 4 | 半环境(疑) | 多出 `c/aa c/ab` 条目；既有归因为 check 侧 ls/fixture，本轮 585 行是 glob 展开非 ls，**待验证**，暂记 fixture 树漂移噪声 |
| exp | 4 | 环境噪声 | `~` 展开为 `C:\Users\Administrator/src/cmd` vs `/src/cmd` — HOME 平台注入 |
| posix2 | 3 | 真账 | `running $@ test failed`，失败计数 1→2 |
| errors | 3 | 环境噪声 | PWD 呈现 `/mnt/d/repo/rubash` vs `D:/repo/rubash`（WSL/Windows 路径形态）；1 行多余空行为小真账 |
| invocation | 2 | 环境噪声 | `cannot execute binary file` vs `ls: No such file or directory` — Windows PATH 注入的命令差异 |
| herestr | 2 | 真账 | herestring 中 `$(echo hi)` 被 rb 展开为 `hi`，GNU 保留原样 |
| nameref | 1 | 真账 | rb 多输出 `declare -r RO_PID`（declare 遍历泄漏） |
| ifs-posix | 1 | 真账 | GNU 自报 `# tests 6856 passed 6856 failed 0`，rb 因 read 拆分残余失败未打出该行（既有已知：仅全量运行复现） |

## B. 汇总（诚实剩余）

- 全真账套件：316 行
- 全环境噪声套件：81 行（extglob 32 + jobs 31 + type 6 + coproc 6 + exp 4 + invocation 2）
- 半环境套件：286 行（history 173 + glob 67 + read 25 + procsub 14 + globstar 4 + errors 3），按内容抽
  样估计约 6:4 偏真账 → 约 170 真账 / 115 噪声
- **真账合计约 480–490 行；环境噪声合计约 195–200 行**（总 683）
- 与既有账本一致性：jobs/history 的截断伪影归因、intl 载体架构缺口、
  globstar 的 check 侧归因均沿用；本轮新增证据（rb.err 中 `/bin/sh`、
  `touch a:b`、`/dev/tty`、临时路径反斜杠）支持把 extglob/jobs 全额、
  read/procsub 大半记入环境账。

## C. harness 改进建议（按收益排序，仅建议不实施）

1. **统一双侧 coreutils/PATH 视图**（预计消除 jobs 31 + invocation 2 + errors 2，
   并压 procsub 噪声）：GNU 侧跑 WSL 自带 /bin/sh 与 coreutils；rubash 侧注入
   同一套 coreutils provider 并提供 `/bin/sh` 别名；run 前 `unset WINUXSH_ROOT`
   前置断言（现状是文档要求，不是 harness 检查）。
2. **平台非法文件名跳过清单**（消 extglob 32）：harness 生成 fixture 时检测
   目标 FS 对 `a:b` 的支持，双侧一致跳过并标记 `SKIP:fs-colon`，而不是让
   `touch` 单侧失败污染 stdout。
3. **locale/collation 前置检查 + 排序归一**（压 glob 67 中排序行、globstar）：
   双侧强制 `LC_ALL=C` 后重跑；或对纯排序差异行做 sort 后二次 diff，仅当
   集合不同才记 diff。
4. **timeout 截断标注**（history 半环境部分）：runner 把 rc=124/137 写入
   `gnu.out`/`rb.out` 尾部标记行（如 `__TRUNCATED__`），ledger 统计时自动把
   截断套件单列，避免真账与截断混计（沿用 2026-09-19 已识别但未机制化的结论）。
5. **stderr 差异入副账**：本轮多个分类证据来自 rb.err（/bin/sh、a:b、/dev/tty、
   路径反斜杠）。建议 ledger 增加 err-diff 列，噪声关键词
   (`No such file or directory` on /dev/*, `command not found` on /bin/*,
   os error 3) 自动打噪声标。
6. **shell 身份归一**（消 type 6、coproc 的 $0 部分）：测试内引用被测 shell 时
   用 `${THIS_SH}`/固定名字注入双侧（symlink/bash 别名），使 `type bash`、
   `$0`、错误前缀双侧一致。
7. **heredoc/路径 CR 与反斜杠守护**（压 procsub 半环境）：rubash 侧
   `TMPDIR` 指到无空格正斜杠路径（如 `/tmp` 映射目录），避免 `C:\Users\ADMINI~1`
   进入被展开文本。
8. **/dev/tty、mkfifo、/etc/passwd 能力探测**（压 read/coproc 噪声）：runner
   启动时探测并写 `env-capabilities.txt`，对应测试段双侧一致降级。

## 与既有文档的关系

- COMPATIBILITY-STATUS 2026-09-19 条目（jobs/history 截断、intl 架构缺口、
  globstar check 侧）与本表一致，无矛盾；本表把"归因"从套件级细化到行级。
- pitfall-taxonomy S11（stderr 顺序）本轮未成为 stdout diff 的主导因素；
  真正的环境主导项是 PATH/coreutils、FS 文件名限制、collation、timeout 截断。
- 注意事项：status 中 2026-08-29 一条曾记"histexp rubash 优于 bash 无需修"，
  本轮 diff 显示 `!!` 原样透传 74 行，该结论已过时，histexp 应回到待修清单。

# niu 产品层基线与消除计划（2026-09-22）

> 双层测试口径（治理文档 3.7 节）的产品层首次全量实测。
> 引擎层：rubash.exe @8e4e27c3（55 零差 / 692 行）；产品层：niu.exe @7ae8e1b
> （rubash 依赖 bump 到 8e4e27c3，已推送）。
> 台账：`true-baseline-ledger-niu.log` + `true-baseline-ledger-engine-8e4e27c3.log`。

## 总表

| 层 | 83 套件 | 零差 | 总差异行 |
|---|---|---|---|
| 引擎 rubash.exe | ✅ | 55 | 692 |
| 产品 niu.exe | ✅ | 47 | 2357 |

## niu 层特有分歧（引擎=0 或显著放大）——消除目标

| 桶 | 套件（行数） | 机制 |
|---|---|---|
| 脚本模式错误不停机 | errors (1367) | GNU 致命错即退出，niu 继续执行后续脚本 |
| CLI 表面差异 | invocation (98) | 自有参数解析/help 文案 |
| 脚本模式无交互历史 | histexp (175) | set -H/fc/history 输出空（reedline 只在交互态） |
| Linux 式 PATH spawn 失败 | heredoc 51、herestr 12、procsub +14、builtins 10 | 套件 PATH=/usr/bin:/bin 下 niu spawn rc=127（探针实证：rubash 能起 /bin/cat，niu command not found） |
| 启动文件污染 | assoc 6 | BASH_ALIASES 被 niu 默认 git 别名（gp/gst/g/gcm/gco）注入脚本模式 |
| 其余小项 | posixexp 8、new-exp/varenv/dbg-support 各 2 | 待逐案归因 |
| 反向改善 | glob −13、history −20、nquote −8、exp −4 | niu 层反而更接近 GNU（保留观察） |

## 消除计划（按收益排序，宿主层修复不进引擎）

1. **P0 脚本模式致命错退出**（errors 1367 行）：niu 的 REPL/脚本循环在引擎报
   fatal（FORCE_EOF/EXIT）后必须终止脚本而不是继续下一条。对齐 GNU eval.c:104
   /shell.c:1471 语义；验收 = errors 套件与引擎层同差或归零。
2. **P1 spawn 路径接管**（heredoc/herestr/procsub/builtins ~87 行）：外部命令
   spawn 必须走引擎的 `external_argument_path`/PATH 解析（含 /bin/cat 类
   POSIX 形态到 winuxcmd 的映射），禁止 niu 自建 spawn 旁路。
3. **P2 histexp 脚本模式历史**（175 行）：非交互模式接引擎 history 子系统
  （set -H/fc/history 内建数据面），reedline 仅负责交互行编辑。
4. **P3 invocation 对齐**（98 行）：CLI 参数解析委托引擎 `ShellInvocation`
   /GNU 语义，自有 help 文案对齐 GNU 措辞；无效参数报
   `niu: -X: invalid option` + rc 2（EX_BADUSAGE）。
5. **P4 assoc 启动污染**（6 行）：git 别名注入限交互模式且不落 BASH_ALIASES。
6. **P5 小项逐案**：posixexp 8、new-exp/varenv/dbg-support 各 2。

## 边界

- 全部为宿主层修复；任何语义下沉需求走引擎 PR（引用治理文档 3.7 分层纪律）。
- 反向改善项（glob/history/nquote/exp）不动，记录即可。
- 每项验收：对应套件在 RUB_OVERRIDE=niu.exe 下与引擎层同差或归零；全量无回归。

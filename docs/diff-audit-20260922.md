# 2026-09-22 全量 DIFF 逐行审计（no-stub + niu-sh 夹具）

口径：WSL GNU Bash 5.3.0 (`/usr/local/bin/bash`) vs `target/debug/rubash.exe`，
`__RUBASH_NO_UPSTREAM_SCRIPTS=1` 经 `WSLENV` 真实跨边界，`TMPDIR` 逐套件隔离，
`timeout --foreground`（jobs 120s），`/bin/sh|/usr/bin/sh` → PATH `sh.exe`
（niubash 挂载本工作树 rubash），`/bin|/usr/bin/X` → PATH basename 回退
（path.rs `unix_bin_basename`，44a56d1c）。

台账：`target/issue-suites/results/true-baseline-ledger.log`
**58 零差 / 25 有 DIFF / 407 行**（含 trap 1 行 SIGCHLD 时序竞态、
nameref 归零）。较上一版口径的变化：dstack 72→0（夹具逻辑根污染消除）、
nameref→0、trap 0→1（时序抖动）、test 17→15、coproc 4→6、invocation 0→2。

## 分类口径

- **[fixture]** 夹具/环境绑定：二进制名、PWD 拼写、辅助二进制版本、宿主机
  文件不存在（/etc/passwd、/bin/*sh glob）。不计入引擎语义差。
- **[semantic]** 真实引擎语义差：可归因到 GNU C 函数的某个行为族。
- **[timeout]** rb 侧真实挂起被截断（rc=138）。

## 逐套件审计

### jobs 62 — [semantic]+[timeout] 作业控制族
- `[1]-`/`[3]+` vs `[1]`/`[2]+`：作业编号与 current/previous 标记记账差
  （GNU jobs.c job working-set 语义），5 行。
- `5: ok 1`/`2: ok 3` vs `bad`：等待状态判定差，2 行。
- `got USR1` vs `wait status not greater than 128`：`wait` 对信号死子进程
  不返回 128+sig（jobs.c wait_for 状态合成），1 行。
- 第 48 行起整段缺失（54 行）：`wait-for-job` 处 `wait %N` 挂起，
  rb.rc=138。孤立复现偶发通过——作业表回收竞态。**最大单点缺陷。**

### glob 67 — [semantic] LC_COLLATE 排序族 + reprint
- 约 50 行：GNU 在 en_US.UTF-8 按 locale 排序（`Beware` 排小写后、
  `.b` 排 `a` 后），rubash 按字节序。glob.c/smatch 的排序谓词根因。
- `*abc.c` vs `\**.c`、`a\*b` vs `a\*b*`：pattern reprint 转义差。
- `a*b/ooo` vs `a*b/*`：未匹配段保留差。

### extglob 32 — [semantic] 同源排序族 + `:` 模式
- 排序差同上（locale collation）。
- `a:b` 条目丢失：含 `:` 的 extglob 模式匹配差。

### history 32 — [semantic] fc 编辑族
- `(left mid right)/A/B` 块 ×4 缺失：fc 编辑器重放多行命令未产出。
- `6 6 4` vs `3 5 2` 等：fc/history 计数差。

### redir 36 — [semantic] fd 重定向族
- `this is redir2.sub` 缺失、`from stdin:` 空、`ab..kl` 全空：
  fd-0 重定向后 read/source 拿不到内容。
- `exec 0<&5-` vs `exec <&5-`、`echo foo 2>&1 | cat` vs `|&`：
  reprint 归一化差（显式 fd 0、`|&` 展开）。
- `c3/c4` 计数差、`whatsis` 缺失、ERR trap rc/set -e 交互差。

### vredir 27 — [semantic]+[fixture]
- `{var}fd` 分配号 10 vs 11-17：GNU 取最小空闲 fd≥10，rubash 记高
  （redir.c manage_varfd）。约 15 行。
- `/bin/bash|/bin/csh|...` 6 行缺失：`/bin/*sh` glob 在 Windows 无 /bin
  实体——[fixture] 文件系统差。
- `foo 1..3` 多打一遍：`{v}>>` 追加语义差。

### cond 19 — [semantic]
- `jbig2dec` vs 空：`[[ =~ ]]` BASH_REMATCH 组捕获差。
- `matches 8`/`ok N` 缺失、`bad 5` 多打、`0/1` vs `2`：cond 退出码差。
- `ERR: 22: -'[[' '-n'...-` vs `-[[ -n $unset ]]-`：ERR trap 上下文
  打印格式差（原始命令文本 vs token 重打）。

### read 16 — [semantic] read -t 族
- `timeout N: ok`/`unset or null N` vs 裸 `1`/`4`：read -t 超时分支
  输出/状态差；`abcde` 多出。

### test 15 — [semantic]
- 8 处 `0`/`1` 翻转：`[`/`test` 谓词在边界参数下的真值差。

### nquote 14 — [semantic]+[fixture]
- `ok` vs `bad`：引词语义差。
- `^I` vs `$'\t'`：recho 控制字符显示格式差（caret vs $'..'）。
- 4-6 行 `od` 列宽差：rb 侧 od=Git od.exe 与 GNU coreutils od 排版
  不同——[fixture] 辅助二进制版本差。

### comsub-posix 14 — [semantic]
- `sh_352.27/28` 行缺失：POSIX `$( )` 括号解析差。
- `hello`/`after 5`/`'` 缺失：comsub 内 `)` 作 heredoc 定界符
  （`cat << ')'`）未产出——heredoc-in-comsub 解析差。
- `we should not see this` 多打：被丢弃分支误执行。
- `ok 3` vs `bad 3`。

### comsub 13 — [semantic]
- `\/tmp\/foo\/bar`：替换结果反斜杠泄漏。
- `ok 2..ok 8` 九行缺失：一段 comsub 用例整体未产出。
- 多余 `Tue Sep 22 ...` date 行、空行差。

### procsub 13 — [semantic]
- `1 0 0 0`/`extern` 行错位、`0 0 0 0` vs `1 1 1`：
  进程替换退出状态/环境传播差。

### intl 8 — [semantic] 载体字节族（M 类）
- `U+00000018/1B/1C/1D` 四处 `$'\030'`/`$'\E'` 等编码失败：
  控制字节与 CTLESC 载体编码碰撞（subst.c dequote 族）。
- cd 诊断中不可打印字节未 `$'..'` 化（GNU 错误消息 quoting）。

### coproc 6 — [semantic]+[fixture]
- `./coproc.tests: line 53` vs `bash: line 53`：诊断名差（$0 归属）。
- `root` 缺失 + `/usr/bin/cat: /etc/passwd: No such`：
  [fixture] Windows 无 /etc/passwd 实体。
- `63 60` 多打。

### type 6 — [fixture]
- 全部 6 行：`$THIS_SH` 复制到 TMPDIR 后 hash/type 输出，
  GNU 二进制名 `bash` vs `rubash.exe`——二进制身份绑定，行为一致。

### set-x 5 — [semantic]
- `+ echo 1..4`/`unset BASH_XTRACEFD` 缺失：xtrace 经 XTRACEFD 输出差。

### globstar 4 — [semantic]
- `c/aa c/ab` 多匹配：`**` 递归边界差（globstar 目录遍历）。

### exp 4 — [semantic]（Windows HOME 暴露）
- `${x#$HOME}` 未剥 `C:\Users\Administrator` 前缀：模式中 `\`
  被当转义——平台暴露的模式匹配语义差。

### iquote 4 — [semantic]
- `argv[1] = <^?x>` 等 DEL(0x7F) 行丢失：引号内 DEL 字节处理差。

### errors 3 — [fixture]
- `/mnt/d/repo/rubash` vs `D:/repo/rubash`：PWD 拼写平台差。
- 1 空行差。

### posix2 3 — [semantic]
- `running $@ test failed` 多打、`2 of 27` vs `1 of 27`：`$@` 语义差。

### invocation 2 — [semantic]
- `cannot execute binary file` vs `No such file or directory`：
  `bash <binary>` 应 PATH 查找+读 magic 报 ENOEXEC，rubash 报 ENOENT
  （execute_cmd.c open_shell_file/binary_file 检测族）。

### nameref 1 — [semantic]
- `declare -r RO_PID` 多打：declare 列表对只读 PID 变量可见性差。

### ifs-posix 1 — [semantic]
- 套件汇总头 `# tests 6856 passed...` 缺失一行。

## 汇总

| 类别 | 行数（约） | 套件 |
|---|---|---|
| [semantic] 引擎语义差 | ~390 | jobs/glob/extglob/history/redir/vredir/cond/read/test/nquote/comsub*/procsub/intl/coproc/set-x/globstar/exp/iquote/posix2/invocation/nameref/ifs-posix |
| [fixture] 环境绑定 | ~15 | type(6) vredir(6) errors(2) coproc(1) nquote(od) |
| [timeout] 挂起截断 | ~54 | jobs（已计入 semantic 行数） |

主要根因族（按收益排序）：
1. **作业控制/回收竞态**（jobs 62：编号标记 + wait 信号状态 + wait 挂起）
2. **LC_COLLATE 排序**（glob+extglob ≈ 60+ 行）
3. **fd 重定向语义**（redir+vredir ≈ 50 行）
4. **fc 编辑/计数**（history 32）
5. **载体字节碰撞**（intl/iquote ≈ 12 行，M 类高危区）

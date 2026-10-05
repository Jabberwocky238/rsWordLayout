# `tools/measure/gate` — 每一片排版改动提交前的闸门

```sh
scripts/verify.sh                               # 全部
scripts/verify.sh --quick                       # 跳过 cargo test / clippy
scripts/verify.sh --quick --against OLD_BIN     # 另做 word_analyse 全夹具新旧对照（scan.py）
scripts/verify.sh --update-baselines            # 重写 baseline.json 与 mac-traces.sha256
```

产出在 `$VERIFY_OUT`（缺省 `/tmp/rswl-verify/<layout-trace sha256>`），最后打印汇总；任一项失败退出码非 0。

| 步骤 | 判据 | 真值来源 |
| --- | --- | --- |
| cargo test（workspace+fontenv、core 单独）与 clippy `-D warnings`（两种 feature） | 全过 | — |
| Android 窄路径回放 `tools/measure/android_replay.py` | standin、phone 两种字体配置都 186 / 186 | word_analyse `*.word.narrow.jsonl` |
| 转录读数 `readings.py` | 基线里一致的读数现在仍一致 | `readings.json`，每条有 `source` |
| Mac 采集回放 `tools/measure/sweep.py` | 25 份 trace.json 与 `mac-traces.sha256` 逐字节相同 | 仓库内 `captures/` |
| `scan.py`（仅 `--against`） | 只报告，不判失败；用来确认改动没有波及读数以外的夹具 | — |

## 字体配置

读数的结论随字体变，所以每个字体先按 SHA-256 认身份。缺文件、或哈希与 `baseline.json` 记的不同，
这一配置就报 **UNDECIDABLE**，不比较：换了文件就是换了实验，不能拿来判对错。

| 配置 | 字体 | 缺省路径（环境变量可改） | SHA-256 前缀 |
| --- | --- | --- | --- |
| standin | Mac Word 的 Calibri | `RSWL_CALIBRI`，`/Applications/Microsoft Word.app/Contents/Resources/DFonts/Calibri.ttf` | `ea801e1f` |
| standin | Noto Sans CJK SC（回退） | `RSWL_FALLBACK`，`~/code/docx-layout/corpus/layout/fonts/NotoSansCJKsc-Regular.otf` | `2c76254f` |
| phone | standin + 手机 Word 的等线 | `RSWL_DENGXIAN`，`~/.local/share/rswl/phonefonts/DengXian-54497409372.ttf` | `56ffa4ac` |

手机字体（等线、手机上的 Calibri 等）**不进仓库**；从手机取下后放在 `~/.local/share/rswl/phonefonts/`。
没有它们时 phone 配置是 UNDECIDABLE，不是失败。`dig-nofont` 这类无字体夹具只有在 phone 配置下才判得了。

## 读数

`readings.json` 每条：

```json
{"fixture": "wa:longpage-auto12pt" | "repo:fixtures/android-phone-2026-10-04/kern-48",
 "view": "print" | "mobile",
 "check": "starts" | "second_start" | "page0_lines" | "pages" | "page1_start",
 "expected": 47, "source": "出处", "note": "已知差异的说明（可选）", "group": "分组"}
```

- 只收 Word 量出来的东西：word_analyse 的 `reports/rsword-diff/*.md`、`findings/state-machines.md`、
  `findings/pagination-path.md`、`*.word.narrow.jsonl`、打印视图页数，以及本仓库对齐文档里记录的手机补测。
  反汇编与 P0 结论只是线索，不进这里。
- `starts` 比较前缀：引擎给出的行首序列前 `len(expected)` 个必须与 Word 相同。
- mobile 视图一律 `--content-width 5329`（手机移动视图的 `w3=0x14d1`）。

加一条读数：先在对齐文档里写下采集前的预测，采集，把结果写进 `readings.json`（写清 `source`），
跑 `readings.py` 看到 `new reading …`，然后在**同一个提交**里用 `--update-baselines` 记进基线。
引擎改动让读数变一致时会看到 `newly agrees …`，同样在说明原因的提交里更新基线。

## 采集脚本 `capture/`

2026-10-04 手机补测用的脚本，路径都可由环境变量改：

| 脚本 | 用途 |
| --- | --- |
| `mkfx.py` `mkfx2.py` `mkcs.py` `mkcs2.py` `mkt3.py` `mkbl_n.py` | 生成 `fixtures/android-phone-2026-10-04/` 的 32 份夹具（`RSWL_FX_OUT`，缺省 `/tmp/rswl-cap/fx`）。按此顺序运行可逐字节复现仓库里的副本。`mkfx2.py` 用手机的 Calibri 选字（`RSWL_PHONE_CALIBRI`） |
| `vis2.sh` | 打开夹具、按视图开关的标签确认进入打印/移动视图，边滚动边截图（读页数、行数靠截图） |
| `cap.sh` `probe2.sh` | 注入 word_analyse 的 line probe 取 `linetrace.log`；`starts.py` 从日志列出各可用宽度下的行首序列 |
| `cmetrics.py` `fontmetrics.py` | 不依赖第三方库的字宽、字体度量读取 |

设备相关：`ANDROID_SERIAL`（缺省 `b0e3d198`）、`WORD_ANALYSE`（缺省 `~/code/word_analyse`；
用它的 `tools/which_apk_libs_mapped.py`、`tools/read_fixture_print.sh` 和 line probe，word_analyse 只读）。
需要 root 的手机与已装的手机 Word；界面文字按中文界面写死（「打印视图」/「移动设备视图」）。

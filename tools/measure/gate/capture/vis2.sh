#!/usr/bin/env bash
# Open fixture, put it in the wanted view (verified by toggle label), then screenshot while scrolling.
# Usage: vis2.sh <fixture.docx> <outdir> <screens> <swipe_px> <print|mobile>
set -u
S=${ANDROID_SERIAL:-b0e3d198}; WA=${WORD_ANALYSE:-$HOME/code/word_analyse}; HERE=$(cd "$(dirname "$0")" && pwd); PKG=com.microsoft.office.word
FIX="$1"; OUT="$2"; N="$3"; STEP="$4"; MODE="$5"; NAME=$(basename "$FIX" .docx); mkdir -p "$OUT"
a(){ adb -s $S shell "$@"; }
label(){ a uiautomator dump /sdcard/ui.xml >/dev/null 2>&1; a cat /sdcard/ui.xml | tr '>' '\n' | grep -oE 'content-desc="(打印视图|移动设备视图)"' | head -1 | cut -d'"' -f2; }
want=$([ "$MODE" = print ] && echo 移动设备视图 || echo 打印视图)
adb -s $S push "$FIX" /sdcard/Documents/probe/ >/dev/null || exit 1
a "su -c 'am force-stop $PKG'"; sleep 2
a "am start -a android.intent.action.VIEW -d file:///sdcard/Documents/probe/$NAME.docx -t application/vnd.openxmlformats-officedocument.wordprocessingml.document -n $PKG/.WordActivity" >/dev/null
sleep 14
for try in 1 2 3 4; do
  l=$(label); [ "$l" = "$want" ] && break
  a input tap 600 1300; sleep 2; a input tap 308 2475; sleep 7
done
l=$(label); echo "$NAME: want $MODE, toggle label '$l'"
[ "$l" = "$want" ] || exit 1
for i in $(seq 0 $((N-1))); do
  adb -s $S exec-out screencap -p > "$OUT/$NAME-$MODE-$i.png"
  if [ $i -lt $((N-1)) ]; then a input swipe 600 2000 600 $((2000-STEP)) 900; sleep 2; fi
done

#!/usr/bin/env bash
# Probe after the target is already open (no doc switch, no view toggle); scroll to force relayout.
# Usage: probe2.sh <fixture.docx> <outdir>
set -u
S=${ANDROID_SERIAL:-b0e3d198}; WA=${WORD_ANALYSE:-$HOME/code/word_analyse}; HERE=$(cd "$(dirname "$0")" && pwd); PKG=com.microsoft.office.word
FIX="$1"; OUT="$2"; NAME=$(basename "$FIX" .docx); mkdir -p "$OUT"
a(){ adb -s $S shell "$@"; }
adb -s $S push "$FIX" /sdcard/Documents/probe/ >/dev/null || exit 1
a "su -c 'am force-stop $PKG'"; sleep 2; a "su -c 'rm -f /data/data/$PKG/files/linetrace.log'"
a "am start -a android.intent.action.VIEW -d file:///sdcard/Documents/probe/$NAME.docx -t application/vnd.openxmlformats-officedocument.wordprocessingml.document -n $PKG/.WordActivity" >/dev/null
PID=""; for i in $(seq 1 30); do sleep 2; P=$(a pidof $PKG | tr -d '\r'); [ -z "$P" ] && continue
  python3 "$WA/tools/which_apk_libs_mapped.py" --serial $S --pid "$P" --find libwlibandroid.so >/dev/null 2>&1 && { PID=$P; break; }; done
[ -n "$PID" ] || { echo "$NAME: engine not loaded"; exit 1; }
sleep 8
a "su -c '/data/local/tmp/inject $PID /data/data/$PKG/liblineprobe.so line'" >/dev/null 2>&1; sleep 3
for k in 1 2 3; do a input swipe 600 1900 600 700 300; sleep 2; a input swipe 600 700 600 1900 300; sleep 2; done; sleep 4
a "su -c 'cat /data/data/$PKG/files/linetrace.log'" > "$OUT/$NAME.log" 2>/dev/null
up=$(a pidof $PKG | tr -d '\r')
echo "$NAME: alive=${up:-no} log=$(wc -l < "$OUT/$NAME.log" | tr -d ' ') lines"
python3 "$HERE/starts.py" "$OUT/$NAME.log"

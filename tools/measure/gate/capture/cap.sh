#!/usr/bin/env bash
# Print-view capture for this Word UI (floating bottom bar, toggle labelled by the
# view it switches to). Usage: cap.sh <fixture.docx> <outdir>
# Writes <outdir>/<name>.log (probe log) and <name>.png (screen after relayout).
set -u
PKG=com.microsoft.office.word
S=${ANDROID_SERIAL:-b0e3d198}; WA=${WORD_ANALYSE:-$HOME/code/word_analyse}; HERE=$(cd "$(dirname "$0")" && pwd)
FIX="$1"; OUT="$2"; NAME=$(basename "$FIX" .docx); mkdir -p "$OUT"
a(){ adb -s $S shell "$@"; }
say(){ printf '[%s] %s: %s\n' "$(date +%H:%M:%S)" "$NAME" "$*"; }
toggle(){ # tap the view toggle; echo the label it had
  a uiautomator dump /sdcard/ui.xml >/dev/null 2>&1
  local line; line=$(a cat /sdcard/ui.xml | tr '>' '\n' | grep -E 'content-desc="(打印视图|移动设备视图)"' | head -1)
  [ -z "$line" ] && { echo none; return 1; }
  local d; d=$(echo "$line" | grep -oE 'content-desc="[^"]+"' | cut -d'"' -f2)
  local n; n=($(echo "$line" | grep -oE 'bounds="[^"]+"' | grep -oE '[0-9]+'))
  a input tap $(( (n[0]+n[2])/2 )) $(( (n[1]+n[3])/2 )); echo "$d"
}
adb -s $S push "$FIX" /sdcard/Documents/probe/ >/dev/null || exit 1
a "settings put system accelerometer_rotation 0; settings put system user_rotation 0" >/dev/null 2>&1
a "su -c 'am force-stop $PKG'"; sleep 2
a "su -c 'rm -f /data/data/$PKG/files/linetrace.log'"
a "am start -a android.intent.action.VIEW -d file:///sdcard/Documents/probe/$NAME.docx -t application/vnd.openxmlformats-officedocument.wordprocessingml.document -n $PKG/.WordActivity" >/dev/null
PID=""
for i in $(seq 1 30); do sleep 2; P=$(a pidof $PKG | tr -d '\r'); [ -z "$P" ] && continue
  python3 "$WA/tools/which_apk_libs_mapped.py" --serial $S --pid "$P" --find libwlibandroid.so >/dev/null 2>&1 && { PID=$P; break; }; done
[ -n "$PID" ] || { say "engine not loaded"; exit 1; }
sleep 6
a "su -c '/data/local/tmp/inject $PID /data/data/$PKG/liblineprobe.so line'" >/dev/null 2>&1; sleep 3
a "su -c 'grep -q LINEHOOK /data/data/$PKG/files/linetrace.log 2>/dev/null && echo yes'" | grep -q yes || { say "probe did not init"; exit 1; }
t1=$(toggle); sleep 6; t2=$(toggle); sleep 6
a dumpsys window | grep -q "mCurrentFocus.*$PKG" || { say "Word left the foreground after toggling ($t1/$t2)"; exit 1; }
a input swipe 600 1800 600 700 300; sleep 3; a input swipe 600 700 600 1800 300; sleep 8
a "su -c 'cat /data/data/$PKG/files/linetrace.log'" > "$OUT/$NAME.log" 2>/dev/null
adb -s $S exec-out screencap -p > "$OUT/$NAME.png"
paper=$(grep -cE 'w3=(28e2|2342)' "$OUT/$NAME.log"); mobile=$(grep -c 'w3=14d1' "$OUT/$NAME.log")
counts=$(grep -oE 'PGIDX n=[0-9]+ count=[0-9a-f]+' "$OUT/$NAME.log" | awk '{print $3}' | uniq -c | tr -s ' ' | tr '\n' ';')
say "toggles $t1 -> $t2; paper=$paper mobile=$mobile; PGIDX runs: $counts"

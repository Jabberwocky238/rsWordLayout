#!/usr/bin/env bash
# One command for every gate a layout slice must pass before it is committed.
#
#   scripts/verify.sh [--against OLD_LAYOUT_TRACE] [--quick] [--update-baselines]
#
# 1. cargo test (workspace with fontenv; core without) and clippy -D warnings (both feature sets)
# 2. Android narrow-path replay: 186 jsonl ranges, standin and phone fonts
# 3. transcribed Android Word readings (tools/measure/gate/readings.json) against baseline.json
# 4. Mac capture replay: the 25 traces must hash as in tools/measure/gate/mac-traces.sha256
# 5. with --against: every word_analyse fixture x 3 modes, old vs new (scan.py)
#
# --quick skips 1. --update-baselines rewrites baseline.json and mac-traces.sha256 from this
# build; use it only in the commit that explains why the verdicts changed.
# Outputs go to $VERIFY_OUT (default /tmp/rswl-verify/<binary sha>); a summary is printed at the end.
set -uo pipefail
cd "$(dirname "$0")/.."
REPO=$PWD
GATE=$REPO/tools/measure/gate
PY=$REPO/tools/measure/.venv/bin/python
WA=${WORD_ANALYSE:-$HOME/code/word_analyse}
AGAINST="" QUICK=0 UPDATE=0
while [ $# -gt 0 ]; do
  case $1 in
    --against) AGAINST=$2; shift ;;
    --quick) QUICK=1 ;;
    --update-baselines) UPDATE=1 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
eval "$("$PY" -c "import sys; sys.path.insert(0, '$GATE'); from readings import FONTS; [print(f'{k.upper()}={v!r}') for k, v in FONTS.items()]")"
SUMMARY=()
FAILED=0
note() { SUMMARY+=("$1"); echo "== $1"; }
fail() { FAILED=1; note "FAIL $1"; }

if [ $QUICK = 0 ]; then
  for args in "--workspace --features rsword-layout-core/fontenv" "-p rsword-layout-core"; do
    out=$(cargo test --offline $args 2>&1)
    counts=$(echo "$out" | grep "test result" | awk '{p+=$4; f+=$6; i+=$8} END {print p" passed "f" failed "i" ignored"}')
    if echo "$out" | grep -qE "FAILED|panicked|^error"; then
      echo "$out" | grep -E "FAILED|panicked|^error" | head -20
      fail "cargo test $args: $counts"
    else
      note "cargo test $args: $counts"
    fi
  done
  for args in "" "--features rsword-layout-core/fontenv"; do
    if cargo clippy --offline --workspace --all-targets $args -- -D warnings >/tmp/rswl-clippy.log 2>&1; then
      note "clippy $args: clean"
    else
      grep -E "^(error|warning)" -A6 /tmp/rswl-clippy.log | head -30
      fail "clippy $args"
    fi
  done
fi

cargo build --offline -q --features fontenv --bin layout-trace || { fail "build layout-trace"; exit 1; }
SHA=$(shasum -a 256 target/debug/layout-trace | cut -c1-64)
OUT=${VERIFY_OUT:-/tmp/rswl-verify/$SHA}
rm -rf "$OUT" && mkdir -p "$OUT"
BIN=$OUT/layout-trace
cp target/debug/layout-trace "$BIN"
note "layout-trace sha256 $SHA"

replay() { # name, extra font args...
  local name=$1; shift
  "$PY" tools/measure/android_replay.py --analysis-root "$WA" --trace-bin "$BIN" --font "$CALIBRI" "$@" \
    --fallback-font "$FALLBACK" --assume-legacy-narrow --output "$OUT/replay-$name" >"$OUT/replay-$name.log" 2>&1
  local line; line=$(grep Assessed "$OUT/replay-$name/summary.md" 2>/dev/null)
  case $line in *"186 / 186"*) note "android replay $name: $line" ;; *) fail "android replay $name: ${line:-no summary}" ;; esac
}
replay standin
if [ -f "$DENGXIAN" ]; then replay phone --font "$DENGXIAN"; else note "android replay phone: UNDECIDABLE (no $DENGXIAN)"; fi

update=(); [ $UPDATE = 1 ] && update=(--update-baseline)
"$PY" "$GATE/readings.py" --trace-bin "$BIN" --analysis-root "$WA" --output "$OUT/readings" ${update[@]+"${update[@]}"} | tee "$OUT/readings.log"
status=${PIPESTATUS[0]}
while read -r line; do note "readings $line"; done < <(grep -E "^(standin|phone):" "$OUT/readings.log")
[ "$status" = 0 ] || fail "readings: a reading that agreed in baseline.json no longer does"

"$PY" tools/measure/sweep.py --trace-bin "$BIN" --font-dir /System/Library/Fonts --font-dir fixtures/fonts \
  --output "$OUT/mac" >"$OUT/mac.log" 2>&1
(cd "$OUT/mac" && find . -name trace.json | sort | xargs shasum -a 256) >"$OUT/mac-traces.sha256"
if [ $UPDATE = 1 ]; then
  cp "$OUT/mac-traces.sha256" "$GATE/mac-traces.sha256"
  note "mac traces: baseline rewritten ($(wc -l <"$GATE/mac-traces.sha256" | tr -d ' ') traces)"
elif diff -u "$GATE/mac-traces.sha256" "$OUT/mac-traces.sha256" >"$OUT/mac.diff"; then
  note "mac traces: $(wc -l <"$OUT/mac-traces.sha256" | tr -d ' ') byte-identical to baseline"
else
  cat "$OUT/mac.diff"
  fail "mac traces differ from tools/measure/gate/mac-traces.sha256"
fi

if [ -n "$AGAINST" ]; then
  "$PY" "$GATE/scan.py" --old "$AGAINST" --new "$BIN" --analysis-root "$WA" --output "$OUT/scan" >"$OUT/scan.txt" 2>&1
  note "scan vs $(shasum -a 256 "$AGAINST" | cut -c1-12): $(head -1 "$OUT/scan.txt") (details: $OUT/scan.txt)"
fi

echo
echo "summary ($OUT)"
printf '  %s\n' "${SUMMARY[@]}"
exit $FAILED

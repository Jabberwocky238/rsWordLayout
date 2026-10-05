#!/usr/bin/env python3
"""Run an old and a new layout-trace over every word_analyse fixture in three modes and report
which traces changed (byte-level) and how their line starts moved.

Catches changes outside the fixtures that have readings: a slice that should only touch tables
must not move a single line anywhere else.

    scan.py --old OLD_BIN --new NEW_BIN --output DIR [--analysis-root ~/code/word_analyse]

Fonts as in readings.py (standin configuration).
"""
import argparse, concurrent.futures as cf, json, os, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from readings import FONTS, HOME, MOBILE_WIDTH  # noqa: E402

MODES = {
    "android-print": ["--platform", "android", "--fallback-font", FONTS["fallback"]],
    "android-mobile": ["--platform", "android", "--view", "mobile", "--content-width", MOBILE_WIDTH,
                       "--fallback-font", FONTS["fallback"]],
    "mac-print": [],
}


def run(binary, tag, mode, path, output):
    out = Path(output) / tag / mode / f"{path.stem}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run([binary, *MODES[mode], "--font", FONTS["calibri"], str(path), str(out)],
                       capture_output=True, text=True, timeout=600)
    return str(out) if r.returncode == 0 else "ERR:" + (r.stderr.strip().splitlines() or ["?"])[-1]


def ranges(path):
    t = json.load(open(path))
    return [[(l["sourceStart"], l["sourceEnd"]) for l in p["lines"]] for p in t["pages"]]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--old", required=True)
    ap.add_argument("--new", required=True)
    ap.add_argument("--output", required=True)
    ap.add_argument("--analysis-root", default=str(HOME / "code/word_analyse"))
    a = ap.parse_args()
    fixtures = sorted((Path(a.analysis_root) / "fixtures").glob("*.docx"))
    jobs = [(b, tag, m, f) for m in MODES for f in fixtures for b, tag in ((a.old, "old"), (a.new, "new"))]
    with cf.ThreadPoolExecutor(os.cpu_count() or 8) as ex:
        results = dict(zip(jobs, ex.map(lambda j: run(*j, a.output), jobs)))
    total, changed = 0, []
    for m in MODES:
        for f in fixtures:
            total += 1
            old, new = results[(a.old, "old", m, f)], results[(a.new, "new", m, f)]
            if old.startswith("ERR") or new.startswith("ERR"):
                if old != new:
                    changed.append((m, f.stem, f"error changed: {old!r} -> {new!r}"))
                continue
            if open(old, "rb").read() != open(new, "rb").read():
                ro, rn = ranges(old), ranges(new)
                note = "glyphs only" if ro == rn else \
                    f"lines {[l[0] for p in ro for l in p][:8]} -> {[l[0] for p in rn for l in p][:8]}"
                changed.append((m, f.stem, note))
    print(f"{len(changed)} of {total} runs changed")
    for row in changed:
        print(*row)


if __name__ == "__main__":
    main()

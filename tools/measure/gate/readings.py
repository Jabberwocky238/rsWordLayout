#!/usr/bin/env python3
"""Check layout-trace against the transcribed Android Word readings in `readings.json`.

Each reading is one number (or list of line starts) Word showed on the phone: a print-view
page count, the lines on page 0, or where lines start. The expected values are copied from
word_analyse (reports/rsword-diff/*.md, findings/*.md) and from the phone captures recorded in
docs/WORD-ANALYSE-P0-ALIGNMENT-2026-10-04.md; `source` says which.

Two font configurations, because the verdicts depend on them:

  standin  Mac Word's Calibri + Noto Sans CJK fallback (what every machine here has)
  phone    standin + the phone's DengXian (Android Word's default face; not in the repo)

Every font is identified by SHA-256 before it is used. A configuration whose fonts are missing or
differ from `baseline.json` is reported UNDECIDABLE, never compared: a different file is a
different experiment.

    readings.py --trace-bin target/debug/layout-trace --output /tmp/gate [--config standin|phone|all]
                [--update-baseline]

Exit status: 0 when no reading that agrees in the baseline disagrees now; 1 otherwise.
"""
import argparse, concurrent.futures as cf, hashlib, json, os, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
HOME = Path.home()
FONTS = {
    "calibri": os.environ.get("RSWL_CALIBRI", "/Applications/Microsoft Word.app/Contents/Resources/DFonts/Calibri.ttf"),
    "fallback": os.environ.get("RSWL_FALLBACK", str(HOME / "code/docx-layout/corpus/layout/fonts/NotoSansCJKsc-Regular.otf")),
    "dengxian": os.environ.get("RSWL_DENGXIAN", str(HOME / ".local/share/rswl/phonefonts/DengXian-54497409372.ttf")),
}
CONFIGS = {"standin": ["calibri", "fallback"], "phone": ["calibri", "fallback", "dengxian"]}
MOBILE_WIDTH = "5329"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def fixture_path(spec, analysis_root):
    kind, name = spec.split(":", 1)
    if kind == "wa":
        return Path(analysis_root) / "fixtures" / f"{name}.docx"
    if kind == "repo":
        return REPO / f"{name}.docx"
    raise ValueError(spec)


def run(binary, fonts, fixture, view, out):
    if out.exists():
        return out, None
    out.parent.mkdir(parents=True, exist_ok=True)
    cmd = [binary, "--platform", "android", "--view", view, "--font", fonts["calibri"], "--fallback-font", fonts["fallback"]]
    if "dengxian" in fonts:
        cmd += ["--font", fonts["dengxian"]]
    if view == "mobile":
        cmd += ["--content-width", MOBILE_WIDTH]
    cmd += [str(fixture), str(out)]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    if r.returncode:
        return None, (r.stderr.strip().splitlines() or ["?"])[-1][:120]
    return out, None


def observe(trace, check):
    pages = trace["pages"]
    starts = [l["sourceStart"] for p in pages for l in p["lines"]]
    if check == "starts":
        return starts
    if check == "second_start":
        return starts[1] if len(starts) > 1 else None
    if check == "page0_lines":
        return len(pages[0]["lines"])
    if check == "pages":
        return len(pages)
    if check == "page1_start":
        return pages[1]["lines"][0]["sourceStart"] if len(pages) > 1 and pages[1]["lines"] else None
    raise ValueError(check)


def case_id(r):
    return f'{r["fixture"]}@{r["view"]}:{r["check"]}'


def evaluate(binary, config, readings, analysis_root, output):
    fonts = {k: FONTS[k] for k in CONFIGS[config]}
    missing = [p for p in fonts.values() if not Path(p).is_file()]
    if missing:
        return {"status": "UNDECIDABLE", "reason": f"font missing: {missing}"}
    identity = {k: {"path": p, "sha256": sha256(p)} for k, p in fonts.items()}
    jobs = sorted({(r["fixture"], r["view"]) for r in readings})
    def one(job):
        fixture, view = job
        out = Path(output) / config / f'{fixture.replace(":", "_").replace("/", "_")}.{view}.json'
        return run(binary, fonts, fixture_path(fixture, analysis_root), view, out)
    with cf.ThreadPoolExecutor(os.cpu_count() or 8) as ex:
        done = dict(zip(jobs, ex.map(one, jobs)))
    verdicts = {}
    for r in readings:
        path, err = done[(r["fixture"], r["view"])]
        if err:
            verdicts[case_id(r)] = {"verdict": "ERROR", "engine": err}
            continue
        got = observe(json.load(open(path)), r["check"])
        exp = r["expected"]
        good = got[: len(exp)] == exp if r["check"] == "starts" else got == exp
        shown = got[: len(exp) + 1] if r["check"] == "starts" else got
        verdicts[case_id(r)] = {"verdict": "OK" if good else "DIFF", "engine": shown}
    return {"status": "DECIDED", "fonts": identity, "verdicts": verdicts}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--trace-bin", required=True)
    ap.add_argument("--output", required=True, help="fresh directory; existing traces in it are reused")
    ap.add_argument("--analysis-root", default=str(HOME / "code/word_analyse"))
    ap.add_argument("--config", choices=[*CONFIGS, "all"], default="all")
    ap.add_argument("--update-baseline", action="store_true")
    a = ap.parse_args()
    readings = json.load(open(HERE / "readings.json"))
    baseline_path = HERE / "baseline.json"
    baseline = json.load(open(baseline_path)) if baseline_path.exists() else {}
    configs = list(CONFIGS) if a.config == "all" else [a.config]
    result = {"traceBin": {"path": a.trace_bin, "sha256": sha256(a.trace_bin)}, "configs": {}}
    regressions = 0
    for config in configs:
        res = evaluate(a.trace_bin, config, readings, a.analysis_root, a.output)
        result["configs"][config] = res
        if res["status"] != "DECIDED":
            print(f"{config}: UNDECIDABLE ({res['reason']})")
            continue
        base = baseline.get(config)
        if base and base["fonts"] != {k: v["sha256"] for k, v in res["fonts"].items()}:
            print(f"{config}: UNDECIDABLE (font files differ from baseline.json)")
            res["status"] = "UNDECIDABLE"
            continue
        v = res["verdicts"]
        groups = {}
        for r in readings:
            g = groups.setdefault(r.get("group", "-"), [0, 0])
            g[0] += v[case_id(r)]["verdict"] == "OK"
            g[1] += 1
        ok = sum(x[0] for x in groups.values())
        print(f"{config}: {ok}/{len(readings)} agree  (" + ", ".join(f"{g} {x[0]}/{x[1]}" for g, x in sorted(groups.items())) + ")")
        if base:
            for cid, was in base["verdicts"].items():
                now = v.get(cid, {}).get("verdict", "MISSING")
                if was == "OK" and now != "OK":
                    regressions += 1
                    print(f"  REGRESSED {cid}: engine={v.get(cid, {}).get('engine')}")
                elif was != "OK" and now == "OK":
                    print(f"  newly agrees {cid}")
            for cid in v.keys() - base["verdicts"].keys():
                print(f"  new reading {cid}: {v[cid]['verdict']}")
    Path(a.output).mkdir(parents=True, exist_ok=True)
    (Path(a.output) / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=1))
    by_id = {case_id(r): r for r in readings}
    with open(Path(a.output) / "verdicts.txt", "w") as f:
        for config, res in result["configs"].items():
            for cid, x in res.get("verdicts", {}).items():
                f.write(f'{config:8s} {x["verdict"]:5s} {cid:70s} word={by_id[cid]["expected"]} engine={x["engine"]}\n')
    if a.update_baseline:
        for config, res in result["configs"].items():
            if res["status"] == "DECIDED":
                baseline[config] = {"fonts": {k: x["sha256"] for k, x in res["fonts"].items()},
                                    "verdicts": {cid: x["verdict"] for cid, x in res["verdicts"].items()}}
        baseline_path.write_text(json.dumps(baseline, ensure_ascii=False, indent=1, sort_keys=True) + "\n")
        print(f"baseline written: {baseline_path}")
    return 1 if regressions else 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Performance gate: wall time and peak memory of a release layout-trace on generated documents.

Roadmap §10 asks for a repeatable baseline before any optimisation: 1/10/100/1000 pages of body
text, single-column tables and a mix of both. The documents are generated here, deterministically,
so the baseline needs no files outside the repo; fonts are the standin configuration of readings.py.

    perf.py --trace-bin target/release/layout-trace --output /tmp/perf [--sizes 1,10,100]
            [--repeat 3] [--update-baseline]

layout-trace runs with --no-trace (no JSON: a 100-page trace is ~200 MB and its serialisation would
dominate both numbers) and --timing, so each case records the layout and paint phases as well as the
whole process. Each document runs --repeat times; medians of the times and the largest peak RSS are
kept. The 1000-page size is left out of the default until body layout is fast enough to run it
routinely (2026-10-05: ~9 pages/s, see docs/AS-BUILT.md).
A run is compared with perf-baseline.json only on the same machine (CPU, core count, memory) and
the same font files; anything else is UNDECIDABLE. A regression is more than 1.5x the baseline
time (plus 50 ms for start-up noise) or more than 1.3x its peak memory. Exit 1 on a regression.
"""
import argparse, json, os, platform, re, statistics, subprocess, sys, time, zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from readings import FONTS, sha256  # noqa: E402

HERE = Path(__file__).resolve().parent
BASELINE = HERE / "perf-baseline.json"
TIME_FACTOR, TIME_SLACK_S, RSS_FACTOR = 1.5, 0.05, 1.3

CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
SECT = ('<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" '
        'w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>')
RPR = '<w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr>'
WORDS = ("layout paragraph section measure glyph cluster baseline column table border spacing "
         "indent kerning advance shaping fallback pagination grid").split()
HAN = "排版段落分页表格字体行高网格"
TABLE_PR = ('<w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/>'
            '<w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr>')
# Rough page capacities at Calibri 11pt on A4 with 1440 margins, only to size the documents.
PARAS_PER_PAGE, ROWS_PER_PAGE = 9, 28


def text(i):
    # ~5 lines: Latin words with a few Han runs, varied so lines do not repeat exactly
    words = [WORDS[(i * 7 + k * 3) % len(WORDS)] for k in range(70)]
    for k in range(10, 70, 17):
        words[k] = HAN[(i + k) % len(HAN):] + HAN[: (i + k) % len(HAN)]
    return f"P{i:05d} " + " ".join(words)


def para(i):
    return f'<w:p><w:r>{RPR}<w:t xml:space="preserve">{text(i)}</w:t></w:r></w:p>'


def table(start, rows):
    trs = "".join(f'<w:tr><w:trPr><w:trHeight w:val="480" w:hRule="exact"/></w:trPr><w:tc><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr>'
                  f'<w:r>{RPR}<w:t>R{start + r:06d} {WORDS[r % len(WORDS)]}</w:t></w:r></w:p></w:tc></w:tr>' for r in range(rows))
    return f"<w:tbl>{TABLE_PR}{trs}</w:tbl>"


def body(kind, pages):
    if kind == "body":
        return "".join(para(i) for i in range(pages * PARAS_PER_PAGE))
    if kind == "table":
        return table(0, pages * ROWS_PER_PAGE) + para(0)
    parts = []  # mixed: per page, half a page of text then half a page of table
    for p in range(pages):
        parts += [para(p * 5 + k) for k in range(PARAS_PER_PAGE // 2)]
        parts.append(table(p * ROWS_PER_PAGE, ROWS_PER_PAGE // 2))
    return "".join(parts) + para(0)


def make(kind, pages, out):
    path = out / f"{kind}-{pages}.docx"
    if not path.exists():
        xml = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>'
               + body(kind, pages) + SECT + "</w:body></w:document>")
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
            for name, data in (("[Content_Types].xml", CT), ("_rels/.rels", RELS), ("word/document.xml", xml)):
                z.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), data)
    return path


def machine():
    m = {"system": platform.system(), "arch": platform.machine(), "cpus": os.cpu_count()}
    if m["system"] == "Darwin":
        q = lambda k: subprocess.run(["sysctl", "-n", k], capture_output=True, text=True).stdout.strip()
        m["cpu"], m["memory"] = q("machdep.cpu.brand_string"), q("hw.memsize")
    else:
        info = Path("/proc/cpuinfo").read_text() if Path("/proc/cpuinfo").exists() else ""
        mm = re.search(r"model name\s*:\s*(.*)", info)
        m["cpu"] = mm.group(1) if mm else platform.processor()
        mem = re.search(r"MemTotal:\s*(\d+)", Path("/proc/meminfo").read_text()) if Path("/proc/meminfo").exists() else None
        m["memory"] = mem.group(1) if mem else "?"
    return m


def timed(cmd):
    """Run cmd under /usr/bin/time; return (seconds, peak RSS bytes, phases, pages)."""
    darwin = platform.system() == "Darwin"
    full = ["/usr/bin/time", "-l" if darwin else "-v", *cmd]
    t0 = time.perf_counter()
    r = subprocess.run(full, capture_output=True, text=True)
    dt = time.perf_counter() - t0
    if r.returncode:
        raise RuntimeError(f"{cmd[-2]}: {r.stderr.strip().splitlines()[-1:]}")
    if darwin:
        rss = int(re.search(r"(\d+)\s+maximum resident set size", r.stderr).group(1))
    else:
        rss = int(re.search(r"Maximum resident set size \(kbytes\): (\d+)", r.stderr).group(1)) * 1024
    phases = {k: float(v) for k, v in re.findall(r"(\w+)=([\d.]+)", re.search(r"^timing (.*)$", r.stderr, re.M).group(1))}
    pages = int(re.search(r"页 (\d+)", r.stdout).group(1))
    return dt, rss, phases, pages


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--trace-bin", required=True, help="a release build of layout-trace")
    ap.add_argument("--output", required=True)
    ap.add_argument("--sizes", default="1,10,100")
    ap.add_argument("--repeat", type=int, default=3)
    ap.add_argument("--update-baseline", action="store_true")
    a = ap.parse_args()
    out = Path(a.output)
    (out / "docs").mkdir(parents=True, exist_ok=True)
    fonts = {k: FONTS[k] for k in ("calibri", "fallback")}
    missing = [p for p in fonts.values() if not Path(p).is_file()]
    if missing:
        print(f"perf: UNDECIDABLE (font missing: {missing})")
        return 0
    env = {"machine": machine(), "fonts": {k: sha256(p) for k, p in fonts.items()}}
    results = {}
    for kind in ("body", "table", "mixed"):
        for pages in [int(s) for s in a.sizes.split(",")]:
            doc = make(kind, pages, out / "docs")
            cmd = [a.trace_bin, "--no-trace", "--timing", "--font", fonts["calibri"], "--fallback-font", fonts["fallback"],
                   str(doc), str(out / "unused.json")]
            runs = [timed(cmd) for _ in range(a.repeat)]
            laid = runs[0][3]
            secs = statistics.median(r[0] for r in runs)
            layout = statistics.median(r[2]["layout"] for r in runs)
            paint = statistics.median(r[2]["paint"] for r in runs)
            rss = max(r[1] for r in runs)
            results[f"{kind}-{pages}"] = {"seconds": round(secs, 4), "layoutSeconds": round(layout, 4),
                                          "paintSeconds": round(paint, 4), "peakRssBytes": rss, "pages": laid}
            print(f"{kind}-{pages:<5} {laid:5d} pages  {secs:8.3f} s (layout {layout:7.3f}, paint {paint:6.3f})"
                  f"  {laid / secs:8.1f} pages/s  {rss / 2**20:7.1f} MiB")
    result = {"traceBin": sha256(a.trace_bin), **env, "results": results}
    (out / "perf.json").write_text(json.dumps(result, indent=1))
    base = json.load(open(BASELINE)) if BASELINE.exists() else None
    regressions = 0
    if base and (base["machine"], base["fonts"]) != (env["machine"], env["fonts"]):
        print("perf: UNDECIDABLE against baseline (different machine or fonts)")
    elif base:
        for case, now in results.items():
            was = base["results"].get(case)
            if not was:
                print(f"  new case {case}")
                continue
            if now["seconds"] > was["seconds"] * TIME_FACTOR + TIME_SLACK_S:
                regressions += 1
                print(f"  REGRESSED {case}: {was['seconds']} s -> {now['seconds']} s")
            if now["peakRssBytes"] > was["peakRssBytes"] * RSS_FACTOR:
                regressions += 1
                print(f"  REGRESSED {case}: peak {was['peakRssBytes'] >> 20} MiB -> {now['peakRssBytes'] >> 20} MiB")
        print(f"perf: {len(results)} cases, {regressions} regressions against baseline")
    if a.update_baseline:
        BASELINE.write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
        print(f"baseline written: {BASELINE}")
    return 1 if regressions else 0


if __name__ == "__main__":
    sys.exit(main())

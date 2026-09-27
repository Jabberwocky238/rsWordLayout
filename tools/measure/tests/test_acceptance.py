import json
from pathlib import Path
import zipfile

import acceptance


def write(path: Path, value) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")
    return path


def docx(path: Path, document: str = "<w:body/>", footnotes: str | None = None) -> Path:
    with zipfile.ZipFile(path, "w") as package:
        package.writestr("word/document.xml", document)
        if footnotes is not None:
            package.writestr("word/footnotes.xml", footnotes)
    return path


def mac_case(name="probe", evidence="strict"):
    return {"id": "mac-print/" + name, "group": "probe", "families": ["vertical-pagination"],
            "config": {"id": "mac"}, "document": {"path": None, "sha256": "doc"},
            "reference": {"kind": "mac-pdf-glyph-bundle", "metaSha256": "meta"}, "evidence": evidence}


def comparison(state="OK", dx=0.0, dy=0.0, glyphs=(10, 10), pages=(1, 1), codes=()):
    return {"state": state, "comparison": {
        "state": state, "tolerancePt": 0.0, "maxDx": dx, "maxDy": dy,
        "failures": [{"code": code} for code in codes],
        "structure": {"candidateGlyphs": glyphs[0], "referenceGlyphs": glyphs[1],
                      "candidatePages": pages[0], "referencePages": pages[1]}},
        "decomposition": {"lineStart": {"dx": {"maxAbs": -dx}, "dy": {"maxAbs": -dy}}}}


def sweep_bundle(root: Path, name: str, fixture: Path, *, compare, selfcheck="OK", trace=None):
    out = root / name
    write(out / "result.json", {"state": compare["state"], "reason": None, "metaSha256": "meta",
                                "traceRun": {"exitCode": 0},
                                "binding": {"state": "OK", "fixture": str(fixture), "fixtureSha256": "doc"}})
    write(out / "trace.json", trace or {"layoutInput": {"skippedBlocks": 0, "diagnostics": []}, "pages": []})
    write(out / "comparison.json", compare)
    write(out / "selfcheck.json", {"state": selfcheck, "checks": [
        {"check": "terminator", "state": selfcheck, "reason": None if selfcheck == "OK" else "no line"}]})


def registry(*cases):
    return {"schema": acceptance.REGISTRY_SCHEMA, "version": 1, "codeBaseline": "x", "cases": list(cases)}


def test_strict_pass_requires_every_layer(tmp_path):
    fixture = docx(tmp_path / "doc.docx")
    sweep = tmp_path / "sweep"
    sweep_bundle(sweep, "pass", fixture, compare=comparison())
    sweep_bundle(sweep, "selfcheck", fixture, compare=comparison(), selfcheck="UNDECIDABLE")
    sweep_bundle(sweep, "fail", fixture, compare=comparison("FAIL", dx=0.5, dy=4.8))
    sweep_bundle(sweep, "partial", fixture, compare=comparison(),
                 trace={"layoutInput": {"skippedBlocks": 1, "diagnostics": ["x"]}})
    panel = acceptance.build_panel(registry(*(mac_case(n) for n in ("pass", "selfcheck", "fail", "partial"))),
                                   sweep, None, None, None)
    rows = {row["id"].split("/")[1]: row for row in panel["cases"]}
    assert rows["pass"]["strict"] == "PASS"
    assert rows["selfcheck"]["strict"] == "UNDECIDABLE"
    assert rows["fail"]["strict"] == "FAIL"
    assert [layer["layer"] for layer in rows["fail"]["layers"]] == ["geometry-vertical", "geometry-horizontal"]
    assert rows["fail"]["layers"][0]["lineStartMaxAbsPt"] == 4.8
    assert rows["partial"]["strict"] == "UNDECIDABLE"
    assert "SUPPORT_PARTIAL" in rows["partial"]["reasons"]
    assert panel["northStar"]["strictPass"] == 1
    assert panel["northStar"]["byFamily"]["tables"]["cases"] == 0


def test_bindings_and_void_references_stay_undecidable(tmp_path):
    fixture = docx(tmp_path / "doc.docx")
    sweep = tmp_path / "sweep"
    sweep_bundle(sweep, "changed", fixture, compare=comparison())
    write(sweep / "void" / "result.json", {"state": "UNDECIDABLE", "reason": "CAPTURE_VOID"})
    changed = mac_case("changed")
    changed["document"]["sha256"] = "other"
    panel = acceptance.build_panel(registry(changed, mac_case("void", "reference-invalid"), mac_case("absent")),
                                   sweep, None, None, None)
    rows = {row["id"].split("/")[1]: row for row in panel["cases"]}
    assert rows["changed"]["reasons"] == ["DOCUMENT_HASH_MISMATCH"]
    assert rows["void"]["strict"] == "UNDECIDABLE"
    assert rows["void"]["support"]["state"] == "not-evaluated"
    assert rows["absent"]["strict"] == "NOT_RUN"


def test_structural_layers_are_named():
    layers = acceptance.mac_layers(comparison("FAIL", glyphs=(252, 63), pages=(4, 5),
                                              codes=("LINE_COUNT_MISMATCH", "PAGE_UNDECIDABLE")))
    assert [layer["layer"] for layer in layers] == [
        "glyph-count", "pagination", "line-structure", "pairing-undecidable"]


def test_unlaid_stories_make_support_partial(tmp_path):
    notes = '<w:footnotes><w:footnote w:type="separator" w:id="-1"/><w:footnote w:id="0"/><w:footnote w:id="1"/></w:footnotes>'
    body = "<w:body><w:drawing/><w:headerReference w:id='r1'/></w:body>"
    document = docx(tmp_path / "notes.docx", body, notes)
    assert acceptance.story_inventory(document) == {"footnotes": 1, "headerReferences": 1, "drawings": 1}
    support = acceptance.support_of({"layoutInput": {"skippedBlocks": 0, "diagnostics": []}}, document)
    assert support["state"] == "partial"
    plain = docx(tmp_path / "plain.docx", footnotes='<w:footnotes><w:footnote w:id="0"/></w:footnotes>')
    assert acceptance.support_of({"layoutInput": {}}, plain) == {"state": "full"}


def test_conditional_and_report_results_never_count_as_strict(tmp_path):
    fixture = docx(tmp_path / "doc.docx")
    trace = write(tmp_path / "engine.json", {"layoutInput": {}})
    replay = tmp_path / "replay"
    write(replay / "summary.json", {"results": [{
        "fixture": "lines", "trace": {"path": str(trace)},
        "inputs": {"fixture": {"sha256": "doc"}, "capture": {"sha256": "cap"}},
        "comparison": {"state": "OK", "boundary": {"matchedLines": 3, "wordLines": 3, "engineLines": 3}}}]})
    android = {"id": "android-mobile-legacy/lines", "group": "lines", "families": ["line-breaking"],
               "config": {"id": "android"}, "evidence": "conditional",
               "document": {"path": str(fixture), "sha256": "doc"},
               "reference": {"kind": "android-narrow-line-log", "sha256": "cap"},
               "baseline": {"state": "OK", "matchedLines": 3}}
    run = tmp_path / "reports"
    write(run / "manifest.json", {"results": [{"id": "android-print-report/two", "documentSha256": "doc"}]})
    write(run / "two" / "trace.json", {"layoutInput": {}, "pages": [
        {"lines": [{"sourceStart": 0}]}, {"lines": [{"sourceStart": 40}]}]})
    report = {"id": "android-print-report/two", "group": "two", "families": ["tables"],
              "config": {"id": "report"}, "evidence": "report-only",
              "document": {"path": str(fixture), "sha256": "doc"},
              "reference": {"kind": "report-constraint", "constraint": {"pages": 2, "secondPageStart": 40}}}
    panel = acceptance.build_panel(registry(android, report), None, replay, run, None)
    rows = {row["id"]: row for row in panel["cases"]}
    assert rows[android["id"]]["strict"] == "UNDECIDABLE"
    assert rows[android["id"]]["conditional"] == "OK"
    assert rows[android["id"]]["baselineReproduced"] is True
    assert rows[report["id"]]["strict"] == "UNDECIDABLE"
    assert rows[report["id"]]["report"]["state"] == "MATCH"
    assert panel["northStar"]["strictPass"] == 0


def test_previous_pass_that_no_longer_passes_is_a_regression(tmp_path):
    fixture = docx(tmp_path / "doc.docx")
    sweep = tmp_path / "sweep"
    sweep_bundle(sweep, "probe", fixture, compare=comparison("FAIL", dy=1.0))
    previous = {"createdAt": "earlier", "cases": [{"id": "mac-print/probe", "strict": "PASS"}]}
    panel = acceptance.build_panel(registry(mac_case()), sweep, None, None, previous)
    assert panel["guardrails"]["stability"]["newRegressions"] == ["mac-print/probe"]
    assert panel["guardrails"]["stability"]["strictChanges"] == [
        {"id": "mac-print/probe", "before": "PASS", "after": "FAIL"}]


def test_committed_registry_covers_every_capture_bundle():
    registry = json.loads((acceptance.REPO / "tools/measure/acceptance/cases-v1.json").read_text())
    ids = [case["id"] for case in registry["cases"]]
    assert registry["schema"] == acceptance.REGISTRY_SCHEMA
    assert len(ids) == len(set(ids))
    bundles = sorted(p.name for p in (acceptance.REPO / "captures").iterdir() if (p / "META.json").is_file())
    assert sorted(i.split("/", 1)[1] for i in ids if i.startswith("mac-print/")) == bundles
    for case in registry["cases"]:
        assert case["evidence"] in registry["evidenceStates"]
        assert set(case["families"]) <= set(registry["families"])

"""The preregistered evaluator consumes offline evidence, never Word or system fonts."""

import copy
import hashlib
import json
import zipfile
from fractions import Fraction
from pathlib import Path

import pytest

import prereg_vertical_precision as probe
from wordmeasure import FAIL, OK, UNDECIDABLE, sweep_stability

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value))


def evidence(tmp_path, sizes=(20,), *, paint_ratio=.66, advance_ratio=.66,
             rise_ratio=.34, drop_ratio=.08, scaling=1):
    bundle = tmp_path / "capture"
    bundle.mkdir()
    cases, text, glyphs, positions = [], "", [], []
    for line_index, hp in enumerate(sizes):
        name = "F0S%d" % hp
        parts = {"tag": name + ":", "anchorA": "A", "normal": "HHHHHH", "anchorB": "B",
                 "superscript": "HHHHHH", "anchorC": "C", "subscript": "HHHHHH", "anchorD": "D"}
        start, spans, size, x, baseline = len(text), {}, hp / 2, 40, 50 + 30 * line_index
        for role, part in parts.items():
            spans[role] = [len(text), len(text) + len(part)]
            text += part
            marked = role in ("superscript", "subscript")
            paint_size = size * paint_ratio if marked else size
            advance = size * .6 * (advance_ratio if marked else 1)
            y = baseline - size * rise_ratio if role == "superscript" else baseline + size * drop_ratio if role == "subscript" else baseline
            for character in part:
                glyphs.append({"index": len(glyphs), "glyphOrigin": [x, y], "unraisedOrigin": [-999, -999],
                               "advanceVector": [-999, -999], "text": character, "textStatus": "mapped",
                               "fontName": "ABCDEF+SyntheticPS", "fontSize": 2,
                               "effectiveSizePt": -999, "scaling": scaling,
                               "matrix": [paint_size / 2 / scaling, 0, 0, paint_size / 2, 10, -20],
                               "rise": 0, "geometryStatus": "RAW_HORIZONTAL"})
                x += advance
        text += "\r"
        glyphs.append({**copy.deepcopy(glyphs[-1]), "index": len(glyphs), "text": " ", "glyphOrigin": [x, baseline]})
        cases.append({"id": name, "family": "Synthetic", "sizeHalfPoints": hp, "sourceStart": start,
                      "sourceEnd": len(text), "text": text[start:], "spans": spans})
        positions.extend({"offset": offset, "line": line_index + 1, "page": 1} for offset in range(start, len(text)))
    source = tmp_path / "fixture.docx"
    with zipfile.ZipFile(source, "w") as archive:
        body = "".join("<w:p><w:r><w:t>%s</w:t></w:r></w:p>" % case["text"][:-1] for case in cases)
        archive.writestr("word/document.xml", '<w:document xmlns:w="%s"><w:body>%s</w:body></w:document>' % (W, body))
    font_inputs = tmp_path / "fonts.json"
    write(font_inputs, {"schema": "rsword-layout-font-inputs/1", "fonts": [
        {"family": "Synthetic", "postscript": "SyntheticPS", "upem": 1000, "H": {"hmtxAdvance": 600},
         "OS2": {"ySuperscriptXSize": 700, "ySuperscriptYSize": 650, "ySuperscriptYOffset": 450,
                 "ySubscriptXSize": 700, "ySubscriptYSize": 650, "ySubscriptYOffset": 140}}]})
    manifest = tmp_path / "probes.json"
    write(manifest, {"schema": "rsword-layout-vertical-probes/1", "fixtureSha256": sha(source.read_bytes()),
                     "fontInputsSha256": sha(font_inputs.read_bytes()), "expectedPages": 1,
                     "expectedLines": len(cases), "cases": cases})
    pdf = b"%PDF-1.7\n% Synthetic offline evidence only.\n"
    (bundle / "case.pdf").write_bytes(pdf)
    sweep = {"contentText": text.replace("\r", "\n"), "endOfContent": len(text), "platform": "mac", "boxAvailable": False,
             "paragraphs": [{"start": c["sourceStart"], "end": c["sourceEnd"]} for c in cases], "positions": positions}
    raw = (str(len(text)) + "\n---\n" + "\n".join("%d,%d,%d" % (p["offset"], p["line"], p["page"]) for p in positions) + "\n").encode()
    scans = []
    for name in ("first", "repeat"):
        (bundle / ("sweep-%s.raw.txt" % name)).write_bytes(raw)
        scan = sweep_stability.parse_receipt(raw.decode())
        scan["rawReceipt"] = {"file": "sweep-%s.raw.txt" % name, "sha256": sha(raw), "bytes": len(raw)}
        scans.append(scan)
    digest = sha(source.read_bytes())
    meta = {"fixture": {"unchanged": True, "before": {"sha256": digest}, "after": {"sha256": digest}},
            "preflight": {"result": "PASS", "requiredFamilies": {"Synthetic": True}},
            "fontSubstitution": {"result": "PASS", "requiredFamiliesSeenInPdf": {"Synthetic": True}},
            "pdfSha256": sha(pdf), "sweepStability": sweep_stability.verify(*scans, sweep["contentText"])}
    write(bundle / "META.json", meta)
    write(bundle / "sweep.json", sweep)
    write(bundle / "glyphs.json", {"sourceSha256": sha(pdf), "pages": [{"index": 0, "width": 612, "height": 792, "glyphs": glyphs}]})
    return bundle, manifest, source, font_inputs


def mutate(paths, name, change):
    path = paths[0] / name
    document = json.loads(path.read_text())
    change(document)
    write(path, document)


def metric(result, subject="superscript", name="paintX", case=0):
    return next(c for c in result["cases"][case]["conditions"] if c["subject"] == subject)["metrics"][name]


def test_valid_source_ordered_readings_ignore_effective_size_rise_and_advance_vector(tmp_path):
    paths = evidence(tmp_path, sizes=(17, 20, 21, 24, 26, 36, 43))
    before = {p: p.read_bytes() for p in tmp_path.rglob("*") if p.is_file()}
    result = probe.evaluate(*paths)
    assert result["state"] == OK, result["admission"]
    assert result["denominators"]["expectedConditions"] == 14
    assert result["sourceAnnotation"]["derived"] is True
    assert result["sourceAnnotation"]["backtest"] is True
    assert result["metrics"]["paintX"]["candidates"]["absolute/ratio_0.66"]["ok"] == 14
    assert result["metrics"]["advance"]["candidates"]["ratio_0.66"]["ok"] == 14
    assert result["metrics"]["offset"]["candidates"]["empirical_unquantized"]["ok"] == 14
    assert metric(result, name="advance", case=1)["advanceEquivalentSizePt"] == pytest.approx(6.6)
    assert metric(result, name="offset", case=1)["observedPt"] == pytest.approx([3.4] * 6)
    assert metric(result, "subscript", "offset", case=1)["observedPt"] == pytest.approx([.8] * 6)
    assert all(p.read_bytes() == data for p, data in before.items())


@pytest.mark.parametrize("field", ["preflight", "fontSubstitution", "sweepStability"])
def test_common_gates_retain_full_denominators(tmp_path, field):
    paths = evidence(tmp_path, sizes=(20, 21))
    mutate(paths, "META.json", lambda doc: doc.pop(field))
    result = probe.evaluate(*paths)
    assert result["state"] == UNDECIDABLE
    assert result["denominators"]["undecidableCases"] == 2
    assert all(m["conditions"] == m["undecidable"] == 4 for m in result["metrics"].values())


@pytest.mark.parametrize("corruption", ["fixture", "font_inputs", "pdf", "pdf_meta", "pdf_glyphs", "raw_receipt", "same_length_source", "line_range", "glyph_count", "glyph_order", "font", "identity", "unmapped"])
def test_evidence_corruption_is_not_a_prediction_failure(tmp_path, corruption):
    paths = evidence(tmp_path)
    if corruption == "fixture":
        with zipfile.ZipFile(paths[2], "a") as archive:
            archive.writestr("unrelated.txt", "changed")
    elif corruption == "font_inputs":
        paths[3].write_text(paths[3].read_text() + "\n")
    elif corruption == "pdf":
        (paths[0] / "case.pdf").write_bytes(b"changed")
    elif corruption == "pdf_meta":
        mutate(paths, "META.json", lambda d: d.update(pdfSha256="0" * 64))
    elif corruption == "pdf_glyphs":
        mutate(paths, "glyphs.json", lambda d: d.update(sourceSha256="0" * 64))
    elif corruption == "raw_receipt":
        (paths[0] / "sweep-repeat.raw.txt").write_text("different receipt")
    elif corruption == "same_length_source":
        mutate(paths, "sweep.json", lambda d: d.update(contentText=d["contentText"].replace("A", "Z")))
    elif corruption == "line_range":
        mutate(paths, "sweep.json", lambda d: d["positions"][0].update(line=99))
    elif corruption == "glyph_count":
        mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"].pop())
    elif corruption == "glyph_order":
        mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][0].update(index=1))
    elif corruption == "font":
        mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][7].update(fontName="OtherPS"))
    elif corruption == "identity":
        # NFKC would equate these, but the prereg requires exact visible ASCII identity.
        mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][7].update(text="\u210c"))
    else:
        mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][7].update(textStatus="unmapped"))
    result = probe.evaluate(*paths)
    assert result["state"] == UNDECIDABLE, corruption
    assert result["admission"]["reasons"]
    assert result["metrics"]["paintX"]["undecidable"] == 2


def test_unscaled_and_unshifted_subjects_are_readable_counterevidence(tmp_path):
    result = probe.evaluate(*evidence(tmp_path, paint_ratio=1, advance_ratio=1, rise_ratio=0, drop_ratio=0))
    assert result["state"] == OK
    for subject in ("superscript", "subscript"):
        for name in probe.METRICS:
            actual = metric(result, subject, name)
            assert actual["state"] == OK
            assert actual["matchingCandidates"] == []
            assert all(candidate["state"] == FAIL for candidate in actual["candidates"].values())


def test_two_painted_axes_include_scaling_without_making_scalar_claim(tmp_path):
    paths = evidence(tmp_path, scaling=.8)
    result = probe.evaluate(*paths)
    assert result["state"] == OK
    assert metric(result)["observedPt"] == pytest.approx([6.6] * 6)
    assert metric(result, name="paintY")["observedPt"] == pytest.approx([6.6] * 6)
    assert result["cases"][0]["conditions"][0]["paintScalarApplicable"] is False
    def anisotropic(doc):
        for glyph in doc["pages"][0]["glyphs"][14:20]:
            glyph["matrix"][0] *= 7 / 6.6
            glyph["matrix"][3] *= 6.5 / 6.6
    mutate(paths, "glyphs.json", anisotropic)
    result = probe.evaluate(*paths)
    assert metric(result)["candidates"]["absolute/OS2_XY"]["state"] == OK
    assert metric(result, name="paintY")["candidates"]["absolute/OS2_XY"]["state"] == OK
    assert metric(result)["candidates"]["absolute/OS2_Y_isotropic"]["state"] == FAIL


def test_anchor_disagreement_only_blocks_offset(tmp_path):
    paths = evidence(tmp_path)
    mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][13]["glyphOrigin"].__setitem__(1, 50.01))
    result = probe.evaluate(*paths)
    assert result["state"] == OK
    assert metric(result, name="offset")["state"] == UNDECIDABLE
    assert metric(result, name="paintX")["state"] == OK
    assert metric(result, name="advance")["state"] == OK
    assert result["metrics"]["offset"]["undecidable"] == 2


@pytest.mark.parametrize("change", [lambda g: g.update(geometryStatus="ROTATED"),
                                   lambda g: g["matrix"].__setitem__(1, .01),
                                   lambda g: g["matrix"].__setitem__(0, -1),
                                   lambda g: g.update(fontSize=0),
                                   lambda g: g.update(scaling=float("nan"))])
def test_unreadable_subject_geometry_keeps_other_subject_readable(tmp_path, change):
    paths = evidence(tmp_path)
    mutate(paths, "glyphs.json", lambda d: change(d["pages"][0]["glyphs"][14]))
    result = probe.evaluate(*paths)
    assert result["state"] == OK
    assert metric(result, name="paintX")["state"] == UNDECIDABLE
    assert metric(result, "subscript", "paintX")["state"] == OK
    json.dumps(result, allow_nan=False)


@pytest.mark.parametrize("span", [0, -1])
def test_zero_or_negative_subject_advance_is_readable_counterevidence(tmp_path, span):
    paths = evidence(tmp_path)
    def collapse(doc):
        glyphs = doc["pages"][0]["glyphs"]
        glyphs[20]["glyphOrigin"][0] = glyphs[14]["glyphOrigin"][0] + span
    mutate(paths, "glyphs.json", collapse)
    result = probe.evaluate(*paths)
    actual = metric(result, name="advance")
    assert actual["state"] == OK
    assert actual["observedPt"] == [span]
    assert all(candidate["state"] == FAIL for candidate in actual["candidates"].values())


def test_invalid_font_size_does_not_discard_readable_origin_quantities(tmp_path):
    paths = evidence(tmp_path)
    mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][14].update(fontSize=0))
    result = probe.evaluate(*paths)
    assert metric(result)["state"] == UNDECIDABLE
    assert metric(result, name="advance")["state"] == OK
    assert metric(result, name="offset")["state"] == OK


def test_invalid_origin_does_not_discard_readable_painted_dimensions(tmp_path):
    paths = evidence(tmp_path)
    mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][14]["glyphOrigin"].__setitem__(0, float("inf")))
    result = probe.evaluate(*paths)
    assert metric(result)["state"] == OK
    assert metric(result, name="advance")["state"] == UNDECIDABLE
    json.dumps(result, allow_nan=False)


def test_grid_half_ties_are_exact_and_candidate_coincidence_is_not_discrimination(tmp_path):
    assert probe.quantize(Fraction(66, 10)) == Fraction(672, 100)
    assert probe.quantize(Fraction(84, 100)) == Fraction(96, 100)
    assert probe.quantize(Fraction(612, 100)) == Fraction(624, 100)
    assert probe.quantize(Fraction(-84, 100)) == Fraction(-96, 100)
    paths = evidence(tmp_path, sizes=(20, 21, 24, 36))
    result = probe.evaluate(*paths)
    assert metric(result)["candidates"]["absolute/ratio_0.66_grid_0.24_nearest"]["expectedPt"] == [6.72] * 6
    assert metric(result, "subscript", "offset", 1)["candidates"]["magnitude_grid_0.24_nearest"]["expectedPt"] == [.96] * 6
    assert metric(result, name="offset", case=3)["candidates"]["magnitude_grid_0.24_nearest"]["expectedPt"] == [6.24] * 6
    pair = result["metrics"]["paintX"]["candidatePairs"]["absolute/ratio_0.66 vs absolute/ratio_0.66_grid_0.24_nearest"]
    assert pair["conditions"] == 8
    assert pair["coincidentPredictions"] == 2
    assert pair["distinguishablePredictions"] == 6


def test_pt_residual_tolerance_is_not_applied_to_dimensionless_ratio(tmp_path):
    paths = evidence(tmp_path)
    # L0 = 36 pt, so a tiny ratio difference is still a 2e-6 pt mismatch.
    mutate(paths, "glyphs.json", lambda d: d["pages"][0]["glyphs"][20]["glyphOrigin"].__setitem__(0, d["pages"][0]["glyphs"][20]["glyphOrigin"][0] + 2e-6))
    result = probe.evaluate(*paths)
    actual = metric(result, name="advance")
    assert abs(actual["ratio"] - .66) < probe.EPS_PT
    assert actual["candidates"]["ratio_0.66"]["state"] == FAIL


def test_cli_preserves_prediction_failures_as_valid_results(tmp_path, capsys):
    paths = evidence(tmp_path, paint_ratio=1)
    args = [str(paths[0]), "--probes", str(paths[1]), "--source-docx", str(paths[2]), "--font-inputs", str(paths[3])]
    assert probe.main(args) == 0
    assert json.loads(capsys.readouterr().out)["state"] == OK
    (paths[0] / "case.pdf").unlink()
    assert probe.main(args) == 2
    assert json.loads(capsys.readouterr().out)["state"] == UNDECIDABLE


def test_frozen_manifest_and_fonts_validate_without_reading_word_capture():
    root = Path(__file__).resolve().parents[3]
    manifest, _ = probe._json(root / "fixtures/vertical-precision.probes.json")
    inputs, inputs_sha = probe._json(root / "fixtures/vertical-precision.font-inputs.json")
    probe._validate_manifest(manifest, sha((root / "fixtures/vertical-precision.docx").read_bytes()), inputs_sha, probe._fonts(inputs))
    assert len(manifest["cases"]) == 21
    assert sum(len(case["text"]) for case in manifest["cases"]) == 609

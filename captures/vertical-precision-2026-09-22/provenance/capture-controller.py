"""Controlled one-shot capture; freeze this file's hash before running."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "tools/measure"))
from wordmeasure import capture
from wordmeasure.applescript import literal, tell_word

PID = "39057"
LOCK = Path("/tmp/rswordlayout-word-measurement.lock")
DOCX = OUT / "vertical-precision-capture-20260922.docx"
BUNDLE = OUT / "capture-01"
observations = {"schema": "rsword-controlled-local-session/1", "events": []}


def now():
    return datetime.now(timezone.utc).isoformat()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def process():
    pid = subprocess.check_output(["pgrep", "-x", "Microsoft Word"], text=True).strip()
    if pid != PID:
        raise RuntimeError("Word PID changed: " + pid)
    return subprocess.check_output(["ps", "-p", PID, "-o", "pid=,uid=,lstart=,command="], text=True).strip()


def checked(body, timeout=60):
    before = process()
    if before != observations["processBefore"]:
        raise RuntimeError("Word process identity changed")
    number = len(observations["events"])
    prefix = OUT / f"event-{number:02d}"
    prefix.with_suffix(".applescript").write_text(body)
    entry = {"script": prefix.with_suffix(".applescript").name, "startedUtc": now()}
    observations["events"].append(entry)
    try:
        result = tell_word(body, timeout=min(timeout, 90))
        prefix.with_suffix(".txt").write_text(result)
        entry["receipt"] = prefix.with_suffix(".txt").name
        entry["receiptSha256"] = digest(prefix.with_suffix(".txt"))
        return result
    except Exception as error:
        entry["error"] = repr(error)
        raise
    finally:
        entry["finishedUtc"] = now()
        entry["processAfter"] = process()
        if entry["processAfter"] != before:
            raise RuntimeError("Word process changed during call")


def inventory():
    return checked('return (version as text) & "|documents=" & (count of documents as text)')


def open_owned(path, timeout=60):
    if Path(path).resolve() != DOCX.resolve():
        raise RuntimeError("Unexpected document path")
    name = original_open(path, timeout=60)
    identity = checked('set d to document ' + literal(name) + '\nreturn (POSIX path of ((full name of d) as alias)) & "|saved=" & (saved of d as text) & "|count=" & (count of documents as text)')
    observations["openedDocument"] = identity
    if identity.strip() != str(DOCX) + "|saved=true|count=1":
        raise RuntimeError("Opened document identity mismatch: " + identity)
    return name


LOCK.mkdir()
owner = {"root": str(ROOT), "purpose": "vertical precision preregistered one-shot",
         "pid": PID, "startedUtc": now(), "controllerSha256": digest(Path(__file__))}
(LOCK / "owner.json").write_text(json.dumps(owner, indent=2) + "\n")
(OUT / "coordination-owner.json").write_text(json.dumps(owner, indent=2) + "\n")
try:
    observations["startedUtc"] = now()
    observations["processBefore"] = process()
    observations["inventoryBefore"] = inventory()
    if observations["inventoryBefore"].strip() != "16.112.3|documents=0":
        raise RuntimeError("Unexpected Word version or preexisting document")
    if DOCX.exists() or BUNDLE.exists():
        raise RuntimeError("Capture destination exists; no overwrite or retry")
    inputs = json.loads((ROOT / "fixtures/vertical-precision.font-inputs.json").read_text())
    for font in inputs["fonts"]:
        if digest(Path(font["path"])) != font["sha256"]:
            raise RuntimeError("Font input changed: " + font["family"])
    shutil.copy2(ROOT / "fixtures/vertical-precision.docx", DOCX)
    capture.tell_word = checked
    original_open = capture.open_document
    capture.open_document = open_owned
    meta = capture.capture(DOCX, BUNDLE,
        required_families=[font["family"] for font in inputs["fonts"]],
        font_files=[Path(font["path"]) for font in inputs["fonts"]],
        slot=None, work_pdf=None, label="vertical-precision-preregistered-01")
    observations["result"] = {"fixture": meta["fixture"], "pageCount": meta["pageCount"],
                               "sweepStability": meta["sweepStability"],
                               "fontSubstitution": meta["fontSubstitution"]}
    observations["inventoryAfter"] = inventory()
    if observations["inventoryAfter"].strip() != "16.112.3|documents=0":
        raise RuntimeError("Word document cleanup unverified")
    print(json.dumps({"pageCount": meta["pageCount"], "glyphTotal": meta["glyphTotal"],
                      "stability": meta["sweepStability"]["state"],
                      "fontSubstitution": meta["fontSubstitution"]["result"]}), flush=True)
except Exception as error:
    observations["error"] = repr(error)
    raise
finally:
    observations["finishedUtc"] = now()
    (OUT / "controller-observations.json").write_text(json.dumps(observations, indent=2) + "\n")
    if (LOCK / "owner.json").read_text() == json.dumps(owner, indent=2) + "\n":
        (LOCK / "owner.json").unlink()
        LOCK.rmdir()

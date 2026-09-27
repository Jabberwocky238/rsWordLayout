#!/usr/bin/env python3
"""Measure an explicit font through pinned bundled DirectWrite, not Word layout."""
import argparse
import base64
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
import tempfile

FRAMEWORK = Path("/Applications/Microsoft Word.app/Contents/Frameworks/dwrite10.framework/Versions/A/dwrite10")
FRAMEWORK_SHA256 = "0c4f1ce675dc4809018481292f48702f8b700439012871ce5943837518fbaa2d"
SOURCE = Path(__file__).resolve()
FIELDS = list(zip(("designUnitsPerEm ascent descent lineGap capHeight xHeight underlinePosition "
                  "underlineThickness strikethroughPosition strikethroughThickness").split(),
                 "HHHSHHSHSH")) + [(name, "S") for name in (
    "glyphBoxLeft glyphBoxTop glyphBoxRight glyphBoxBottom subscriptPositionX subscriptPositionY "
    "subscriptSizeX subscriptSizeY superscriptPositionX superscriptPositionY superscriptSizeX superscriptSizeY"
).split()] + [("hasTypographicMetrics", "I")]
OPTIONS = dict(factoryType=1, faceIndex=0, simulations=0, pixelsPerDip=1.0, transform=None)


def em_size(value):
    try:
        result = struct.unpack("<f", struct.pack("<f", float(value)))[0]
        if not math.isfinite(result) or result <= 0:
            raise ValueError()
        return result
    except (ValueError, OverflowError, TypeError, struct.error):
        raise argparse.ArgumentTypeError("em size must be positive finite binary32") from None


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def bindings(font):
    result = {}
    for name, path in (("font", font), ("framework", FRAMEWORK), ("tool", SOURCE)):
        result[name] = dict(path=str(path), sha256=None)
        try:
            result[name]["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        except OSError as exc:
            result[name]["error"] = str(exc)
    return result


def preflight(bound):
    if any(value["sha256"] is None for value in bound.values()):
        raise ValueError("could not hash every input; see input binding errors")
    if (platform.system(), platform.machine()) != ("Darwin", "arm64"):
        raise ValueError("requires Darwin arm64")
    if bound["framework"]["sha256"] != FRAMEWORK_SHA256:
        raise ValueError("bundled dwrite10 whole-file SHA256 mismatch")


def worker(font, sizes):
    # ctypes and the native image are confined to this disposable subprocess.
    import ctypes as C
    import resource
    import uuid
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    P, I, U, H = C.c_void_p, C.c_int32, C.c_uint32, C.c_uint16

    class GUID(C.Structure):
        _fields_ = [("a", U), ("b", H), ("c", H), ("d", C.c_ubyte * 8)]

    class Metrics(C.Structure):
        _fields_ = [(name, {"H": H, "S": C.c_int16, "I": I}[kind]) for name, kind in FIELDS]

    def emit(stage, **values):
        print(json.dumps(dict(stage=stage, **values), allow_nan=False), flush=True)

    def method(obj, slot, result, *args):
        table = C.cast(obj, C.POINTER(C.POINTER(P))).contents
        return C.CFUNCTYPE(result, P, *args)(table[slot])

    def call(stage, fn, *args, hresult=True):
        emit("before" + stage)
        result = fn(*args)
        emit(stage, **({"hresult": result} if hresult else {}))
        if hresult and result < 0:
            raise RuntimeError(f"{stage}: HRESULT 0x{result & 0xffffffff:08x}")

    def pointer(obj):
        if not obj:
            raise RuntimeError("successful native call returned a null interface")

    def metrics(value):
        return {name: getattr(value, name) for name, _ in FIELDS}

    bound = bindings(font)
    preflight(bound)
    if C.sizeof(GUID) != 16 or C.sizeof(Metrics) != 48:
        raise RuntimeError("unsupported GUID/METRICS1 ABI")
    emit("inputs", bindings=bound, emSizes=sizes, **OPTIONS)
    emit("beforeLoadLibrary")
    library = C.CDLL(str(FRAMEWORK))
    emit("LoadLibrary")
    create = library.DWriteCreateFactory
    create.argtypes, create.restype = [I, C.POINTER(GUID), C.POINTER(P)], I
    factory_iid = GUID.from_buffer_copy(uuid.UUID("b859ee5a-d838-4b5b-a2e8-1adc7d93db48").bytes_le)
    face_iid = GUID.from_buffer_copy(uuid.UUID("a71efdb4-9fdb-4838-ad90-cfc3be8c3daf").bytes_le)
    factory, file_ref, face, face1 = P(), P(), P(), P()
    try:
        call("CreateFactory", create, 1, C.byref(factory_iid), C.byref(factory))
        pointer(factory)
        raw = str(font).encode("utf-16-le") + b"\0\0"
        path = (H * (len(raw) // 2)).from_buffer_copy(raw)
        call("CreateFontFileReference", method(factory, 7, I, C.POINTER(H), P, C.POINTER(P)),
             factory, path, None, C.byref(file_ref))
        pointer(file_ref)
        supported, file_type, face_type, count = I(), I(), I(), U()
        call("Analyze", method(file_ref, 5, I, C.POINTER(I), C.POINTER(I), C.POINTER(I), C.POINTER(U)),
             file_ref, C.byref(supported), C.byref(file_type), C.byref(face_type), C.byref(count))
        emit("analysis", supported=supported.value, fileType=file_type.value, faceType=face_type.value, count=count.value)
        if not supported.value or count.value == 0:
            raise RuntimeError("font Analyze reports no supported face")
        files = (P * 1)(file_ref.value)
        call("CreateFontFace", method(factory, 9, I, I, U, C.POINTER(P), U, I, C.POINTER(P)),
             factory, face_type, 1, files, 0, 0, C.byref(face))
        pointer(face)
        call("QueryInterfaceFace1", method(face, 0, I, C.POINTER(GUID), C.POINTER(P)),
             face, C.byref(face_iid), C.byref(face1))
        pointer(face1)
        output = Metrics()
        call("GetMetrics1", method(face1, 18, None, C.POINTER(Metrics)), face1, C.byref(output), hresult=False)
        emit("metrics", metrics=metrics(output))
        for size in sizes:
            output = Metrics()
            call("GetGdiCompatibleMetrics1", method(face1, 19, I, C.c_float, C.c_float, P, C.POINTER(Metrics)),
                 face1, size, 1.0, None, C.byref(output))
            emit("gdiMetrics", emSize=size, metrics=metrics(output))
    finally:
        for name, obj in (("face1", face1), ("face", face), ("file", file_ref), ("factory", factory)):
            if obj:
                emit("beforeRelease", interface=name)
                emit("Release", interface=name, remaining=method(obj, 2, U)(obj))
    emit("complete", status="MEASURED")


def protocol(raw, bound, sizes):
    def pairs(items):
        result = dict(items)
        if len(result) != len(items):
            raise ValueError("duplicate JSON key")
        return result

    def invalid_constant(value):
        raise ValueError("non-finite JSON constant: " + value)

    def finite_float(value):
        result = float(value)
        if not math.isfinite(result):
            raise ValueError("non-finite JSON number: " + value)
        return result

    records = [json.loads(line, object_pairs_hook=pairs, parse_constant=invalid_constant, parse_float=finite_float)
               for line in raw.decode("utf-8").splitlines()]
    stages = ["inputs", "beforeLoadLibrary", "LoadLibrary"]
    for name in ("CreateFactory", "CreateFontFileReference", "Analyze", "analysis", "CreateFontFace",
                 "QueryInterfaceFace1", "GetMetrics1", "metrics"):
        stages += [name] if name in ("analysis", "metrics") else ["before" + name, name]
    stages += ["beforeGetGdiCompatibleMetrics1", "GetGdiCompatibleMetrics1", "gdiMetrics"] * len(sizes)
    stages += ["beforeRelease", "Release"] * 4 + ["complete"]
    if not raw.endswith(b"\n") or not all(isinstance(r, dict) for r in records) or [r.get("stage") for r in records] != stages:
        raise ValueError("incomplete or unexpected worker stage sequence")
    if json.dumps(records[0], sort_keys=True) != json.dumps(dict(stage="inputs", bindings=bound, emSizes=sizes, **OPTIONS), sort_keys=True):
        raise ValueError("worker input binding mismatch")
    if records[-1] != dict(stage="complete", status="MEASURED"):
        raise ValueError("missing successful completion")
    hrs = {"CreateFactory", "CreateFontFileReference", "Analyze", "CreateFontFace", "QueryInterfaceFace1", "GetGdiCompatibleMetrics1"}
    for row in records:
        if row["stage"] in hrs and (type(row.get("hresult")) is not int or not 0 <= row["hresult"] < 2**31):
            raise ValueError("invalid or failed signed32 HRESULT")
        if row["stage"] in ("metrics", "gdiMetrics"):
            value = row.get("metrics")
            if not isinstance(value, dict) or set(value) != {n for n, _ in FIELDS}:
                raise ValueError("invalid METRICS1 fields")
            for name, kind in FIELDS:
                lo, hi = {"H": (0, 65535), "S": (-32768, 32767), "I": (-2**31, 2**31 - 1)}[kind]
                if type(value[name]) is not int or not lo <= value[name] <= hi:
                    raise ValueError("invalid METRICS1 component: " + name)
            if not value["designUnitsPerEm"]:
                raise ValueError("zero design units per em")
    analysis = next(r for r in records if r["stage"] == "analysis")
    if any(type(analysis.get(k)) is not int or not -2**31 <= analysis[k] < 2**31 for k in ("supported", "fileType", "faceType")) or not analysis["supported"] or type(analysis.get("count")) is not int or not 0 < analysis["count"] < 2**32:
        raise ValueError("invalid font analysis")
    measured_sizes = [r.get("emSize") for r in records if r["stage"] == "gdiMetrics"]
    if any(type(size) is not float for size in measured_sizes) or measured_sizes != sizes:
        raise ValueError("em size sequence mismatch")
    releases = [r for r in records if r["stage"] in ("beforeRelease", "Release")]
    if [r.get("interface") for r in releases] != [n for n in ("face1", "face", "file", "factory") for _ in range(2)]:
        raise ValueError("release sequence mismatch")
    if any(type(r.get("remaining")) is not int or not 0 <= r["remaining"] < 2**32 for r in releases[1::2]):
        raise ValueError("invalid Release return value")
    return records


def capture(font, sizes, timeout=45):
    report = dict(schema="rsword-dwrite-metrics/1", status="FAILED", startedUtc=utc(),
                  platform=dict(system=platform.system(), machine=platform.machine(), release=platform.release()),
                  scope="Native font API measurement; not Word layout or a Word font-size mapping",
                  emSizes=sizes, **OPTIONS, errors=[], exitCode=None, timedOut=False)
    stdout, stderr = b"", b""
    try:
        report["inputsBefore"] = bindings(font)
        preflight(report["inputsBefore"])
        env = {k: v for k, v in os.environ.items() if not k.startswith("DYLD_")}
        env["DYLD_FRAMEWORK_PATH"] = str(FRAMEWORK.parents[3])
        report["dyldFrameworkPath"] = env["DYLD_FRAMEWORK_PATH"]
        command = [sys.executable, "-u", str(SOURCE), "--worker", "--font", str(font)]
        command += [arg for size in sizes for arg in ("--em-size", repr(size))]
        report["command"], report["timeoutSeconds"] = command, timeout
        try:
            result = subprocess.run(command, env=env, capture_output=True, timeout=timeout)
            stdout, stderr, report["exitCode"] = result.stdout, result.stderr, result.returncode
        except subprocess.TimeoutExpired as exc:
            stdout, stderr, report["timedOut"] = exc.stdout or b"", exc.stderr or b"", True
            raise ValueError("worker timed out; subprocess killed and reaped") from None
        if result.returncode != 0:
            raise ValueError(f"worker failed with exit code {result.returncode}")
        report["records"] = protocol(stdout, report["inputsBefore"], sizes)
    except (OSError, ValueError, RecursionError) as exc:
        report["errors"].append(str(exc))
    try:
        report["inputsAfter"] = bindings(font)
        if report.get("inputsBefore") != report["inputsAfter"]:
            report["errors"].append("input or tool hashes changed during measurement")
    except OSError as exc:
        report["errors"].append("post-measurement binding failed: " + str(exc))
    for name, data in (("stdout", stdout), ("stderr", stderr)):
        report[name], report[name + "Base64"] = data.decode("utf-8", errors="replace"), base64.b64encode(data).decode("ascii")
    report["finishedUtc"] = utc()
    if not report["errors"]:
        report["status"] = "MEASURED"
    return report


def publish(path, report):
    data = (json.dumps(report, indent=2, allow_nan=False) + "\n").encode("utf-8")
    temp = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".dwrite-", delete=False) as file:
            temp = Path(file.name)
            file.write(data)
            file.flush()
            os.fsync(file.fileno())
        os.link(temp, path)  # Atomic publication; also refuses dangling symlinks.
    finally:
        if temp is not None:
            temp.unlink()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font", type=Path, required=True)
    parser.add_argument("--em-size", action="append", type=em_size, required=True)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    font = args.font.expanduser().resolve()
    if args.worker:
        worker(font, args.em_size)
        return 0
    if args.out is None:
        parser.error("--out is required")
    if os.path.lexists(args.out):
        parser.error("--out must be a new file, including no existing symlink")
    report = capture(font, args.em_size)
    try:
        publish(args.out, report)
    except (OSError, ValueError) as exc:
        parser.error(str(exc))
    print(f"{report['status']}: {args.out}")
    return 0 if report["status"] == "MEASURED" else 1


if __name__ == "__main__":
    sys.exit(main())

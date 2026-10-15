#!/usr/bin/env python3
"""Build, independently verify, and compare opt-in interned string storage."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "target/interned-strings"
IMPLEMENTATIONS = ["default", "interned", "gd"]
OPERATIONS = ["insert", "copy", "rotate", "replace"]

# Reuse the existing process-CPU-delta load guard, including its idle wait and
# confirmation sampling. Keep this dependency in the recorded source hashes.
spec = importlib.util.spec_from_file_location("text_workflow_compare", ROOT / "benches/text_workflow/compare.py")
load = importlib.util.module_from_spec(spec)
spec.loader.exec_module(load)


def run(command, **kwargs):
    return subprocess.run([str(arg) for arg in command], check=True, text=True, cwd=ROOT, **kwargs)


def text(command):
    return run(command, capture_output=True).stdout.strip()


def build():
    BUILD.mkdir(parents=True, exist_ok=True)
    compat = BUILD / "include/gd"
    compat.mkdir(parents=True, exist_ok=True)
    (compat / "gd_table_column-buffer.h").write_text("#pragma once\n#include <gd_table_column-buffer.h>\n")
    gd = ROOT / "external/gd/source"
    sources = [gd / name for name in [
        "gd_arguments.cpp", "gd_arguments_shared.cpp", "gd_parse.cpp", "gd_table.cpp",
        "gd_table_column.cpp", "gd_table_column-buffer.cpp", "gd_table_table.cpp",
        "gd_types.cpp", "gd_utf8.cpp", "gd_utf8_2.cpp", "gd_variant.cpp", "gd_variant_view.cpp",
    ]]
    commands = []
    with (BUILD / "build.log").open("w") as log:
        env = os.environ.copy()
        env.pop("CARGO_ENCODED_RUSTFLAGS", None)
        env["RUSTFLAGS"] = "-C target-cpu=native"
        command = ["cargo", "build", "--release", "--locked", "--no-default-features",
                   "--example", "interned_strings", "--example", "interned_strings_memory"]
        commands.append(command)
        run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        for memory in [False, True]:
            command = [os.environ.get("CXX", "c++"), "-std=c++20", "-O3", "-DNDEBUG",
                       "-march=native", "-flto=thin", "-Wno-deprecated-enum-enum-conversion",
                       "-I", gd, "-I", BUILD / "include"]
            if memory:
                command.append("-DGD_BENCH_MEMORY")
            command += [ROOT / "benches/cpp-reference/interned_strings.cpp", *sources,
                        "-o", BUILD / ("gd_memory" if memory else "gd")]
            commands.append([str(arg) for arg in command])
            run(command, stdout=log, stderr=subprocess.STDOUT)
    (BUILD / "build-commands.json").write_text(json.dumps(commands, indent=2) + "\n")
    return commands


def executable(implementation, memory=False):
    if implementation == "gd":
        return BUILD / ("gd_memory" if memory else "gd")
    return ROOT / "target/release/examples" / ("interned_strings_memory" if memory else "interned_strings")


def invoke(implementation, mode, operation, rows, unique, length, samples):
    command = [executable(implementation, mode == "memory"), implementation, mode, operation,
               rows, unique, length, samples]
    result = json.loads(run(command, capture_output=True, timeout=300).stdout)
    return result


def oracle(rows, unique, length, operation):
    digest = 0xcbf29ce484222325
    for row in range(rows):
        index = row if operation == "replace" else (row + (operation == "rotate")) % unique
        value = f'{"b" if operation == "replace" else "a"}{index:08}'
        for byte in (value + "x" * (length - len(value))).encode() + b"\0":
            digest = ((digest ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return digest


def verify(result, expected, source):
    if result["digest"] != expected or result["source_digest"] != source:
        raise RuntimeError(f"Checksum mismatch: {result}, expected {expected}, source {source}")


def summarize(report):
    summary = []
    keys = sorted({(m["rows"], m["unique"], m["length"], m["operation"], m["implementation"])
                   for m in report["measurements"]})
    for rows, unique, length, operation, implementation in keys:
        rounds = [statistics.median(m["nanos"]) / 1e6 for m in report["measurements"]
                  if (m["rows"], m["unique"], m["length"], m["operation"], m["implementation"])
                  == (rows, unique, length, operation, implementation)]
        memory = next(m for m in report["memory"] if
                      (m["rows"], m["unique"], m["length"], m["operation"], m["implementation"])
                      == (rows, unique, length, operation, implementation))
        summary.append(dict(rows=rows, unique=unique, length=length, operation=operation,
                            implementation=implementation, median_ms=statistics.median(rounds),
                            min_round_ms=min(rounds), max_round_ms=max(rounds),
                            source_bytes=memory["source_bytes"], result_bytes=memory["result_bytes"]))
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rows", type=int, default=20_000)
    parser.add_argument("--unique", type=int, nargs="+", default=[1, 16, 256, 4096, 20_000])
    parser.add_argument("--lengths", type=int, nargs="+", default=[16, 128])
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--output", type=Path, default=BUILD / "results.json")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--wait-for-idle", action="store_true")
    args = parser.parse_args()
    if args.rows < 1 or args.rows > 99_999_999 or not all(0 < n <= args.rows for n in args.unique):
        parser.error("require 0 < unique <= rows <= 99,999,999")
    if min(args.lengths) < 9 or min(args.samples, args.rounds) < 1:
        parser.error("lengths must be >= 9, samples and rounds >= 1")
    commands = (json.loads((BUILD / "build-commands.json").read_text())
                if args.skip_build else build())
    paths = [*sorted((ROOT / "benches/interned_strings").glob("*.rs")),
             Path(__file__), ROOT / "benches/text_workflow/compare.py",
             ROOT / "benches/cpp-reference/interned_strings.cpp",
             ROOT / "benches/cpp-reference/interned_strings_memory.h",
             ROOT / "Cargo.toml", ROOT / "Cargo.lock", *sorted((ROOT / "src").rglob("*.rs"))]
    hashes = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    binaries = {f"{impl}_{memory}": str(executable(impl, memory))
                for impl in ["default", "gd"] for memory in [False, True]}
    binary_hashes = {name: hashlib.sha256(Path(path).read_bytes()).hexdigest() for name, path in binaries.items()}
    report = {
        "metadata": {
            "utc": datetime.now(timezone.utc).isoformat(), "platform": platform.platform(),
            "cpu": text(["sysctl", "-n", "machdep.cpu.brand_string"]) if sys.platform == "darwin" else platform.processor(),
            "logical_cpus": os.cpu_count(), "rustc": text(["rustc", "-Vv"]), "cxx": text([os.environ.get("CXX", "c++"), "--version"]),
            "revision": text(["git", "rev-parse", "HEAD"]),
            "gd_revision": text(["git", "-C", "external/gd", "rev-parse", "HEAD"]),
            "source_sha256": hashes, "binary_sha256": binary_hashes,
            "gd_source_sha256": load.fingerprint(ROOT / "external/gd/source"),
            "build_commands": commands, "invocation": sys.argv,
            "rust_flags": "release opt-level=3; lto=thin; codegen-units=1; target-cpu=native; no features; --locked",
            "cpp_flags": "C++20 -O3 -DNDEBUG -march=native -flto=thin; no sanitizers",
            "workers": 1, "affinity": "OS scheduling; no affinity; one process at a time",
            "samples": args.samples, "rounds": args.rounds,
            "memory": "separate binaries count retained malloc block sizes for System/global new; size-class rounding included, allocator metadata/fragmentation, stack, code and fixture catalogues excluded",
            "measurement_complete": False,
        }, "verification": [], "memory": [], "measurements": [],
        "load_samples": [], "timing_load_samples": [], "discarded_rounds": [],
    }
    if sys.platform == "darwin":
        report["metadata"].update(ram_bytes=int(text(["sysctl", "-n", "hw.memsize"])),
                                  topology=text(["sysctl", "hw.model", "hw.physicalcpu", "hw.logicalcpu"]),
                                  thermal_before=text(["pmset", "-g", "therm"]))
    args.output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        args.output.write_text(json.dumps(report, indent=2) + "\n")

    try:
        for length in args.lengths:
            for unique in args.unique:
                print(f"{args.rows:,} rows, {unique:,} distinct, {length} bytes", flush=True)
                expected_source = oracle(args.rows, unique, length, "insert")
                expected = {op: oracle(args.rows, unique, length, op) for op in OPERATIONS}
                for op in OPERATIONS:
                    for impl in IMPLEMENTATIONS:
                        key = dict(rows=args.rows, unique=unique, length=length, operation=op, implementation=impl)
                        for mode in ["verify", "memory"]:
                            result = invoke(impl, mode, op, args.rows, unique, length, 1)
                            verify(result, expected[op], expected_source)
                            report["memory" if mode == "memory" else "verification"].append({**key, **result})
                if args.verify_only:
                    save()
                    continue
                for round_index in range(args.rounds):
                    while True:
                        before = load.wait_for_idle(report, False, args.wait_for_idle)
                        measurements = []
                        order = IMPLEMENTATIONS[round_index % 3:] + IMPLEMENTATIONS[:round_index % 3]
                        for op in OPERATIONS:
                            for impl in order:
                                result = invoke(impl, "timing", op, args.rows, unique, length, args.samples)
                                verify(result, expected[op], expected_source)
                                measurements.append(dict(rows=args.rows, unique=unique, length=length,
                                                         operation=op, implementation=impl, round=round_index + 1, **result))
                        after_index = len(report["load_samples"])
                        try:
                            load.load_guard(report, False)
                        except RuntimeError as error:
                            report["discarded_rounds"].append(dict(reason=str(error), measurements=measurements))
                            save()
                            if not args.wait_for_idle:
                                raise
                            continue
                        report["measurements"].extend(measurements)
                        report["timing_load_samples"].extend(before)
                        report["timing_load_samples"].extend(report["load_samples"][after_index:])
                        save()
                        print(f"  round {round_index + 1}/{args.rounds} complete", flush=True)
                        break
        report["summary"] = summarize(report)
        report["metadata"]["measurement_complete"] = not args.verify_only
    finally:
        report["metadata"]["source_unchanged"] = all(hashlib.sha256((ROOT / p).read_bytes()).hexdigest() == digest for p, digest in hashes.items())
        report["metadata"]["binaries_unchanged"] = all(hashlib.sha256(Path(binaries[name]).read_bytes()).hexdigest() == digest for name, digest in binary_hashes.items())
        report["metadata"]["gd_source_unchanged"] = load.fingerprint(ROOT / "external/gd/source") == report["metadata"]["gd_source_sha256"]
        if sys.platform == "darwin":
            report["metadata"]["thermal_after"] = text(["pmset", "-g", "therm"])
        save()
    if not all(report["metadata"][key] for key in ["source_unchanged", "binaries_unchanged", "gd_source_unchanged"]):
        raise RuntimeError("Source or binaries changed during measurement")
    print(f"Wrote {args.output}", flush=True)


if __name__ == "__main__":
    main()

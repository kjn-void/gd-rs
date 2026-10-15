#!/usr/bin/env python3
"""Build and run the matching C++ GD and gd-rs projection examples."""

import argparse
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[2]
GD = ROOT / "external/gd/source"
BUILD = ROOT / "target/projection-example"


def cpp():
    BUILD.mkdir(parents=True, exist_ok=True)
    compat = BUILD / "include/gd"
    compat.mkdir(parents=True, exist_ok=True)
    (compat / "gd_table_column-buffer.h").write_text(
        "#pragma once\n#include <gd_table_column-buffer.h>\n", encoding="utf-8"
    )
    sources = [
        "gd_arguments.cpp",
        "gd_arguments_shared.cpp",
        "gd_parse.cpp",
        "gd_table.cpp",
        "gd_table_column.cpp",
        "gd_table_column-buffer.cpp",
        "gd_table_table.cpp",
        "gd_types.cpp",
        "gd_utf8.cpp",
        "gd_utf8_2.cpp",
        "gd_variant.cpp",
        "gd_variant_view.cpp",
    ]
    executable = BUILD / "gd_flyweight_projection"
    command = [
        os.environ.get("CXX", "c++"),
        "-std=c++20",
        "-O1",
        "-g",
        "-Wno-deprecated-enum-enum-conversion",
        "-I", str(GD),
        "-I", str(BUILD / "include"),
        str(ROOT / "benches/cpp-reference/flyweight_projection.cpp"),
        *[str(GD / source) for source in sources],
        "-o", str(executable),
    ]
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([str(executable)], cwd=ROOT, check=True)

    # A rejected native constructor must not abort the working example process.
    # Put any platform core dump in the ignored build directory, too.
    probe = subprocess.run(
        [str(executable), "--native-subset-constructor"],
        cwd=BUILD,
        capture_output=True,
        text=True,
    )
    expected = "get_column_count() == tableFrom.get_column_count()"
    if probe.returncode == 0 or expected not in probe.stderr:
        raise RuntimeError("Unexpected native subset-constructor result: " + probe.stderr)
    print("GD native: source-table constructor rejects a subset of column definitions")


def rust():
    subprocess.run(
        ["cargo", "run", "--locked", "--no-default-features", "--example", "flyweight_projection"],
        cwd=ROOT,
        check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--implementation", choices=["both", "cpp", "rust"], default="both")
    args = parser.parse_args()
    if args.implementation in ("both", "cpp"):
        cpp()
    if args.implementation in ("both", "rust"):
        rust()


if __name__ == "__main__":
    main()

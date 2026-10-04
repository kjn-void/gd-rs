# Source size and complexity

This is a snapshot of the current `gd-rs` worktree and the `external/gd` submodule
baseline measured on 2026-10-04. It measures source shape, not
implementation quality or feature parity. In particular, the full C++ tree still
contains systems that this crate does not port, including ODBC, logging, console,
filesystem, and COM-style routing. The C++ inclusive scopes add characterization tests
and maintained benchmarks when present; the pinned submodule revision contains no
`external/gd/tests` directory, so its test-inclusive scope equals the product scope.

## Results

SLOC below means Lizard's non-comment source lines (`NLOC`): blank and comment-only
lines are excluded. Average cyclomatic complexity is the sum of per-function CCN
divided by the number of functions recognized by Lizard.

| Tree | Files | SLOC | Functions | Total CCN | Average CCN |
|---|---:|---:|---:|---:|---:|
| Rust product (`src`) | 24 | 6,463 | 215 | 563 | 2.62 |
| Rust product + tests (`src`, `tests`) | 33 | 9,112 | 326 | 765 | 2.35 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 51 | 12,328 | 451 | 1,156 | 2.56 |
| C++ product (`source`) | 140 | 64,587 | 8,638 | 19,875 | 2.30 |
| C++ product + tests (`source`, `tests`; `tests` absent) | 140 | 64,587 | 8,638 | 19,875 | 2.30 |
| C++ product + tests + maintained benchmarks | 160 | 67,471 | 8,804 | 20,520 | 2.33 |

The requested Rust totals are therefore **6,463 SLOC without test/benchmark code**
and **12,328 SLOC with both**. Tests account for 2,649 SLOC and benchmarks for 3,216
SLOC. In the C++ scopes, the pinned baseline has no test directory and the maintained
benchmark references add 2,884 SLOC.

These totals should not be read as a claim that Rust needs 10.0% of the code for an
identical product. The Rust crate implements a deliberately smaller surface, while
the C++ measurement includes unrelated and excluded subsystems. The figures are
useful as repository baselines and for tracking growth, but a subsystem-by-subsystem
comparison is required before attributing a size difference to language or design.

## Method

The measurement uses Lizard 1.17.31 for both languages. The selected files are:

- Rust: `*.rs` below `src`, optionally adding `tests` and `benches`;
- C++: `*.h`, `*.hpp`, `*.c`, `*.cc`, `*.cpp`, and `*.cxx` below the
  `external/gd/source`, optionally adding `external/gd/tests` and the matched references in
  `benches/cpp-reference`;
- excluded from both: documentation, manifests, build scripts, generated build
  output, vendored dependencies, and every directory not named above.

The product-only Rust measurement can be reproduced with:

```sh
python3 -m pip install --target /tmp/gd-code-metrics lizard==1.17.31
find src -type f -name '*.rs' | LC_ALL=C sort > /tmp/gd-rs-files.txt
PYTHONPATH=/tmp/gd-code-metrics python3 -m lizard \
  --languages rust --input_file /tmp/gd-rs-files.txt
```

Add `tests` and `benches` to the `find` roots for the inclusive Rust result. For
C++, run from `gd-rs`, replace the roots with `external/gd/source external/gd/tests
benches/cpp-reference`, select the C/C++ suffixes listed above, and use
`--languages cpp`. The maintained C++ benchmark scope includes matched GD references
and standalone host fixtures; reports identify fixtures that have no Rust counterpart.

Lizard assigns CCN 1 to a straight-line function and adds paths for recognized
branches and loops. Its parsers are language-aware but not compiler front ends.
Macros can hide control flow—especially GoogleTest/Google Benchmark bodies—and
generated or macro-expanded complexity is not represented. Parser recovery can also
change after a purely mechanical file split; the current smaller Rust modules let
Lizard recognize more functions than the previous large files even though this edit
does not add behavior. Consequently, the average is a repeatable static-analysis
indicator, not an exact count of runtime paths. Function count and total CCN are
included so rounding and shifts in the average remain visible.

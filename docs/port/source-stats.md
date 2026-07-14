# Source size and complexity

This is a snapshot of the current `gd-rs` worktree measured on 2026-07-14 and the
sibling `gd` baseline measured on 2026-07-13. It measures source shape, not
implementation quality or feature parity. In particular, the full C++ tree still
contains systems that this crate does not port, including ODBC, logging, console,
filesystem, and COM-style routing. The C++ baseline includes the SQLite
characterization tests and matched benchmark in its inclusive scopes; files below
`../gd/source` remain unchanged.

## Results

SLOC below means Lizard's non-comment source lines (`NLOC`): blank and comment-only
lines are excluded. Average cyclomatic complexity is the sum of per-function CCN
divided by the number of functions recognized by Lizard.

| Tree | Files | SLOC | Functions | Total CCN | Average CCN |
|---|---:|---:|---:|---:|---:|
| Rust product (`src`) | 9 | 3,470 | 97 | 209 | 2.15 |
| Rust product + tests (`src`, `tests`) | 17 | 4,579 | 155 | 291 | 1.88 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 25 | 5,184 | 183 | 384 | 2.10 |
| C++ product (`source`) | 138 | 62,408 | 8,279 | 19,254 | 2.33 |
| C++ product + tests (`source`, `tests`) | 155 | 63,004 | 8,320 | 19,297 | 2.32 |
| C++ product + tests + benchmarks (`source`, `tests`, `benchmarks`) | 163 | 63,731 | 8,366 | 19,413 | 2.32 |

The requested Rust totals are therefore **3,470 SLOC without test/benchmark code**
and **5,184 SLOC with both**. Tests account for 1,109 SLOC and benchmarks for 605
SLOC. In the C++ scopes, tests account for 596 SLOC and benchmarks for a further
727 SLOC.

These totals should not be read as a claim that Rust needs 5.6% of the code for an
identical product. The Rust crate implements a deliberately smaller surface, while
the C++ measurement includes unrelated and excluded subsystems. The figures are
useful as repository baselines and for tracking growth, but a subsystem-by-subsystem
comparison is required before attributing a size difference to language or design.

## Method

The measurement uses Lizard 1.17.31 for both languages. The selected files are:

- Rust: `*.rs` below `src`, optionally adding `tests` and `benches`;
- C++: `*.h`, `*.hpp`, `*.c`, `*.cc`, `*.cpp`, and `*.cxx` below `source`,
  optionally adding `tests` and `benchmarks`;
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
C++, run from `gd-rs`, replace the roots with `../gd/source ../gd/tests
../gd/benchmarks`, select the C/C++ suffixes listed above, and use `--languages cpp`.

Lizard assigns CCN 1 to a straight-line function and adds paths for recognized
branches and loops. Its parsers are language-aware but not compiler front ends.
Macros can hide control flow—especially GoogleTest/Google Benchmark bodies—and
generated or macro-expanded complexity is not represented. Consequently, the
average is a repeatable static-analysis indicator, not an exact count of runtime
paths. Function count and total CCN are included so rounding and shifts in the
average remain visible.

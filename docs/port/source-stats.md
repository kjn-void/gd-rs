# Source size and complexity

This is a snapshot of the current `gd-rs` worktree and sibling `gd` baseline measured
on 2026-07-17. It measures source shape, not
implementation quality or feature parity. In particular, the full C++ tree still
contains systems that this crate does not port, including ODBC, logging, console,
filesystem, and COM-style routing. The C++ inclusive scopes include the current
characterization tests and matched benchmarks.

## Results

SLOC below means Lizard's non-comment source lines (`NLOC`): blank and comment-only
lines are excluded. Average cyclomatic complexity is the sum of per-function CCN
divided by the number of functions recognized by Lizard.

| Tree | Files | SLOC | Functions | Total CCN | Average CCN |
|---|---:|---:|---:|---:|---:|
| Rust product (`src`) | 18 | 4,464 | 160 | 380 | 2.38 |
| Rust product + tests (`src`, `tests`) | 26 | 5,824 | 226 | 484 | 2.14 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 36 | 7,566 | 290 | 692 | 2.39 |
| C++ product (`source`) | 138 | 62,808 | 8,328 | 19,362 | 2.32 |
| C++ product + tests (`source`, `tests`) | 157 | 63,520 | 8,372 | 19,437 | 2.32 |
| C++ product + tests + matched benchmarks | 166 | 64,804 | 8,453 | 19,659 | 2.33 |

The requested Rust totals are therefore **4,464 SLOC without test/benchmark code**
and **7,566 SLOC with both**. Tests account for 1,360 SLOC and benchmarks for 1,742
SLOC. In the C++ scopes, tests account for 712 SLOC and benchmarks for a further
1,284 SLOC.

These totals should not be read as a claim that Rust needs 6.1% of the code for an
identical product. The Rust crate implements a deliberately smaller surface, while
the C++ measurement includes unrelated and excluded subsystems. The figures are
useful as repository baselines and for tracking growth, but a subsystem-by-subsystem
comparison is required before attributing a size difference to language or design.

## Method

The measurement uses Lizard 1.17.31 for both languages. The selected files are:

- Rust: `*.rs` below `src`, optionally adding `tests` and `benches`;
- C++: `*.h`, `*.hpp`, `*.c`, `*.cc`, `*.cpp`, and `*.cxx` below the sibling
  `../gd/source`, optionally adding `../gd/tests` and the matched references in
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
C++, run from `gd-rs`, replace the roots with `../gd/source ../gd/tests
benches/cpp-reference`, select the C/C++ suffixes listed above, and use
`--languages cpp`.

Lizard assigns CCN 1 to a straight-line function and adds paths for recognized
branches and loops. Its parsers are language-aware but not compiler front ends.
Macros can hide control flow—especially GoogleTest/Google Benchmark bodies—and
generated or macro-expanded complexity is not represented. Parser recovery can also
change after a purely mechanical file split; the current smaller Rust modules let
Lizard recognize more functions than the previous large files even though this edit
does not add behavior. Consequently, the average is a repeatable static-analysis
indicator, not an exact count of runtime paths. Function count and total CCN are
included so rounding and shifts in the average remain visible.

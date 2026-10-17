# Source size and complexity

This is a snapshot of the current `gd-rs` worktree and the `external/gd` submodule
baseline measured and revalidated on 2026-10-17. Rust scopes describe the current
worktree; GD is pinned to `cb11cff90d05260a88d59a30c9da421cd4e19c34`. It measures
source shape, not implementation quality or feature parity. In particular, the full C++ tree still
contains systems that this crate does not port, including ODBC, logging, console,
filesystem, and COM-style routing. The C++ inclusive scopes include maintained
benchmarks; the pinned submodule revision contains no `external/gd/tests` directory, so
its test-inclusive scope equals the product scope.

## Results

SLOC below means Lizard's non-comment source lines (`NLOC`): blank and comment-only
lines are excluded. Average cyclomatic complexity is the sum of per-function CCN
divided by the number of functions recognized by Lizard.

| Tree | Files | SLOC | Functions | Total CCN | Average CCN |
|---|---:|---:|---:|---:|---:|
| Rust product (`src`) | 34 | 9,543 | 338 | 859 | 2.54 |
| Rust product + tests (`src`, `tests`) | 52 | 15,227 | 540 | 1,245 | 2.31 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 80 | 20,275 | 740 | 1,891 | 2.56 |
| C++ product (`source`) | 140 | 64,587 | 8,638 | 19,875 | 2.30 |
| C++ product + tests (`source`, `tests`; `tests` absent) | 140 | 64,587 | 8,638 | 19,875 | 2.30 |
| C++ product + tests + maintained benchmarks | 167 | 68,466 | 8,892 | 20,798 | 2.34 |

The Rust totals are **9,543 SLOC in `src`** and **20,275 SLOC including `tests`
and `benches`**. The separate test tree adds 5,684 SLOC and benchmarks add 5,048
SLOC, including the lane-split float column sums, their summation-order tests and
benchmark, the interned string timing and allocation benchmark,
interned string storage, entry-handle copying and its integration/ownership tests,
owning immutable selections, shared mutable projections and their
ownership/composition/concurrency tests,
the shared-record API, ownership tests, whole-record workflow and
sharing strategy experiment, the typed fixed-array SoA experiment with per-cell tag/length metadata, and the
Arc core-scaling harness and parallel-cleanup API.
Inline unit tests and the doctest-only guide bridge inside `src` are included in
every Rust scope. The included Markdown text is excluded from source counts. In the C++
scopes, the pinned baseline has no test directory and the maintained benchmark
references add 3,879 SLOC, including the interned string benchmark and allocation
observer, the GD SIMD order-workflow adapter,
the GD/std::string text-workflow cases, whole-record filter-copy benchmark, string
layout probe and projection correctness fixture in this worktree.

These totals do not establish the source size of an identical product. The Rust
crate implements a deliberately smaller surface, while
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

Add `tests` and `benches` to the `find` roots for the inclusive Rust result. For C++,
run from `gd-rs`, replace the roots with `external/gd/source benches/cpp-reference` (add
`external/gd/tests` only when that directory exists), select the C/C++ suffixes listed
above, and use `--languages cpp`. The maintained C++ benchmark scope includes matched GD
references and standalone host fixtures; reports identify fixtures that have no Rust
counterpart.

Lizard assigns CCN 1 to a straight-line function and adds paths for recognized branches
and loops. Its parsers are language-aware but not compiler front ends. Macros can hide
control flow—especially Google Benchmark bodies—and generated or macro-expanded
complexity is not represented. Parser recovery can also change after a purely mechanical
file split without a change in behavior. Smaller modules can let Lizard recognize
functions it missed in larger files. Consequently, the average is a repeatable
static-analysis indicator, not an exact count of runtime paths. Function count and total
CCN are included so rounding and shifts in the average remain visible.

# Source size and complexity

This is a snapshot of the `gd-rs` source tree published with the StringTransport
addition and the `external/gd` submodule baseline, measured on 2026-10-19. Uncommitted
benchmark experiments are excluded from this publication snapshot; GD is pinned to
`1bba194909d997891c05fe5a4ce1d3d3a888e0cf`. It measures
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
| Rust product (`src`) | 35 | 9,710 | 341 | 868 | 2.55 |
| Rust product + tests (`src`, `tests`) | 54 | 15,494 | 548 | 1,263 | 2.30 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 83 | 20,658 | 748 | 1,909 | 2.55 |
| C++ product (`source`) | 142 | 64,864 | 8,683 | 19,943 | 2.30 |
| C++ product + tests (`source`, `tests`; `tests` absent) | 142 | 64,864 | 8,683 | 19,943 | 2.30 |
| C++ product + tests + maintained benchmarks | 170 | 68,815 | 8,942 | 20,879 | 2.33 |

The Rust totals are **9,710 SLOC in `src`** and **20,658 SLOC including `tests`
and `benches`**. The separate test tree adds 5,784 SLOC and benchmarks add 5,164
SLOC, including the string-transport comparison, the lane-split float column sums, their summation-order tests and
benchmark, the interned string timing and allocation benchmark,
interned string storage, entry-handle copying and its integration/ownership tests,
owning immutable selections, shared mutable projections and their
ownership/composition/concurrency tests,
the shared-record API, ownership tests, whole-record workflow and
sharing strategy experiment, the typed fixed-array SoA experiment with per-cell tag/length metadata, and the
Arc core-scaling harness and parallel-cleanup API.
The product includes the public `StringTransport` builder, table-value conversions,
and corresponding ownership and heap-fallback tests. Inline unit tests and the
doctest-only guide bridge inside `src` are included in
every Rust scope. The included Markdown text is excluded from source counts. In the C++
scopes, the pinned baseline has no test directory and the maintained benchmark
references add 3,951 SLOC, including the `string_transport` benchmark, the interned
string benchmark and allocation
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
functions it missed in larger files. Lizard 1.17.31's Rust parser also stops
recognizing functions after a lifetime parameter such as `<'a>`: it finds 1 of the 11
functions in `benches/string_transport.rs` and only 3 in `src/table/views.rs`. Rust
function counts and total CCN are therefore lower bounds, while SLOC is unaffected.
Consequently, the average is a repeatable
static-analysis indicator, not an exact count of runtime paths. Function count and total
CCN are included so rounding and shifts in the average remain visible.

# Source size and complexity

This is a snapshot of the `gd-rs` source tree published with the StringTransport
addition, the fixed-string gather optimization, lazily resolved `RowMut` views, the
matched GD DTO row-sum benchmarks and split value/validity storage for nullable numeric
columns, direct compatible-column append, sampled low-cardinality index capacity,
reused CSV/JSON import buffers, the opt-in German/Umbra string column layout, and the
standalone cache-latency fixture, scoped mutable-row thread tests, the matched
index-sort benchmark, and the three-sort-method benchmark and its ports, remeasured on 2026-10-30.
The C++ product scope uses GD pinned to
`a5b1e3e7eba69a5e2d3fbd8e6a8f057d972cd868`, exported from Git independently of the
working-tree submodule checkout. All scopes were remeasured on 2026-10-30. It measures
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
| Rust product (`src`) | 36 | 10,704 | 367 | 988 | 2.69 |
| Rust product + tests (`src`, `tests`) | 56 | 16,954 | 588 | 1,416 | 2.41 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 89 | 22,881 | 828 | 2,144 | 2.59 |
| C++ product (`source`) | 142 | 65,038 | 8,718 | 20,006 | 2.29 |
| C++ product + tests (`source`, `tests`; `tests` absent) | 142 | 65,038 | 8,718 | 20,006 | 2.29 |
| C++ product + tests + maintained benchmarks | 177 | 69,776 | 9,039 | 21,085 | 2.33 |

The Rust totals are **10,704 SLOC in `src`** and **22,881 SLOC including `tests`
and `benches`**. The separate test tree adds 6,250 SLOC and benchmarks add 5,927
SLOC, including the index-sort and three-sort-method ports, the GD DTO row-sum port, the string-transport comparison, the lane-split float column sums, their summation-order tests and
benchmark, the interned string timing and allocation benchmark,
interned string storage, entry-handle copying and its integration/ownership tests,
owning immutable selections, shared mutable projections and their
ownership/composition/concurrency tests,
the shared-record API, ownership tests, whole-record workflow and
sharing strategy experiment, the typed fixed-array SoA experiment with per-cell tag/length metadata, and the
Arc core-scaling harness and parallel-cleanup API.
The product includes the public `StringTransport` builder, table-value conversions,
the optional German/Umbra string column layout, and corresponding ownership and
heap-fallback tests. Inline unit tests and the
doctest-only guide bridge inside `src` are included in
every Rust scope. The included Markdown text is excluded from source counts. In the C++
scopes, the pinned baseline has no test directory and the maintained benchmark
references add 4,738 SLOC, including the index-sort and three-sort-method benchmarks and the GD table row iterator, the GD DTO row-sum test, the `string_transport` benchmark, the interned
string benchmark and allocation
observer, the GD SIMD order-workflow adapter,
the GD/std::string text-workflow cases, whole-record filter-copy benchmark, string
layout probe, projection correctness fixture, and standalone cache-latency fixture
in this worktree. The cache-latency fixture has no matched Rust counterpart.

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
  `benches/cpp-reference` and `benches/index_sort/cpp`;
- excluded from both: documentation, manifests, build scripts, generated build
  output (including any `target` directory below `benches`), vendored dependencies, and every directory not named above.

Measure a clean checkout or Git export of the source being reported so unrelated
untracked files and local edits do not enter the published totals.

The product-only Rust measurement can be reproduced with:

```sh
python3 -m pip install --target /tmp/gd-code-metrics lizard==1.17.31
find src -type f -name '*.rs' | LC_ALL=C sort > /tmp/gd-rs-files.txt
PYTHONPATH=/tmp/gd-code-metrics python3 -m lizard \
  --languages rust --input_file /tmp/gd-rs-files.txt
```

Add `tests` and `benches` to the `find` roots for the inclusive Rust result. For C++,
run from `gd-rs`, replace the roots with
`external/gd/source benches/cpp-reference benches/index_sort/cpp` (add
`external/gd/tests` only when that directory exists), select the C/C++ suffixes listed
above, and use `--languages cpp`. The maintained C++ benchmark scope includes matched GD
references and standalone host fixtures; reports identify fixtures that have no Rust
counterpart.

If the GD working-tree checkout differs from the pinned revision above, export the
pinned `source` tree with `git archive` and measure that export instead. This keeps
the product scope tied to the committed submodule pointer.

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

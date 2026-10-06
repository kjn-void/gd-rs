# Source size and complexity

This is a snapshot of the current `gd-rs` worktree and the `external/gd` submodule
baseline measured and revalidated on 2026-10-06. Rust scopes describe the current
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
| Rust product (`src`) | 27 | 7,412 | 254 | 688 | 2.71 |
| Rust product + tests (`src`, `tests`) | 39 | 11,017 | 395 | 945 | 2.39 |
| Rust product + tests + benchmarks (`src`, `tests`, `benches`) | 57 | 14,273 | 515 | 1,314 | 2.55 |
| C++ product (`source`) | 140 | 64,587 | 8,638 | 19,875 | 2.30 |
| C++ product + tests (`source`, `tests`; `tests` absent) | 140 | 64,587 | 8,638 | 19,875 | 2.30 |
| C++ product + tests + maintained benchmarks | 161 | 67,575 | 8,817 | 20,557 | 2.33 |

The Rust totals are **7,412 SLOC in `src`** and **14,273 SLOC including `tests`
and `benches`**. The separate test tree adds 3,605 SLOC and benchmarks add 3,256
SLOC. Inline unit tests inside `src` are included in every Rust scope. In the C++
scopes, the pinned baseline has no test directory and the maintained benchmark references add 2,988 SLOC, including the GD SIMD order-workflow adapter
in this worktree.

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

# Interned string comparison

Run from the repository root (macOS or Linux with Clang):

```sh
./benches/run_interned_strings.sh --wait-for-idle
```

The default matrix is 20,000 rows, 1/16/256/4,096/20,000 distinct values, and
16/128-byte ASCII strings. It runs three rounds of five samples per case. Output
is `target/interned-strings/results.json`; compiler output is in `build.log` in
the same directory. On Linux select Clang with `CXX=clang++`. `--skip-build`
reuses binaries; record the original build command alongside those results.

A small correctness pass exercises every operation, both string lengths, and
both the timing and memory executables:

```sh
./benches/run_interned_strings.sh --rows 64 --unique 1 16 64 \
  --lengths 16 128 --verify-only --output target/interned-strings/smoke.json
cargo test --example interned_strings_memory --no-default-features
```

The [Rust workload](workload.rs) and [C++ workload](../cpp-reference/interned_strings.cpp)
use one required string column, no nulls, no other columns, and one thread.
String `i` is `a` plus an eight-digit decimal ID, padded with `x`; row `r` uses
ID `r % distinct`. The four operations are:

- `insert`: create a table with row capacity and insert every string through
  `push_row` / `row_add`, including temporary value construction.
- `copy`: clone the complete table with independently mutable cells. Rust uses
  `Table::clone`; C++ uses its native copy constructor plus one schema reference
  acquisition to compensate for the pinned GD constructor's missing acquisition.
  Rust interned copies share immutable entries and payloads without string hashing
  or cache locking.
  C++ copies its reference pool. This is **not** an O(1) selection/view benchmark.
  Revalidate the schema-reference adapter when updating the pinned GD revision.
- `rotate`: replace every row through `set_cell` / `cell_set` with ID
  `(r + 1) % distinct`, using the existing dictionary. For one distinct value it
  measures setting each cell to its current value.
- `replace`: overwrite every row with a new unique `b`-prefixed string, regardless
  of the starting dictionary size. The result has 20,000 distinct values.

Inputs, initial mutation tables, result destruction, and full-content hashing are outside
the timer. Timing includes allocation and schema setup for insertion/copy. Each
sample is one operation; results contain all nanosecond samples and medians of
round medians. Fresh processes separate implementations and operations. Implementation
order rotates each round. The existing text-workflow load guard samples process
CPU deltas before/after each round, confirms aggregate background load over one
core, and rejects affected rounds. No sanitizers are enabled for measurements.

The runner also rejects entire rounds after a process spends over one second
off CPU, any timed sample exceeds 30 seconds, or sample spread exceeds both
10 times the minimum and a one-millisecond grace interval. These limits catch
system suspension and preemption; they are the same for every implementation.
Rejected records remain in the report and never enter its timing summary.
Run `python3 -B -m unittest discover -s benches/interned_strings -p test_compare.py`
to test these checks.

The runner calculates full FNV-1a digests independently in Python, validates both
binaries of each implementation, and validates every timed result. Native checks
also mutate the result and confirm source independence in verification mode, and
check a copy after its immediate source is destroyed.

Memory uses separate instrumented executables. The Rust observer delegates every
request unchanged to `System`; C++ replaces global `new`/`delete` with malloc/free
and equivalent aligned allocation. Both count live allocator block sizes using
`malloc_size` on macOS or `malloc_usable_size` on Linux. This includes size-class
rounding but excludes allocator bookkeeping, unused arena pages, stack and code.
GD's table/reference storage uses `new`; unobserved direct C `malloc` calls are
outside the count. Fixtures and the shared schema are built before the baseline.
`source_bytes` is one populated table; `result_bytes` is additional storage for
the operation result with that source still alive. Shared allocations are counted
once. These are retained heap measurements, **not** peak RSS or allocation traffic.
Allocation counters do not appear in the timing executable.

The Rust allocator wrapper has focused alignment/zeroing/reallocation tests. It
needs a system allocator; unsupported platforms still build the examples but
reject execution of the memory executable. All reference sources are read-only;
the runner builds only maintained C++ fixtures and writes under `target`.

## Comparing an earlier implementation

Build a frozen baseline from a local committed revision, then add it to the same
rotating process order as the current implementation and C++ GD:

```sh
python3 benches/interned_strings/baseline.py ba8c32a \
  --output target/interned-strings/baseline
./benches/run_interned_strings.sh --wait-for-idle --rounds 4 \
  --baseline-dir target/interned-strings/baseline
```

The baseline builder requires a fresh output directory, extracts `git archive`
under that directory, and builds only Rust executables there. It changes no
checkout or branch. `baseline.json` records the revision, flags, source hashes
and executable hashes. The runner checks the executable hashes before running
and again afterward, and verifies the baseline against the same content oracle.
The native implementation argument remains `interned`; report entries distinguish
it as `interned-baseline`. The main report includes the baseline manifest.
Use `--operations copy` for a focused copy recheck; omitting it runs all four
operations. Four rounds give each of the four implementations the first position
once. Retained memory and mutation costs should also be compared because an
entry-handle layout can trade per-value overhead for cheaper cell copies.

Add `--resume --skip-build` to continue an interrupted timing run from its output
file using the same options. The runner verifies native source and executable
hashes, reaudits earlier samples, preserves earlier metadata as run segments,
and skips completed valid rounds. A harness-only update is allowed so older
checkpoints can be audited; changes to Rust/C++ sources or binaries are rejected.
Older records without process CPU accounting are checked by duration and spread.

Measured results: [interned string results](../../docs/high-level/interned-string-results.md).

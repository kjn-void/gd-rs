# Benchmark methodology and initial results

The C++ baseline uses Google Benchmark from the pinned CMake release preset. Rust uses
Criterion with the release profile. Results below were produced on the same Apple
Silicon host on 2026-07-13; the open-schema measurements were refreshed on
2026-07-14. They are snapshots, not cross-machine thresholds.

Commands:

```sh
cd ../gd
cmake --preset release
cmake --build --preset release
./build/release/benchmarks/gd_core_benchmarks

cd ../gd-rs
cargo bench
```

The open-schema subset can be reproduced directly with:

```sh
cd ../gd
./build/release/benchmarks/gd_core_benchmarks \
  --benchmark_filter=OpenSchema \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true

cd ../gd-rs
cargo bench --bench table -- OpenSchema
```

Both harnesses use optimized builds, warm-up, repeated sampling, and black-box barriers.
Small sub-nanosecond view benchmarks mostly confirm that no allocation or payload copy
occurs; differences at that scale should not be interpreted as application throughput.

Every `Rust/C++` column is a speed factor calculated as **C++ elapsed time divided by
Rust elapsed time**. Therefore **×1.20 means Rust is 1.20 times faster**, ×1.00 is a
tie, and ×0.80 means Rust is 0.80 times as fast as C++.

Rust timings use Criterion's reported point estimates. C++ timings use Google
Benchmark's CPU estimate; each matched pair was run sequentially to avoid contention.

## Dynamic values

Central point estimates in nanoseconds:

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| construct integer | 0.53 | 0.91 | ×0.58 |
| construct 8-byte string | 18.5 | 2.66 | ×6.95 |
| construct 64-byte string | 19.6 | 20.3 | ×0.97 |
| construct 512-byte string | 27.2 | 26.7 | ×1.02 |
| construct 4 KiB string | 62.9 | 58.7 | ×1.07 |
| construct 32 KiB string | 652 | 360 | ×1.81 |
| borrow string | 0.31 | about 0.89 | about ×0.35 |

The 8-byte result reflects inline `CompactString` storage. At 64 bytes and above both
implementations allocate. The 32 KiB measurement has allocator variance and must be
repeated when comparing changes.

## URI-shaped arguments

The fixture contains eleven named string, integer, and Boolean fields.

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| read all fields by linear name lookup | 186 ns | 122 ns | ×1.52 |
| read all fields positionally | 46 ns | 22.4 ns | ×2.06 |
| read all fields through hash name index | n/a | 88.4 ns | n/a |
| build companion/index structure | 47 ns | 140 ns | ×0.34 |

The C++ companion structure accelerates positional access but still scans its slots for
name lookup. Rust already has direct positional vector access; its optional structure is
an `ahash` name index. These build operations are therefore informative but not
semantically identical. For eleven fields, the Rust hash index pays for itself only
across repeated name reads.

## Three-column table

Rows contain `u64 id`, a 16-way short group name, and `i64 value`. Construction includes
group-name formatting in both fixtures.

| Rows | C++ construct | Rust construct | Rust/C++ |
|---:|---:|---:|---:|
| 10 | 0.528 µs | 0.646 µs | ×0.82 |
| 100 | 3.25 µs | 4.79 µs | ×0.68 |
| 1,000 | 30.8 µs | 47.6 µs | ×0.65 |
| 10,000 | 310 µs | 474 µs | ×0.65 |

When the 16 group strings are prepared before table construction, inserting 10,000
rows takes about 210 µs in C++ and 109 µs in Rust (**Rust/C++ ×1.93**). The gap in the
table above therefore comes from the fixtures' standard integer-to-string formatting
paths rather than the storage insertion alone.

For a 100,000-row `i64` scan:

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| resolve column once, scan by position/view | 212 µs | 106 µs | ×1.99 |
| resolve name for every cell | 882 µs | 719 µs | ×1.23 |

The typed Rust column scan does less decoding and has contiguous values. Allocation
counts and retained bytes are still required before drawing a memory conclusion.

### Open schemas

The small fixture reserves its rows with one fixed `u64` column and stores two short
extra strings per row. “Late” adds each field through the ordinary named setter;
“atomic” supplies both extras to `push_row_with_extras`. The C++ comparison has no
single-call equivalent to the atomic Rust operation.

Construction point estimates:

| Rows | C++ late fields | Rust late fields | Rust/C++ | Rust atomic |
|---:|---:|---:|---:|---:|
| 100 | 6.47 µs | 5.67 µs | ×1.14 | 4.80 µs |
| 1,000 | 63.9 µs | 56.9 µs | ×1.12 | 45.9 µs |
| 10,000 | 643 µs | 616 µs | ×1.04 | 481 µs |

Lookup reads both extras by name on every row:

| Rows | C++ lookup | Rust lookup | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 2.04 µs | 1.81 µs | ×1.13 |
| 1,000 | 20.3 µs | 18.1 µs | ×1.12 |
| 10,000 | 202 µs | 181 µs | ×1.12 |
| 100,000 | 2.01 ms | 1.81 ms | ×1.11 |

The Rust sidecar keeps up to four fields in a compact linear representation, with the
first two entries inline. It promotes to an `AHashMap` on the fifth unique name. The
promotion threshold was selected from a container crossover measurement: at four
entries linear lookup measured about 11.5 ns versus 6.0 ns for hashing, while compact
construction was still substantially cheaper. The two-field fast path therefore does
not allocate a hash table.

The wide fixture stresses the promoted representation with **1,000 rows × 1,000
extra `u64` fields**, or one million extras. Both implementations reserve all rows and
prepare the 1,000 field names before timing.

| Wide open-schema workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| build through replacement-capable named setters | 1.168 s | 32.90 ms | ×35.50 |
| Rust atomic validated build | no exact API | 20.15 ms | n/a |
| C++ append-only build / Rust atomic build | 8.90 ms | 20.15 ms | ×0.44 |
| look up all one million fields by name | 1.160 s | 23.59 ms | ×49.16 |

The replacement-capable C++ setter searches the row's packed argument buffer before
every insertion, making this construction shape quadratic in fields per row. Its
named lookup is also linear: a separate position probe measured approximately 16 ns
for the first field, 1.17 µs for the middle field, and 2.32 µs for the last. Rust
lookups remained approximately 19 ns at every position after promotion.

`cell_add_argument` explains the fast C++ append-only result: it assumes the name is
new, skips replacement lookup, permits duplicates, and appends directly to the packed
buffer. Rust's atomic API still rejects fixed-schema conflicts and gives repeated
extra names last-value-wins semantics, so that row is useful as an upper-bound
comparison rather than an equivalent contract.

Peak process RSS while constructing one wide table was approximately 33.3 MiB for C++
and 111.6 MiB for Rust. This is a process-level peak rather than retained-allocation
accounting, but it exposes the expected trade-off: the C++ packed buffer is much more
compact, while Rust spends hash-table capacity to make large-row lookup and replacement
expected constant time. A thousand row-local extras should still be treated as an
exceptional shape; fields that are common across rows belong in typed schema columns.

### Row ordering

The fixture orders one deterministic `u64` key column. C++ selection sort mutates the
table; Rust constructs a stable borrowed row permutation and leaves payload columns
in place. Fixture construction is outside both timed regions.

| Rows | C++ selection sort | Rust row order | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 11.3 µs | 1.53 µs | ×7.39 |
| 1,000 | 969 µs | 23.6 µs | ×41.09 |
| 5,000 | 23.65 ms | 148 µs | ×159.34 |

Google Benchmark's complexity fit reports **0.95 × N²** for the C++ workload. The
Rust path uses the standard stable **O(n log n)** slice sort and stores an **O(n)**
permutation. These APIs do different post-sort work: C++ has physically reordered
rows, while Rust consumers traverse the returned order.

## Binary operations

The hex fixture converts byte arrays to lowercase text and back. The endian fixture
writes or reads 4,096 `u64` values in big-endian order. The search fixture looks for a
16-byte sequence near the end of a 64 KiB buffer.

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| encode 64 KiB as hex | 23.6 µs | 2.88 µs | ×8.21 |
| decode 128 KiB of hex | 25.7 µs | 7.26 µs | ×3.54 |
| write 4,096 big-endian `u64` values | 3.37 µs | 1.34 µs | ×2.51 |
| read 4,096 big-endian `u64` values | 3.37 µs | 1.27 µs | ×2.65 |
| find the tail sequence | 143 µs | 1.45 µs | ×98.82 |

Rust uses the safe APIs of `hex-simd` and `memchr`; its cursor arithmetic remains
bounds checked. The C++ byte finder is a naive scan, so its time is **O(h n)** in the
worst case for haystack length `h` and needle length `n`. `memchr::memmem` uses a
specialized substring-search implementation while keeping **O(1)** auxiliary space
for this call site.

## Text conversion

The fixture repeats ASCII, XML punctuation, an accented character, an astral
character, URI punctuation, and a newline. The JSON workload produces a complete
quoted string literal in both languages. The URI decoder writes/returns validated
UTF-8; Rust additionally checks every percent triple before decoding.

Central estimates for 64 KiB of input:

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| JSON string encode | 185 µs | 26.9 µs | ×6.87 |
| URI component encode | 241 µs | 163 µs | ×1.48 |
| URI component decode | 109 µs | 178 µs | ×0.61 |
| XML entity escape | 172 µs | 87.9 µs | ×1.96 |

At 4 KiB, the corresponding C++/Rust estimates are 11.9/1.98 µs, 14.9/10.3 µs,
6.82/11.1 µs, and 11.0/5.63 µs, giving Rust/C++ factors of ×6.02, ×1.45, ×0.62,
and ×1.95. URI decoding is the one measured text path where the Rust implementation
takes longer; its timing includes syntax validation and UTF-8 validation that the C++
buffer overload does not perform. Both implementations are **O(n)** for these fixtures
and allocate output proportional to the encoded or decoded result.

## Compiled expressions

Both harnesses preload `x = 10` and `y = 20`. Compilation includes parsing and owned
compiled-form construction. Evaluation starts from an already compiled formula and
returns an owned scalar; the Rust timing includes conversion from Rhai's dynamic
result into `Value`.

| Formula | C++ compile | Rust compile | Rust/C++ compile | C++ evaluate | Rust evaluate | Rust/C++ evaluate |
|---|---:|---:|---:|---:|---:|---:|
| `x + y * 2` | 263 ns | 730 ns | ×0.36 | 131 ns | 93.8 ns | ×1.40 |
| `abs(x - y) + max(x, y)` | 406 ns | 1.35 µs | ×0.30 | 269 ns | 312 ns | ×0.86 |
| `x > y && x < 100` | 299 ns | 970 ns | ×0.31 | 161 ns | 79.4 ns | ×2.03 |

The C++ compiled form is a postfix token vector; Rust uses a Rhai AST. Parsing and
compilation are **O(source bytes)** for these straight-line formulas. Evaluation is
**O(executed tokens/AST operations)** plus variable and function lookup. Retained AST
and token-vector bytes have not yet been measured, so this table makes no memory-use
claim.

## Interchange formatting

The table fixture has 10,000 rows containing `u64`, one of 16 short strings, and
`i64`. JSON is a complete array of named row objects. CSV includes a header. Both
timed regions begin with an already constructed table.

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| table JSON, 10,000 rows | 799 µs | 521 µs | ×1.53 |
| table CSV, 10,000 rows | 592 µs | 382 µs | ×1.55 |
| URI and JSON for 11 arguments | 1.40 µs | 0.863 µs | ×1.62 |

The Rust table writers stream into one output buffer. JSON uses `serde_json` for
scalar escaping; CSV uses the `csv` state machine and stack-backed `itoa`/`ryu`
numeric text. The argument JSON workload additionally checks for duplicate names,
and both Rust argument formats reject unnamed entries rather than silently omitting
them. All three workloads are **O(values + output bytes)**.

## SQLite table materialization

The C++ fixture uses bundled SQLite 3.53.2. Rust uses bundled SQLite 3.51.3 from
`libsqlite3-sys` 0.37, the newest dependency line in this pass that compiles on the
crate's declared Rust 1.86 minimum. Both use the same in-memory table with columns
`id INTEGER`, `group_name TEXT`, and `value INTEGER`. Setup and inserts occur outside
the timed region. Timing includes statement preparation, row stepping, SQLite storage-
class validation, owned text copies, and materialization into typed column vectors.

The existing C++ SQLite record wrapper is not a valid comparison target: its copy and
move ownership, record-buffer deletion, binding lifetimes, alignment, and `BLOB`
classification have confirmed defects. Because product files below `../gd/source`
must remain unchanged, the Google Benchmark fixture uses the SQLite C API and a small
typed structure-of-arrays adapter. Rust measures `query_table_with_schema`, including
construction of its `Schema`, null metadata, and `ahash` column-name index. The two
adapters therefore implement the same observable row result, but do not have identical
metadata overhead.

Central estimates:

| Rows | C++ median | Rust median | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 12.72 µs | 12.60 µs | ×1.01 |
| 1,000 | 118.97 µs | 109.77 µs | ×1.08 |
| 10,000 | 1.184 ms | 1.077 ms | ×1.10 |

Both implementations are **O(rows + copied text bytes)** time and retain
**O(rows + copied text bytes)** result space. The C++ figures are medians from ten
Google Benchmark repetitions; the Rust figures are Criterion point estimates. No
memory-use conclusion follows from these timings.

## Regression policy

Store raw benchmark output for investigations, but do not compare absolute timings
from different machines. Review confidence intervals, fixture equivalence, algorithmic
complexity, and allocation behavior. A benchmark change is actionable only when the
work performed and ownership boundary are the same.

# Benchmark methodology and initial results

The C++ baseline uses Google Benchmark from the pinned CMake release preset. Rust uses
Criterion with the release profile. Results below were produced on the same Apple
Silicon host on 2026-07-13; the open-schema measurements were refreshed on
2026-07-14 and the mixed-numeric table measurements were added on 2026-07-15.
They are snapshots, not cross-machine thresholds.

Commands:

```sh
cd benches/cpp-reference
cmake --preset release
cmake --build --preset release
../../target/cpp-reference/release/gd_cpp_reference_benchmarks

cd ../..
cargo bench
```

The open-schema subset can be reproduced directly with:

```sh
cd benches/cpp-reference
../../target/cpp-reference/release/gd_cpp_reference_benchmarks \
  --benchmark_filter=OpenSchema \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true

cd ../..
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

### Ten-million-row mixed-numeric sheet

This fixture models a large spreadsheet with 10,000,000 rows and six fixed columns:
`u8`, `f64`, `u16`, `u64`, `f32`, and `i32`, in that order. Construction allocates the complete table
and inserts every row. The statistics workload excludes construction and calculates
average, minimum, maximum, and median for every column. For the even row count, median
is the mean of the two central values, matching spreadsheet `MEDIAN` behavior.

Rows use the deterministic permutation `p = (row × 48271) mod 10000000`, so median
selection does not receive pre-sorted input. The six values are `p mod 251`,
`p × 0.5 - 2500000`, `p mod 65521`, `p × 1000`, `p - 5000000` as `f32`, and
`p - 5000000` as `i32`. Both implementations validate these results outside the
timed loop:

| Field | Average | Minimum | Maximum | Median |
|---|---:|---:|---:|---:|
| `u8` | 124.999272 | 0 | 250 | 125 |
| `f64` | -0.25 | -2500000 | 2499999.5 | -0.25 |
| `u16` | 32709.5755948 | 0 | 65520 | 32679 |
| `u64` | 4999999500 | 0 | 9999999000 | 4999999500 |
| `f32` | -0.5 | -5000000 | 4999999 | -0.5 |
| `i32` | -0.5 | -5000000 | 4999999 | -0.5 |

The C++ benchmark was compiled twice. Both binaries and the GD core use `-O3` and no
sanitizers; the assertions-off build additionally uses `-DNDEBUG`. Google Benchmark
labels the assertions-on binary as a debug library solely because `NDEBUG` is absent,
but the compile database confirms that optimization remains `-O3`. Rust uses the
ordinary optimized release profile and safe table API; no unchecked Rust comparison is
included.

Central estimates:

| Build 10,000,000 rows | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| complete table | 329 ms | 335 ms | 224.88 ms |

Every bulk operation below scans exactly one named field over all 10,000,000 rows; no
row combines multiple fields and no timing aggregates several operations. C++ values
are means of three optimized repetitions. Rust values are Criterion means from ten
flat samples. `ValueRef` uses `Column::iter` and repeats dynamic storage dispatch for
each cell. “Dispatch once” uses `Column::for_each_value`, which selects storage and
nullability once but still presents every cell to the callback as `ValueRef`. `&[T]`
is the explicitly typed slice path. “Dispatch gain” is `ValueRef time / dispatch-once
time`; “dispatch / slice” is the two Rust times divided, so ×1.00 is parity, above one
means dispatch-once is slower, and below one means it is faster. The final two columns
are `C++ assertions-off time / Rust time`, where a value above one favors Rust.
Minimum is still checked when validating the fixture, but is not timed separately
because it has the same traversal and reduction shape as Maximum.

Average:

| Field | C++ off | C++ on | Rust `ValueRef` | Rust dispatch once | Rust `&[T]` | Dispatch gain | Dispatch / slice | Dispatch vs C++ off | Slice vs C++ off |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 8.316 ms | 11.695 ms | 13.351 ms | 0.178 ms | 0.179 ms | ×74.88 | ×1.00 | ×46.64 | ×46.54 |
| `f64` | 8.231 ms | 11.680 ms | 13.392 ms | 6.488 ms | 6.929 ms | ×2.06 | ×0.94 | ×1.27 | ×1.19 |
| `u16` | 8.248 ms | 11.391 ms | 13.373 ms | 0.704 ms | 0.703 ms | ×18.98 | ×1.00 | ×11.71 | ×11.74 |
| `u64` | 10.940 ms | 11.407 ms | 13.402 ms | 0.954 ms | 0.958 ms | ×14.05 | ×1.00 | ×11.47 | ×11.42 |
| `f32` | 8.244 ms | 11.733 ms | 13.396 ms | 6.712 ms | 6.773 ms | ×2.00 | ×0.99 | ×1.23 | ×1.22 |
| `i32` | 8.251 ms | 11.426 ms | 13.389 ms | 0.701 ms | 0.701 ms | ×19.11 | ×1.00 | ×11.78 | ×11.77 |

Maximum:

| Field | C++ off | C++ on | Rust `ValueRef` | Rust dispatch once | Rust `&[T]` | Dispatch gain | Dispatch / slice | Dispatch vs C++ off | Slice vs C++ off |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 8.261 ms | 12.032 ms | 13.510 ms | 0.098 ms | 0.098 ms | ×138.13 | ×1.00 | ×84.46 | ×84.53 |
| `f64` | 8.220 ms | 11.706 ms | 13.530 ms | 5.383 ms | 5.380 ms | ×2.51 | ×1.00 | ×1.53 | ×1.53 |
| `u16` | 8.256 ms | 12.065 ms | 13.522 ms | 0.206 ms | 0.207 ms | ×65.76 | ×1.00 | ×40.15 | ×39.96 |
| `u64` | 8.242 ms | 13.597 ms | 13.620 ms | 1.439 ms | 1.460 ms | ×9.47 | ×0.99 | ×5.73 | ×5.64 |
| `f32` | 8.242 ms | 11.670 ms | 13.470 ms | 5.411 ms | 5.417 ms | ×2.49 | ×1.00 | ×1.52 | ×1.52 |
| `i32` | 8.385 ms | 11.702 ms | 13.528 ms | 0.468 ms | 0.491 ms | ×28.91 | ×0.95 | ×17.92 | ×17.08 |

Median:

| Field | C++ off | C++ on | Rust `ValueRef` | Rust dispatch once | Rust `&[T]` | Dispatch gain | Dispatch / slice | Dispatch vs C++ off | Slice vs C++ off |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 40.224 ms | 44.783 ms | 22.785 ms | 14.753 ms | 9.574 ms | ×1.54 | ×1.54 | ×2.73 | ×4.20 |
| `f64` | 13.981 ms | 21.321 ms | 25.599 ms | 18.108 ms | 13.406 ms | ×1.41 | ×1.35 | ×0.77 | ×1.04 |
| `u16` | 43.220 ms | 46.828 ms | 23.105 ms | 15.130 ms | 10.040 ms | ×1.53 | ×1.51 | ×2.86 | ×4.30 |
| `u64` | 14.073 ms | 21.014 ms | 24.924 ms | 16.866 ms | 12.743 ms | ×1.48 | ×1.32 | ×0.83 | ×1.10 |
| `f32` | 13.957 ms | 17.891 ms | 24.713 ms | 17.052 ms | 11.748 ms | ×1.45 | ×1.45 | ×0.82 | ×1.19 |
| `i32` | 12.478 ms | 16.442 ms | 22.699 ms | 14.559 ms | 9.755 ms | ×1.56 | ×1.49 | ×0.86 | ×1.28 |

The C++ benchmark copies each cell into an aligned local value with fixed-size
`memcpy`; this retains `cell_get` assertions in the assertions-on build while avoiding
undefined behavior from dereferencing the `f64` field at offset 4. This is
source-level defined behavior, not sanitizer instrumentation.

Rust `ValueRef` iteration performs runtime storage dispatch, bounds checking, and
dynamic tag reconstruction for every cell. `Column::for_each_value` hoists storage and
nullability dispatch out of the loop while retaining a `ValueRef` callback. For simple
averages and extrema it is at parity with the typed slice in almost every case; this is
consistent with LLVM inlining the callback, eliminating the known `ValueRef` variant,
and vectorizing the resulting direct slice loop. `Column::as_slice::<T>` remains the
explicit way to guarantee a monomorphic contiguous input.

The Maximum paths use the same explicit accumulator loop for all three Rust APIs. An
earlier typed-`f32` measurement used `Iterator::reduce` instead and took about twice as
long; repeating it confirmed the number, but aligning the reduction shape restored
typed-slice and dispatch-once parity (5.417 and 5.411 ms). That discrepancy was compiler
code generation for different loop forms, not slice-access overhead. The float fixture
contains no NaNs; both languages use ordinary finite comparisons in these bulk cases.
Table ordering continues to use `total_cmp`.

Median copies one scratch vector and partitions it with `std::nth_element` or
`select_nth_unstable`. Dispatch-once improves the old `ValueRef` path by ×1.41–×1.56,
but remains ×1.32–×1.54 slower than the typed path. A typed slice iterator can be
collected with a bulk copy, whereas the callback writes one reconstructed value at a
time; the branch-heavy selection phase then dominates both paths.

Row-bearing capacity accounting is:

| Implementation | Row model | Bytes per row | Table bytes | MiB | Relative to C++ |
|---|---|---:|---:|---:|---:|
| C++ | physical fixed-stride row | 32 | 320,000,000 | 305.18 | ×1.00 |
| Rust | virtual sum across typed vectors | 27 | 270,000,000 | 257.49 | ×0.84 |

These figures exclude allocator bookkeeping and the small fixed table/schema objects.
The C++ figure comes from `size_reserved_total()`. GD aligns the start of every field
to four bytes, not to that field's natural alignment. Its physical 32-byte row is:

```text
C++ table_column_buffer: one physical row, repeated 10,000,000 times

byte offset   00 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15
              ├u8┤--padding--├────────── f64 ──────────┤├u16┤pad

byte offset   16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
              ├────────── u64 ──────────┤├── f32 ──┤├── i32 ──┤

payload: 1 + 8 + 2 + 8 + 4 + 4    = 27 bytes
padding: 3 after u8 + 2 after u16 =  5 bytes
physical row:                       32 bytes
```

The `f64` begins at offset 4, which satisfies GD's four-byte rule but not C++'s
eight-byte `double` alignment requirement. This is why the benchmark reads cells with
fixed-size `memcpy`. The following `u64` happens to land at naturally aligned offset
16. Ten million physical rows reserve `10,000,000 × 32 = 320,000,000` bytes.

Rust has no physical row object. Each required column is an independent `Vec<T>` whose
allocation starts suitably aligned for `T`; adjacent elements have exactly
`size_of::<T>()` stride. “27 bytes per row” is therefore a virtual or amortized row
contribution obtained by adding one same-index element from each separate vector:

```text
Rust Table: separate allocations (not adjacent in RAM)

Vec<u8>       [u8₀ ][u8₁ ][u8₂ ]...      1 byte  × 10M =  10 MB
Vec<f64>      [ f64₀ ][ f64₁ ]...        8 bytes × 10M =  80 MB
Vec<u16>      [u16₀][u16₁][u16₂]...      2 bytes × 10M =  20 MB
Vec<u64>      [ u64₀ ][ u64₁ ]...        8 bytes × 10M =  80 MB
Vec<f32>      [f32₀][f32₁][f32₂]...      4 bytes × 10M =  40 MB
Vec<i32>      [i32₀][i32₁][i32₂]...      4 bytes × 10M =  40 MB
                                                  total = 270 MB

virtual row r = u8[r] + f64[r] + u16[r] + u64[r] + f32[r] + i32[r]
                1   +   8    +   2    +   8    +   4    +   4     = 27 bytes
```

There is no inter-column or per-row padding in those 27 bytes. Each vector base is
aligned independently, outside the per-element accounting. The benchmark schema uses
`UnknownFields::Reject`, so `Table` disables extras storage and neither reserves nor
populates a per-row sidecar vector.

An open schema using `UnknownFields::Store` instead enables a parallel
`Vec<Option<Box<RowExtras>>>`. `Option<Box<RowExtras>>` uses the null-pointer niche and
is eight bytes on this target, so an open ten-million-row table reserves another
80,000,000 bytes even when every slot is `None`:

```text
closed schema: 27 typed bytes                        = 27 bytes/row = 270 MB
open schema:   27 typed bytes + 8-byte sidecar slot  = 35 bytes/row = 350 MB
```

The accounting excludes the small `Vec` headers, schema objects, allocator metadata,
allocator size-class rounding, and unused capacity beyond the requested ten million
elements; it is not an RSS measurement.

Median uses one temporary vector at a time, adding at most 80,000,000 bytes (76.29 MiB)
of live scratch space in either implementation; that scratch is not part of the table
figures. A validity bitmap would be a more compact future representation for nullable
primitive columns.

`Column::as_slice::<T>` now exposes dense required columns without exposing the storage
enum itself. Borrowing ties the slice lifetime to the table and prevents mutation while
it is in use. Nullable columns deliberately reject this API until they have an explicit
typed nullable view; callers can continue using `ValueRef` iteration for them.

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

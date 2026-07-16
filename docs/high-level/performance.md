# Benchmark methodology and results

The C++ reference uses Google Benchmark and the pinned CMake release presets. Rust uses
Criterion and Cargo's release profile. The stable comparison results were refreshed on
2026-07-15 from `gd-rs` commit `11a876c7ab84a25c6aa01da7620c6f9aae4d82fb` and C++
GD commit `3d1e112b0806845854e863f9fd8288a2f79ba378`. Three configurations are shown
separately so every table compares C++ with Rust under one hardware and ISA policy:

| Configuration | Host and operating system | C++ toolchain | Rust toolchain | ISA policy |
|---|---|---|---|---|
| M3 Max release | Apple M3 Max, 16 cores, macOS 26.5 (`arm64`) | Apple Clang 21.0.0 | rustc 1.97.0 | normal release defaults |
| Core Ultra portable | Intel Core Ultra 5 225H, Ubuntu 24.04, Linux 6.17 | GCC 13.3.0 | rustc 1.97.0 | normal portable `x86-64` release defaults |
| Core Ultra native | same Core Ultra host | GCC 13.3.0 | rustc 1.97.0 | C++ `-march=native`; Rust `-C target-cpu=native` |

The stable Core Ultra single-threaded processes were pinned to performance core 0 with
`taskset -c 0`. Its `intel_pstate` governor reported `powersave`, which still permits
demand-based turbo. The M3 processes were not pinned. These are benchmark snapshots,
not thresholds that can be compared across machines.

Commands:

```sh
cd benches/cpp-reference
cmake --preset release
cmake --build --preset release
../../target/cpp-reference/release/gd_cpp_reference_benchmarks \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true

cd ../..
cargo bench
```

The checked-in native presets reproduce the flags used for the Core Ultra native
binaries. Rust uses a separate target directory so native Criterion output cannot
replace the portable samples:

```sh
cd benches/cpp-reference
cmake --preset release-native
cmake --build --preset release-native
../../target/cpp-reference/release-native/gd_cpp_reference_benchmarks \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true

cd ../..
CARGO_TARGET_DIR=target/native RUSTFLAGS="-C target-cpu=native" cargo bench
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

Both harnesses use optimized builds, adaptive measurement, repeated sampling, and
black-box barriers. Small sub-nanosecond view benchmarks mostly confirm that no
allocation or payload copy occurs; differences at that scale should not be interpreted
as application throughput.

Every `Rust/C++` column reports **C++ elapsed time divided by Rust elapsed time**.
Therefore **×1.20 means Rust is 1.20 times faster**, ×1.00 is parity, and ×0.80 means
Rust is 0.80 times as fast as C++ for that fixture.
`n/a` means that no equivalent checked-in API or measurement exists.

Rust timings use Criterion's reported mean point estimates. C++ timings are the median
CPU time from three optimized repetitions. Each matched pair was run sequentially to
avoid contention. The full benchmark suites were run for all three configurations;
the tables select the checked-in workloads described by each section.

## Dynamic values

Benchmark sources: [Rust `construct_integer` and `dynamic_strings`](../../benches/value.rs#L8-L35) ·
[C++ `ConstructInteger`, `ConstructString`, and `BorrowString`](../../benches/cpp-reference/variant_benchmark.cpp#L11-L43)

Central point estimates in nanoseconds:

**M3 Max release**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| construct integer | 0.540 ns | 0.932 ns | ×0.58 |
| construct 8-byte string | 18.7 ns | 2.72 ns | ×6.89 |
| construct 64-byte string | 18.1 ns | 18.7 ns | ×0.97 |
| construct 512-byte string | 26.7 ns | 25.1 ns | ×1.07 |
| construct 4 KiB string | 64.1 ns | 59.0 ns | ×1.09 |
| construct 32 KiB string | 454 ns | 422 ns | ×1.08 |
| borrow string view, all tested sizes | about 0.322 ns | about 0.913 ns | about ×0.35 |

**Core Ultra portable**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| construct integer | 0.206 ns | 5.99 ns | ×0.03 |
| construct 8-byte string | 6.59 ns | 6.33 ns | ×1.04 |
| construct 64-byte string | 7.18 ns | 5.26 ns | ×1.36 |
| construct 512-byte string | 7.61 ns | 7.77 ns | ×0.98 |
| construct 4 KiB string | 46.5 ns | 53.2 ns | ×0.87 |
| construct 32 KiB string | 420 ns | 598 ns | ×0.70 |
| borrow string view, all tested sizes | about 0.307 ns | about 6.49 ns | about ×0.05 |

**Core Ultra native**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| construct integer | 0.206 ns | 5.99 ns | ×0.03 |
| construct 8-byte string | 6.57 ns | 6.39 ns | ×1.03 |
| construct 64-byte string | 7.18 ns | 5.34 ns | ×1.34 |
| construct 512-byte string | 7.43 ns | 7.54 ns | ×0.99 |
| construct 4 KiB string | 46.4 ns | 60.5 ns | ×0.77 |
| construct 32 KiB string | 420 ns | 516 ns | ×0.81 |
| borrow string view, all tested sizes | about 0.307 ns | about 6.49 ns | about ×0.05 |

The 8-byte result reflects inline `CompactString` storage. At 64 bytes and above both
implementations allocate. The 32 KiB measurement has allocator variance and must be
repeated when comparing changes.

## URI-shaped arguments

Benchmark sources: [Rust `uri`](../../benches/arguments.rs#L90-L150) ·
[C++ URI reads and companion construction](../../benches/cpp-reference/arguments_benchmark.cpp#L77-L130).
The C++ side has no hash-name-index equivalent.

The fixture contains eleven named string, integer, and Boolean fields.

**M3 Max release**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| read by name using a linear scan | 167 ns | 120 ns | ×1.39 |
| read positionally: C++ companion / Rust direct iteration | 46.7 ns | 22.3 ns | ×2.09 |
| read by name using Rust's `AHashMap` index | n/a | 88.0 ns | n/a |
| build C++ positional companion / Rust name index | 47.5 ns | 139 ns | ×0.34 |

**Core Ultra portable**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| read by name using a linear scan | 174 ns | 87.9 ns | ×1.98 |
| read positionally: C++ companion / Rust direct iteration | 54.3 ns | 15.9 ns | ×3.41 |
| read by name using Rust's `AHashMap` index | n/a | 90.4 ns | n/a |
| build C++ positional companion / Rust name index | 28.8 ns | 131 ns | ×0.22 |

**Core Ultra native**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| read by name using a linear scan | 184 ns | 85.5 ns | ×2.15 |
| read positionally: C++ companion / Rust direct iteration | 53.7 ns | 15.8 ns | ×3.41 |
| read by name using Rust's `AHashMap` index | n/a | 74.9 ns | n/a |
| build C++ positional companion / Rust name index | 29.2 ns | 122 ns | ×0.24 |

The C++ companion structure accelerates positional access but still scans its slots for
name lookup. Rust already has direct positional vector access; its optional structure is
an `ahash` name index. These build operations are therefore informative but not
semantically identical. For eleven fields, the Rust hash index pays for itself only
across repeated name reads.

## Three-column table

Benchmark sources: [Rust `append_rows` and `scans`](../../benches/table.rs#L703-L761) ·
[C++ append and scan benchmarks](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L299-L352)

Rows contain `u64 id`, a 16-way short group name, and `i64 value`. Construction includes
group-name formatting in both fixtures.

**M3 Max release**

| Rows and fixture | C++ construct | Rust construct | Rust/C++ |
|---|---:|---:|---:|
| 10 | 0.537 µs | 0.657 µs | ×0.82 |
| 100 | 3.74 µs | 4.91 µs | ×0.76 |
| 1,000 | 36.4 µs | 47.7 µs | ×0.76 |
| 10,000 | 334 µs | 476 µs | ×0.70 |
| 10,000, group strings prepared | 177 µs | 114 µs | ×1.55 |

**Core Ultra portable**

| Rows and fixture | C++ construct | Rust construct | Rust/C++ |
|---|---:|---:|---:|
| 10 | 0.226 µs | 0.533 µs | ×0.42 |
| 100 | 1.68 µs | 3.96 µs | ×0.42 |
| 1,000 | 16.1 µs | 38.1 µs | ×0.42 |
| 10,000 | 161 µs | 395 µs | ×0.41 |
| 10,000, group strings prepared | 112 µs | 239 µs | ×0.47 |

**Core Ultra native**

| Rows and fixture | C++ construct | Rust construct | Rust/C++ |
|---|---:|---:|---:|
| 10 | 0.223 µs | 0.533 µs | ×0.42 |
| 100 | 1.73 µs | 4.11 µs | ×0.42 |
| 1,000 | 16.5 µs | 40.3 µs | ×0.41 |
| 10,000 | 164 µs | 412 µs | ×0.40 |
| 10,000, group strings prepared | 112 µs | 242 µs | ×0.46 |

Preparing the 16 group strings removes integer-to-string formatting from the timed
insertion loop. That changes the M3 result enough for Rust to lead, but not the Core
Ultra result; construction costs are therefore both fixture- and platform-sensitive.

For a 100,000-row `i64` scan:

**M3 Max release**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| C++ cell view by column index / Rust pre-resolved `Column::iter` | 216 µs | 134 µs | ×1.62 |
| resolve the column name for every cell | 1.026 ms | 775 µs | ×1.32 |

**Core Ultra portable**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| C++ cell view by column index / Rust pre-resolved `Column::iter` | 144 µs | 72.6 µs | ×1.99 |
| resolve the column name for every cell | 730 µs | 950 µs | ×0.77 |

**Core Ultra native**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| C++ cell view by column index / Rust pre-resolved `Column::iter` | 143 µs | 72.6 µs | ×1.97 |
| resolve the column name for every cell | 745 µs | 864 µs | ×0.86 |

The Rust column-view scan resolves the column once and traverses its contiguous
column-major storage, but still yields `ValueRef`; it is not the typed-slice API used
later in the mixed-numeric benchmark. Allocation counts and retained bytes are still
required before drawing a memory conclusion.

### Open schemas

Benchmark sources: [Rust open-schema benchmarks](../../benches/table.rs#L763-L850) ·
[C++ open-schema benchmarks](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L354-L424).
The C++ side has no equivalent to Rust's validated atomic row build.

The small fixture reserves its rows with one fixed `u64` column and stores two short
extra strings per row. “Late” adds each field through the ordinary named setter;
“atomic” supplies the fixed row and both extras to `push_row_with_extras` as one
all-or-nothing validation and insertion operation. Here, atomic describes failure
semantics, not thread synchronization. The C++ comparison has no single-call
equivalent.

Construction point estimates:

**M3 Max release**

| Rows | C++ late fields | Rust late fields | Rust/C++ | Rust atomic |
|---:|---:|---:|---:|---:|
| 100 | 6.19 µs | 5.55 µs | ×1.12 | 4.85 µs |
| 1,000 | 60.7 µs | 57.8 µs | ×1.05 | 46.9 µs |
| 10,000 | 607 µs | 585 µs | ×1.04 | 485 µs |

**Core Ultra portable**

| Rows | C++ late fields | Rust late fields | Rust/C++ | Rust atomic |
|---:|---:|---:|---:|---:|
| 100 | 4.47 µs | 7.72 µs | ×0.58 | 8.52 µs |
| 1,000 | 43.6 µs | 82.5 µs | ×0.53 | 93.1 µs |
| 10,000 | 441 µs | 821 µs | ×0.54 | 929 µs |

**Core Ultra native**

| Rows | C++ late fields | Rust late fields | Rust/C++ | Rust atomic |
|---:|---:|---:|---:|---:|
| 100 | 4.45 µs | 7.64 µs | ×0.58 | 8.49 µs |
| 1,000 | 43.3 µs | 81.6 µs | ×0.53 | 91.4 µs |
| 10,000 | 439 µs | 835 µs | ×0.53 | 949 µs |

Lookup reads both extras by name on every row:

**M3 Max release**

| Rows | C++ lookup | Rust lookup | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 1.78 µs | 1.81 µs | ×0.99 |
| 1,000 | 17.6 µs | 18.0 µs | ×0.98 |
| 10,000 | 178 µs | 182 µs | ×0.98 |
| 100,000 | 1.79 ms | 1.81 ms | ×0.98 |

**Core Ultra portable**

| Rows | C++ lookup | Rust lookup | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 1.76 µs | 1.43 µs | ×1.23 |
| 1,000 | 16.8 µs | 14.1 µs | ×1.19 |
| 10,000 | 169 µs | 142 µs | ×1.19 |
| 100,000 | 1.78 ms | 1.51 ms | ×1.17 |

**Core Ultra native**

| Rows | C++ lookup | Rust lookup | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 1.71 µs | 1.21 µs | ×1.41 |
| 1,000 | 16.9 µs | 12.1 µs | ×1.40 |
| 10,000 | 170 µs | 123 µs | ×1.38 |
| 100,000 | 1.76 ms | 1.31 ms | ×1.34 |

The Rust sidecar keeps up to four fields in a compact linear representation, with the
first two entries inline, and promotes to an `AHashMap` on the fifth unique name. The
small fixture above exercises the two-field inline path; the wide fixture below
exercises the hashed path. The checked-in suite does not separately measure the
four-to-five-field crossover.

The wide fixture stresses the promoted representation with **1,000 rows × 1,000
extra `u64` fields**, or one million extras. Both implementations reserve all rows and
prepare the 1,000 field names before timing.

**M3 Max release**

| Wide open-schema workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| build through replacement-capable named setters | 1.146 s | 32.7 ms | ×35.00 |
| validated atomic row build (Rust only) | no exact API | 21.3 ms | n/a |
| C++ append-only / Rust validated atomic (different contracts) | 8.80 ms | 21.3 ms | ×0.41 |
| look up all one million fields by name | 1.142 s | 22.3 ms | ×51.29 |

**Core Ultra portable**

| Wide open-schema workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| build through replacement-capable named setters | 2.024 s | 75.8 ms | ×26.70 |
| validated atomic row build (Rust only) | no exact API | 75.3 ms | n/a |
| C++ append-only / Rust validated atomic (different contracts) | 17.4 ms | 75.3 ms | ×0.23 |
| look up all one million fields by name | 2.019 s | 24.6 ms | ×81.95 |

**Core Ultra native**

| Wide open-schema workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| build through replacement-capable named setters | 2.030 s | 74.6 ms | ×27.20 |
| validated atomic row build (Rust only) | no exact API | 72.2 ms | n/a |
| C++ append-only / Rust validated atomic (different contracts) | 17.2 ms | 72.2 ms | ×0.24 |
| look up all one million fields by name | 2.020 s | 21.0 ms | ×96.21 |

The replacement-capable C++ setter searches the row's packed argument buffer before
every insertion, making this construction shape quadratic in fields per row. Named
C++ lookup also scans the packed row, while promoted Rust lookup uses an `AHashMap` with
expected constant-time access. The one-million-lookup row measures those complete
lookup paths rather than isolated first, middle, and last positions.

`cell_add_argument` explains the fast C++ append-only result: it assumes the name is
new, skips replacement lookup, permits duplicates, and appends directly to the packed
buffer. Rust's atomic API still rejects fixed-schema conflicts and gives repeated
extra names last-value-wins semantics, so that row is useful as an upper-bound
comparison rather than an equivalent contract.

A separate, non-harness observation of peak process RSS while constructing one wide
table was approximately 33.3 MiB for C++ and 111.6 MiB for Rust. This is process-level
peak RSS, not retained-allocation accounting or a Criterion/Google Benchmark result.
It nevertheless illustrates the expected trade-off: the C++ packed buffer is more
compact, while Rust spends hash-table capacity to make large-row lookup and replacement
expected constant time. A thousand row-local extras should still be treated as an
exceptional shape; fields common across rows belong in typed schema columns.

### Large tables

The ten-million-row mixed-numeric results, storage diagrams, harvested-vector
experiments, and the 100-million-row parallel transform are collected in
[Large-table performance](perf_large_table.md).

### Row ordering

Benchmark sources: [Rust `row_order`](../../benches/table.rs#L960-L981) ·
[C++ `RowSortSelection`](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L505-L519)

The fixture orders one deterministic `u64` key column. C++ selection sort mutates the
table; Rust constructs a stable borrowed row permutation and leaves payload columns
in place. Fixture construction is outside both timed regions.

**M3 Max release**

| Rows | C++ selection sort | Rust row order | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 11.8 µs | 1.51 µs | ×7.80 |
| 1,000 | 990 µs | 23.5 µs | ×42.11 |
| 5,000 | 24.03 ms | 148 µs | ×162.14 |

**Core Ultra portable**

| Rows | C++ selection sort | Rust row order | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 12.8 µs | 1.36 µs | ×9.44 |
| 1,000 | 1.002 ms | 21.8 µs | ×45.91 |
| 5,000 | 23.65 ms | 141 µs | ×167.81 |

**Core Ultra native**

| Rows | C++ selection sort | Rust row order | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 12.8 µs | 1.67 µs | ×7.67 |
| 1,000 | 996 µs | 21.4 µs | ×46.61 |
| 5,000 | 23.65 ms | 142 µs | ×166.27 |

Google Benchmark's complexity fit remains quadratic for the C++ workload. The Rust
path uses the standard stable **O(n log n)** slice sort and stores an **O(n)**
permutation. These APIs do different post-sort work: C++ has physically reordered
rows, while Rust consumers traverse the returned order.

## Binary operations

Benchmark sources: [Rust binary benchmarks](../../benches/binary.rs#L8-L61) ·
[C++ binary benchmarks](../../benches/cpp-reference/binary_benchmark.cpp#L11-L81)

The hex fixture converts byte arrays to lowercase text and back. The endian fixture
writes or reads 4,096 `u64` values in big-endian order. The search fixture looks for the
six-byte string `needle`, appended after a 64 KiB buffer.

**M3 Max release**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| encode 64 KiB as hex | 23.5 µs | 2.89 µs | ×8.14 |
| decode 128 KiB of hex | 25.4 µs | 7.26 µs | ×3.50 |
| write 4,096 big-endian `u64` values | 3.32 µs | 1.34 µs | ×2.47 |
| read 4,096 big-endian `u64` values | 3.77 µs | 1.30 µs | ×2.89 |
| find the tail sequence | 141 µs | 1.44 µs | ×97.90 |

**Core Ultra portable**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| encode 64 KiB as hex | 52.3 µs | 2.96 µs | ×17.67 |
| decode 128 KiB of hex | 21.0 µs | 3.00 µs | ×7.00 |
| write 4,096 big-endian `u64` values | 1.69 µs | 2.33 µs | ×0.73 |
| read 4,096 big-endian `u64` values | 1.80 µs | 2.36 µs | ×0.77 |
| find the tail sequence | 62.4 µs | 1.08 µs | ×57.53 |

**Core Ultra native**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| encode 64 KiB as hex | 41.8 µs | 1.84 µs | ×22.70 |
| decode 128 KiB of hex | 21.0 µs | 3.07 µs | ×6.85 |
| write 4,096 big-endian `u64` values | 1.69 µs | 2.31 µs | ×0.73 |
| read 4,096 big-endian `u64` values | 1.80 µs | 2.48 µs | ×0.72 |
| find the tail sequence | 62.1 µs | 1.33 µs | ×46.84 |

Rust uses the safe APIs of `hex-simd` and `memchr`; its cursor arithmetic remains
bounds checked. The C++ byte finder is a naive scan, so its time is **O(h n)** in the
worst case for haystack length `h` and needle length `n`. `memchr::memmem` uses a
specialized substring-search implementation while keeping **O(1)** auxiliary space
for this call site.

## Text conversion

Benchmark sources: [Rust `text_benchmarks`](../../benches/text.rs#L18-L55) ·
[C++ UTF-8/text benchmarks](../../benches/cpp-reference/utf8_benchmark.cpp#L25-L82)

The fixture repeats ASCII, XML punctuation, an accented character, an astral
character, URI punctuation, and a newline. The JSON workload produces a complete
quoted string literal in both languages. The URI decoder writes/returns validated
UTF-8; Rust additionally checks every percent triple before decoding.

The percent-encoded decoder input is larger than the named source size.

**M3 Max release**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| JSON string encode, 4 KiB | 12.6 µs | 1.99 µs | ×6.30 |
| JSON string encode, 64 KiB | 198 µs | 27.1 µs | ×7.32 |
| URI component encode, 4 KiB | 15.7 µs | 10.6 µs | ×1.48 |
| URI component encode, 64 KiB | 253 µs | 165 µs | ×1.53 |
| URI component decode, 4 KiB | 6.76 µs | 11.4 µs | ×0.59 |
| URI component decode, 64 KiB | 112 µs | 180 µs | ×0.62 |
| XML entity escape, 4 KiB | 10.8 µs | 5.78 µs | ×1.87 |
| XML entity escape, 64 KiB | 170 µs | 90.4 µs | ×1.88 |

**Core Ultra portable**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| JSON string encode, 4 KiB | 6.72 µs | 1.39 µs | ×4.85 |
| JSON string encode, 64 KiB | 104 µs | 25.8 µs | ×4.04 |
| URI component encode, 4 KiB | 7.31 µs | 4.74 µs | ×1.54 |
| URI component encode, 64 KiB | 117 µs | 76.6 µs | ×1.53 |
| URI component decode, 4 KiB | 3.14 µs | 6.94 µs | ×0.45 |
| URI component decode, 64 KiB | 50.1 µs | 109 µs | ×0.46 |
| XML entity escape, 4 KiB | 5.45 µs | 3.58 µs | ×1.52 |
| XML entity escape, 64 KiB | 85.9 µs | 57.0 µs | ×1.51 |

**Core Ultra native**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| JSON string encode, 4 KiB | 5.96 µs | 1.57 µs | ×3.79 |
| JSON string encode, 64 KiB | 104 µs | 23.4 µs | ×4.42 |
| URI component encode, 4 KiB | 6.97 µs | 4.92 µs | ×1.42 |
| URI component encode, 64 KiB | 114 µs | 75.6 µs | ×1.50 |
| URI component decode, 4 KiB | 3.53 µs | 6.80 µs | ×0.52 |
| URI component decode, 64 KiB | 49.8 µs | 109 µs | ×0.46 |
| XML entity escape, 4 KiB | 5.25 µs | 3.65 µs | ×1.44 |
| XML entity escape, 64 KiB | 83.9 µs | 58.0 µs | ×1.45 |

URI decoding is the one measured text path where the Rust implementation
takes longer; its timing includes syntax validation and UTF-8 validation that the C++
buffer overload does not perform. Both implementations are **O(n)** for these fixtures
and allocate output proportional to the encoded or decoded result.

## Compiled expressions

Benchmark sources: [Rust compile and evaluate benchmarks](../../benches/expression.rs#L14-L50) ·
[C++ compile and evaluate benchmarks](../../benches/cpp-reference/expression_benchmark.cpp#L49-L84)

Both harnesses preload `x = 10` and `y = 20`. Compilation includes parsing and owned
compiled-form construction. Evaluation starts from an already compiled formula and
returns an owned scalar; the Rust timing includes conversion from Rhai's dynamic
result into `Value`.

**M3 Max release**

| Formula | C++ compile | Rust compile | Rust/C++ compile | C++ evaluate | Rust evaluate | Rust/C++ evaluate |
|---|---:|---:|---:|---:|---:|---:|
| `x + y * 2` | 267 ns | 741 ns | ×0.36 | 144 ns | 94.9 ns | ×1.51 |
| `abs(x - y) + max(x, y)` | 423 ns | 1.40 µs | ×0.30 | 299 ns | 318 ns | ×0.94 |
| `x > y && x < 100` | 302 ns | 994 ns | ×0.30 | 180 ns | 80.9 ns | ×2.23 |

**Core Ultra portable**

| Formula | C++ compile | Rust compile | Rust/C++ compile | C++ evaluate | Rust evaluate | Rust/C++ evaluate |
|---|---:|---:|---:|---:|---:|---:|
| `x + y * 2` | 109 ns | 661 ns | ×0.17 | 85.1 ns | 151 ns | ×0.56 |
| `abs(x - y) + max(x, y)` | 152 ns | 1.19 µs | ×0.13 | 188 ns | 332 ns | ×0.57 |
| `x > y && x < 100` | 121 ns | 848 ns | ×0.14 | 114 ns | 124 ns | ×0.92 |

**Core Ultra native**

| Formula | C++ compile | Rust compile | Rust/C++ compile | C++ evaluate | Rust evaluate | Rust/C++ evaluate |
|---|---:|---:|---:|---:|---:|---:|
| `x + y * 2` | 111 ns | 666 ns | ×0.17 | 84.4 ns | 151 ns | ×0.56 |
| `abs(x - y) + max(x, y)` | 158 ns | 1.32 µs | ×0.12 | 188 ns | 344 ns | ×0.55 |
| `x > y && x < 100` | 122 ns | 854 ns | ×0.14 | 113 ns | 124 ns | ×0.91 |

The C++ compiled form is a postfix token vector; Rust uses a Rhai AST. Parsing and
compilation are **O(source bytes)** for these straight-line formulas. Evaluation is
**O(executed tokens/AST operations)** plus variable and function lookup. Retained AST
and token-vector bytes have not yet been measured, so this table makes no memory-use
claim.

## Interchange formatting

Benchmark sources: [Rust table and argument formatting](../../benches/format.rs#L37-L77) ·
[C++ table formatting](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L521-L551) ·
[C++ argument formatting](../../benches/cpp-reference/arguments_benchmark.cpp#L132-L145)

The table fixture has 10,000 rows containing `u64`, one of 16 short strings, and
`i64`. JSON is a complete array of named row objects. CSV includes a header. Both
timed regions begin with an already constructed table.

**M3 Max release**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| table JSON, 10,000 rows | 647 µs | 547 µs | ×1.18 |
| table CSV, 10,000 rows | 655 µs | 390 µs | ×1.68 |
| URI and JSON for 11 arguments | 1.64 µs | 0.886 µs | ×1.85 |

**Core Ultra portable**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| table JSON, 10,000 rows | 460 µs | 766 µs | ×0.60 |
| table CSV, 10,000 rows | 525 µs | 286 µs | ×1.83 |
| URI and JSON for 11 arguments | 0.969 µs | 0.521 µs | ×1.86 |

**Core Ultra native**

| Workload | C++ | Rust | Rust/C++ |
|---|---:|---:|---:|
| table JSON, 10,000 rows | 506 µs | 803 µs | ×0.63 |
| table CSV, 10,000 rows | 570 µs | 283 µs | ×2.01 |
| URI and JSON for 11 arguments | 0.952 µs | 0.517 µs | ×1.84 |

The Rust table writers stream into one output buffer. JSON uses `serde_json` for
scalar escaping; CSV uses the `csv` state machine and stack-backed `itoa`/`ryu`
numeric text. The argument JSON workload additionally checks for duplicate names,
and both Rust argument formats reject unnamed entries rather than silently omitting
them. All three workloads are **O(values + output bytes)**.

## SQLite table materialization

Benchmark sources: [Rust `query`](../../benches/sqlite.rs#L41-L70) ·
[C++ `QueryTableSchema`](../../benches/cpp-reference/sqlite_benchmark.cpp#L128-L145)

The C++ fixture uses bundled SQLite 3.53.2. Rust uses bundled SQLite 3.51.3 from
`libsqlite3-sys` 0.37, the newest dependency line in this pass that compiles on the
crate's declared Rust 1.86 minimum. Both use the same in-memory table with columns
`id INTEGER`, `group_name TEXT`, and `value INTEGER`. Setup and inserts occur outside
the timed region. Timing includes statement preparation, row stepping, SQLite storage-
class validation, owned text copies, and materialization into typed column vectors.

The existing C++ SQLite record wrapper is not a valid comparison target because of the
[documented ownership, lifetime, alignment, deletion, and `BLOB` classification defects](../port/cpp-gd-issues.md#sqlite-connection-copies-can-double-close-the-same-handle).
The Google Benchmark fixture therefore uses the SQLite C API and a small typed
structure-of-arrays adapter rather than modifying C++ product code. Rust measures
`query_table_with_schema`, including construction of its `Schema`, null metadata, and
`ahash` column-name index. The two adapters produce the same observable rows but do not
have identical metadata overhead.

Central estimates:

**M3 Max release**

| Rows | C++ | Rust | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 12.6 µs | 13.0 µs | ×0.97 |
| 1,000 | 117 µs | 109 µs | ×1.07 |
| 10,000 | 1.174 ms | 1.074 ms | ×1.09 |

**Core Ultra portable**

| Rows | C++ | Rust | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 7.69 µs | 10.3 µs | ×0.75 |
| 1,000 | 67.8 µs | 86.8 µs | ×0.78 |
| 10,000 | 667 µs | 857 µs | ×0.78 |

**Core Ultra native**

| Rows | C++ | Rust | Rust/C++ |
|---:|---:|---:|---:|
| 100 | 7.63 µs | 10.7 µs | ×0.71 |
| 1,000 | 68.3 µs | 93.2 µs | ×0.73 |
| 10,000 | 674 µs | 912 µs | ×0.74 |

Both implementations are **O(rows + copied text bytes)** time and retain
**O(rows + copied text bytes)** result space. The C++ figures are medians from three
Google Benchmark repetitions; the Rust figures are Criterion mean point estimates.
No memory-use conclusion follows from these timings.

## Regression policy

Store raw benchmark output for investigations, but do not compare absolute timings
from different machines. Review confidence intervals, fixture equivalence, algorithmic
complexity, and allocation behavior. A benchmark change is actionable only when the
work performed and ownership boundary are the same.

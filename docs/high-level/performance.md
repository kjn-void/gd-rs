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

The mixed-numeric maximum section additionally reports explicit-SIMD configurations
built with rustc 1.99.0-nightly (2026-07-14). The M3 uses the same normal release ISA
policy as its stable result. The Core Ultra uses `-C target-cpu=native` and is measured
separately on CPU 0 (Lion Cove) and CPU 4 (Skymont).

The stable Core Ultra processes were pinned to performance core 0 with `taskset -c 0`;
the nightly maximum additionally uses efficiency core 4 as labeled in that section.
Its `intel_pstate` governor reported `powersave`, which still permits demand-based
turbo. The M3 processes were not pinned. These are benchmark snapshots, not thresholds
that can be compared across machines.

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

The mixed-numeric C++ workload also has an optimized assertions-on build:

```sh
cd benches/cpp-reference
cmake --preset release-asserts
cmake --build --preset release-asserts
../../target/cpp-reference/release-asserts/gd_cpp_reference_benchmarks \
  --benchmark_filter=MixedNumeric \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true
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

cmake --preset release-native-asserts
cmake --build --preset release-native-asserts
../../target/cpp-reference/release-native-asserts/gd_cpp_reference_benchmarks \
  --benchmark_filter=MixedNumeric \
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

### Ten-million-row mixed-numeric sheet

Benchmark sources: [Rust `mixed_numeric_statistics`](../../benches/table.rs#L853-L935) ·
[Rust nightly `std::simd` maximum](../../benches/table_nightly_simd.rs#L1-L188) ·
[C++ mixed-numeric benchmarks](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L426-L503)

This fixture models a large spreadsheet with 10,000,000 rows and six fixed columns:
`u8`, `f64`, `u16`, `u64`, `f32`, and `i32`, in that order. Construction allocates the
complete table and inserts every row. Validation calculates average, minimum, maximum,
and median for every column. The timed statistics are average, maximum, and median;
minimum is validated but not timed separately. For the even row count, median is the
mean of the two central values, matching spreadsheet `MEDIAN` behavior.

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

The C++ benchmark was compiled in assertions-off and assertions-on variants using the
[portable and native release presets](../../benches/cpp-reference/CMakePresets.json#L19-L61).
All four binaries and the GD core use `-O3` and no sanitizers; assertions-off builds
additionally use `-DNDEBUG`, and native builds add `-march=native`. Google Benchmark
labels an assertions-on binary as a debug library solely because `NDEBUG` is absent,
but the compile database confirms that optimization remains `-O3`. Rust uses the safe
table API in the ordinary optimized release profile, with `-C target-cpu=native` only
for the named Core Ultra native configuration. No unchecked Rust comparison is
included.

The nightly maximum is reproduced separately; `std::simd` remains an unstable
standard-library API and is not part of the crate's Rust 1.86 compatibility surface:

```sh
# M3 Max
CARGO_TARGET_DIR=target/nightly-simd-release \
  RUSTFLAGS='--cfg nightly_simd' \
  cargo +nightly bench --bench table_nightly_simd

# Core Ultra 5 225H, Lion Cove CPU 0
taskset -c 0 env \
  CARGO_TARGET_DIR=target/nightly-simd-native \
  RUSTFLAGS='--cfg nightly_simd -C target-cpu=native' \
  cargo +nightly bench --bench table_nightly_simd

# Core Ultra 5 225H, Skymont CPU 4
taskset -c 4 env \
  CARGO_TARGET_DIR=target/nightly-simd-native \
  RUSTFLAGS='--cfg nightly_simd -C target-cpu=native' \
  cargo +nightly bench --bench table_nightly_simd

# Core Ultra 5 225H, Skymont CPU 4, optimized C++ baseline
taskset -c 4 \
  target/cpp-reference/release-native/gd_cpp_reference_benchmarks \
  --benchmark_filter='MixedNumeric/Maximum/' \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true
```

Central estimates:

**M3 Max release**

| Build 10,000,000 rows | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| complete table | 341 ms | 336 ms | 212 ms |

**Core Ultra portable**

| Build 10,000,000 rows | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| complete table | 303 ms | 308 ms | 346 ms |

**Core Ultra native**

| Build 10,000,000 rows | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| complete table | 309 ms | 307 ms | 350 ms |

Every bulk operation below scans exactly one column over all 10,000,000 rows; no timing
combines multiple columns or aggregates several operations. C++ values are medians of
three optimized repetitions. Rust values are Criterion means from ten
flat samples. The three Rust paths are `Column::iter` (shown as `iter`),
`Column::for_each_value` (shown as `for_each_value`), and `Column::as_slice::<T>` (shown
as `&[T]`). `iter` repeats storage dispatch for every cell. `for_each_value` selects
storage and nullability once, then presents each value to its callback as `ValueRef`.
The ratio headers state their formulas directly; values above ×1.00 favor the
denominator.

#### Average

**M3 Max release**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 8.190 ms | 11.693 ms | 13.347 ms | 0.178 ms | 0.179 ms | ×75.03 | ×0.99 | ×46.04 | ×45.80 |
| `f64` | 8.194 ms | 11.345 ms | 13.377 ms | 6.492 ms | 6.876 ms | ×2.06 | ×0.94 | ×1.26 | ×1.19 |
| `u16` | 8.189 ms | 11.347 ms | 13.347 ms | 0.704 ms | 0.705 ms | ×18.95 | ×1.00 | ×11.63 | ×11.61 |
| `u64` | 8.225 ms | 11.133 ms | 13.403 ms | 0.948 ms | 0.944 ms | ×14.13 | ×1.00 | ×8.67 | ×8.71 |
| `f32` | 10.890 ms | 11.414 ms | 13.381 ms | 6.715 ms | 6.729 ms | ×1.99 | ×1.00 | ×1.62 | ×1.62 |
| `i32` | 8.229 ms | 11.088 ms | 13.329 ms | 0.703 ms | 0.698 ms | ×18.97 | ×1.01 | ×11.71 | ×11.78 |

**Core Ultra portable**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 13.572 ms | 13.331 ms | 7.258 ms | 1.042 ms | 1.043 ms | ×6.96 | ×1.00 | ×13.02 | ×13.02 |
| `f64` | 50.180 ms | 64.730 ms | 25.842 ms | 4.859 ms | 4.955 ms | ×5.32 | ×0.98 | ×10.33 | ×10.13 |
| `u16` | 13.493 ms | 13.406 ms | 8.616 ms | 1.744 ms | 1.579 ms | ×4.94 | ×1.10 | ×7.74 | ×8.54 |
| `u64` | 13.633 ms | 13.262 ms | 11.420 ms | 2.835 ms | 2.836 ms | ×4.03 | ×1.00 | ×4.81 | ×4.81 |
| `f32` | 48.263 ms | 61.482 ms | 20.900 ms | 5.478 ms | 5.468 ms | ×3.82 | ×1.00 | ×8.81 | ×8.83 |
| `i32` | 13.477 ms | 13.316 ms | 10.077 ms | 1.603 ms | 1.616 ms | ×6.29 | ×0.99 | ×8.41 | ×8.34 |

**Core Ultra native**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 13.546 ms | 13.230 ms | 7.216 ms | 0.341 ms | 0.340 ms | ×21.18 | ×1.00 | ×39.75 | ×39.80 |
| `f64` | 50.791 ms | 63.941 ms | 24.632 ms | 4.826 ms | 5.104 ms | ×5.10 | ×0.95 | ×10.52 | ×9.95 |
| `u16` | 13.501 ms | 13.312 ms | 8.564 ms | 1.083 ms | 1.097 ms | ×7.91 | ×0.99 | ×12.47 | ×12.30 |
| `u64` | 13.639 ms | 13.228 ms | 12.324 ms | 5.477 ms | 5.382 ms | ×2.25 | ×1.02 | ×2.49 | ×2.53 |
| `f32` | 48.014 ms | 65.038 ms | 21.197 ms | 5.064 ms | 5.103 ms | ×4.19 | ×0.99 | ×9.48 | ×9.41 |
| `i32` | 13.506 ms | 13.398 ms | 9.969 ms | 1.364 ms | 1.365 ms | ×7.31 | ×1.00 | ×9.90 | ×9.89 |

#### Maximum

**M3 Max release**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 8.190 ms | 11.999 ms | 13.360 ms | 0.096 ms | 0.096 ms | ×139.38 | ×1.00 | ×85.45 | ×85.40 |
| `f64` | 8.190 ms | 11.653 ms | 13.361 ms | 5.402 ms | 5.335 ms | ×2.47 | ×1.01 | ×1.52 | ×1.54 |
| `u16` | 8.203 ms | 11.989 ms | 13.378 ms | 0.197 ms | 0.197 ms | ×68.05 | ×1.00 | ×41.72 | ×41.65 |
| `u64` | 8.203 ms | 11.463 ms | 13.353 ms | 1.411 ms | 1.404 ms | ×9.46 | ×1.00 | ×5.81 | ×5.84 |
| `f32` | 8.200 ms | 11.456 ms | 13.397 ms | 5.334 ms | 5.332 ms | ×2.51 | ×1.00 | ×1.54 | ×1.54 |
| `i32` | 8.185 ms | 11.464 ms | 13.374 ms | 0.423 ms | 0.429 ms | ×31.64 | ×0.99 | ×19.36 | ×19.08 |

**M3 Max nightly `std::simd`**

| Field | C++ off | Rust stable `&[T]` | Rust nightly `std::simd` | `stable / std::simd` | `C++ off / std::simd` |
|---|---:|---:|---:|---:|---:|
| `u8` | 8.190 ms | 0.096 ms | 0.097 ms | ×0.99 | ×84.18 |
| `f64` | 8.190 ms | 5.335 ms | 1.465 ms | ×3.64 | ×5.59 |
| `u16` | 8.203 ms | 0.197 ms | 0.205 ms | ×0.96 | ×40.05 |
| `u64` | 8.203 ms | 1.404 ms | 1.449 ms | ×0.97 | ×5.66 |
| `f32` | 8.200 ms | 5.332 ms | 0.735 ms | ×7.25 | ×11.15 |
| `i32` | 8.185 ms | 0.429 ms | 0.465 ms | ×0.92 | ×17.59 |

The nightly path explicitly loads 128-bit `Simd` values and uses four independent
vector accumulators, allowing the M3 to overlap reductions instead of serializing every
vector maximum through one dependency chain. This makes the finite `f64` maximum
×3.64 faster than the stable typed-slice loop and the finite `f32` maximum ×7.25
faster. The integer stable loops are already as fast or slightly faster, so explicit
SIMD is not a blanket improvement. The fixture contains no NaNs; adopting this as a
table API would require an explicit floating-point NaN and signed-zero policy rather
than assuming scalar and `simd_max` edge semantics are interchangeable.

**Core Ultra portable**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 13.556 ms | 13.360 ms | 8.333 ms | 0.215 ms | 0.214 ms | ×38.71 | ×1.01 | ×62.98 | ×63.34 |
| `f64` | 33.999 ms | 34.121 ms | 26.366 ms | 8.894 ms | 8.942 ms | ×2.96 | ×0.99 | ×3.82 | ×3.80 |
| `u16` | 13.499 ms | 13.369 ms | 8.599 ms | 0.899 ms | 0.901 ms | ×9.57 | ×1.00 | ×15.02 | ×14.98 |
| `u64` | 13.496 ms | 13.444 ms | 13.601 ms | 3.099 ms | 3.107 ms | ×4.39 | ×1.00 | ×4.35 | ×4.34 |
| `f32` | 32.951 ms | 34.421 ms | 22.754 ms | 8.626 ms | 8.601 ms | ×2.64 | ×1.00 | ×3.82 | ×3.83 |
| `i32` | 13.547 ms | 13.395 ms | 8.776 ms | 1.475 ms | 1.488 ms | ×5.95 | ×0.99 | ×9.19 | ×9.10 |

**Core Ultra native**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 13.585 ms | 13.961 ms | 8.337 ms | 0.207 ms | 0.207 ms | ×40.23 | ×1.00 | ×65.55 | ×65.57 |
| `f64` | 34.449 ms | 34.415 ms | 25.724 ms | 8.866 ms | 8.751 ms | ×2.90 | ×1.01 | ×3.89 | ×3.94 |
| `u16` | 13.484 ms | 13.370 ms | 8.566 ms | 1.715 ms | 1.708 ms | ×4.99 | ×1.00 | ×7.86 | ×7.89 |
| `u64` | 13.502 ms | 13.383 ms | 13.188 ms | 2.893 ms | 2.917 ms | ×4.56 | ×0.99 | ×4.67 | ×4.63 |
| `f32` | 33.449 ms | 34.531 ms | 22.826 ms | 8.539 ms | 8.554 ms | ×2.67 | ×1.00 | ×3.92 | ×3.91 |
| `i32` | 13.486 ms | 13.425 ms | 8.906 ms | 7.422 ms | 7.392 ms | ×1.20 | ×1.00 | ×1.82 | ×1.82 |

**Core Ultra native nightly `std::simd` — CPU 0, Lion Cove**

| Field | C++ off | Rust stable `&[T]` | Rust nightly `std::simd` | `stable / std::simd` | `C++ off / std::simd` |
|---|---:|---:|---:|---:|---:|
| `u8` | 13.585 ms | 0.207 ms | 0.212 ms | ×0.98 | ×64.17 |
| `f64` | 34.449 ms | 8.751 ms | 4.794 ms | ×1.83 | ×7.19 |
| `u16` | 13.484 ms | 1.708 ms | 0.594 ms | ×2.88 | ×22.71 |
| `u64` | 13.502 ms | 2.917 ms | 3.195 ms | ×0.91 | ×4.23 |
| `f32` | 33.449 ms | 8.554 ms | 2.314 ms | ×3.70 | ×14.45 |
| `i32` | 13.486 ms | 7.392 ms | 1.414 ms | ×5.23 | ×9.54 |

**Core Ultra native nightly `std::simd` — CPU 4, Skymont**

| Field | C++ off, CPU 4 | Rust nightly `std::simd`, CPU 4 | `C++ off / std::simd` | Rust `CPU 0 / CPU 4` |
|---|---:|---:|---:|---:|
| `u8` | 11.763 ms | 0.144 ms | ×81.62 | ×1.47 |
| `f64` | 23.554 ms | 3.603 ms | ×6.54 | ×1.33 |
| `u16` | 11.626 ms | 0.752 ms | ×15.46 | ×0.79 |
| `u64` | 11.618 ms | 3.101 ms | ×3.75 | ×1.03 |
| `f32` | 23.546 ms | 2.364 ms | ×9.96 | ×0.98 |
| `i32` | 11.616 ms | 2.076 ms | ×5.59 | ×0.68 |

Both Rust runs use the same native binary and rustc nightly. CPU 0 and CPU 4 were
pinned with `taskset`; `lscpu` reports maximum frequencies of 4.9 and 4.4 GHz for
their respective core groups. The last column is CPU 0 time divided by CPU 4 time, so
values above ×1.00 favor Skymont. Skymont leads for `u8`, `f64`, and narrowly `u64`,
while Lion Cove leads for `u16`, `i32`, and narrowly `f32`. This is not a uniform
“P-core faster than E-core” result: element width and the generated reduction sequence
interact differently with the two microarchitectures. The Skymont C++ values are a
new assertions-off `-march=native` baseline pinned to CPU 4; the Lion Cove C++ values
are the CPU 0 native baseline already shown above.

#### Harvest cost and maximum over one reused vector

Benchmark sources: [C++ `MixedNumeric/HarvestCost/u8` and
`MixedNumeric/MaximumReusedHarvest/u8`](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L514-L549) ·
[Rust typed-slice maximum](../../benches/table.rs#L903-L935)

This additional `u8` experiment separates the two phases that `harvest<T>` introduces.
The harvest row includes allocating a 10,000,000-element `std::vector<u8>`, gathering
the fixed-stride cells into it, and destroying the vector. The maximum row reuses one
vector harvested before timing and scans only its contiguous values. Rust has no
harvest phase because the table already owns the column as a typed vector; its value is
the existing `&[u8]` maximum measurement above.

**M3 Max release**

| Phase | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| harvest materialization | 18.494 ms | 21.502 ms | n/a: already column-major |
| maximum over reused contiguous values | 0.095 ms | 0.095 ms | 0.096 ms |

**Core Ultra portable**

| Phase | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| harvest materialization | 16.193 ms | 32.222 ms | n/a: already column-major |
| maximum over reused contiguous values | 0.209 ms | 0.207 ms | 0.214 ms |

**Core Ultra native**

| Phase | C++ assertions off | C++ assertions on | Rust |
|---|---:|---:|---:|
| harvest materialization | 15.782 ms | 29.813 ms | n/a: already column-major |
| maximum over reused contiguous values | 0.214 ms | 0.214 ms | 0.207 ms |

For a single maximum, harvest plus the contiguous scan is slower than the direct C++
maximum in every configuration. Once materialized, however, C++ and Rust perform the
contiguous `u8` maximum at effectively the same speed. This supports harvest as an
amortization strategy only when several later operations reuse the vector.

There is also a correctness blocker in this exact fixture: `harvest` calls
`cell_get_variant_view`, whose fixed 8-byte path does `*(uint64_t*)puRowValue`. The
`f64` starts at offset 4, so that is an unaligned typed dereference and C++ undefined
behavior. The measured harvest experiment therefore uses the safely aligned `u8`
column; `harvest<double>` should not be benchmarked here until that load is made
source-level defined.

#### Median

**M3 Max release**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 42.100 ms | 43.688 ms | 22.863 ms | 14.755 ms | 9.590 ms | ×1.55 | ×1.54 | ×2.85 | ×4.39 |
| `f64` | 14.123 ms | 20.770 ms | 25.641 ms | 18.363 ms | 13.661 ms | ×1.40 | ×1.34 | ×0.77 | ×1.03 |
| `u16` | 42.931 ms | 46.654 ms | 23.492 ms | 15.294 ms | 10.192 ms | ×1.54 | ×1.50 | ×2.81 | ×4.21 |
| `u64` | 14.004 ms | 20.737 ms | 25.435 ms | 17.364 ms | 13.084 ms | ×1.46 | ×1.33 | ×0.81 | ×1.07 |
| `f32` | 13.934 ms | 17.307 ms | 25.058 ms | 17.434 ms | 12.043 ms | ×1.44 | ×1.45 | ×0.80 | ×1.16 |
| `i32` | 12.583 ms | 16.048 ms | 23.222 ms | 14.797 ms | 9.988 ms | ×1.57 | ×1.48 | ×0.85 | ×1.26 |

**Core Ultra portable**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 41.237 ms | 39.967 ms | 32.061 ms | 29.611 ms | 25.336 ms | ×1.08 | ×1.17 | ×1.39 | ×1.63 |
| `f64` | 58.054 ms | 60.141 ms | 47.876 ms | 49.115 ms | 44.599 ms | ×0.97 | ×1.10 | ×1.18 | ×1.30 |
| `u16` | 46.829 ms | 47.176 ms | 22.773 ms | 19.440 ms | 14.277 ms | ×1.17 | ×1.36 | ×2.41 | ×3.28 |
| `u64` | 57.170 ms | 57.862 ms | 44.764 ms | 42.803 ms | 43.126 ms | ×1.05 | ×0.99 | ×1.34 | ×1.33 |
| `f32` | 45.881 ms | 47.096 ms | 34.070 ms | 31.937 ms | 27.955 ms | ×1.07 | ×1.14 | ×1.44 | ×1.64 |
| `i32` | 42.315 ms | 42.972 ms | 31.382 ms | 29.124 ms | 25.792 ms | ×1.08 | ×1.13 | ×1.45 | ×1.64 |

**Core Ultra native**

| Field | C++ off | C++ on | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ off / for_each_value` | `C++ off / &[T]` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `u8` | 42.171 ms | 45.593 ms | 31.831 ms | 29.459 ms | 25.172 ms | ×1.08 | ×1.17 | ×1.43 | ×1.68 |
| `f64` | 58.628 ms | 59.738 ms | 48.518 ms | 49.968 ms | 45.014 ms | ×0.97 | ×1.11 | ×1.17 | ×1.30 |
| `u16` | 47.734 ms | 47.353 ms | 22.726 ms | 19.599 ms | 14.325 ms | ×1.16 | ×1.37 | ×2.44 | ×3.33 |
| `u64` | 56.668 ms | 57.129 ms | 45.608 ms | 41.962 ms | 42.513 ms | ×1.09 | ×0.99 | ×1.35 | ×1.33 |
| `f32` | 45.935 ms | 47.271 ms | 34.650 ms | 34.437 ms | 27.803 ms | ×1.01 | ×1.24 | ×1.33 | ×1.65 |
| `i32` | 42.482 ms | 43.349 ms | 31.462 ms | 28.953 ms | 25.577 ms | ×1.09 | ×1.13 | ×1.47 | ×1.66 |

The C++ benchmark copies each cell into an aligned local value with fixed-size
`memcpy`; this retains `cell_get` assertions in the assertions-on build while avoiding
undefined behavior from dereferencing the `f64` field at offset 4. This is
source-level defined behavior, not sanitizer instrumentation.

C++ `harvest<T>` was not substituted for these individual scans. It first allocates a
`std::vector<T>` and gathers the strided cells through `cell_get_variant_view`, so one
average or maximum would pay both the gather and the later contiguous scan. The `u8`
experiment above reports those phases separately. Harvesting once and reusing that
vector for average, minimum, maximum, and median could amortize the gather and may
improve that combined pipeline, but it would no longer measure each statistic directly
over the table.

`Column::iter` performs runtime storage dispatch, bounds checking, and dynamic tag
reconstruction for every cell. `Column::for_each_value` hoists storage and nullability
dispatch out of the loop while retaining a `ValueRef` callback. For simple averages
and maxima it is close to the typed slice in almost every case. This is consistent with
LLVM inlining the callback and eliminating the known `ValueRef` variant; the timings
alone do not prove which stable loops were auto-vectorized. `Column::as_slice::<T>`
remains the explicit way to guarantee a monomorphic contiguous input. The separate
nightly maximum uses `std::simd` explicitly and is not one of these three stable paths.

The stable Maximum paths use the same explicit accumulator loop for all three Rust APIs
so the access mechanisms are compared with the same reduction shape. The float fixture
contains no NaNs; both languages use ordinary finite comparisons in these bulk cases.
Table ordering elsewhere continues to use `total_cmp`. The nightly SIMD path is a
separate algorithmic comparison with four vector accumulators.

Median copies one scratch vector and partitions it with `std::nth_element` or
`select_nth_unstable`. A typed slice iterator can be collected with a bulk copy,
whereas the callback writes one reconstructed value at a time; the branch-heavy
selection phase then dominates both paths. The M3 shows the clearest typed-slice
advantage. On the Core Ultra, selection cost narrows the access-path differences and
the `u64` callback and slice results are effectively tied.

#### What the two hosts say about SIMD and bandwidth

The additional host does not support a single-cause explanation. The Core Ultra native
configuration enables AVX2 but not AVX-512. Adding that ISA target leaves the C++
maximum loops effectively unchanged: portable/native times are 13.556/13.585 ms for
`u8` and 33.999/34.449 ms for `f64`. The row-strided `memcpy` reduction shape therefore
does not benefit from the wider enabled ISA in this benchmark.

Rust demonstrates that native code generation is operation-specific. For average,
the `u8` typed slice improves from 1.043 to 0.340 ms, while `f64` remains about 5 ms.
For maximum, however, `u8` remains about 0.21 ms and `f64` remains about 8.8 ms. Native
`u16` and `i32` maximum are slower than the portable build; a second isolated run of
the entire native mixed-numeric group reproduced those results. `target-cpu=native`
therefore is not a universal speed switch even for contiguous slices; LLVM selects
different reduction strategies for different operations and element types.

The M3 nightly experiment isolates a different variable: an explicit four-accumulator
SIMD reduction with ordinary release ISA defaults. Its large `f32` and `f64` gains,
alongside parity or small regressions for integers, show that the stable floating-point
maximum shape was leaving reduction parallelism unavailable while the integer cases
were not. This evidence is specific to finite maximum reduction and does not generalize
to average or median.

The Core Ultra native run confirms the floating-point result on a second ISA: explicit
SIMD reduces the Lion Cove `f64` maximum from 8.751 to 4.794 ms and `f32` from 8.554
to 2.314 ms. Its integer outcome is again operation- and code-generation-specific.
The separate Skymont sample also prevents treating core class as a scalar multiplier:
it beats Lion Cove for some element widths and loses for others under the identical
binary.

Memory traffic still cannot be removed from the explanation. A Rust `u8` column is
10 MB and its `f64` column is 80 MB. A 128-bit vector also holds eight times as many
`u8` values as `f64` values, and a 256-bit vector preserves that same 8:1 lane ratio.
Thus element width changes both values processed per vector operation and bytes read by
a factor of eight. On the M3, the typed-slice maximum takes 0.096 ms for `u8` and
5.335 ms for `f64`, a ×55.6 time ratio that is consistent with both effects combining;
it does not identify either one in isolation. The C++ column scan, by contrast, walks
the same 320 MB row-address span for every type because each next value is 32 bytes
away. The measured evidence supports “layout plus reduction code generation plus the
memory hierarchy,” not bandwidth alone and not SIMD operations per cycle alone.

#### Row-storage accounting

This is capacity accounting, not RSS:

| Implementation | Row model | Bytes per row | Row-storage bytes | MiB | Relative bytes |
|---|---|---:|---:|---:|---:|
| C++ | physical fixed-stride row | 32 | 320,000,000 | 305.18 | ×1.00 |
| Rust | virtual sum across typed vectors | 27 | 270,000,000 | 257.49 | ×0.84 |

These figures cover row-bearing payload capacity and exclude allocator bookkeeping and
the small fixed table/schema objects. The C++ fixture checks `size_reserved_total()`.
The Rust figure is the requested row capacity multiplied by the sum of the six element
widths; it is logical payload accounting, not allocator-reported usable size. GD aligns
the start of every field to four bytes, not to that field's natural alignment. Its
physical 32-byte row is:

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

The `MB` labels in the diagram are decimal millions of bytes; the table above also
shows the binary MiB equivalents.

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

`Column::as_slice::<T>` exposes dense required columns without exposing the storage
enum itself. Borrowing ties the slice lifetime to the table and prevents mutation while
it is in use. Nullable columns deliberately reject this API until they have an explicit
typed nullable view; callers can continue using `ValueRef` iteration for them.

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

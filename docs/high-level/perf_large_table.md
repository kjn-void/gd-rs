# Large-table performance

This companion to [Benchmark methodology and results](performance.md) collects
fixtures whose row counts, memory footprint, or run time make them easier to
understand separately from the smaller API microbenchmarks.

## Ten-million-row mixed-numeric sheet

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

The optimized assertions-on C++ runs are reproduced with:

```sh
cd benches/cpp-reference
cmake --preset release-asserts
cmake --build --preset release-asserts
../../target/cpp-reference/release-asserts/gd_cpp_reference_benchmarks \
  --benchmark_filter=MixedNumeric \
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
```

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

#### Harvest cost and operations over one reused vector

Benchmark sources: [C++ `MixedNumeric/HarvestCost/u8`,
`MixedNumeric/MaximumReusedHarvest/u8`, and
`MixedNumeric/MedianReusedHarvest/u8`](../../benches/cpp-reference/table_column_buffer_benchmark.cpp#L525-L578) ·
[Rust typed-slice maximum](../../benches/table.rs#L550-L583),
[typed-slice median](../../benches/table.rs#L620-L645), and
[their `u8` registrations](../../benches/table.rs#L885-L900).

This additional `u8` experiment separates the materialization that `harvest<T>`
introduces from operations over the resulting contiguous vector. The harvest row
includes allocating a 10,000,000-element `std::vector<u8>`, gathering the fixed-stride
cells into it, and destroying the vector. Both C++ operation rows harvest one vector
before timing and reuse it as their source. Maximum scans that vector directly. Median
must copy the reusable source into a fresh scratch vector on every iteration because
`std::nth_element` mutates its input. Rust has no harvest phase because the table
already owns the column as a typed vector; its `&[u8]` median performs the corresponding
scratch copy before `select_nth_unstable`.

The ratio column is C++ assertions-off time divided by Rust time, so values above
×1.00 favor Rust. Table construction, C++ harvest construction, and correctness checks
are outside the timed operation rows.

**M3 Max release**

| Phase | C++ assertions off | C++ assertions on | Rust | C++ off / Rust |
|---|---:|---:|---:|---:|
| harvest materialization | 18.494 ms | 21.502 ms | n/a: already column-major | n/a |
| maximum over reused contiguous values | 0.095 ms | 0.095 ms | 0.097 ms | ×0.98 |
| median from reused contiguous values | 30.1 ms | 30.0 ms | 9.558 ms | ×3.15 |

**Core Ultra portable baseline**

| Phase | C++ assertions off | C++ assertions on | Rust | C++ off / Rust |
|---|---:|---:|---:|---:|
| harvest materialization | 16.193 ms | 32.222 ms | n/a: already column-major | n/a |
| maximum over reused contiguous values | 0.218 ms | 0.218 ms | 0.217 ms | ×1.00 |
| median from reused contiguous values | 26.4 ms | 26.4 ms | 25.361 ms | ×1.04 |

For a single maximum, harvest plus the contiguous scan remains slower than the direct
C++ maximum. Once materialized, however, C++ and Rust perform the contiguous `u8`
maximum at effectively the same speed on both CPUs. This supports harvest as an
amortization strategy only when several later operations reuse the vector.

Reusing the harvest reduces the M3 C++ median from 42.100 to 30.1 ms, but Rust remains
×3.15 faster. On the Core Ultra it reduces C++ from 41.237 to 26.4 ms and leaves C++
and Rust within 4%. The table-layout gather was therefore only part of the original M3
median gap. The remaining M3-specific difference is consistent with libc++
`std::nth_element` and Rust's `select_nth_unstable` generating materially different
partition code for this pseudo-random, 251-value fixture; it is not a general
language-level median advantage. The new 225H measurement deliberately uses only the
portable baseline because `-march=native` did not improve this benchmark suite
consistently.

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

## Hundred-million-row parallel recursive transform

Benchmark sources: [Rust/Rayon `par_benchmark`](../../benches/par_benchmark.rs) ·
[C++/OpenMP `par_benchmark`](../../benches/cpp-reference/par_benchmark.cpp) ·
[OpenMP build configuration](../../benches/cpp-reference/CMakeLists.txt#L54-L89).

This fixture constructs a closed-schema table with 100,000,000 rows and two required
`u32` columns named `arg` and `result`. Row `r` starts as
`arg = (r mod 23) + 1` and `result = 0`, giving a repeating distribution over
`1..=23`. The timed transform reads each stored `arg`, evaluates the deliberately
naive recursive definition below, and stores the value in the corresponding `result`
cell:

```text
fib(n) = n                         when n < 2
fib(n) = fib(n - 1) + fib(n - 2)  otherwise
```

Both implementations prevent inlining of the recursive function. Rust borrows the
required columns as disjoint `&[u32]` and `&mut [u32]` slices, then zips Rayon parallel
slice iterators. C++ uses an OpenMP `schedule(dynamic, 4096)` loop and fixed-size
`memcpy` reads and writes on distinct row cells. Dynamic 4,096-row chunks let faster
cores claim more work on asymmetric CPUs without paying one scheduling operation per
row. The `u32` fields are naturally aligned in this
fixture, but `memcpy` also keeps the access source-level defined independently of typed
pointer alignment. An iterative Fibonacci implementation validates every result in a
separate parallel pass after timing.

The default programs are intentionally one-pass harnesses rather than adaptive
Criterion or Google Benchmark cases: repeating this exponential workload
automatically would make an already long run impractical. `GD_PAR_ROWS` and
`GD_PAR_MAX_ARG` can reduce the fixture for calibration. `GD_PAR_OPERATION=square`
selects the cheaper transform used below, while `GD_PAR_WARMUPS` and
`GD_PAR_REPETITIONS` control repeated execution. Release execution without those
variables uses the exact one-pass 100M-row, `1..=23` Fibonacci defaults. Cargo's
debug/test execution automatically substitutes 400 rows and `1..=20`, so the
repository's all-target test command remains bounded.

```sh
# Rust, release defaults: 100M rows, arg 1..=23
cargo bench --bench par_benchmark

# C++, portable optimized baseline with OpenMP
cd benches/cpp-reference
cmake --preset release
cmake --build --preset release --target gd_cpp_reference_par_benchmark
../../target/cpp-reference/release/gd_cpp_reference_par_benchmark

# Example calibration override, identical in either implementation
GD_PAR_ROWS=4000 GD_PAR_MAX_ARG=23 <benchmark executable>
```

The CMake target uses the compiler OpenMP target on Linux. Apple Clang requires
Homebrew `libomp`; configuration locates it and supplies `-Xpreprocessor -fopenmp`,
the include directory, and the runtime library automatically. If OpenMP is unavailable,
CMake skips only this separate executable and leaves the other C++ references usable.

### Storage

Both table payloads reserve exactly 800,000,000 bytes (762.94 MiB), excluding small
schema/table objects and allocator bookkeeping:

```text
C++: 100M physical rows × [arg: u32 | result: u32] = 100M × 8 bytes = 800 MB

Rust: Vec<u32> arg     = 100M × 4 bytes = 400 MB
      Vec<u32> result  = 100M × 4 bytes = 400 MB
                                           total = 800 MB
```

The Rust figure is the sum of two separate column allocations, not a physical
eight-byte row. The schema rejects unknown fields, so it has no per-row extras
sidecar. Neither timed transform allocates a 100M-element scratch vector.

### Results

These are literal one-pass measurements over 100,000,000 rows. Both programs used
their complete default thread pools and validated every `result` cell after the timed
transform. The C++ binary is the optimized assertions-off portable release build;
Rust uses the ordinary portable release profile. Neither uses native-ISA flags or
sanitizer instrumentation.

| Host | Threads | C++ build | Rust build | C++ transform | Rust transform | C++ / Rust transform |
|---|---:|---:|---:|---:|---:|---:|
| M3 Max | 16 | 1.264 s | 0.579 s | 59.194 s | 62.677 s | ×0.94 |
| Core Ultra 5 225H | 14 | 0.953 s | 0.877 s | 54.530 s | 64.325 s | ×0.85 |

The last column divides C++ transform time by Rust transform time, so values above
×1.00 favor Rust. Equivalent transform throughput is:

| Host | C++/OpenMP | Rust/Rayon |
|---|---:|---:|
| M3 Max | 1.689 million rows/s | 1.595 million rows/s |
| Core Ultra 5 225H | 1.834 million rows/s | 1.555 million rows/s |

By throughput, C++ is about 5.9% faster on the M3 Max and 18.0% faster on the Core
Ultra. The Core Ultra C++ transform previously took 87.363 s with equal-size static
partitions; dynamic chunks reduce it to 54.530 s by allowing its faster cores to claim
more work. That discarded static result is useful only as a scheduler warning, not as
the C++ baseline on an asymmetric CPU.

This is not a table-bandwidth result. Each pass moves only about 1.6 GB of table
payload—one four-byte input and one four-byte output per row—over roughly one minute.
That is tens of MB/s, far below either machine's memory bandwidth, because `fib`
performs many recursive calls for each eight bytes of table traffic.

The layouts still differ: C++ reads and writes two fields in one eight-byte physical
row, while Rust reads the `arg` and writes the `result` through separate contiguous
vectors. With this arithmetic intensity, however, those access patterns are a small
part of elapsed time. The result should be read as a comparison of the complete
dynamic-OpenMP and Rayon transform paths generated by these toolchains, not as a
universal language or scheduler ranking.

## One-million-row parallel square transform

Benchmark sources: [Rust/Rayon `par_benchmark`](../../benches/par_benchmark.rs) ·
[C++/OpenMP `par_benchmark`](../../benches/cpp-reference/par_benchmark.cpp) ·
[OpenMP build configuration](../../benches/cpp-reference/CMakeLists.txt#L54-L89).

This variation keeps the same closed schema and parallel access paths but reduces the
table to 1,000,000 rows and replaces recursive Fibonacci with one unsigned
multiplication:

```text
result[row] = arg[row] * arg[row]
```

The two required `u32` columns reserve 8,000,000 bytes (7.63 MiB). Both programs use
their complete default worker pools. Every timed repetition launches a new Rayon or
OpenMP parallel operation, so the warmed measurement retains parallel scheduling
overhead rather than timing a single long-lived worker loop.

Run the first-transform case with:

```sh
GD_PAR_ROWS=1000000 GD_PAR_OPERATION=square \
  GD_PAR_WARMUPS=0 GD_PAR_REPETITIONS=1 <benchmark executable>
```

Run the warmed repeated case with:

```sh
GD_PAR_ROWS=1000000 GD_PAR_OPERATION=square \
  GD_PAR_WARMUPS=5 GD_PAR_REPETITIONS=1000 <benchmark executable>
```

The results below are medians of five process-level samples. “First transform” comes
from five fresh processes and therefore includes initial parallel-runtime activation
and cold-state effects. “Warm transform” is the median per-pass time from five
1,000-repetition processes after five untimed warmups. Build time is taken from the
fresh-process samples. Validation runs after timing.

| Host | Workers | C++ build | Rust build | C++ first transform | Rust first transform | C++ warm transform | Rust warm transform |
|---|---:|---:|---:|---:|---:|---:|---:|
| M3 Max | 16 | 12.470 ms | 6.239 ms | 0.872 ms | 0.521 ms | 0.168 ms | 0.222 ms |
| Core Ultra 5 225H | 14 | 9.754 ms | 8.292 ms | 0.515 ms | 0.645 ms | 0.176 ms | 0.101 ms |

The transform is no longer the dominant end-to-end cost. Adding the median build and
first-transform measurements gives about 13.34 ms for C++ versus 6.76 ms for Rust on
the M3 Max, and 10.27 ms for C++ versus 8.94 ms for Rust on the Core Ultra. Those sums
exclude the separate validation pass.

The warmed comparison reverses between hosts: OpenMP is about 1.32 times as fast as
Rayon on the M3 Max, while Rayon is about 1.75 times as fast as OpenMP on the Core
Ultra. At only 0.1–0.9 ms per pass, parallel-region entry and exit, chunk scheduling,
heterogeneous-core placement, cache state, and ordinary timing noise are material
parts of the result. This is consequently a useful parallel-overhead case, but not a
stable ranking of the languages or a measurement of scalar multiplication throughput.

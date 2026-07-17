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

These results were refreshed on 2026-07-17 from `gd-rs` commit
`e312813db841a4debc63b19b05d0d1031be566fe` and GD commit
`3d1e112b0806845854e863f9fd8288a2f79ba378`. The configurations match
[performance.md](performance.md): an unpinned M3 Max release run and two Core Ultra
runs using the same portable binaries pinned to CPU 0 (Lion Cove P-core) or CPU 4
(Skymont E-core). No `-march=native`, `target-cpu=native`, sanitizers, or unchecked
Rust paths are included.

The M3 Max used Apple Clang 21.0.0 and rustc 1.97.0. The Core Ultra used GCC
13.3.0 and rustc 1.97.0; the explicit-SIMD runs used rustc 1.99.0-nightly
(`da80ed070`, 2026-07-14).

The C++ benchmark and GD core use the optimized portable release preset with `-O3`
and `-DNDEBUG`, matching GD's release configuration. Rust uses the safe table API in
Cargo's ordinary optimized release profile.

The optimized C++ and Rust runs are reproduced with:

```sh
cd benches/cpp-reference
cmake --preset release
cmake --build --preset release
../../target/cpp-reference/release/gd_cpp_reference_benchmarks \
  --benchmark_filter=MixedNumeric \
  --benchmark_min_time=1s \
  --benchmark_repetitions=3 \
  --benchmark_report_aggregates_only=true

cd ../..
cargo bench --bench table -- MixedNumeric

# Core Ultra: rerun the two benchmark invocations above with `taskset -c 0`
# for the P-core or `taskset -c 4` for the E-core.
```

The nightly maximum is reproduced separately; `std::simd` remains an unstable
standard-library API and is not part of the crate's Rust 1.86 compatibility surface:

```sh
# M3 Max
CARGO_TARGET_DIR=target/nightly-simd-release \
  RUSTFLAGS='--cfg nightly_simd' \
  cargo +nightly bench --bench table_nightly_simd

# Core Ultra 5 225H, Skymont CPU 4, portable ISA policy
taskset -c 4 env \
  CARGO_TARGET_DIR=target/nightly-simd-release \
  RUSTFLAGS='--cfg nightly_simd' \
  cargo +nightly bench --bench table_nightly_simd
```

Central estimates:

**M3 Max release**

| Build 10,000,000 rows | C++ | Rust |
| --- | ---: | ---: |
| complete table | 315 ms | 212 ms |

**Core Ultra P-core (CPU 0)**

| Build 10,000,000 rows | C++ | Rust |
| --- | ---: | ---: |
| complete table | 303 ms | 322 ms |

**Core Ultra E-core (CPU 4)**

| Build 10,000,000 rows | C++ | Rust |
| --- | ---: | ---: |
| complete table | 347 ms | 396 ms |

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

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 8.100 ms | 13.354 ms | 0.179 ms | 0.178 ms | ×74.65 | ×1.00 | ×45.28 | ×45.41 |
| `f64` | 8.130 ms | 13.372 ms | 6.943 ms | 6.996 ms | ×1.93 | ×0.99 | ×1.17 | ×1.16 |
| `u16` | 8.050 ms | 13.367 ms | 0.703 ms | 0.705 ms | ×19.00 | ×1.00 | ×11.44 | ×11.42 |
| `u64` | 8.110 ms | 13.387 ms | 0.969 ms | 0.991 ms | ×13.82 | ×0.98 | ×8.37 | ×8.18 |
| `f32` | 10.800 ms | 13.360 ms | 7.079 ms | 7.008 ms | ×1.89 | ×1.01 | ×1.53 | ×1.54 |
| `i32` | 8.120 ms | 16.065 ms | 0.703 ms | 0.714 ms | ×22.86 | ×0.99 | ×11.55 | ×11.38 |

**Core Ultra P-core (CPU 0)**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 13.500 ms | 7.320 ms | 1.067 ms | 1.068 ms | ×6.86 | ×1.00 | ×12.66 | ×12.64 |
| `f64` | 47.300 ms | 25.199 ms | 4.921 ms | 4.978 ms | ×5.12 | ×0.99 | ×9.61 | ×9.50 |
| `u16` | 13.400 ms | 8.580 ms | 1.663 ms | 1.691 ms | ×5.16 | ×0.98 | ×8.06 | ×7.92 |
| `u64` | 13.500 ms | 12.493 ms | 2.839 ms | 2.836 ms | ×4.40 | ×1.00 | ×4.76 | ×4.76 |
| `f32` | 51.200 ms | 20.902 ms | 5.440 ms | 5.398 ms | ×3.84 | ×1.01 | ×9.41 | ×9.48 |
| `i32` | 13.400 ms | 10.107 ms | 1.615 ms | 1.620 ms | ×6.26 | ×1.00 | ×8.30 | ×8.27 |

**Core Ultra E-core (CPU 4)**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 11.700 ms | 14.016 ms | 1.173 ms | 1.174 ms | ×11.95 | ×1.00 | ×9.97 | ×9.97 |
| `f64` | 28.300 ms | 23.646 ms | 4.719 ms | 4.719 ms | ×5.01 | ×1.00 | ×6.00 | ×6.00 |
| `u16` | 11.700 ms | 11.821 ms | 1.827 ms | 1.851 ms | ×6.47 | ×0.99 | ×6.40 | ×6.32 |
| `u64` | 11.600 ms | 11.824 ms | 2.541 ms | 2.544 ms | ×4.65 | ×1.00 | ×4.57 | ×4.56 |
| `f32` | 28.300 ms | 23.360 ms | 4.883 ms | 4.938 ms | ×4.78 | ×0.99 | ×5.80 | ×5.73 |
| `i32` | 11.600 ms | 11.857 ms | 3.299 ms | 3.330 ms | ×3.59 | ×0.99 | ×3.52 | ×3.48 |

#### Maximum

**M3 Max release**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 8.110 ms | 13.359 ms | 0.096 ms | 0.096 ms | ×139.05 | ×1.00 | ×84.42 | ×84.30 |
| `f64` | 8.040 ms | 13.379 ms | 5.377 ms | 5.347 ms | ×2.49 | ×1.01 | ×1.50 | ×1.50 |
| `u16` | 8.090 ms | 13.354 ms | 0.203 ms | 0.209 ms | ×65.64 | ×0.97 | ×39.76 | ×38.75 |
| `u64` | 8.080 ms | 13.367 ms | 1.417 ms | 1.410 ms | ×9.43 | ×1.01 | ×5.70 | ×5.73 |
| `f32` | 8.050 ms | 13.398 ms | 5.357 ms | 5.334 ms | ×2.50 | ×1.00 | ×1.50 | ×1.51 |
| `i32` | 8.090 ms | 13.424 ms | 0.448 ms | 0.442 ms | ×29.96 | ×1.01 | ×18.06 | ×18.30 |

**M3 Max nightly `std::simd`**

| Field | C++ | Rust stable `&[T]` | Rust nightly `std::simd` | `stable / std::simd` | `C++ / std::simd` |
|---|---:|---:|---:|---:|---:|
| `u8` | 8.110 ms | 0.096 ms | 0.096 ms | ×1.00 | ×84.21 |
| `f64` | 8.040 ms | 5.347 ms | 1.445 ms | ×3.70 | ×5.56 |
| `u16` | 8.090 ms | 0.209 ms | 0.202 ms | ×1.03 | ×40.07 |
| `u64` | 8.080 ms | 1.410 ms | 1.439 ms | ×0.98 | ×5.61 |
| `f32` | 8.050 ms | 5.334 ms | 0.724 ms | ×7.36 | ×11.11 |
| `i32` | 8.090 ms | 0.442 ms | 0.451 ms | ×0.98 | ×17.95 |

The nightly path explicitly loads 128-bit `Simd` values and uses four independent
vector accumulators, allowing the M3 to overlap reductions instead of serializing every
vector maximum through one dependency chain. This makes the finite `f64` maximum
×3.70 faster than the stable typed-slice loop and the finite `f32` maximum ×7.36
faster. The integer stable loops are already as fast or slightly faster, so explicit
SIMD is not a blanket improvement. The fixture contains no NaNs; adopting this as a
table API would require an explicit floating-point NaN and signed-zero policy rather
than assuming scalar and `simd_max` edge semantics are interchangeable.

**Core Ultra P-core (CPU 0)**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 13.500 ms | 8.368 ms | 0.211 ms | 0.210 ms | ×39.72 | ×1.00 | ×64.08 | ×64.25 |
| `f64` | 34.700 ms | 26.587 ms | 8.749 ms | 8.935 ms | ×3.04 | ×0.98 | ×3.97 | ×3.88 |
| `u16` | 13.300 ms | 8.607 ms | 0.948 ms | 0.990 ms | ×9.08 | ×0.96 | ×14.04 | ×13.44 |
| `u64` | 13.300 ms | 13.911 ms | 3.108 ms | 3.105 ms | ×4.48 | ×1.00 | ×4.28 | ×4.28 |
| `f32` | 34.800 ms | 22.774 ms | 8.621 ms | 8.712 ms | ×2.64 | ×0.99 | ×4.04 | ×3.99 |
| `i32` | 13.500 ms | 9.118 ms | 1.494 ms | 1.478 ms | ×6.10 | ×1.01 | ×9.03 | ×9.14 |

**Core Ultra E-core (CPU 4)**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 11.800 ms | 12.839 ms | 0.175 ms | 0.175 ms | ×73.50 | ×1.00 | ×67.55 | ×67.51 |
| `f64` | 23.600 ms | 23.628 ms | 4.719 ms | 4.718 ms | ×5.01 | ×1.00 | ×5.00 | ×5.00 |
| `u16` | 11.600 ms | 12.886 ms | 1.109 ms | 1.083 ms | ×11.62 | ×1.02 | ×10.46 | ×10.71 |
| `u64` | 11.600 ms | 13.068 ms | 3.129 ms | 2.619 ms | ×4.18 | ×1.19 | ×3.71 | ×4.43 |
| `f32` | 23.600 ms | 23.377 ms | 4.757 ms | 4.870 ms | ×4.91 | ×0.98 | ×4.96 | ×4.85 |
| `i32` | 11.600 ms | 12.984 ms | 3.138 ms | 3.138 ms | ×4.14 | ×1.00 | ×3.70 | ×3.70 |

**Core Ultra E-core (CPU 4) nightly `std::simd`**

| Field | C++ | Rust stable `&[T]` | Rust nightly `std::simd` | `stable / std::simd` | `C++ / std::simd` |
|---|---:|---:|---:|---:|---:|
| `u8` | 11.800 ms | 0.175 ms | 0.148 ms | ×1.18 | ×79.75 |
| `f64` | 23.600 ms | 4.718 ms | 3.533 ms | ×1.34 | ×6.68 |
| `u16` | 11.600 ms | 1.083 ms | 1.097 ms | ×0.99 | ×10.58 |
| `u64` | 11.600 ms | 2.619 ms | 4.444 ms | ×0.59 | ×2.61 |
| `f32` | 23.600 ms | 4.870 ms | 2.791 ms | ×1.74 | ×8.46 |
| `i32` | 11.600 ms | 3.138 ms | 2.777 ms | ×1.13 | ×4.18 |

The nightly E-core run uses the same portable ISA policy as the stable E-core run.
Explicit SIMD improves `u8`, `f64`, `f32`, and `i32`, is effectively neutral for
`u16`, and regresses `u64`. As on the M3, explicit vector syntax is not a universal
speed switch: element width, reduction dependencies, and code generation all matter.

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

The ratio column is C++ time divided by Rust time, so values above
×1.00 favor Rust. Table construction, C++ harvest construction, and correctness checks
are outside the timed operation rows.

**M3 Max release**

| Phase | C++ | Rust | C++ / Rust |
| --- | ---: | ---: | ---: |
| harvest materialization | 18.8 ms | n/a: already column-major | n/a |
| maximum over reused contiguous values | 0.098 ms | 0.096 ms | ×1.02 |
| median from reused contiguous values | 32.2 ms | 9.589 ms | ×3.36 |

**Core Ultra P-core (CPU 0)**

| Phase | C++ | Rust | C++ / Rust |
| --- | ---: | ---: | ---: |
| harvest materialization | 16.3 ms | n/a: already column-major | n/a |
| maximum over reused contiguous values | 0.206 ms | 0.210 ms | ×0.98 |
| median from reused contiguous values | 26.4 ms | 25.278 ms | ×1.04 |

**Core Ultra E-core (CPU 4)**

| Phase | C++ | Rust | C++ / Rust |
| --- | ---: | ---: | ---: |
| harvest materialization | 18.0 ms | n/a: already column-major | n/a |
| maximum over reused contiguous values | 0.180 ms | 0.175 ms | ×1.03 |
| median from reused contiguous values | 29.7 ms | 13.057 ms | ×2.27 |

For a single maximum, harvest plus the contiguous scan remains slower than the direct
C++ maximum. Once materialized, however, C++ and Rust perform the contiguous `u8`
maximum at effectively the same speed in all three configurations. This supports
harvest as an amortization strategy only when several later operations reuse the
vector.

Reusing the harvest reduces the M3 C++ median from 39.0 to 32.2 ms, but Rust remains
×3.36 faster. The P-core leaves C++ and Rust within 4%, while Rust is ×2.27 faster on
the E-core. The table-layout gather is therefore only part of the median gap. The
remaining implementation- and host-specific difference is consistent with
`std::nth_element` and Rust's `select_nth_unstable` generating materially different
partition code for this pseudo-random, 251-value fixture; it is not a general
language-level median advantage. The E-core access-path results were repeated
separately and reproduced within normal run variation.

There is also a correctness blocker in this exact fixture: `harvest` calls
`cell_get_variant_view`, whose fixed 8-byte path does `*(uint64_t*)puRowValue`. The
`f64` starts at offset 4, so that is an unaligned typed dereference and C++ undefined
behavior. The measured harvest experiment therefore uses the safely aligned `u8`
column; `harvest<double>` should not be benchmarked here until that load is made
source-level defined.

#### Median

**M3 Max release**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 39.000 ms | 22.877 ms | 14.784 ms | 9.588 ms | ×1.55 | ×1.54 | ×2.64 | ×4.07 |
| `f64` | 13.700 ms | 25.706 ms | 18.062 ms | 13.472 ms | ×1.42 | ×1.34 | ×0.76 | ×1.02 |
| `u16` | 42.300 ms | 23.233 ms | 15.135 ms | 10.089 ms | ×1.54 | ×1.50 | ×2.79 | ×4.19 |
| `u64` | 13.900 ms | 25.044 ms | 17.034 ms | 12.748 ms | ×1.47 | ×1.34 | ×0.82 | ×1.09 |
| `f32` | 13.600 ms | 24.815 ms | 17.121 ms | 11.799 ms | ×1.45 | ×1.45 | ×0.79 | ×1.15 |
| `i32` | 12.400 ms | 22.912 ms | 14.660 ms | 9.932 ms | ×1.56 | ×1.48 | ×0.85 | ×1.25 |

**Core Ultra P-core (CPU 0)**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 40.700 ms | 31.991 ms | 29.591 ms | 25.278 ms | ×1.08 | ×1.17 | ×1.38 | ×1.61 |
| `f64` | 58.500 ms | 48.429 ms | 50.014 ms | 44.477 ms | ×0.97 | ×1.12 | ×1.17 | ×1.32 |
| `u16` | 46.800 ms | 22.673 ms | 19.387 ms | 14.252 ms | ×1.17 | ×1.36 | ×2.41 | ×3.28 |
| `u64` | 57.500 ms | 45.639 ms | 42.129 ms | 42.838 ms | ×1.08 | ×0.98 | ×1.36 | ×1.34 |
| `f32` | 45.900 ms | 33.993 ms | 32.562 ms | 28.107 ms | ×1.04 | ×1.16 | ×1.41 | ×1.63 |
| `i32` | 42.300 ms | 31.767 ms | 28.522 ms | 26.055 ms | ×1.11 | ×1.09 | ×1.48 | ×1.62 |

**Core Ultra E-core (CPU 4)**

| Field | C++ | Rust `iter` | Rust `for_each_value` | Rust `&[T]` | `iter / for_each_value` | `for_each_value / &[T]` | `C++ / for_each_value` | `C++ / &[T]` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `u8` | 42.200 ms | 24.198 ms | 29.623 ms | 13.057 ms | ×0.82 | ×2.27 | ×1.42 | ×3.23 |
| `f64` | 51.100 ms | 50.280 ms | 52.948 ms | 41.668 ms | ×0.95 | ×1.27 | ×0.97 | ×1.23 |
| `u16` | 54.000 ms | 26.259 ms | 25.290 ms | 11.742 ms | ×1.04 | ×2.15 | ×2.14 | ×4.60 |
| `u64` | 51.600 ms | 49.366 ms | 50.644 ms | 40.012 ms | ×0.97 | ×1.27 | ×1.02 | ×1.29 |
| `f32` | 36.000 ms | 38.313 ms | 39.291 ms | 25.414 ms | ×0.98 | ×1.55 | ×0.92 | ×1.42 |
| `i32` | 34.400 ms | 37.038 ms | 37.155 ms | 23.116 ms | ×1.00 | ×1.61 | ×0.93 | ×1.49 |

The C++ benchmark copies each cell into an aligned local value with fixed-size
`memcpy`, avoiding undefined behavior from dereferencing the `f64` field at offset 4.
This is source-level defined behavior, not sanitizer instrumentation.

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

#### What the configurations say about SIMD and bandwidth

The M3 nightly experiment isolates explicit four-accumulator SIMD while retaining the
ordinary release ISA policy. Its large `f32` and `f64` gains, alongside parity or small
regressions for integers, show that the stable floating-point maximum shape leaves
reduction parallelism unavailable while the integer cases generally do not. This
evidence is specific to finite maximum reduction and does not generalize to average or
median.

The portable Core Ultra E-core result points in the same direction, but with different
magnitudes: explicit SIMD makes `f64` ×1.34 and `f32` ×1.74 faster, while `u64`
regresses to ×0.59 of stable typed-slice performance. The P-core and E-core stable
tables also prevent treating core class as a scalar multiplier. The E-core wins some
wide floating-point scans but loses other operations, despite both core classes using
the identical portable binary.

Memory traffic still cannot be removed from the explanation. A Rust `u8` column is
10 MB and its `f64` column is 80 MB. A 128-bit vector also holds eight times as many
`u8` values as `f64` values, and a 256-bit vector preserves that same 8:1 lane ratio.
Thus element width changes both values processed per vector operation and bytes read by
a factor of eight. On the M3, the typed-slice maximum takes 0.096 ms for `u8` and
5.347 ms for `f64`, a ×55.7 time ratio that is consistent with both effects combining;
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

Both programs used their complete default thread pools and validated every `result`
cell after each process-level sample. The C++ binary is the optimized portable release
build; Rust uses the ordinary portable release profile. Neither uses
native-ISA flags or sanitizer instrumentation.

Each host ran the order-balanced sequence `C++, Rust, Rust, C++` back-to-back. The
table reports the median of the two process-level samples per implementation, with the
individual transform range in parentheses. With two samples, that median is their
midpoint; the ranges remain visible because sustained thermal state is material.

| Host | Threads | C++ build | Rust build | C++ transform | Rust transform | C++ / Rust transform |
|---|---:|---:|---:|---:|---:|---:|
| M3 Max | 16 | 1.267 s | 0.600 s | 59.615 s (59.441–59.789) | 63.640 s (63.060–64.220) | ×0.94 |
| Core Ultra 5 225H | 14 | 1.262 s | 1.013 s | 62.495 s (59.364–65.627) | 65.804 s (65.693–65.914) | ×0.95 |

The last column divides C++ transform time by Rust transform time, so values above
×1.00 favor Rust. Equivalent transform throughput is:

| Host | C++/OpenMP | Rust/Rayon |
|---|---:|---:|
| M3 Max | 1.677 million rows/s | 1.571 million rows/s |
| Core Ultra 5 225H | 1.600 million rows/s | 1.520 million rows/s |

The balanced central estimates put C++ about 6.8% ahead on the M3 Max and 5.3% ahead
on the Core Ultra for this complete recursive-transform path. Those are fixture-level
results, not a general OpenMP-versus-Rayon ranking.

### Run-order and thermal sensitivity

The new balanced sequence still exposes position sensitivity. On the M3, C++ changes
by 0.6% between positions one and four and Rust by 1.8% between positions two and
three. On the Core Ultra, the identical C++ executable changes from 59.364 seconds in
the first position to 65.627 seconds in the fourth—a 10.5% slowdown—while the two
middle Rust samples differ by only 0.3%. Symmetric ordering prevents either
implementation from owning only the cold or hot endpoint, but two samples do not
remove the underlying thermal and sustained-power uncertainty.

Future long-running comparisons should continue to alternate or randomize execution
order, cool the machine to a defined starting condition, collect more samples, and
record frequency, power, and temperature telemetry. The wide Core Ultra C++ range is
more important than the few-percent difference between the central estimates.

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

The results below are medians of five process-level samples with implementation order
alternated between samples. “First transform” comes from five fresh processes and
therefore includes initial parallel-runtime activation and cold-state effects. “Warm
transform” is the median per-pass time from five 1,000-repetition processes after five
untimed warmups. Build time is taken from the fresh-process samples. Validation runs
after timing.

| Host | Workers | C++ build | Rust build | C++ first transform | Rust first transform | C++ warm transform | Rust warm transform |
|---|---:|---:|---:|---:|---:|---:|---:|
| M3 Max | 16 | 13.218 ms | 5.978 ms | 0.746 ms | 0.767 ms | 0.165 ms | 0.252 ms |
| Core Ultra 5 225H | 14 | 9.630 ms | 8.190 ms | 0.664 ms | 0.917 ms | 0.180 ms | 0.106 ms |

The transform is no longer the dominant end-to-end cost. Adding the median build and
first-transform measurements gives about 13.96 ms for C++ versus 6.75 ms for Rust on
the M3 Max, and 10.29 ms for C++ versus 9.11 ms for Rust on the Core Ultra. Those sums
exclude the separate validation pass.

The warmed comparison reverses between hosts: OpenMP is about 1.32 times as fast as
Rayon on the M3 Max, while Rayon is about 1.70 times as fast as OpenMP on the Core
Ultra. At only 0.1–0.9 ms per pass, parallel-region entry and exit, chunk scheduling,
heterogeneous-core placement, cache state, and ordinary timing noise are material
parts of the result. This is consequently a useful parallel-overhead case, but not a
stable ranking of the languages or a measurement of scalar multiplication throughput.

The 100M investigation above also makes thermal and run-order state a plausible
contributor to this reversal. A complete 1,000-pass batch lasts only about 0.1–0.2
seconds, so it does not by itself demonstrate sustained thermal throttling. However,
back-to-back samples can inherit temperature, boost clocks, package power budgets,
and core-placement state from earlier benchmarks. Future repetitions of this short
case should therefore alternate implementation order and record the starting machine
state instead of assuming that five sequential process samples are independent.

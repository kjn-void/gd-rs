# C++ reference benchmarks

This directory contains the Google Benchmark fixtures used as the C++ GD reference
for the Criterion benchmarks in the parent directory. Keeping the comparison sources
here makes the benchmark methodology reviewable from the Rust repository.

The build expects the C++ GD git submodule at `external/gd` relative to the `gd-rs`
repository. Initialize it with `git submodule update --init external/gd` after cloning.
Override that default with `-DGD_SOURCE_DIR=/absolute/path/to/gd` when configuring
against another checkout. The pinned upstream revision does not track its CMake build,
so configure with `-DGD_SOURCE_DIR` pointing at a checkout that contains
`CMakeLists.txt` when building the reference.
Google Benchmark is pinned to v1.9.5, matching the GD build configuration.

From this directory, build and run the optimized assertions-off reference:

```sh
cmake --preset release
cmake --build --preset release
../../target/cpp-reference/release/gd_cpp_reference_benchmarks
```

Build and run the optimized assertions-on reference:

```sh
cmake --preset release-asserts
cmake --build --preset release-asserts
../../target/cpp-reference/release-asserts/gd_cpp_reference_benchmarks
```

The assertions-on preset deliberately replaces the usual Release flags with `-O3`,
leaving `NDEBUG` undefined. Neither preset enables ASan, UBSan, or other sanitizer
instrumentation.

Each C++ source corresponds to the like-named Rust Criterion fixture:

| C++ source | Rust benchmark |
|---|---|
| `arguments_benchmark.cpp` | `../arguments.rs` |
| `binary_benchmark.cpp` | `../binary.rs` |
| `expression_benchmark.cpp` | `../expression.rs` |
| `sqlite_benchmark.cpp` | `../sqlite.rs` |
| `sqlite_to_table_benchmark.cpp` | `../sqlite_to_table.rs` |
| `table_copy_benchmark.cpp` | `../table_copy.rs` |
| `table_column_buffer_benchmark.cpp`, `table_index_benchmark.cpp` | `../table.rs` |
| `utf8_benchmark.cpp` | `../text.rs` |
| `variant_benchmark.cpp` | `../value.rs` |
| `simd_comments_benchmark.cpp` | `../simd_comments.rs` |

These files are comparison fixtures owned by `gd-rs`; update them alongside changes
to the corresponding Rust benchmark or the C++ API being measured.

Run `../run_sqlite_to_table.sh` for the matched SQLite cursor-to-table comparison.
Both sides generate three in-memory SQLite tables with a deterministic xorshift64
schema generator, 3–5 numeric columns per table, and the same seed and SQL value
formulas. `GD_SQLITE_TO_TABLE_ROWS` defaults to `1000000` rows per table;
`GD_SQLITE_TO_TABLE_SEED` defaults to `0x6a09e667f3bcc909`. Fixture construction is
outside timing; schema discovery, query preparation, conversion, allocation, and
population are timed.

Run `../run_table_copy.sh` for the matched table-copy comparison. Both executables
read `GD_TABLE_COPY_SOURCE_ROWS` (default `1000000`) and
`GD_TABLE_COPY_PERCENT` (default `25`). Destination construction and destruction are
timed. The logical throughput is the selected row count multiplied by the 15 bytes in
the `u8`, `u64`, `f32`, and `u16` schema; the C++ benchmark additionally reports GD's
padded physical row size.

The Rust suite additionally compares sequential column copying with Rayon-backed
parallel column copying. It creates a persistent worker pool outside the timed loop,
limited to half the fixed-column count rounded down. Each column remains an independent
task so the workers can dynamically balance columns of different widths.

The SIMD comment benchmark has its own executable because `gd_table_simd.cpp` is not
part of `gd::core`. CMake generates a corrected build-tree copy of
`gd_table_simd.h`, replacing the literal `...` placeholder in `pack_set_values`; the
`external/gd` submodule remains unchanged.

Run `../run_simd_comments.sh` for the matched comparison. The Rust executable measures
the 64-byte pack hybrid, a direct scan of a normal typed `U8` SoA column, and a
line-oriented standard-library implementation. The C++ executable measures the GD
pack hybrid, its scalar table-access counterpart, and a `std::string_view::find`
implementation. All variants validate identical output before timing.

`stream_benchmark.cpp` is a standalone POSIX array-loop diagnostic retained for
experimentation. It deliberately has no Rust counterpart, does not use Google
Benchmark, and does not link against GD:

```sh
c++ -O3 -march=native -std=c++20 -pthread \
  stream_benchmark.cpp -o /tmp/stream_benchmark
```

`memops_benchmark.cpp` is the libc `memcpy`/`memset` fixture used by the
[`memory-operation report`](../../docs/high-level/perf_memory.md). It also has no Rust
counterpart and is built independently of GD and Google Benchmark:

```sh
c++ -O3 -march=native -std=c++20 -pthread \
  -fno-builtin-memcpy -fno-builtin-memset \
  memops_benchmark.cpp -o /tmp/memops_benchmark
```

The Ky X1 RISC-V GCC does not implement `-march=native`; omit that flag there as
documented in the memory-operation report.

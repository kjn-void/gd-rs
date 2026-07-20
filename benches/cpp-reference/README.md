# C++ reference benchmarks

This directory contains the Google Benchmark fixtures used as the C++ GD reference
for the Criterion benchmarks in the parent directory. Keeping the comparison sources
here makes the benchmark methodology reviewable from the Rust repository.

The build expects the C++ GD repository at `../gd` relative to the `gd-rs` repository.
Override that default with `-DGD_SOURCE_DIR=/absolute/path/to/gd` when configuring.
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
| `table_column_buffer_benchmark.cpp`, `table_index_benchmark.cpp` | `../table.rs` |
| `utf8_benchmark.cpp` | `../text.rs` |
| `variant_benchmark.cpp` | `../value.rs` |

These files are comparison fixtures owned by `gd-rs`; update them alongside changes
to the corresponding Rust benchmark or the C++ API being measured.

`stream_benchmark.cpp` is a standalone POSIX STREAM-style host-memory
benchmark used by the
[`STREAM report`](../../docs/high-level/perf_stream.md). It deliberately has no Rust
counterpart because it measures the benchmark hosts rather than either table API. It
does not use Google Benchmark or link against GD and can be built directly:

```sh
c++ -O3 -march=native -std=c++20 -pthread \
  stream_benchmark.cpp -o /tmp/stream_benchmark
```

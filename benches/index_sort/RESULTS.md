# Index sort results

These are the results of the six cases described in the [README](README.md), with GD's
selection sort replaced by `std::sort` in the C++ `std::sort` column. The README covers
the table formats, the sort algorithms, and how to run the benchmarks.

The sources are:

- **C++:** [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp)
- **Rust:** [`rust/src/lib.rs`](rust/src/lib.rs) and
  [`rust/benches/index_sort.rs`](rust/benches/index_sort.rs)
- **C#:** [`csharp/`](csharp/)
- **Kotlin:** [`kotlin/src/main/kotlin/indexsort/`](kotlin/src/main/kotlin/indexsort/)

## Environment

All results were measured on 2026-10-30 on an Apple M6 (2 performance and 4 efficiency
cores, 32 GB, macOS 27.0). There was no CPU pinning, and the load average stayed
between 1.6 and 2.1. They were produced with:

```bash
benches/index_sort/run_all.sh fast <out-dir>
```

```bash
benches/index_sort/run_all.sh selection <out-dir>
```

| Port | Toolchain and settings | Value | ± |
|---|---|---|---|
| C++ | Apple clang 21.0.0 (clang-2100.3.34.2), CMake `Release` (`-O3 -DNDEBUG`), no `-march`, GD `a5b1e3e`, Google Benchmark 1.9.5 | mean of 5 repetitions | standard deviation |
| C++ GD selection sort, 10⁶ rows | as above | one timed iteration | – |
| Rust | rustc 1.99.1, `bench` profile, no `target-cpu`, Criterion 0.8 | point estimate | half the 95% confidence interval |
| C# | .NET 10.0.13 runtime, SDK 10.0.402, BenchmarkDotNet 0.15.8; default job at 1,000 rows; 3 warm-up and 10 measured iterations at 10⁶ rows | mean | half the 99.9% confidence interval |
| Kotlin | Kotlin 2.4.10, OpenJDK 25.0.4.1 (Microsoft build), JMH through kotlinx-benchmark 0.4.14; one fork; 5 warm-up and 5 measured iterations at 1,000 rows; 3 and 5 at 10⁶ rows | mean | half the 99.9% confidence interval |

Every value includes clearing the target table and appending the source rows before
the sort.

## 1,000 rows (µs)

| Case | C++ GD | C++ `std::sort` | Rust | C# | Kotlin |
|---|---:|---:|---:|---:|---:|
| 01 physical uint64 | 893 ± 3.3 | 51.7 ± 0.04 | 15.8 ± 0.01 | 34.1 ± 0.41 | 33.2 ± 2.6 |
| 02 physical string(8) | 1,209 ± 2.9 | 63.6 ± 0.73 | 17.4 ± 0.01 | 36.9 ± 0.65 | 39.0 ± 3.3 |
| 03 physical string(50) | 1,357 ± 3.5 | 65.9 ± 0.70 | 37.9 ± 0.05 | 68.0 ± 0.77 | 54.2 ± 5.4 |
| 04 index uint64 | 92.3 ± 0.07 | | 15.1 ± 0.01 | 18.9 ± 0.05 | 24.1 ± 0.21 |
| 05 index string(8) | 113 ± 0.05 | | 17.0 ± <0.01 | 21.6 ± 0.06 | 37.3 ± 0.30 |
| 06 index string(50) | 127 ± 0.24 | | 36.1 ± 0.05 | 51.8 ± 0.05 | 40.4 ± 1.0 |

## 1,000,000 rows (ms)

| Case | C++ GD | C++ `std::sort` | Rust | C# | Kotlin |
|---|---:|---:|---:|---:|---:|
| 01 physical uint64 | 918,153 | 114 ± 0.07 | 69.7 ± 0.08 | 91.6 ± 1.8 | 211 ± 2.1 |
| 02 physical string(8) | 1,314,520 | 140 ± 0.26 | 90.9 ± 0.09 | 108 ± 0.19 | 213 ± 0.64 |
| 03 physical string(50) | 1,450,633 | 165 ± 0.20 | 179 ± 0.14 | 247 ± 0.90 | 332 ± 1.6 |
| 04 index uint64 | 433 ± 0.56 | | 159 ± 0.68 | 166 ± 1.2 | 183 ± 1.2 |
| 05 index string(8) | 484 ± 0.85 | | 179 ± 0.15 | 189 ± 0.38 | 240 ± 2.3 |
| 06 index string(50) | 538 ± 0.68 | | 280 ± 0.28 | 355 ± 1.3 | 313 ± 2.0 |

GD's 10⁶-row selection sorts take 15 to 24 minutes each.

## Observations

- **The upstream speedup comes from the algorithm.** Replacing GD's selection sort
  with `std::sort` is faster than GD's own index sort, even though `std::sort` still
  moves whole rows: 1.8 to 1.9× at 1,000 rows and 3.3 to 3.8× at 10⁶ rows. GD's index
  sort compares through `cell_get_variant_view` on both rows for every comparison. The
  `std::sort` variant reads typed keys straight from the row bytes.
- **With the same algorithm, sorting an index does not win at 10⁶ rows.**
  - In Rust and C#, the physical quicksort is faster than the index quicksort for
    every key, by up to 2.3× on uint64.
  - In Kotlin, which swaps more arrays per row, the two sorts are close.
  - The likely cause is memory access. The physical sort's partition scans read rows
    in order. The index sort reads the key column through row numbers that become
    scattered as sorting proceeds, so many comparisons miss the cache.
- **At 1,000 rows the table fits in cache, and moving rows costs more.** There the
  index sort is faster than the physical sort:
  - up to 1.8× in C#, which swaps 7 arrays per row;
  - up to 1.4× in Kotlin, which swaps 16 arrays per row;
  - within 5% in Rust.
- **German strings help only when the prefix differs.**
  - C2's comparisons are decided by the inline prefix, so cases 02 and 05 stay close
    to the uint64 cases.
  - C7's prefix is the same in every row and never decides, so cases 03 and 06 follow
    the heap pointer and compare from byte 4 onward on every comparison.

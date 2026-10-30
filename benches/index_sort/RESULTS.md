# Index sort results

These are the results of the six cases described in the [README](README.md), with GD's
selection sort replaced by `std::sort` in the C++ `std::sort` column. The README covers
the table formats, the sort algorithms, and how to run the benchmarks.

The sources are:

- **C++:** [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp), with the
  `std::sort` row iterator in
  [`../cpp-reference/gd_table_row_iterator.h`](../cpp-reference/gd_table_row_iterator.h)
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

In the 1,000- and 10⁶-row tables, the Rust, C#, and Kotlin index sorts (cases 04 to
06) applied the sorted order in place by swapping rows along permutation cycles, as GD
does. That code is at commit [`8754801`](https://github.com/kjn-void/gd-rs/tree/8754801/benches/index_sort).
The ports now build a new table instead, after the 10,000-row comparison below, and
these tables have not been remeasured.

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

## Swap versus gather, 10,000 rows (µs)

This comparison led the ports to replace the in-place swap with a gather. Cases 04 to
06 applied the index sort's permutation by swapping rows in place with the cycle walk.
Cases 07 to 09 ran the same quicksort over row numbers, then built a new table by
gathering each column in sorted order. Both variants are at commit `8754801`:
[`rust/src/lib.rs`](https://github.com/kjn-void/gd-rs/blob/8754801/benches/index_sort/rust/src/lib.rs) (`index_sort`, `index_gather_sort`),
[`csharp/Sorts.cs`](https://github.com/kjn-void/gd-rs/blob/8754801/benches/index_sort/csharp/Sorts.cs) and [`csharp/Table.cs`](https://github.com/kjn-void/gd-rs/blob/8754801/benches/index_sort/csharp/Table.cs)
(`IndexSort`, `IndexGatherSort`, `Gather`), and
[`kotlin/src/main/kotlin/indexsort/`](https://github.com/kjn-void/gd-rs/tree/8754801/benches/index_sort/kotlin/src/main/kotlin/indexsort)
(`indexSort`, `indexGatherSort`, `Table.gather`). There is no C++ counterpart; GD's
key sort below also gathers, but caches keys and runs on a table with every column
set.

It was measured on 2026-10-30 on the same M6 and toolchains as above, with a load
average between 1.0 and 2.0, using that commit's `run_all.sh gather`. Rust and Kotlin used the
1,000-row settings; C# used BenchmarkDotNet's default job. ± has the same meaning as in
the environment table.

| Key | Rust swap | Rust gather | C# swap | C# gather | Kotlin swap | Kotlin gather |
|---|---:|---:|---:|---:|---:|---:|
| C1 uint64 | 209 ± 0.4 | 138 ± 0.3 | 252 ± 0.9 | 211 ± 0.4 | 324 ± 7.0 | 169 ± 0.2 |
| C2 string(8) | 248 ± 0.2 | 180 ± 0.1 | 315 ± 1.4 | 258 ± 0.4 | 335 ± 6.4 | 189 ± 1.3 |
| C7 string(50) | 766 ± 0.5 | 725 ± 9.3 | 1,074 ± 4.1 | 980 ± 2.8 | 703 ± 32 | 495 ± 106 |

- **Gathering is faster in every port and for every key.** It saves 28 to 48% on
  C1 and C2. On C7 it saves 5% in Rust, 9% in C#, and about 30% in Kotlin, where the
  error margin is wide.
- **The saving is in applying the permutation; the sort itself is identical.** The
  cycle walk swaps every column at two scattered positions, reading and writing
  both. The gather reads each value once from a scattered position and writes it
  sequentially. Kotlin swaps 16 arrays per row, so it gains the most.
- **C7 is dominated by comparisons.** Each one follows the heap pointer past the
  shared prefix, so the faster apply step is a small share of the total.
- **Extra memory per row** (heap text is shared, never copied):

  | Port | Row | Swap | Gather |
  |---|---:|---:|---:|
  | Rust | 80 B | 8 B index | 24 B: index plus one column, as each old column is freed once moved |
  | C# | 96 B | 4 B index | 100 B: index plus a whole new table |
  | Kotlin | 75 B | 4 B index | 79 B: index plus a whole new table |

## Three sort methods in four languages, 10,000 rows (µs)

GD's three sort methods next to the best of the port sorts, for every column of the
10,000-row table with every column set. The sources are
[`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp),
[`rust/benches/sort_methods.rs`](rust/benches/sort_methods.rs) with the sorts in
[`rust/src/lib.rs`](rust/src/lib.rs), [`csharp/SortMethods.cs`](csharp/SortMethods.cs)
with the sorts in [`csharp/Sorts.cs`](csharp/Sorts.cs), and the `SortMethods` class in
[`kotlin/src/main/kotlin/indexsort/Benchmarks.kt`](kotlin/src/main/kotlin/indexsort/Benchmarks.kt)
with the sorts in [`Sorts.kt`](kotlin/src/main/kotlin/indexsort/Sorts.kt).

It was measured on 2026-10-30 on the same M6 and toolchains as above, with a load
average between 1.0 and 2.1, using `run_all.sh methods`. C++ ran 5 repetitions; Rust
and Kotlin used the 1,000-row settings; C# used BenchmarkDotNet's default job. ± has
the same meaning as in the environment table.

| Column | Type | C++ GD physical | C++ GD index | C++ GD key | Rust key | C# index | Kotlin index |
|---|---|---:|---:|---:|---:|---:|---:|
| 0 | uint64 | 81,007 ± 86 | 1,117 ± 4.7 | 670 ± 17 | 130 ± 0.07 | 211 ± 0.31 | 163 ± 0.92 |
| 1 | string(8) | 110,455 ± 220 | 1,366 ± 0.68 | 904 ± 4.0 | 173 ± 0.07 | 265 ± 0.33 | 186 ± 0.62 |
| 2 | double | 81,401 ± 8.9 | 1,288 ± 4.2 | 715 ± 1.7 | 142 ± 0.13 | 220 ± 0.60 | 170 ± 1.5 |
| 3 | double | 81,415 ± 41 | 1,291 ± 3.1 | 688 ± 34 | 166 ± 7.3 | 234 ± 0.39 | 191 ± 0.54 |
| 4 | int32 | 88,555 ± 31 | 1,252 ± 2.2 | 610 ± 3.4 | 125 ± 0.05 | 217 ± 0.35 | 177 ± 0.93 |
| 5 | uint32 | 88,605 ± 14 | 1,257 ± 1.0 | 668 ± 0.78 | 136 ± 3.1 | 223 ± 0.54 | 181 ± 1.1 |
| 6 | string(50) | 125,027 ± 180 | 1,676 ± 3.0 | 1,020 ± 5.5 | 621 ± 1.0 | 1,031 ± 4.4 | 455 ± 33 |

Each port also has a physical, an index, and a key sort; the table shows the one with
the lowest total over the seven columns. All three produce the same row order, and
all build a new table except the physical sort.

| Port | Physical total | Index total | Key total | Shown |
|---|---:|---:|---:|---|
| Rust | 1,941 | 1,618 | 1,492 | key |
| C# | 4,118 | 2,400 | 2,441 | index |
| Kotlin | 3,405 | 1,523 | 1,579 | index |

- **The physical quicksort loses in every port.** It swaps all seven columns at each
  step.
- **Rust's key sort wins** because its (key, row) pairs hold numbers by value and
  strings by reference, 16 bytes each. Comparisons read the pair instead of following
  a row number into the column.
- **C# and Kotlin have no cheap string reference.** The C# pairs copy a 24-byte
  German string; Kotlin copies the whole German string column and swaps its four
  arrays. Their key sorts win on numbers by up to 16% (C#) and 20% (Kotlin), but lose
  on string(8) by 60% and 39%, so the index sort has the lower total.
- **Against GD's key sort,** the shown port sorts are 2.8 to 5.2 times faster on the
  numeric and string(8) columns. On string(50), Rust and Kotlin are 1.6 and 2.2 times
  faster, and C# is level.

## Three sort methods, 10,000 rows (µs)

This reproduces the follow-up benchmark in
[forum post 21314807](https://www.sweclockers.com/forum/post/21314807), described in the
[README](README.md#three-sort-methods). The source is
[`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp). It is C++ only, with
no Rust, C#, or Kotlin counterpart.

It was measured on 2026-10-30 on the same M6, with the same C++ toolchain and settings
as above, and a load average below 1.1. Each value is the mean of 5 repetitions ± the
standard deviation. The Dell 7780 columns are the figures from the forum post.

| Column | Type | M6 physical | M6 index | M6 key | Dell physical | Dell index | Dell key |
|---|---|---:|---:|---:|---:|---:|---:|
| 0 | uint64 | 80,835 ± 40 | 1,106 ± 1.6 | 618 ± 6.3 | 157,747 | 2,882 | 1,835 |
| 1 | string(8) | 110,235 ± 60 | 1,357 ± 0.9 | 896 ± 9.9 | 173,485 | 2,587 | 1,965 |
| 2 | double | 81,329 ± 7.3 | 1,293 ± 0.6 | 734 ± 1.0 | 126,247 | 2,217 | 1,475 |
| 3 | double | 81,350 ± 33 | 1,279 ± 9.1 | 727 ± 17 | 121,365 | 2,253 | 1,427 |
| 4 | int32 | 88,499 ± 21 | 1,235 ± 0.4 | 605 ± 3.9 | 124,804 | 2,406 | 1,437 |
| 5 | uint32 | 88,536 ± 27 | 1,245 ± 0.6 | 622 ± 0.6 | 113,424 | 2,188 | 1,487 |
| 6 | string(50) | 124,299 ± 118 | 1,700 ± 0.7 | 1,033 ± 1.2 | 213,917 | 2,537 | 2,053 |

- **Key sort beats index sort on every column.** It takes 34 to 51% less time on the
  M6, and 19 to 40% less in the Dell figures.
- **Physical selection sort is 111 to 146× slower than key sort** on the M6, and 76
  to 104× in the Dell figures.
- **The M6 is 1.3 to 3.0× faster than the Dell,** with the smallest gap for physical
  sort and the largest for key sort. The shape reproduces, but the absolute times
  do not.

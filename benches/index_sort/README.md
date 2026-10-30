# Index sort versus physical sort

Reproduces the six "sort table" cases from the upstream GD benchmark described in
[this forum post](https://www.sweclockers.com/forum/post/21314377), then runs the same
cases in Rust, C#, and Kotlin and at 10⁶ rows as well as the upstream 1,000.

## Data

The table has 7 columns: `uint64`, `string(8)`, `double`, `double`, `int32`, `uint32`,
and `string(50)`. Only C1, C2, C3, and C7 are set; C4 to C6 stay null. Each row draws,
in this order:

- **C2:** 8 random lowercase letters, such as `wikovaaw`.
- **C7:** the fixed 20 characters `01234567890123456789` plus 25 random lowercase
  letters, 45 bytes in total, such as `01234567890123456789blgyoniwsbhlszjfujmwuhxkm`.
  Every row shares the first 20 characters, so each comparison has to get past them.
- **C1:** a uniform integer from 0 to 1,000,000. At 10⁶ rows many values repeat.
- **C3:** a uniform double from 0 to 1,000.

C++ uses the upstream `std::mt19937_64(42)` generator. The other ports use their own
seeded generators with the same distributions, so their values differ but the shape is
the same.

## Cases

Each timed iteration clears the target table, appends all source rows, and sorts on
one column:

| Case | Key | C++ GD | Rust, C#, Kotlin |
|---|---|---|---|
| 00 | – | append only, no sort | append only, no sort |
| 01 | C1 `uint64` | `sort(0, true)`: selection sort | physical quicksort |
| 02 | C2 `string(8)` | `sort(1, true)`: selection sort | physical quicksort |
| 03 | C7 `string(50)` | `sort(6, true)`: selection sort | physical quicksort |
| 04 | C1 `uint64` | `sort(0, true, tag_sort_index{})` | index quicksort |
| 05 | C2 `string(8)` | `sort(1, true, tag_sort_index{})` | index quicksort |
| 06 | C7 `string(50)` | `sort(6, true, tag_sort_index{})` | index quicksort |
| 01s–03s | C1, C2, C7 | `std::sort` over the table's rows | – |

Case 00 and cases 01s to 03s are not upstream cases.

### C++ GD

GD's `table_column_buffer` stores each row as one block of bytes: 104 bytes of data per
row, with C2 and C7 inline as a length plus the text. Each row's 8 bytes of null flags
and row status live in a separate region.

- **Physical, cases 01 to 03:** GD's default sort is a selection sort. It makes
  n(n−1)/2 comparisons through `cell_get_variant_view` and at most n−1 row swaps.
- **Index, cases 04 to 06:** `tag_sort_index` runs `std::stable_sort` over a vector of
  row numbers, comparing through `cell_get_variant_view`. It then applies the
  permutation in place by following cycles, with at most n−1 row swaps.
- **`std::sort`, cases 01s to 03s:** `std::sort` over a random-access iterator whose
  elements are rows. Each element is a row's data bytes plus its metadata bytes. The
  comparator reads the key straight from the row bytes.

The upstream comparison therefore changes two things at once: selection sort becomes
a merge sort, and row moves become index moves. Cases 01s to 03s keep physical row
moves but replace the selection sort.

### Rust, C#, and Kotlin

Each port stores the table as a struct of arrays, one array per column. C4 to C6 are
nullable. C2 and C7 hold German (Umbra) strings, which keep the length and a 4-byte
prefix inline:

- C2 is 8 bytes, short enough to be stored entirely inline.
- C7 is 45 bytes, so its text lives in a shared heap buffer.
- Appending a row shares C7's buffer instead of copying the text.
- Comparisons check the prefix first. For C7 the prefix is `0123` in every row, so it
  never decides a comparison.

| | Table | German string |
|---|---|---|
| Rust | one `Vec` per column | `strumbra::SharedString`, 16 bytes, `Arc` buffer |
| C# | one array per column; `double?`, `int?`, `uint?` for C4 to C6 | `GermanString` struct, 24 bytes, shared `byte[]` (a managed reference cannot overlap the inline bytes) |
| Kotlin | one primitive array per column, plus validity arrays for C4 to C6 | `GermanStringColumn`: parallel arrays for length, prefix, inline bytes, and a shared `ByteArray` |

The physical and index sorts in each port run the same quicksort:

- median-of-three pivot;
- partitioning that stops on keys equal to the pivot;
- recursion into the smaller part only;
- insertion sort for ranges of 16 or fewer.

The two cases differ only in what the quicksort swaps:

- **Physical:** whole rows, which swaps all seven columns.
- **Index:** row numbers, compared through the rows they name. The permutation is then
  applied in place with the same cycle walk as GD.

Both cases make the same comparisons and produce the same row order, and the tests
check that row for row. Neither sort is stable.

## Projects

| Port | Harness | Sources |
|---|---|---|
| C++ | Google Benchmark 1.9.5, CMake, GD via [`../cpp-reference/cmake/GdCore.cmake`](../cpp-reference/cmake/GdCore.cmake) | [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp) |
| Rust | Criterion 0.8 | [`rust/src/lib.rs`](rust/src/lib.rs), [`rust/benches/index_sort.rs`](rust/benches/index_sort.rs) |
| C# | .NET 10, C# 14, BenchmarkDotNet 0.15.8 | [`csharp/`](csharp/), sorts in [`csharp/Sorts.cs`](csharp/Sorts.cs) |
| Kotlin | Kotlin 2.4.10, kotlinx-benchmark 0.4.14 (JMH) | [`kotlin/src/main/kotlin/indexsort/`](kotlin/src/main/kotlin/indexsort/) |

The correctness checks run with:

```bash
cd benches/index_sort/rust && cargo test --release
```

```bash
cd benches/index_sort/csharp && dotnet run -c Release -- verify
```

```bash
cd benches/index_sort/kotlin && ./gradlew test
```

[`run_all.sh`](run_all.sh) builds and runs every benchmark, and records the toolchains
and load average in its output directory. Run it from the repository root. The
`selection` phase holds GD's three 10⁶-row selection sorts. Each runs for a single
iteration, and together they take about an hour.

```bash
benches/index_sort/run_all.sh fast <out-dir>
```

```bash
benches/index_sort/run_all.sh selection <out-dir>
```

## Results

All results were measured on 2026-10-30 on an Apple M6 (2 performance and 4
efficiency cores, 32 GB, macOS 27.0) with the load average between 1.6 and 2.1. There
was no CPU pinning. The toolchains were:

- **C++:** Apple clang 21.0.0 (clang-2100.3.34.2), CMake `Release` (`-O3 -DNDEBUG`),
  no `-march` flags, GD at `a5b1e3e`.
- **Rust:** rustc 1.99.1, `bench` profile, no `target-cpu`.
- **C#:** .NET 10.0.13 runtime, SDK 10.0.402, BenchmarkDotNet's default job at 1,000
  rows, 3 warm-up and 10 measured iterations at 10⁶ rows.
- **Kotlin:** OpenJDK 25.0.4.1 (Microsoft build), JMH through kotlinx-benchmark, one
  fork, 5 warm-up and 5 measured iterations at 1,000 rows, 3 and 5 at 10⁶ rows.

The reported values are means, except Criterion's point estimates for Rust:

- **C++:** 5 Google Benchmark repetitions.
- **C# and Kotlin:** the iterations listed above.
- **GD's 10⁶-row selection sorts:** one timed iteration each.

### 1,000 rows (µs)

| Case | C++ GD | C++ `std::sort` | Rust | C# | Kotlin |
|---|---:|---:|---:|---:|---:|
| 00 append only | 18.3 | | 3.1 | 1.4 | 1.2 |
| 01 physical uint64 | 893 | 51.7 | 15.8 | 34.1 | 33.2 |
| 02 physical string(8) | 1,209 | 63.6 | 17.4 | 36.9 | 39.0 |
| 03 physical string(50) | 1,357 | 65.9 | 37.9 | 68.0 | 54.2 |
| 04 index uint64 | 92.3 | | 15.1 | 18.9 | 24.1 |
| 05 index string(8) | 113 | | 17.0 | 21.6 | 37.3 |
| 06 index string(50) | 127 | | 36.1 | 51.8 | 40.4 |

### 1,000,000 rows (ms)

| Case | C++ GD | C++ `std::sort` | Rust | C# | Kotlin |
|---|---:|---:|---:|---:|---:|
| 00 append only | 18.8 | | 3.9 | 1.6 | 1.2 |
| 01 physical uint64 | 918,153 | 114 | 69.7 | 91.6 | 211 |
| 02 physical string(8) | 1,314,520 | 140 | 90.9 | 108 | 213 |
| 03 physical string(50) | 1,450,633 | 165 | 179 | 247 | 332 |
| 04 index uint64 | 433 | | 159 | 166 | 183 |
| 05 index string(8) | 484 | | 179 | 189 | 240 |
| 06 index string(50) | 538 | | 280 | 355 | 313 |

GD's 10⁶-row selection sorts take 15 to 24 minutes each.

## Observations

- **The upstream speedup comes from the algorithm.** Replacing GD's selection sort
  with `std::sort`, while still moving whole rows, is faster than GD's own index sort:
  1.8 to 1.9× at 1,000 rows and 3.3 to 3.8× at 10⁶ rows. GD's index sort compares through
  `cell_get_variant_view` on both rows for every comparison. The `std::sort` variant
  reads typed keys straight from the row bytes.
- **With the same algorithm, sorting an index does not win at 10⁶ rows.** In Rust and
  C#, the physical quicksort is faster than the index quicksort for every key, by up to
  2.3× on uint64. The physical sort's partition scans read rows in order. The index
  sort reads the key column through row numbers that become random as sorting
  proceeds, so most comparisons miss the cache. Kotlin's physical sort, which swaps
  more arrays per row, is close to its index sort.
- **At 1,000 rows the table fits in cache, and moving rows costs more.** There the index
  sort is faster than the physical sort: by up to 1.8× in C# and 1.4× in Kotlin, which
  swap 7 and 16 arrays per row. Rust's two sorts are within 5% of each other.
- **Shared prefixes cost German strings their shortcut.** C2's comparisons are decided
  by the inline prefix, so cases 02 and 05 stay close to the uint64 cases.
  C7's prefix never decides, so cases 03 and 06 follow the heap pointer and compare
  from byte 4 onward on every comparison.
- **Appends are cheap in the ports.** Appending copies arrays and shares C7's heap
  buffers: 1.2 to 3.9 ms for 10⁶ rows. GD copies cell by cell through `variant_view`,
  which takes 18.8 ms.

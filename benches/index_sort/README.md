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
  elements are rows, from the shared
  [`../cpp-reference/gd_table_row_iterator.h`](../cpp-reference/gd_table_row_iterator.h).
  Each element is a row's data bytes plus its metadata bytes. The comparator reads the
  key straight from the row bytes.

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
- **Index:** row numbers, compared through the rows they name. The sorted order is then
  applied by building a new table: each new column is filled in sorted order from the
  old one. Rust moves each column's values and frees the old column right away; C# and
  Kotlin allocate a whole new table and drop the old one.

Both cases make the same comparisons and produce the same row order, and the tests
check that row for row. Neither sort is stable.

GD applies its index sort's order in place instead, by swapping rows along permutation
cycles. The ports did the same until this was measured against building a new table,
which was faster in every port; see
[RESULTS.md](RESULTS.md#swap-versus-gather-10000-rows-µs).

## Three sort methods

[`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp) reproduces the
follow-up benchmark in [forum post 21314807](https://www.sweclockers.com/forum/post/21314807).
It is C++ only. Unlike the six cases above:

- the table has 10,000 rows;
- every column is set, with C3 and C4 uniform doubles from 0 to 1,000, and C5 and C6
  uniform integers from 0 to 999;
- every column is sorted three ways: physical (GD's selection sort), index
  (`tag_sort_index`), and key.

The key sort reads each row's key once into a vector of (`variant_view`, row number)
pairs and sorts that vector with `std::stable_sort`. It then copies the rows, in sorted
order, into a buffer and copies the buffer back over the table, and does the same for
the row metadata. The post adds it to GD as `sort(column, ascending, tag_sort_key{})`,
but GD has not published it, so the benchmark carries the posted function rewritten
against GD's public row accessors.

```bash
target/index_sort/cpp/sort_methods_benchmark --benchmark_repetitions=5 --benchmark_report_aggregates_only=true
```

## Projects

| Port | Harness | Sources |
|---|---|---|
| C++ | Google Benchmark 1.9.5, CMake, GD via [`../cpp-reference/cmake/GdCore.cmake`](../cpp-reference/cmake/GdCore.cmake) | [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp), row iterator in [`../cpp-reference/gd_table_row_iterator.h`](../cpp-reference/gd_table_row_iterator.h); [`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp) |
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

The measurements, toolchains, and observations are in [RESULTS.md](RESULTS.md).

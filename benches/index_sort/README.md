# Index sort versus physical sort

Reproduces the six "sort table" cases from the upstream GD benchmark described in
[this forum post](https://www.sweclockers.com/forum/post/21314377).

The table has 1000 rows and 7 columns (`uint64`, `string(8)`, `double`, `double`,
`int32`, `uint32`, `string(50)`), with `eTableFlagNull32 | eTableFlagRowStatus`. The
data comes from `std::mt19937_64(42)`. Only C1, C2, C3, and C7 are set; C4 to C6 stay
null. Every C7 value starts with the same 20-character prefix,
`01234567890123456789`, followed by 25 random letters.

Each timed iteration runs `row_clear()`, `append(source)`, and then one sort:

| Case | Column | Call |
|---|---|---|
| 01 | C1 `uint64` | `sort(0, true)`: selection sort |
| 02 | C2 `string(8)` | `sort(1, true)`: selection sort |
| 03 | C7 `string(50)` | `sort(6, true)`: selection sort |
| 04 | C1 `uint64` | `sort(0, true, tag_sort_index{})` |
| 05 | C2 `string(8)` | `sort(1, true, tag_sort_index{})` |
| 06 | C7 `string(50)` | `sort(6, true, tag_sort_index{})` |

Case 00, append only, isn't an upstream case. It measures the clear and append that
every case pays before it sorts.

## Projects

Each port runs the same cases on the same layout: the C++ `table_column_buffer` from GD,
and in the other languages a struct-of-arrays table with one array per column.

- [`cpp/`](cpp/): Google Benchmark v1.9.5 with CMake. It builds GD from
  `external/gd` with [`../cpp-reference/cmake/GdCore.cmake`](../cpp-reference/cmake/GdCore.cmake).
  The cases are in [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp).
- [`rust/`](rust/): Criterion 0.8. The table and both sorts are in
  [`rust/src/lib.rs`](rust/src/lib.rs), and the cases in
  [`rust/benches/index_sort.rs`](rust/benches/index_sort.rs).
- [`kotlin/`](kotlin/): a kotlinx-benchmark 0.4.14 (JMH) harness. The cases aren't
  ported yet, so there is no Kotlin counterpart to the results below.

```bash
cmake -S benches/index_sort/cpp -B target/index_sort/cpp -DCMAKE_BUILD_TYPE=Release
cmake --build target/index_sort/cpp -j
target/index_sort/cpp/index_sort_benchmark --benchmark_repetitions=5 --benchmark_report_aggregates_only=true
```

```bash
cd benches/index_sort/rust && cargo test --release && cargo bench --bench index_sort
```

```bash
cd benches/index_sort/kotlin && ./gradlew mainBenchmark
```

## C++ results

These are means of five repetitions in µs. They were measured on an Apple M3 Max with
Apple clang 21.0.0 (clang-2100.3.34.2), using CMake `Release` (`-O3 -DNDEBUG`) with no
`-march` flags. GD was at `a5b1e3e`. The run had no CPU pinning, and the load average
was about 6, so treat the numbers as indicative. The forum columns are the post's
means converted from ns.

| Case | M3 Max | Dell 7780 (post) | Ryzen 12-core (post) |
|---|---:|---:|---:|
| 00 append only | 34.5 | – | – |
| 01 physical uint64 | 1177 | 1375 | 1837 |
| 02 physical string(8) | 1754 | 1819 | 2558 |
| 03 physical string(50) | 1838 | 2131 | 3389 |
| 04 index uint64 | 141 | 188 | 240 |
| 05 index string(8) | 171 | 187 | 244 |
| 06 index string(50) | 191 | 208 | 269 |

The ratio between physical and index sort is about 8 to 10 times, which matches the
post's roughly 7.5 times.

## What the comparison measures

Both variants sort in place and move whole rows with the same `swap(row1, row2)`. The
rows are 104 data bytes each, plus 8 bytes of null and status metadata in a separate
region. The difference is the algorithm:

- **Physical sort** (`tag_sort_selection`, the default): selection sort. It makes
  n(n−1)/2 = 499,500 `cell_get_variant_view` comparisons and at most n−1 row swaps.
- **Index sort** (`tag_sort_index`): `std::stable_sort` over a `std::vector<uint64_t>`
  of row numbers, about n·log₂n ≈ 10k comparisons. The resulting permutation is then
  applied in place by following cycles, with at most n−1 row swaps.

The speedup therefore comes from moving from O(n²) to O(n log n) comparisons. Row data
movement is about the same in both variants.

## Rust

The Rust port keeps the C++ structure:

- **Physical sort**: the same selection sort, swapping whole rows.
- **Index sort**: `slice::sort_by`, which is stable, over a `Vec<usize>` of row
  numbers, followed by the same in-place cycle swaps.
- **Each iteration**: clears the target, appends clones of the source rows, and sorts.

The `Table` keeps one `Vec` per column, and a row swap swaps all seven.

- C4 to C6 are `Vec<Option<_>>` and always `None`.
- C2 and C7 are `Vec<strumbra::SharedString>`. Each value takes 16 bytes. Strings up
  to 12 bytes, such as C2, are stored inline. Longer strings, such as C7, share an
  `Arc` buffer, so appending a row increments a reference count instead of copying
  text.
- Comparisons check strumbra's 4-byte inline prefix first.

The data follows the C++ distributions and draw order:

- C2 is 8 random lowercase letters.
- C7 is the fixed prefix plus 25 random lowercase letters, 45 bytes in total.
- The engine is `StdRng::seed_from_u64(42)`, so the values differ from the C++ data.

This layout and string type were chosen after an A/B comparison. It covered
array-of-structs against struct-of-arrays, and `String`, `strumbra::UniqueString`,
`strumbra::SharedString`, and `umbra_slice::UmbraString`.

- **Strings:** `SharedString` had the lowest mean across the index sorts at both row
  counts.
- **Layout:** struct-of-arrays was faster than array-of-structs in every index sort at
  1,000,000 rows.

### 1,000 rows

The 1,000-row cases were measured on the Apple M3 Max with rustc 1.99.1 in the `bench`
profile, with no `target-cpu`. Each value is a Criterion point estimate in µs. Other
work kept the load average at 14 to 49, but the results stayed within about 3% of an
earlier run under lighter load.

| Case | C++ GD | Rust |
|---|---:|---:|
| 00 append only | 34.5 | 4.4 |
| 01 physical uint64 | 1177 | 872 |
| 02 physical string(8) | 1754 | 1494 |
| 03 physical string(50) | 1838 | 4995 |
| 04 index uint64 | 141 | 20.8 |
| 05 index string(8) | 171 | 27.9 |
| 06 index string(50) | 191 | 59.0 |

### 1,000,000 rows

The `1000000/` group uses the same data generator with 10⁶ rows. It skips cases 01 to
03: selection sort makes n²/2 = 5·10¹¹ comparisons, which extrapolates to roughly
20 minutes per iteration.

```bash
cd benches/index_sort/rust && cargo bench --bench index_sort -- '^1000000/'
```

The results were measured on an Apple M6 (2 performance and 4 efficiency cores,
32 GB) with rustc 1.99.1 in the `bench` profile. The load average was 1 to 3. Each
value is a Criterion point estimate from 10 samples, in ms.

| Case | Rust |
|---|---:|
| 00 append only | 3.92 |
| 04 index uint64 | 120.5 |
| 05 index string(8) | 134.4 |
| 06 index string(50) | 215.3 |

### Observations

- **Append:** Rust's append is cheap because C2 is stored inline and C7 shares its
  buffer. GD's `append` copies cell by cell instead: it reads each of the 7,000 cells as a
  `variant_view` and writes it back with `cell_set` or `cell_set_null`.
- **C7 selection sort (case 03):** this is the one case where Rust is slower than C++.
  Every C7 value shares the 20-character prefix, so strumbra's 4-byte prefix check
  never decides a comparison. Each of the 499,500 comparisons then follows the `Arc`
  pointer, while GD compares C7 inline in the row.
- **C2 sorts:** the 8-byte strings are compared entirely inline, so these cases stay
  close to the uint64 cases.
- **Index sorts:** the Rust index sorts are 3 to 7 times faster than GD's. The Rust
  comparator reads typed columns directly. GD builds a `variant_view` for both
  operands on every comparison.

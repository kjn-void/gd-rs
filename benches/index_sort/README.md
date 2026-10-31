# Index sort versus physical sort

Reproduces two upstream GD sort benchmarks in C++:

- the six "sort table" cases described in
  [forum post 21314377](https://www.sweclockers.com/forum/post/21314377), at the
  upstream 1,000 rows and at 10⁶ rows;
- the follow-up in [forum post 21314807](https://www.sweclockers.com/forum/post/21314807),
  which sorts every column of a 10,000-row table three ways. Additional C++ cases
  sort a GD row index and materialize a table, or sort a standard-container SoA
  table by caching keys through a templated row iterator and gathering columns.

Rust, C#, and Kotlin ports run the follow-up with allocating and reusable-buffer
variants. A gd-rs benchmark builds a stable borrowing row order, with German strings
for C2 and ordinary strings for C7.

## Data

The table has 7 columns: `uint64`, `string(8)`, `double`, `double`, `int32`, `uint32`,
and `string(50)`. Each row draws, in this order:

- **C2:** 8 random lowercase letters, such as `wikovaaw`.
- **C7:** the fixed 20 characters `01234567890123456789` plus 25 random lowercase
  letters, 45 bytes in total, such as `01234567890123456789blgyoniwsbhlszjfujmwuhxkm`.
  Every row shares the first 20 characters, so each comparison has to get past them.
- **C1:** a uniform integer from 0 to 1,000,000. At 10⁶ rows many values repeat.
- **C3:** a uniform double from 0 to 1,000.
- **C4 to C6, follow-up only:** C4 a uniform double from 0 to 1,000, and C5 and C6 a
  uniform integer from 0 to 1,000,000 modulo 1000. In the six cases they stay null.

C++ uses the upstream `std::mt19937_64(42)` generator. The ports use their own seeded
generators with the same distributions, so their values differ but the shape is the
same.

## Six cases (C++)

Each timed iteration clears the target table, appends all source rows, and sorts on
one column:

| Case | Key | C++ GD |
|---|---|---|
| 00 | – | append only, no sort |
| 01 | C1 `uint64` | `sort(0, true)`: selection sort |
| 02 | C2 `string(8)` | `sort(1, true)`: selection sort |
| 03 | C7 `string(50)` | `sort(6, true)`: selection sort |
| 04 | C1 `uint64` | `sort(0, true, tag_sort_index{})` |
| 05 | C2 `string(8)` | `sort(1, true, tag_sort_index{})` |
| 06 | C7 `string(50)` | `sort(6, true, tag_sort_index{})` |
| 01s–03s | C1, C2, C7 | `std::sort` over the table's rows |

Case 00 and cases 01s to 03s are not upstream cases.

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

Until commit 906a410 the ports ran these six cases too; RESULTS.md links their figures
to the code that produced them.

## Three sort methods

The GD and Rust/C#/Kotlin cases include appending the 10,000 source rows, with
every column set, and sorting on one of the seven columns. The C++ SoA case below
gathers directly from the source into reusable output columns.

### C++ GD

Every column is sorted three ways: physical (GD's selection sort), index
(`tag_sort_index`), and key, plus a fourth way that is not upstream.

The key sort implements [the updated GD version](https://www.sweclockers.com/forum/post/21315112)
in [`../cpp-reference/gd_table_key_sort.h`](../cpp-reference/gd_table_key_sort.h):
raw cell views are cached once, the numeric comparator is selected once, and
`std::stable_sort` orders `(variant_view, row)` pairs. One
`std::make_unique_for_overwrite<uint8_t[]>` buffer is reused for row and metadata
gathers, avoiding zero initialization and a second allocation. The buffer is sized
for the larger region.

Pinned GD `a5b1e3e` lacks the post's `tag_same_type` overload. The helper retains a
per-comparison type switch for the string fallback and compares bytes/lengths
directly; uniform-length benchmark strings have the same ordering. Other fallback
kinds use the pinned GD comparator. The malformed posted null-check call is adapted
to `cell_is_null(row, column)`. The submodule stays unchanged. Fixture types remain
`string(8)` and `string(50)`; the latest post's results label C7 `rstring(50)` but
provide no replacement fixture.

The registered case remains `key`, replacing its previous implementation. CTest's
`gd_key_sort_verify` checks stable ties, both directions, partial ranges, complete
row/metadata integrity, empty and short tables, and metadata larger than row data.

The fourth way sorts a vector of row numbers with `std::sort`, comparing keys read
straight from the row bytes with the typed `Less<T>` from
[`../cpp-reference/gd_table_row_iterator.h`](../cpp-reference/gd_table_row_iterator.h)
instead of through `variant_view`. It then materializes a new table: the rows and
their metadata are copied into it in sorted order, and it replaces the old one. It is
not stable.

### C++ standard-container SoA

`soa_rows/<column>` uses the same values as the GD fixture, in
[`soa_rows::Table<Cells...>`](../cpp-reference/soa_table_row_iterator.h): a
`std::tuple` of seven `std::vector`s, with `std::optional` for C4 to C6 and
`std::string` for both string columns. C2 uses small-string storage; C7 owns its
heap text. It uses ordinary standard strings throughout.

[`KeySortWorkspace<Column, Table>`](../cpp-reference/soa_table_key_sort.h) reads
keys through `RowIterator<const Table>` and caches numeric values or
`std::string_view`s with their row positions. Nullable keys are checked once during
cache construction and must be populated. `std::sort` orders the cached pairs by
key, then a column-wise gather copies each cell directly from the unchanged source
into the target. Comparison specializes the column/type at compile time.

There is no preliminary append and no whole-row sorting. The workspace retains
its key vector, and the target retains column capacity and owning `std::string`
buffers between iterations. Each long string's text is copied once into its output
slot; after initial sizing, the buffers are reused. Key-cache fill, null validation,
sorting and all seven column gathers are timed. Workspace/vector reservation is
outside timing. The result is an independent, physically ordered table; ties are
unstable. Retained output/key storage is the memory tradeoff.

The standalone [verifier](../cpp-reference/soa_table_row_iterator_verify.cpp) checks
all seven keys, nulls, duplicate keys, empty and short inputs, repeated use, iterator
arithmetic, copy/move semantics, whole-row integrity, source immutability, invalid
gather positions, null-key rejection and output string-buffer reuse. A separate
move-only schema exercises the iterator's physical-sort utility. It runs via
CTest alongside the GD key verifier:

```bash
cmake -S benches/index_sort/cpp -B target/index_sort/cpp -DCMAKE_BUILD_TYPE=Release
cmake --build target/index_sort/cpp -j
ctest --test-dir target/index_sort/cpp --output-on-failure
INDEX_SORT_ROWS=10000 target/index_sort/cpp/sort_methods_benchmark --benchmark_filter='^soa_rows/' --benchmark_repetitions=5 --benchmark_report_aggregates_only=true --benchmark_out=<soa.json> --benchmark_out_format=json
```

### Rust, C#, and Kotlin

Each port stores the table as a struct of arrays, one array per column. C4 to C6 are
nullable, and like GD's sorts, the ports require sort keys to be set. C2 and C7 hold
German (Umbra) strings, which keep the length and a 4-byte prefix inline:

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

The materializing port variants include appending source rows and gathering every
column into sorted order. They are unstable and require non-null sort keys:

| Port | Variant | Timed operation |
|---|---|---|
| Rust | `key` | Cache numeric keys by value and string keys by reference; `sort_unstable_by`; allocate and move output columns |
| Rust | `reuse_index` | Validate nullable keys once; `sort_unstable_by` on row indexes; reuse index and destination columns |
| Rust | `reuse_key` | Reuse numeric key/index buffers; use index comparisons for strings; reuse destination columns |
| C#, Kotlin | `index` | Custom quicksort on row indexes; allocate index and destination table |
| C#, Kotlin | `reuse` | Same quicksort and comparator; reuse index and destination table |
| C#, Kotlin | `validated` | Reuse buffers and validate nullable keys once before the quicksort |
| C#, Kotlin | `cached` | Cache numeric keys once in reusable buffers; use the reusable index sort for strings |
| C#, Kotlin | `optimized` | Measured per-column policy exposed by `SortOptimized` / `optimizedSort` |

The C#/Kotlin quicksort uses a median-of-three pivot, partitions on equal keys,
recurses into the smaller partition, and uses insertion sort below 17 items. Kotlin
sorts numeric keys and indexes together in two primitive arrays; sortable unsigned
bit keys preserve signed/unsigned integer and `Double.compareTo` ordering. C# applies the same
quicksort directly to typed `(key, row)` structs. C# cached nullable
keys and Kotlin cached validity checks are validated during cache construction.

The C# policy caches numeric keys and reuses index sorting for both string columns.
The Kotlin policy reuses C1, caches C3 to C6, and keeps allocating index sorting
for C2/C7, where repeated policy measurements did not establish a reliable speedup. Rust keeps
the allocating standard-library key sort; neither reusable candidate improved it.

Workspaces are allocated outside timing, but every append, index initialization,
key-cache fill, null-validation scan, comparison, and gather is timed. Workspace
sizes follow `INDEX_SORT_ROWS`. C# reserves four typed key buffers; Kotlin reserves
one `LongArray`; Rust reserves four typed key vectors. These capacities and a second
table trade retained memory for fewer allocations. Rust's safe reusable string path
moves each descriptor and leaves an empty inline string behind; it does not add an
Arc increment or new unsafe code. The original allocating Rust gather retains its
existing ownership-transfer invariant.

### gd-rs order views

[`rust/src/gd_order.rs`](rust/src/gd_order.rs) uses the same generated Rust source
rows and seven column types. C2 uses `ColumnSpec::german_string`; C7 uses ordinary
`DataType::String` storage (`CompactString`). C4 to C6 remain nullable and populated.

- `gd_rs/order_view`: time only the public stable `Table::row_order` operation on
  the existing table. It returns row positions and borrows the table, leaving every
  column in place. Equal keys retain insertion order.
- `gd_rs/view_materialize`: build the same order view over the source, then call
  `source.copy_rows(order.positions())` to gather all columns into a new table in
  that order. Order creation, output allocation, ordinary long-string text copies,
  and destruction are included. The source table is not appended or changed.

`view_materialize` returns physically ordered rows; `order_view` retains only row
positions. Neither gd-rs case appends an input copy first. GD and Rust/C#/Kotlin
benchmarks retain their timed append. C++ SoA creates the
order and materializes directly into retained output buffers.
The gd-rs dependency disables default features and enables only `german-strings`;
no Rayon workers are used. Both cases consume the returned view through `black_box`.
The general `row_order` implementation resolves storage once, copies numeric keys
or borrows other payloads, separates null positions, and stably sorts cached keys.
It supports all storage types, directions, and null policies; it has no knowledge
of this fixture's key ranges, string lengths, or shared prefix.

## Projects

| Port | Harness | Sources |
|---|---|---|
| C++ | Google Benchmark 1.9.5, CMake, GD via [`../cpp-reference/cmake/GdCore.cmake`](../cpp-reference/cmake/GdCore.cmake) | six cases in [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp), with the row iterator in [`../cpp-reference/gd_table_row_iterator.h`](../cpp-reference/gd_table_row_iterator.h); three methods and the `std::sort` index sort, using the same header's comparator, in [`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp) |
| C++ SoA | Google Benchmark 1.9.5, C++20 standard containers | [`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp), [`../cpp-reference/soa_table_row_iterator.h`](../cpp-reference/soa_table_row_iterator.h), [`../cpp-reference/soa_table_key_sort.h`](../cpp-reference/soa_table_key_sort.h) |
| Rust | Criterion 0.8 | [`rust/src/lib.rs`](rust/src/lib.rs), [`rust/benches/sort_methods.rs`](rust/benches/sort_methods.rs) |
| C# | .NET 10, C# 14, BenchmarkDotNet 0.15.8 | [`csharp/`](csharp/), sort in [`csharp/Sorts.cs`](csharp/Sorts.cs), benchmark in [`csharp/SortMethods.cs`](csharp/SortMethods.cs) |
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
and load average in its output directory. Run it from the repository root.

- **`fast`:** the C++ six cases at 1,000 and 10⁶ rows, except GD's three 10⁶-row
  selection sorts.
- **`selection`:** those three selection sorts, one timed iteration each, about an
  hour in total.
- **`methods`:** the C++ methods, all port variants, and gd-rs order views.
- **`ports`:** only port variants and gd-rs views. C# uses 3 warm-ups and 10 measured
  iterations targeting 250 ms, with allocation diagnostics.

```bash
benches/index_sort/run_all.sh fast <out-dir>
```

```bash
benches/index_sort/run_all.sh selection <out-dir>
```

```bash
benches/index_sort/run_all.sh methods <out-dir>
```

The optimization run and summary are reproducible with:

```bash
INDEX_SORT_ROWS=10000 benches/index_sort/run_all.sh ports target/index_sort/results.noindex
python3 benches/index_sort/summarize.py target/index_sort/results.noindex
```

To run just the current comparison table (all seven columns, the existing
optimized policies and both gd-rs workflows), use a fresh output directory:

```bash
INDEX_SORT_ROWS=10000 benches/index_sort/run_comparison.sh target/index_sort/comparison.noindex
```

[`run_comparison.sh`](run_comparison.sh) builds and verifies C++, Rust, C# and Kotlin
before timing them sequentially. It pools two five-repetition C++ processes, runs
the Rust key sort and gd-rs workflows with Criterion, filters C# to `optimized`,
and runs Kotlin's `optimized` policy with three forks and five warm-up/measurement
iterations. It writes raw reports and `summary.json`, records toolchains and source
hashes, and samples process load plus Linux CPU temperature/frequency. Build
parallelism defaults to two jobs and can be set with `INDEX_SORT_BUILD_JOBS`.
The policies stay fixed across hosts; this run does not choose new per-host winners.

The output retains raw Criterion/JMH/BenchmarkDotNet reports, toolchains, load
snapshots, a source diff, and hashes of tracked and untracked source files. Existing
reports are preserved. Run only on an otherwise idle host.

## Results

The measurements, toolchains, and observations are in [RESULTS.md](RESULTS.md).

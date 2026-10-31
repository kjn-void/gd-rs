# Index sort results

These are the results of the six cases and the three sort methods described in the
[README](README.md). The README covers the table formats, the sort algorithms, and how
to run the benchmarks.

The sources of the six-case tables are:

- **C++:** [`cpp/index_sort_benchmark.cpp`](cpp/index_sort_benchmark.cpp), with the
  `std::sort` row iterator in
  [`../cpp-reference/gd_table_row_iterator.h`](../cpp-reference/gd_table_row_iterator.h)
- **Rust, C#, and Kotlin:** the ports' six-case benchmarks at commit
  [`8754801`](https://github.com/kjn-void/gd-rs/tree/8754801/benches/index_sort):
  [`rust/benches/index_sort.rs`](https://github.com/kjn-void/gd-rs/blob/8754801/benches/index_sort/rust/benches/index_sort.rs)
  with [`rust/src/lib.rs`](https://github.com/kjn-void/gd-rs/blob/8754801/benches/index_sort/rust/src/lib.rs),
  [`csharp/`](https://github.com/kjn-void/gd-rs/tree/8754801/benches/index_sort/csharp), and
  [`kotlin/src/main/kotlin/indexsort/`](https://github.com/kjn-void/gd-rs/tree/8754801/benches/index_sort/kotlin/src/main/kotlin/indexsort).
  The ports no longer run these cases.

## Environment

The six-case results were measured on 2026-10-30 on an Apple M6 (12 cores:
2 Super, 4 Performance and 6 Efficiency; 32 GB, macOS 27.0). There was no CPU
pinning, and the load average stayed between 1.6 and 2.1. They were produced with:

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

Every value in the six-case tables includes clearing the target and appending the
source rows before sorting.

In the 1,000- and 10⁶-row tables, the Rust, C#, and Kotlin index sorts (cases 04 to
06) applied the sorted order in place by swapping rows along permutation cycles, as GD
does.

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

## Optimized ports and gd-rs order views, M6, 10,000 rows

Measured on **2026-10-31**, on `192.168.0.5` / Apple M6 (12 cores:
2 Super, 4 Performance and 6 Efficiency; 32 GB, macOS 27.0). All seven columns are
populated. The fixture, including C7's actual 45-byte text and shared 20-byte prefix,
is unchanged. gd-rs
uses the exact Rust-generated rows, German string storage for C2, and ordinary
`CompactString` storage for C7. The other ports retain their shared German C7 strings.

The GD C++ key cells implement [post 21315112](https://www.sweclockers.com/forum/post/21315112),
replacing the previous GD key implementation and figures. The current materializing
gd-rs workflow creates a view and then copies rows in its order.

Sources: [GD key helper](../cpp-reference/gd_table_key_sort.h),
[GD key verification](../cpp-reference/gd_table_key_sort_verify.cpp), [Rust harness](rust/benches/sort_methods.rs), [Rust key sort](rust/src/lib.rs),
[Rust reusable candidates](rust/src/reused.rs), [gd-rs fixture](rust/src/gd_order.rs),
[gd-rs ordering implementation](../../src/table/ordering.rs),
[C# harness/policy](csharp/SortMethods.cs), [C# workspace](csharp/SortWorkspace.cs),
[Kotlin harness](kotlin/src/main/kotlin/indexsort/Benchmarks.kt),
[Kotlin workspace/policy](kotlin/src/main/kotlin/indexsort/SortWorkspace.kt), and
[C++ control](cpp/sort_methods_benchmark.cpp) with its
[typed comparator](../cpp-reference/gd_table_row_iterator.h), and the standard-container
[SoA table/iterator](../cpp-reference/soa_table_row_iterator.h),
[cached-key materializer](../cpp-reference/soa_table_key_sort.h) and
[verifier](../cpp-reference/soa_table_row_iterator_verify.cpp).

Settings:

- Rust 1.99.1, Criterion 0.8.2, the independent benchmark crate's default optimized
  bench profile, no LTO or `target-cpu` override. gd-rs disables default features and
  enables only `german-strings`. Criterion uses 1 s warm-up, 3 s measurement and
  100 samples. Sorts are single-threaded; no Rayon workers or CPU pinning.
- C# 14 / .NET 10.0.13, SDK 10.0.402, BenchmarkDotNet 0.15.8, Release, 3 warm-ups,
  10 measured iterations targeting 250 ms, with memory diagnostics.
- Kotlin 2.4.10, Microsoft OpenJDK 25.0.4.1, JMH 1.21. Ablations use one fork,
  five 1 s warm-ups and five 1 s measurements. The final policy uses two forks,
  three warm-ups and five measurements. C1 and the final C2 control use three
  forks, five warm-ups and five measurements; C2 was also checked with ten
  warm-ups and ten measurements per fork.
- C++ Apple clang 21.0.0, CMake Release (`-O3 -DNDEBUG`), no `-march`, GD
  `a5b1e3e`, Google Benchmark 1.9.5. `std_sort` is the mean of five repetitions; the updated
  GD `key` and `soa_rows` figures each pool two independent five-repetition runs
  (ten samples). All use libc++ and C++20; SoA uses only standard containers.

Steam was stopped before measurement. Runs were sequential, with process/load
snapshots and no observed heavy unrelated CPU work. Base revision was `4cfac1f`
plus the working-tree changes; source diffs, hashes and snapshots are retained with
the reports in `target/index_sort/m6-optimized-20261031/`. `final/` contains the
gd-rs improvement and corrected C# key-cache measurements; `final/policy/` contains
the directly measured policy entry points and Kotlin confirmations. An initial
C# `Span.Sort` cache prototype is excluded from these tables; current cached sorts
use the same custom quicksort as their index baseline.

### Comparison (µs)

GD C++, the C++ `std::sort` index case, Rust, C# and Kotlin include append plus
physical materialization. **C++ SoA key/gather** builds the order from the existing
source and gathers directly into reused output columns and string buffers, without
an input append. **gd-rs view** only constructs a stable order over an existing table.
**gd-rs view + materialize** creates the view, then gathers a new table with
`source.copy_rows(order.positions())`, including copying ordinary long strings and
output destruction. It does not append an input copy first. View-only retains
indexes; materialization produces physically ordered rows.

Point estimates below; the SoA column also shows pooled sample standard deviation.
C++ reports mean CPU time; Criterion reports its fitted estimate; C#/Kotlin report
mean elapsed time. Ablations show the ports' confidence intervals.

| Column | Type | C++ GD key | C++ std::sort index | C++ SoA key/gather | Rust key | C# optimized | Kotlin optimized | gd-rs view | gd-rs view + materialize |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | uint64 | 440.3 | 377.9 | 141.3 ± 0.3 | 116.6 | 145.4 | 153.7 | 69.9 | 235.3 |
| 1 | string(8) | 740.8 | 727.0 | 451.0 ± 13.7 | 173.2 | 183.0 | 289.5 ± 82.3† | 122.4 | 286.6 |
| 2 | double | 445.2 | 375.9 | 146.0 ± 1.8 | 127.2 | 149.5 | 150.3 | 83.5 | 245.5 |
| 3 | double, nullable | 446.0 | 376.9 | 146.0 ± 1.5 | 128.3 | 150.4 | 152.3 | 87.7 | 251.6 |
| 4 | int32, nullable | 440.6 | 366.0 | 153.3 ± 9.3 | 115.6 | 139.4 | 139.1 | 71.0 | 234.2 |
| 5 | uint32, nullable | 454.7 | 365.6 | 136.7 ± 1.5 | 115.3 | 138.4 | 138.5 | 71.5 | 237.7 |
| 6 | string(50) | 924.7 | 808.3 | 570.8 ± 11.3 | 430.0 | 928.3 | 455.3 | 420.0 | 587.5 |

† Kotlin C2 is **inconclusive across forks**. The final policy keeps the original
allocating index algorithm. Its repeated baseline was 229 ± 60 µs and policy was
290 ± 82 µs (99.9% intervals), despite tighter within-fork results in the initial
ablation. Longer warm-up did not remove this variability. Do not infer a C2
speedup or regression from the policy point estimate. Raw samples are preserved.

The measured policies are explicit code paths, not a table assembled from the
minimum sample of each variant:

- Rust retains the allocating `sort_unstable_by` key sort.
- C# caches numeric keys, and uses reusable index sorting for C2/C7.
- Kotlin reuses C1's index/destination, caches C3–C6 in primitive arrays, and keeps
  allocating index sorting for C2/C7. C1 uses the direct reuse entry point.

C# numeric time fell 26–33% from its allocating index baseline, and steady-state
allocation fell from about **1 MB/op to 0 B/op** for every column. Workspace
allocation is outside timing; retained workspace memory is the tradeoff. Kotlin's
C3–C6 numeric times fell 11–23%; the integer-key improvement was confirmed in two
forks (179 ± 0.3 µs baseline versus 137 ± 2.4 µs cached). No Kotlin long-string
reuse gain was established, so C7 keeps its baseline algorithm.

### C++ standard-container SoA key sort and gather

`soa_rows` uses the [typed const row iterator](../cpp-reference/soa_table_row_iterator.h)
and [`KeySortWorkspace<Column, Table>`](../cpp-reference/soa_table_key_sort.h) from the
[C++ harness](cpp/sort_methods_benchmark.cpp). The source has the same GD-generated
values in seven `std::vector`s, with ordinary `std::string` for C2/C7 and
`std::optional` for C4 to C6. Each timed operation caches numeric keys by value or
borrows string views, validates nullable keys once, runs `std::sort` over key/index
pairs, then gathers all seven columns directly from the unchanged source.

The key workspace and output retain capacity across iterations. Long-string text
is copied once into an independent output string whose buffer is reused after
initial sizing. There is no preliminary append or physical whole-row sort. Vector
and key reservation happen outside timing; key fill, validation, sorting and every
column assignment are timed. The returned order is unstable. Retained memory is
the tradeoff, and output allocation/destruction is not repeated each operation.

Only the SoA case was remeasured on 2026-10-31, in two independent five-repetition
processes on each host. The M6 pooled means are 137–153 µs for numeric keys, 451 µs
for C2 and 571 µs for C7. On M6 this is 2.1–2.6× faster on numeric keys than the
archived physical-row implementation. All other comparison cells retain their
previous measurements. The raw current runs, source archive/hashes, compiler flags,
command and load snapshots are in `target/index_sort/m6-optimized-20261031/soa_cpp/`;
the superseded reports remain under `soa_cpp_before_key_gather/`.

The [verifier](../cpp-reference/soa_table_row_iterator_verify.cpp) passes with both
Clang/libc++ and GCC/libstdc++, plus local ASan/UBSan. It covers row integrity,
source immutability, output-buffer reuse, nullable-key rejection/recovery, duplicate
keys, empty/short inputs, invalid/duplicate gather positions, and the iterator's
copy/move and move-only utilities.

```bash
cmake -S benches/index_sort/cpp -B target/index_sort/cpp -DCMAKE_BUILD_TYPE=Release
cmake --build target/index_sort/cpp -j
ctest --test-dir target/index_sort/cpp --output-on-failure
# Run twice with separate output files, then pool all ten repetitions.
INDEX_SORT_ROWS=10000 target/index_sort/cpp/sort_methods_benchmark --benchmark_filter='^soa_rows/' --benchmark_repetitions=5 --benchmark_report_aggregates_only=true --benchmark_out=<soa.json> --benchmark_out_format=json
```

### Updated GD key sort and materialization

The maintained [GD helper](../cpp-reference/gd_table_key_sort.h) follows
[post 21315112](https://www.sweclockers.com/forum/post/21315112): raw cached views,
once-selected numeric comparators, and one uninitialized overwrite buffer shared
by row and metadata gathers. The pinned submodule is unchanged. It lacks the posted
same-type overload, so the helper adapts its string fallback; all fixture strings
have equal lengths within each column. C7 stays the controlled `string(50)` fixture.

The new key cases were measured on the same M6 and flags on 2026-10-31. The table
uses the pooled mean of two five-repetition processes; legacy tables below use the
pooled sample standard deviation after replacing their old GD key cells. Other
legacy cells retain their original measurements. The latest GD key takes about
440–455 µs on numeric columns, 741 µs on C2, and 925 µs on C7.

The two gd-rs columns now have these exact meanings:

- `order_view`: create the stable borrowing order over the existing source.
- `view_materialize`: create that order, then call `source.copy_rows(order.positions())`
  to produce a fresh table whose rows are physically in the requested order. It
  includes allocation, ordinary C7 text copies, and destruction. There is no timed
  append before creating the view. Materialization was verified cell-by-cell for
  every column; repeated C1/C7 measurements agreed closely.

Raw new runs, the independent repeat, and environment/source hashes are in
`target/index_sort/m6-optimized-20261031/latest_gd/`. CTest and an AddressSanitizer
run of the maintained C++ verifier passed; the Rust benchmark release checks passed.
The verifier covers stability, ranges, whole-row and null/status metadata integrity,
empty/short inputs, and metadata larger than data rows.

```bash
cmake -S benches/index_sort/cpp -B target/index_sort/cpp -DCMAKE_BUILD_TYPE=Release
cmake --build target/index_sort/cpp -j
ctest --test-dir target/index_sort/cpp --output-on-failure
# Run twice; pool the two means/standard deviations, five repetitions per run.
INDEX_SORT_ROWS=10000 target/index_sort/cpp/sort_methods_benchmark --benchmark_filter='^key/' --benchmark_repetitions=5 --benchmark_report_aggregates_only=true --benchmark_out=<key.json> --benchmark_out_format=json
INDEX_SORT_ROWS=10000 cargo bench --manifest-path benches/index_sort/rust/Cargo.toml --bench sort_methods -- '^gd_rs/view_materialize/'
```

### General gd-rs improvement

`Table::row_order` resolves the physical column layout once, caches numeric keys by
value or borrows other key payloads, separates null positions, and stably sorts
cached keys. Comparisons no longer repeat storage dispatch or fallible cell access.
It applies to every supported storage family, with no assumptions about this
fixture's ranges, lengths, or prefixes. Stable ties, independent null placement,
NaN payload ordering and signed zeros are preserved. The returned position vector
may reuse the key allocation and retain spare capacity; temporary key storage is
an O(rows) memory tradeoff.

The baseline used the [pre-change public implementation at 4cfac1f](https://github.com/kjn-void/gd-rs/blob/4cfac1f/src/table.rs).
Both rounds use the same checked-in harness and data. A second complete view-only
run matched the improved estimates within 1% on every column.

Sources: [Rust row-order harness](rust/benches/sort_methods.rs) and
[ordering implementation](../../src/table/ordering.rs). There is no exact C++
counterpart to this borrowing-view comparison; the [C++ sort harness](cpp/sort_methods_benchmark.cpp)
materializes rows, as described above.

| Column | Before view | Improved view | Speedup |
|---|---:|---:|---:|
| 0 | 289.06 | 69.89 | 4.14× |
| 1 | 380.97 | 122.44 | 3.11× |
| 2 | 312.41 | 83.48 | 3.74× |
| 3 | 351.89 | 87.69 | 4.01× |
| 4 | 309.24 | 71.01 | 4.36× |
| 5 | 295.53 | 71.48 | 4.13× |
| 6 | 686.02 | 420.04 | 1.63× |

The general regression tests cover required/nullable columns across every storage
family, both directions and null placements, stable duplicate keys, empty/all-null
columns, NaN payloads and signed zero. Full `scripts/ci.sh`, the independent Rust
benchmark's release tests/Clippy, Rust 1.86 checks, C# verification and Kotlin tests
passed. No new unsafe code was added.

### Rust ablation (µs, 95% confidence intervals)

Sources: [Rust harness](rust/benches/sort_methods.rs), [key sort](rust/src/lib.rs)
and [reusable buffers](rust/src/reused.rs). The [C++ cases](cpp/sort_methods_benchmark.cpp)
use different storage/materialization; there is no exact C++ counterpart to all three ablations.

The allocating key sort wins on every column. Safe buffer reuse avoids allocation
but adds descriptor/default writes and retains scratch buffers; it was not promoted
to the Rust policy. The custom quicksort was replaced by `sort_unstable_by`.

| Column | key | reuse_index | reuse_key |
|---|---:|---:|---:|
| 0 | 116.60 ± 0.22 | 144.19 ± 0.09 | 134.23 ± 0.18 |
| 1 | 173.17 ± 0.12 | 182.27 ± 0.61 | 180.27 ± 0.31 |
| 2 | 127.20 ± 0.03 | 148.22 ± 0.12 | 139.11 ± 0.28 |
| 3 | 128.34 ± 0.03 | 179.63 ± 0.11 | 140.92 ± 0.15 |
| 4 | 115.57 ± 0.04 | 168.36 ± 0.10 | 135.27 ± 0.09 |
| 5 | 115.28 ± 0.06 | 167.24 ± 0.40 | 135.00 ± 0.07 |
| 6 | 430.03 ± 0.07 | 452.90 ± 0.11 | 452.78 ± 0.12 |

### C# ablation (µs, 99.9% confidence intervals)

Sources: [C# harness](csharp/SortMethods.cs) and [workspace](csharp/SortWorkspace.cs).
The related [Rust](rust/benches/sort_methods.rs) and [C++](cpp/sort_methods_benchmark.cpp)
benchmarks use different storage/allocation policies, rather than exact counterparts to each C# ablation.

| Column | index | reuse | validated | cached | optimized |
|---|---:|---:|---:|---:|---:|
| 0 | 196.70 ± 1.38 | 155.63 ± 0.45 | 156.12 ± 0.40 | 148.30 ± 0.51 | 145.37 ± 0.46 |
| 1 | 223.83 ± 0.99 | 183.13 ± 0.23 | 183.79 ± 1.23 | 185.50 ± 1.04 | 182.99 ± 0.63 |
| 2 | 204.85 ± 1.53 | 162.50 ± 0.85 | 161.80 ± 0.73 | 149.19 ± 0.44 | 149.49 ± 0.92 |
| 3 | 214.47 ± 1.88 | 174.32 ± 0.80 | 168.77 ± 0.51 | 150.94 ± 0.47 | 150.44 ± 1.47 |
| 4 | 205.05 ± 0.97 | 164.95 ± 0.62 | 155.85 ± 0.77 | 139.02 ± 0.45 | 139.35 ± 1.24 |
| 5 | 205.84 ± 1.54 | 165.49 ± 0.16 | 157.04 ± 0.33 | 139.04 ± 0.38 | 138.39 ± 0.96 |
| 6 | 974.04 ± 8.42 | 929.94 ± 13.33 | 921.92 ± 19.02 | 933.79 ± 27.22 | 928.29 ± 19.36 |

### Kotlin ablation (µs, 99.9% confidence intervals)

Sources: [Kotlin harness](kotlin/src/main/kotlin/indexsort/Benchmarks.kt) and
[workspace](kotlin/src/main/kotlin/indexsort/SortWorkspace.kt). The related
[Rust](rust/benches/sort_methods.rs) and [C++](cpp/sort_methods_benchmark.cpp)
benchmarks use different storage/allocation policies, rather than exact counterparts to each Kotlin ablation.

| Column | index | reuse | validated | cached | optimized |
|---|---:|---:|---:|---:|---:|
| 0 | 164.81 ± 3.09 | 154.96 ± 1.41 | 155.57 ± 4.90 | 157.41 ± 0.53 | 153.70 ± 1.58 |
| 1 | 195.35 ± 2.57 | 185.98 ± 2.43 | 181.31 ± 2.06 | 186.15 ± 1.60 | 289.50 ± 82.31 |
| 2 | 168.97 ± 4.02 | 160.22 ± 3.71 | 165.07 ± 41.76 | 149.76 ± 1.74 | 150.28 ± 2.87 |
| 3 | 192.51 ± 1.06 | 184.44 ± 1.89 | 165.20 ± 0.76 | 154.72 ± 3.06 | 152.34 ± 0.62 |
| 4 | 179.82 ± 1.36 | 172.00 ± 1.14 | 153.40 ± 1.23 | 139.45 ± 0.43 | 139.10 ± 2.36 |
| 5 | 178.90 ± 2.46 | 172.92 ± 1.75 | 166.67 ± 93.42 | 140.04 ± 2.56 | 138.50 ± 0.73 |
| 6 | 469.01 ± 120.65 | 466.93 ± 24.95 | 475.33 ± 90.45 | 478.49 ± 169.08 | 455.26 ± 4.68 |

The first four Kotlin columns are the initial single-fork ablation. The optimized
column uses the multi-fork confirmations described above; notably C2's broader
uncertainty captures the cross-fork variability missed by its single-fork interval.
C7's policy retains the allocating index path; differences across runs are not an
algorithm change or evidence that reuse wins.

### Reproduction

From the repository root on an idle M6:

```bash
INDEX_SORT_ROWS=10000 benches/index_sort/run_all.sh ports <out-dir>
python3 benches/index_sort/summarize.py <out-dir>
```

The table's additional confirmations ran sequentially after the main ablation:

```bash
# General gd-rs improvement and an independent full view-only repeat.
INDEX_SORT_ROWS=10000 cargo bench --manifest-path benches/index_sort/rust/Cargo.toml --bench sort_methods -- '^gd_rs/'
INDEX_SORT_ROWS=10000 cargo bench --manifest-path benches/index_sort/rust/Cargo.toml --bench sort_methods -- '^gd_rs/order_view/'
# C++ control, after the normal CMake Release build.
INDEX_SORT_ROWS=10000 target/index_sort/cpp/sort_methods_benchmark --benchmark_filter='^std_sort/' --benchmark_repetitions=5 --benchmark_report_aggregates_only=true
# Directly measured C# policy (Release build).
INDEX_SORT_ROWS=10000 dotnet benches/index_sort/csharp/bin/Release/net10.0/IndexSort.dll --filter '*optimized*' --warmupCount 3 --iterationCount 10 --iterationTime 250 --memory --exporters json
# Directly measured Kotlin policy (after ./gradlew test mainBenchmarkJar).
INDEX_SORT_ROWS=10000 java -jar benches/index_sort/kotlin/build/benchmarks/main/jars/index-sort-bench-main-jmh-JMH.jar 'indexsort.SortMethods.index' -p variant=optimized -f 2 -wi 3 -i 5 -w 1s -r 1s
# C1 direct reuse follow-up and C2 baseline control.
INDEX_SORT_ROWS=10000 java -jar benches/index_sort/kotlin/build/benchmarks/main/jars/index-sort-bench-main-jmh-JMH.jar 'indexsort.SortMethods.index' -p column=0,1 -p variant=optimized -f 3 -wi 5 -i 5 -w 1s -r 1s
INDEX_SORT_ROWS=10000 java -jar benches/index_sort/kotlin/build/benchmarks/main/jars/index-sort-bench-main-jmh-JMH.jar 'indexsort.SortMethods.index' -p column=1 -p variant=index,optimized -f 3 -wi 5 -i 5 -w 1s -r 1s
```

## Rock / Orange Pi 5 Plus, 10,000 rows (µs)

Measured on **2026-10-31** on `172.30.30.2` (`rock`), an Orange Pi 5 Plus / RK3588
with 16 GB RAM, four Cortex-A76 cores up to 2.4 GHz and four Cortex-A55 cores up to
1.8 GHz. The OS is Ubuntu 26.04.1, running Linux `7.0.0-31-generic-64k` with 64 KiB
pages. Sorts are single-threaded, without CPU affinity; Linux's `ondemand` governor
is unchanged. This uses the same 10,000-row fixture shape and fixed policies as the
M6 comparison, including both gd-rs workflows. All nullable columns are populated.

Sources: [C++ harness](cpp/sort_methods_benchmark.cpp),
[updated GD key sort](../cpp-reference/gd_table_key_sort.h),
[C++ SoA iterator](../cpp-reference/soa_table_row_iterator.h) and
[key materializer](../cpp-reference/soa_table_key_sort.h),
[Rust harness](rust/benches/sort_methods.rs) and [key sort](rust/src/lib.rs),
[gd-rs fixture](rust/src/gd_order.rs) and [ordering](../../src/table/ordering.rs),
[C# policy](csharp/SortMethods.cs) and [workspace](csharp/SortWorkspace.cs),
[Kotlin harness](kotlin/src/main/kotlin/indexsort/Benchmarks.kt) and
[policy/workspace](kotlin/src/main/kotlin/indexsort/SortWorkspace.kt).

Settings:

- GCC 15.2.0 / libstdc++, CMake 4.2.3, C++20 Release (`-O3 -DNDEBUG`), no architecture
  override, Google Benchmark 1.9.5. Every C++ cell pools two independent processes
  with five repetitions each; values use mean CPU time.
- Rust 1.99.1 and Criterion 0.8.2, default optimized bench profile, no LTO or
  `target-cpu` override. The recorded Cargo fingerprint has empty `rustflags`.
  gd-rs disables default features and enables `german-strings`. Criterion requests
  1 s warm-up, 3 s measurement and 100 samples, extending collection when necessary.
- .NET 10.0.13 / SDK 10.0.402, BenchmarkDotNet 0.15.8, Release, 3 warm-ups and 10
  measured iterations targeting 250 ms, with memory diagnostics.
- Microsoft OpenJDK 25.0.4.1, Kotlin 2.4.10 through the project's Gradle 9.8.2 wrapper,
  JMH 1.21, three forks, five 1 s warm-ups and five 1 s measurements per fork.
  The Java and .NET versions match the M6; the system apt Kotlin/Gradle are unused.

GD C++, the C++ `std::sort` index case, Rust, C# and Kotlin include appending the
source and producing physically ordered rows. **C++ SoA key/gather** sorts cached
keys from the existing source and gathers directly into retained output columns
and string buffers, without an input append. **gd-rs view** creates only a stable
order over the existing source;
**gd-rs view + materialize** creates the view and copies rows into a new table,
including ordinary long-string text copies and output destruction, without first
appending an input copy. C++ SoA uses ordinary `std::string` in both string columns;
Rust/C#/Kotlin retain their shared German strings, and gd-rs uses German C2 and
ordinary C7 strings. These are comparisons of the specified storage and algorithms.

The SoA column shows mean ± pooled sample standard deviation. Kotlin shows mean ±
99.9% confidence interval; its fork means and all other uncertainty estimates are
retained in the raw reports. Other cells are point estimates.

| Column | Type | C++ GD key | C++ std::sort index | C++ SoA key/gather | Rust key | C# optimized | Kotlin optimized | gd-rs view | gd-rs view + materialize |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | uint64 | 2683.7 | 2699.0 | 1182.0 ± 34.4 | 751.6 | 1286.3 | 1281.4 ± 33.5 | 300.9 | 1624.4 |
| 1 | string(8) | 4082.4 | 3856.5 | 2282.4 ± 18.0 | 981.6 | 1536.2 | 2250.1 ± 42.9 | 570.9 | 1901.8 |
| 2 | double | 2790.6 | 2739.8 | 1235.7 ± 21.5 | 830.6 | 1331.5 | 1166.8 ± 46.4 | 396.1 | 1721.6 |
| 3 | double, nullable | 2807.5 | 2749.7 | 1262.4 ± 32.6 | 810.1 | 1337.4 | 1135.9 ± 41.8 | 410.8 | 1784.2 |
| 4 | int32, nullable | 2714.9 | 2604.2 | 1120.2 ± 36.7 | 735.3 | 1139.1 | 1081.5 ± 45.6 | 293.4 | 1648.1 |
| 5 | uint32, nullable | 2692.0 | 2613.4 | 1119.6 ± 27.9 | 760.9 | 1160.5 | 1065.7 ± 26.5 | 314.4 | 1624.4 |
| 6 | string(50) | 4368.9 | 4048.1 | 2650.2 ± 47.4 | 2633.7 | 4505.2 | 3637.7 ± 77.1 | 2103.5 | 3455.6 |

The C++ SoA case was remeasured separately in two five-repetition processes after
changing it to cached-key sorting and direct reusable materialization. Numeric
means are now 1120–1262 µs, C2 is 2282 µs and C7 is 2650 µs. Numeric cases are
2.9–3.1× faster than the archived physical-row implementation. Only this column
changed. Raw new runs and source/command/load evidence are in
`target/index_sort/rock-comparison-20261031/soa_cpp/`; the original C++ reports retain
the superseded SoA case alongside the unchanged GD key and `std::sort` index cells.

The C++ source and output still own ordinary strings, so text is copied into output
buffers. Rust's shared German strings share text and its current policy allocates
key/output storage per operation; the C++ workspace retains storage. These are
explicit storage and allocation choices in addition to their library sort routines.

All four languages' verification passed before timing. The cached-key/gather and
iterator checks pass with GCC/libstdc++, Clang/libc++ and local ASan/UBSan. The OS
upgrade and apt installation completed before the published run; the earlier
overlapping run is excluded. The original run's sampled thermal zones ranged from
52.7 to 63.8°C. The separate
SoA rerun has its own temperature/frequency snapshots; neither published run shows
heavy unrelated CPU work.
The base is `index-sort` at `4cfac1f`, with the recorded working-tree changes and GD
`a5b1e3e`; the submodule is unchanged.

Reproduce from the repository root, using the matched user-local toolchains on Rock:

```bash
export DOTNET_ROOT="$HOME/.local/share/index-sort-toolchains/dotnet"
export JAVA_HOME="$HOME/.local/share/index-sort-toolchains/jdk-25.0.4.1"
export PATH="$DOTNET_ROOT:$JAVA_HOME/bin:$HOME/.cargo/bin:$PATH"
INDEX_SORT_ROWS=10000 benches/index_sort/run_comparison.sh <new-output-dir>
```

The [comparison runner](run_comparison.sh) builds and verifies before measuring each
harness sequentially. Raw output, the exact command, toolchains, source hashes,
Cargo fingerprint, source archive and load/temperature/frequency snapshots are in
`target/index_sort/rock-comparison-20261031/`; [summarize.py](summarize.py) reproduces
the table's inputs. No Pi 5 timing results are included.

## Three sort methods in four languages, 10,000 rows (µs)

GD's three sort methods, the C++ `std::sort` index sort that is not upstream, and each
port's sort, for every column of the 10,000-row table with every column set. The historical port sources are frozen at `4cfac1f`; the current optimized results
are above. Sources for this measurement are
[`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp),
[`rust/benches/sort_methods.rs`](https://github.com/kjn-void/gd-rs/blob/4cfac1f/benches/index_sort/rust/benches/sort_methods.rs) with the sort in
[`rust/src/lib.rs`](https://github.com/kjn-void/gd-rs/blob/4cfac1f/benches/index_sort/rust/src/lib.rs), [`csharp/SortMethods.cs`](https://github.com/kjn-void/gd-rs/blob/4cfac1f/benches/index_sort/csharp/SortMethods.cs)
with the sort in [`csharp/Sorts.cs`](https://github.com/kjn-void/gd-rs/blob/4cfac1f/benches/index_sort/csharp/Sorts.cs), and
[`kotlin/src/main/kotlin/indexsort/Benchmarks.kt`](https://github.com/kjn-void/gd-rs/blob/4cfac1f/benches/index_sort/kotlin/src/main/kotlin/indexsort/Benchmarks.kt)
with the sort in [`Sorts.kt`](https://github.com/kjn-void/gd-rs/blob/4cfac1f/benches/index_sort/kotlin/src/main/kotlin/indexsort/Sorts.kt).

It was measured on 2026-10-30 on the same M6 and toolchains as above, with a load
average between 0.8 and 1.7, using `run_all.sh methods`. C++ ran 5 repetitions; Rust
and Kotlin used the 1,000-row settings; C# used BenchmarkDotNet's default job. ± has
the same meaning as in the environment table.

| Column | Type | C++ GD physical | C++ GD index | C++ GD key | C++ `std::sort` index | Rust key | C# index | Kotlin index |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 0 | uint64 | 82,321 ± 720 | 1,128 ± 0.58 | 440.3 ± 0.9 | 379 ± 0.73 | 134 ± 0.12 | 198 ± 0.37 | 161 ± 0.47 |
| 1 | string(8) | 110,481 ± 4.3 | 1,399 ± 10 | 740.8 ± 6.1 | 763 ± 35 | 176 ± 0.06 | 229 ± 0.46 | 188 ± 0.26 |
| 2 | double | 82,927 ± 42 | 1,290 ± 2.0 | 445.2 ± 3.4 | 373 ± 0.48 | 140 ± 0.06 | 206 ± 0.33 | 166 ± 0.57 |
| 3 | double | 82,895 ± 340 | 1,288 ± 3.3 | 446.0 ± 3.0 | 375 ± 1.7 | 143 ± 0.15 | 218 ± 0.22 | 188 ± 0.39 |
| 4 | int32 | 88,613 ± 91 | 1,248 ± 2.0 | 440.6 ± 2.5 | 366 ± 2.6 | 131 ± 0.07 | 210 ± 0.30 | 176 ± 0.29 |
| 5 | uint32 | 88,545 ± 59 | 1,263 ± 1.0 | 454.7 ± 11.6 | 365 ± 0.26 | 128 ± 0.06 | 208 ± 0.47 | 179 ± 1.4 |
| 6 | string(50) | 127,048 ± 190 | 1,676 ± 1.9 | 924.7 ± 3.0 | 811 ± 5.4 | 630 ± 1.8 | 960 ± 2.9 | 509 ± 160 |

This measurement kept the sort with the lowest total over the seven columns in an earlier
run, on the same M6 the same afternoon, in which every port had a physical, an index,
and a key sort. The totals below are from that run, in µs. That code is at commit
[`906a410`](https://github.com/kjn-void/gd-rs/tree/906a410/benches/index_sort). All
three sorts produced the same row order, and all but the physical sort built a new
table.

| Port | Physical total | Index total | Key total | Kept |
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
- **GD key figures have been replaced by the latest implementation.** These cells
  are measured on 2026-10-31, while the other cells in this historical comparison
  remain from 2026-10-30. Use the current comparison above for the optimized ports
  and the two current gd-rs workflows. The new GD source is the maintained
  [key helper](../cpp-reference/gd_table_key_sort.h); no old key values are retained.

## Three sort methods, 10,000 rows (µs)

This reproduces the follow-up benchmark in
[forum post 21314807](https://www.sweclockers.com/forum/post/21314807), described in the
[README](README.md#three-sort-methods). The sources are
[`cpp/sort_methods_benchmark.cpp`](cpp/sort_methods_benchmark.cpp) and the updated
[key helper](../cpp-reference/gd_table_key_sort.h). This block records the C++ fixture and forum figures; the cross-language comparison
is above.

Physical/index figures were measured on 2026-10-30 on the same M6 and C++ toolchain,
with a load average below 1.1, and show means of five repetitions ± standard deviation.
M6 key figures replace the old results with the pooled mean ± standard deviation of
ten repetitions on 2026-10-31. Physical/index Dell columns are from the original post. The key column is replaced
by the latest [post 21315112](https://www.sweclockers.com/forum/post/21315112); that
post does not identify the machine or report C2 and labels C7 `rstring(50)`. Missing
C2 is marked —. These forum key values are not a controlled hardware/fixture match
to the M6 results. M6 key values are the new pooled 2026-10-31 measurements.

| Column | Type | M6 physical | M6 index | M6 key | Dell physical | Dell index | Latest forum key |
|---|---|---:|---:|---:|---:|---:|---:|
| 0 | uint64 | 80,835 ± 40 | 1,106 ± 1.6 | 440.3 ± 0.9 | 157,747 | 2,882 | 924 |
| 1 | string(8) | 110,235 ± 60 | 1,357 ± 0.9 | 740.8 ± 6.1 | 173,485 | 2,587 | — |
| 2 | double | 81,329 ± 7.3 | 1,293 ± 0.6 | 445.2 ± 3.4 | 126,247 | 2,217 | 937 |
| 3 | double | 81,350 ± 33 | 1,279 ± 9.1 | 446.0 ± 3.0 | 121,365 | 2,253 | 981 |
| 4 | int32 | 88,499 ± 21 | 1,235 ± 0.4 | 440.6 ± 2.5 | 124,804 | 2,406 | 923 |
| 5 | uint32 | 88,536 ± 27 | 1,245 ± 0.6 | 454.7 ± 11.6 | 113,424 | 2,188 | 1,140 |
| 6 | string(50) | 124,299 ± 118 | 1,700 ± 0.7 | 924.7 ± 3.0 | 213,917 | 2,537 | 1,665 |

- **The updated key sort beats the recorded M6 index sort on every column**, taking
  about 45–66% less time. The M6 key cells replace the previous algorithm's values.
- **Selection sort remains much slower:** approximately 134–201× the updated M6 key
  sort on this fixture.
- The newest forum key figures are reproduced as reported, including 1,140 µs for
  uint32 and the unreported short-string case. No speed ratio is inferred across
  its unspecified hardware and changed C7 label.

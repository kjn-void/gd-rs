# Text filtering and transforms: GD and gd-rs

This comparison exercises text directly in ordinary application tables: filtering, rewriting a string column, and filtering plus materializing and transforming rows. It compares four representations of the same five-column data.

| Case | Storage and access |
|---|---|
| GD buffer | Unmodified `gd::table::table_column_buffer`, row-major (AoS), preallocated bounded inline text slots |
| C++ std::string | `std::vector<StringRow>`, AoS rows with three ordinary `std::string` fields; a STL baseline rather than a GD table |
| gd-rs CompactString | Existing `gd::Table` SoA string storage, checked typed descriptor slices |
| gd-rs fixed buffer | `gd::Table` SoA, one fixed-slot byte buffer per string column; every row has an offset and length |

The new gd-rs storage is a supported table layout, with UTF-8 byte limits, nullability, conversions, atomic failed writes, independent copies, append between layouts, compaction, indexes, and disjoint parallel mutable views. See the [API](../api/tables.md#fixed-capacity-string-buffers) and [storage implementation](../../src/table/fixed_string.rs).

## Findings

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

- For small tables, dispatch dominates: eight workers take roughly 18–24 microseconds at 32 rows, while direct execution takes less than 1.5 microseconds in every case.
- With 1,000,000 rows and 16-byte messages, ordinary C++ `std::string` is fastest with one worker for all three operations. Fixed buffers cost more memory here and slow filtering and the complete pipeline relative to existing gd-rs strings.
- With 128-byte messages at the same row count, existing gd-rs strings filter fastest with both worker counts. Both gd-rs layouts rewrite text substantially faster than the two C++ cases in this workload.
- For the 128-byte pipeline with eight workers, gd-rs fixed buffers take 3.14 ms, GD buffers 4.09 ms, existing gd-rs strings 6.17 ms, and C++ `std::string` 7.09 ms. The fixed-buffer lead was reproduced in the targeted rechecks below.
- At 128 bytes, fixed-buffer transform peak RSS is about 318 MiB versus 516 MiB for existing gd-rs strings. GD uses about 295 MiB. At 16 bytes, existing short-string storage is smaller than the new fixed slots.

The winning representation changes with string length, operation, row count, and worker count. These results demonstrate working text operations in both layouts; they do not support a blanket claim that column storage fails when text is present.

## Conditions and interpretation

Measured 2026-10-07 on Apple M3 Max, 16 logical CPUs, macOS-27.0.1-arm64-arm-64bit-Mach-O. Rust: `rustc 1.98.1 (48a229cea 2026-09-01)`. C++: `Apple clang version 21.0.0 (clang-2100.3.34.2)`. GD is pinned to `cb11cff90d05260a88d59a30c9da421cd4e19c34`. The gd-rs worktree was based on `5f85227ba35373039e710e6e8acb6eb59493d6e9`; exact source hashes are in the raw results.

Rust flags: `release -O3; codegen-units=1; lto=thin; target-cpu=native; no-default-features; features=rayon; --locked`. C++ flags: `Release -O3 -DNDEBUG -march=native; interprocedural optimization ON; sanitizers OFF`. Scheduling: OS scheduling, no affinity; persistent pools, exactly one static row task per worker; one process at a time.

Final timings come from rounds that passed checks for competing background work. The runner sampled unrelated process CPU time before timing and after every round and rejected sustained activity above its one-busy-core threshold, confirming elevated snapshots with a second sample. OS scheduling and ordinary desktop activity remain part of these host measurements.

Unrelated CPU snapshots ranged from 11.7% to 167.8% (100% is one CPU core; only processes above 5% are counted). These are boundary snapshots, not continuous profiling. The final run checked 288 implementation/size/worker/workload combinations against the independent oracle and recorded 1152 timed processes. Each process recorded 7 calibrated batches; the minimum batch target was 50 ms. Tables report the median of 4 process-round medians. Round ranges and all samples are retained in the raw JSON.

1 round was discarded because substantial background work returned. Discarded samples and waiting-period load snapshots are kept in the raw JSON and excluded from the result tables.

[Raw measurements](measurements/text-workflow-m3max.json) contain commands, source fingerprints, compiler versions, topology, thermal state, background load, and process RSS. The GD source fingerprint was unchanged.

## Workload and timing boundaries

Rows: 32, 100, 1,000, 10,000, 100,000, 1,000,000. Message lengths: 16, 128 ASCII bytes. Workers: 1, 8. The schema is `id: u64`, `region: text`, `message: text`, `output: text`, and `score: u64`. IDs and message prefixes vary by row. `north` occurs every fourth row, `error` every third row, and score is `row % 100`.

- Filter: collect ordered row positions where region is `north`, score is at least 20, and message contains `error`. Approximately 6.67% of large tables match.
- Transform: rewrite every output cell as ASCII uppercase message plus `|ok`. The output becomes three bytes longer. The source column is unchanged.
- Pipeline: filter, copy all five columns into independently owned output tables, then transform their output strings. Each worker returns its own ordered shard; no final merge is timed.

Pool construction, fixture generation, and full output digest checks are outside timing. One-worker operations execute directly; eight-worker operations dispatch exactly eight disjoint row tasks. Transform measures repeated writes after output capacity has warmed. Filter and pipeline include result allocation and destruction. These timing boundaries are the same for all cases.

GD gathers whole rows through its public row-buffer API. The benchmark schema contains inline strings and no null metadata or indexed references, so byte copying owns the complete payload. The STL case copies `StringRow` values, including string ownership. gd-rs uses native column-wise `copy_rows`. GD and fixed-buffer transforms use one scratch string per worker; the ordinary C++/Rust strings rewrite their own output buffers. Search functions, case-conversion loops, runtime scheduling, bounds checks, and UTF-8 validation also differ; this experiment does not isolate layout or language alone.

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

![Text workload scaling](measurements/text-workflow-m3max.png)

## Filtering text

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

Times are microseconds per complete operation; lower is faster.

### 16-byte messages

| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 0.0908 | 0.0747 | 0.0703 | 0.185 |
| 32 | 8 | 22.3 | 22.4 | 19 | 18.5 |
| 100 | 1 | 0.292 | 0.229 | 0.279 | 0.711 |
| 100 | 8 | 22.9 | 22.7 | 19.4 | 19.8 |
| 1,000 | 1 | 2.3 | 1.7 | 2.54 | 6.71 |
| 1,000 | 8 | 24.4 | 24.4 | 21.9 | 22.2 |
| 10,000 | 1 | 21.3 | 15.5 | 24.1 | 66.1 |
| 10,000 | 8 | 31.1 | 30.2 | 28.6 | 42.4 |
| 100,000 | 1 | 216 | 157 | 253 | 664 |
| 100,000 | 8 | 58.7 | 50.8 | 81.7 | 139 |
| 1,000,000 | 1 | 2.17e+03 | 1.58e+03 | 2.38e+03 | 6.6e+03 |
| 1,000,000 | 8 | 684 | 722 | 523 | 1.07e+03 |

### 128-byte messages

| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 0.1 | 0.0837 | 0.0702 | 0.194 |
| 32 | 8 | 21.7 | 21.8 | 18.9 | 18.7 |
| 100 | 1 | 0.361 | 0.292 | 0.264 | 0.775 |
| 100 | 8 | 22.6 | 22.8 | 18.9 | 19.7 |
| 1,000 | 1 | 2.97 | 2.63 | 2.3 | 7.42 |
| 1,000 | 8 | 25 | 24.7 | 21.7 | 22.2 |
| 10,000 | 1 | 32.3 | 27.8 | 21.1 | 72.1 |
| 10,000 | 8 | 31 | 31 | 26.5 | 44.5 |
| 100,000 | 1 | 396 | 295 | 220 | 730 |
| 100,000 | 8 | 197 | 107 | 78.6 | 149 |
| 1,000,000 | 1 | 1.5e+04 | 1.07e+04 | 2.69e+03 | 7.73e+03 |
| 1,000,000 | 8 | 2.75e+03 | 2.27e+03 | 1.02e+03 | 1.25e+03 |

## Rewriting text

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

Times are microseconds per complete operation; lower is faster.

### 16-byte messages

| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 0.478 | 0.145 | 0.52 | 0.34 |
| 32 | 8 | 21.8 | 22.4 | 18.4 | 18.2 |
| 100 | 1 | 1.46 | 0.4 | 1.5 | 1.02 |
| 100 | 8 | 23.2 | 22.4 | 20.7 | 19.3 |
| 1,000 | 1 | 14.5 | 3.76 | 14.8 | 10.1 |
| 1,000 | 8 | 27.1 | 24 | 24.9 | 23 |
| 10,000 | 1 | 147 | 38.5 | 144 | 100 |
| 10,000 | 8 | 46 | 31.9 | 68.3 | 51.7 |
| 100,000 | 1 | 1.48e+03 | 402 | 1.5e+03 | 1.01e+03 |
| 100,000 | 8 | 229 | 80.1 | 335 | 217 |
| 1,000,000 | 1 | 1.48e+04 | 4.01e+03 | 1.58e+04 | 1.02e+04 |
| 1,000,000 | 8 | 1.99e+03 | 766 | 2.45e+03 | 1.86e+03 |

### 128-byte messages

| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 1.35 | 1.45 | 0.376 | 0.383 |
| 32 | 8 | 23.1 | 22.8 | 18.3 | 18.5 |
| 100 | 1 | 4.12 | 4.46 | 1.13 | 1.14 |
| 100 | 8 | 24.2 | 24 | 19.6 | 19.8 |
| 1,000 | 1 | 41.5 | 45.3 | 11.6 | 11.5 |
| 1,000 | 8 | 31.5 | 32.6 | 25 | 23 |
| 10,000 | 1 | 411 | 448 | 120 | 114 |
| 10,000 | 8 | 144 | 86.3 | 60.5 | 55 |
| 100,000 | 1 | 4.11e+03 | 4.47e+03 | 1.39e+03 | 1.15e+03 |
| 100,000 | 8 | 1.21e+03 | 623 | 312 | 379 |
| 1,000,000 | 1 | 4.33e+04 | 4.46e+04 | 1.19e+04 | 1.14e+04 |
| 1,000,000 | 8 | 1.12e+04 | 5.93e+03 | 3.59e+03 | 3.34e+03 |

## Filter, copy, and transform

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

Times are microseconds per complete operation; lower is faster.

### 16-byte messages

| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 0.169 | 0.0993 | 0.2 | 0.377 |
| 32 | 8 | 22.6 | 21.5 | 20.7 | 21.2 |
| 100 | 1 | 0.488 | 0.311 | 0.562 | 1.22 |
| 100 | 8 | 24.7 | 23 | 24.8 | 26.5 |
| 1,000 | 1 | 3.51 | 2.28 | 4.34 | 10.1 |
| 1,000 | 8 | 26.1 | 25.1 | 31 | 34 |
| 10,000 | 1 | 36.2 | 23.9 | 46 | 97.4 |
| 10,000 | 8 | 32.8 | 31.8 | 58.8 | 74 |
| 100,000 | 1 | 353 | 234 | 440 | 1.01e+03 |
| 100,000 | 8 | 92.9 | 92.6 | 167 | 243 |
| 1,000,000 | 1 | 3.74e+03 | 2.57e+03 | 4.8e+03 | 1.19e+04 |
| 1,000,000 | 8 | 1.17e+03 | 1.3e+03 | 1.44e+03 | 2.13e+03 |

### 128-byte messages

| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 0.227 | 0.176 | 0.245 | 0.401 |
| 32 | 8 | 23.5 | 21.8 | 21.5 | 21.9 |
| 100 | 1 | 0.796 | 0.835 | 0.853 | 1.32 |
| 100 | 8 | 30.1 | 23.1 | 27.4 | 28 |
| 1,000 | 1 | 7.78 | 9.37 | 7.98 | 10.9 |
| 1,000 | 8 | 27.8 | 29.1 | 43.3 | 34.9 |
| 10,000 | 1 | 74.2 | 96.4 | 95.8 | 112 |
| 10,000 | 8 | 51.7 | 68.6 | 89.6 | 83.4 |
| 100,000 | 1 | 840 | 1.1e+03 | 969 | 1.13e+03 |
| 100,000 | 8 | 315 | 550 | 624 | 298 |
| 1,000,000 | 1 | 2.23e+04 | 2.55e+04 | 1.79e+04 | 1.89e+04 |
| 1,000,000 | 8 | 4.09e+03 | 7.09e+03 | 6.17e+03 | 3.14e+03 |

## Scaling at the largest size

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

1,000,000 rows. Speedup is one-worker time divided by eight-worker time. A value below 1 means the parallel run is slower.

| Text bytes | Operation | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---|---:|---:|---:|---:|
| 16 | filter | 3.17× | 2.19× | 4.55× | 6.16× |
| 16 | transform | 7.44× | 5.24× | 6.47× | 5.50× |
| 16 | pipeline | 3.20× | 1.97× | 3.34× | 5.55× |
| 128 | filter | 5.46× | 4.69× | 2.64× | 6.18× |
| 128 | transform | 3.89× | 7.53× | 3.32× | 3.41× |
| 128 | pipeline | 5.44× | 3.60× | 2.90× | 6.02× |

## Process memory

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

Peak RSS at 1,000,000 rows during transform, in MiB. This includes the source fixture, verification, worker runtime, and allocator overhead, so it is not an isolated table footprint.

| Text bytes | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |
|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 81.7 | 85.6 | 86.2 | 104.4 |
| 16 | 8 | 82.0 | 85.7 | 86.4 | 104.6 |
| 128 | 1 | 295.4 | 393.2 | 515.8 | 318.0 |
| 128 | 8 | 295.7 | 393.4 | 516.1 | 318.3 |

On this 64-bit host, ordinary Rust/C++ string descriptors are 24 bytes and use inline storage for these short values. The new fixed-buffer descriptors use two `usize` fields (16 bytes) plus each reserved slot. Region capacity is 8 bytes, message capacity is its byte length, and output capacity is message length plus 3. Slots are reserved even for nulls; this benchmark has no nulls. Larger-than-needed capacities increase the footprint.

## Targeted rechecks and variability

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [runner and independent oracle](../../benches/text_workflow/compare.py).

After the full matrix, 27 selected configurations were repeated in three fresh processes each, with five batches per process and a 25 ms minimum batch target. This covers every 1,000,000-row, 128-byte configuration and three additional configurations with wide round ranges. All 81 output digests matched the independent oracle. The [confirmation measurements](measurements/text-workflow-confirmations.json) retain commands, individual samples, and boundary load checks. Executable sources and optimization settings match the primary run; these targeted runs do not replace its four rotated process rounds.

Times below are milliseconds per operation. Ranges are the minimum and maximum process medians, not confidence intervals.

| Configuration | Primary median | Primary range | Recheck median | Recheck range |
|---|---:|---:|---:|---:|
| GD, 10,000 rows, 128 bytes, 8 workers, transform | 0.144 | 0.082–0.146 | 0.142 | 0.142–0.143 |
| CompactString, 100,000 rows, 16 bytes, 1 worker, transform | 1.50 | 1.28–1.76 | 1.70 | 1.30–1.78 |
| CompactString, 1,000,000 rows, 128 bytes, 1 worker, filter | 2.69 | 2.67–3.41 | 3.50 | 3.38–3.58 |
| CompactString, 1,000,000 rows, 16 bytes, 1 worker, transform | 15.84 | 14.36–16.89 | 14.65 | 13.90–15.21 |

The long-message single-worker CompactString filter changed appreciably between runs, while remaining faster than the other representations. Close comparisons should not be treated as stable rankings: GD and STL single-worker long-message transform exchanged order in the rechecks. The eight-worker long-message pipeline result remained clear: fixed buffer 3.10 ms, GD 4.11 ms, CompactString 6.22 ms, and STL 7.08 ms.

## What the experiment establishes

Sources: [Rust workloads](../../benches/text_workflow/driver.rs), [GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), [fixed-buffer storage](../../src/table/fixed_string.rs).

Text is supported in the gd-rs column layout, including filtering, longer output strings, owned materialization, and parallel writes. The fixed buffer removes per-cell string allocations but adds offsets, reserved capacity, and UTF-8 validation on borrow. It is an alternative storage contract rather than a guaranteed speed improvement. The measurements above determine which implementation is faster for each tested operation and size.

The comparison covers fixed-length ASCII messages and one selectivity. Unicode boundary behavior is tested for correctness, but Unicode case folding, variable-length distributions, joins, random edits, database import, nullable scans, and oversized-write performance are outside this timing matrix. It does not establish a universal AoS/SoA ranking or any OLAP preprocessing cost.

Validation: the repository CI script passed formatting, strict Clippy, all-features tests, minimal-feature tests, rustdoc, and the Rust 1.86 library check. Fixed-string tests include randomized equivalence with ordinary tables, UTF-8 byte limits, null/empty values, failed-write atomicity, copies, append between layouts, compaction, index/sort/format integration, and scoped parallel mutation. Separate C++ AddressSanitizer runs passed the oracle on 1, 32, and 1,001 rows with one and eight workers for both C++ representations and all three operations; [all 36 checks are recorded](measurements/text-workflow-asan.json). Sanitized executables were not used for performance measurements.

Reproduce the final matrix:

```sh
./benches/run_text_workflow.sh --samples 7 --rounds 4 --sample-ms 50 --wait-for-idle
```

The [benchmark README](../../benches/text_workflow/README.md) describes smaller runs and the diagnostic mode. The [report generator](../../benches/text_workflow/summarize.py) recreates these tables and optional charts from the raw JSON.

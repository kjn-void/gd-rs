# Arc scaling and GD comparison on Apple M6

Sources: [Arc scaling harness](../../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../../src/table/shared_record.rs), [Rust comparison driver](../../../benches/filter_copy/driver.rs), [C++ comparison driver](../../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.

gd-rs Arc is **1.377× faster with eight workers than with one** in the core sweep. In the comparison with GD, its geometric-mean speed is **1.676× GD memcpy** across all 12 cases. Each string size, selection rate and worker count receives equal weight within its comparison. Arc shares records; GD and the other three representations deep-copy their payloads.

Measured from 2026-10-09T10:20:04.026054+00:00 (UTC).

**Contended diagnostics:** these runs retain current host load and OS scheduling. The primary results and separate confirmations are both retained.

## Current implementation

Sources: [Arc scaling harness](../../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../../src/table/shared_record.rs), [Rust comparison driver](../../../benches/filter_copy/driver.rs), [C++ comparison driver](../../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.

`SharedRecordTable<Record>` stores one contiguous `Vec<Arc<Record>>`. Each record contains three `u64` fields and two `CompactString` fields. Rayon processes source chunks in parallel, evaluating the numeric predicate and cloning each matched Arc handle into a worker-local vector. The local vectors are concatenated in source order into one destination table. The implementation uses two safe public APIs:

- `par_filter_chunked(chunk_rows, predicate)` gives each source chunk a local vector reserved to its source length, avoiding local growth. It concatenates the matched handles in source order without additional Arc clones.
- `par_drop()` consumes the result and releases its handles on Rayon workers, joining before returning. Ordinary `drop` remains sequential. Small targets below 4,096 handles stay on the caller; larger targets use a minimum cleanup grain of 4,096.

The M6 comparison uses **1 source chunk per worker** (approximately 125,000 source rows per chunk at eight workers). At one worker it uses serial filtering and cleanup. Reserving each local vector to its source-chunk length avoids growth during filtering; concatenation moves handles without cloning them again. Joined parallel cleanup distributes reference-count decrements across the Rayon workers.

Primary-batch CPU time divided by elapsed time averages **7.19 active-core equivalents** at eight workers. This measures concurrent CPU activity, including scheduling and any worker spinning; it does not measure useful work alone. Each record has its own reference count: there is no single global counter shared by all rows. Per-record atomic operations and pointer chasing still remain; greater CPU use does not imply linear speedup. Moving reference-count updates between caller and worker cores can also create cache-coherence traffic, but this experiment does not isolate that cost with hardware counters.

The destination remains readable after source destruction and preserves input order. No record or string payload is copied. Construction, temporary buffers, concatenation and completed cleanup are all timed. The source stays alive during timing, so target cleanup releases handles without freeing record or string payloads.

## Scaling across cores

Sources: [Arc scaling harness](../../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../../src/table/shared_record.rs), [Rust comparison driver](../../../benches/filter_copy/driver.rs), [C++ comparison driver](../../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.

| Workers | gd-rs Arc speed vs 1 worker | gd-rs Arc active CPU cores |
|---:|---:|---:|
| 1 | 1.000× | 1.00 |
| 2 | 1.174× | 1.94 |
| 4 | 1.225× | 3.60 |
| 6 | 1.239× | 5.41 |
| 8 | 1.377× | 7.19 |
| 12 | 1.209× | 10.01 |

![gd-rs Arc performance relative to GD memcpy](images/arc-scaling-m6.png)

The graph uses the comparison at one and eight workers, where GD was also measured. Higher is faster, with GD memcpy at 1×. The table above shows the separate Arc-only sweep at all six worker counts.

CPU-core equivalents are process CPU seconds divided by wall seconds inside each primary batch, excluding source construction and verification. The process clock sums work across threads; it does not identify particular physical cores or core types. The M6 has two Super, four Performance and six Efficiency cores. Eight workers give the best measured aggregate time; twelve use more CPU but take longer. Without affinity or hardware counters, this test cannot separate core placement from memory-system and scheduling costs.

| Text bytes | Selected | gd-rs Arc build ms | gd-rs Arc cleanup ms |
|---:|---:|---:|---:|
| 16 | 10% | 0.652 | 0.233 |
| 16 | 50% | 1.014 | 0.786 |
| 16 | 90% | 1.280 | 1.060 |
| 128 | 10% | 0.706 | 0.213 |
| 128 | 50% | 1.038 | 0.805 |
| 128 | 90% | 1.316 | 1.080 |

Stage timings come from seven separately instrumented operations after the primary batches; they are diagnostic and are not added to the primary timing samples.

## Comparison with GD and standard C++ containers

Sources: [Arc scaling harness](../../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../../src/table/shared_record.rs), [Rust comparison driver](../../../benches/filter_copy/driver.rs), [C++ comparison driver](../../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.

The tables and graphs show GD row memcpy, standard C++ `std::vector`/`std::string`, gd-rs CompactString, gd-rs fixed buffers, and gd-rs Arc. These representations were measured together. One million source rows contain three numbers and two 16- or 128-byte strings; 10%, 50% or 90% of rows are selected into one ordered target. The first four variants deep-copy values. gd-rs Arc shares records and times handle release while the source remains alive. C++ and the other Rust variants use caller-thread cleanup.

The [M6 memory-layout illustration](filter-copy-results.md) shows the five representations, inline versus heap strings, and copied versus shared payloads.

**Speed relative to GD** (`GD time / variant time`; higher is faster). Geometric means weight each case equally.

| Group | GD memcpy | C++ STL std::string | gd-rs CompactString | gd-rs fixed buffer | gd-rs Arc |
|---|---:|---:|---:|---:|---:|
| All 12 cases | 1.000× | 0.610× | 0.573× | 0.670× | 1.676× |
| 1 worker, 16 B | 1.000× | 0.958× | 0.676× | 0.338× | 1.117× |
| 1 worker, 128 B | 1.000× | 0.604× | 0.491× | 1.137× | 3.400× |
| 8 workers, 16 B | 1.000× | 0.588× | 0.860× | 0.497× | 0.830× |
| 8 workers, 128 B | 1.000× | 0.408× | 0.377× | 1.060× | 2.504× |

![M6 performance relative to GD memcpy](images/filter-copy-arc-m6.png)

| Text bytes | Workers | Selected | GD memcpy ms | C++ STL std::string ms | gd-rs CompactString ms | gd-rs fixed buffer ms | gd-rs Arc ms |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 10% | 1.237 | 1.165 | 2.064 | 2.548 | 0.956 |
| 16 | 1 | 50% | 2.107 | 2.315 | 2.896 | 6.825 | 2.226 |
| 16 | 1 | 90% | 2.868 | 3.152 | 4.056 | 11.177 | 2.524 |
| 16 | 8 | 10% | 0.718 | 0.867 | 0.898 | 1.083 | 0.935 |
| 16 | 8 | 50% | 1.373 | 2.268 | 1.630 | 3.211 | 1.823 |
| 16 | 8 | 90% | 2.338 | 5.769 | 2.476 | 5.412 | 2.364 |
| 128 | 1 | 10% | 7.315 | 8.717 | 6.906 | 5.928 | 2.088 |
| 128 | 1 | 50% | 8.675 | 14.681 | 28.425 | 7.203 | 3.373 |
| 128 | 1 | 90% | 11.062 | 24.933 | 30.128 | 11.194 | 2.535 |
| 128 | 8 | 10% | 2.594 | 4.348 | 4.558 | 2.930 | 1.329 |
| 128 | 8 | 50% | 5.992 | 16.056 | 19.651 | 5.041 | 1.821 |
| 128 | 8 | 90% | 7.904 | 25.980 | 25.602 | 6.979 | 3.232 |

## Confirmation measurements

Sources: [Arc scaling harness](../../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../../src/table/shared_record.rs), [Rust comparison driver](../../../benches/filter_copy/driver.rs), [C++ comparison driver](../../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.

The case with the largest relative round-median range in each text-size/worker group was repeated. The 128-byte, eight-worker, 90%-selection case was also repeated to check timing variability. Primary and repeat results are reported separately.

| Text bytes | Workers | Selected | Primary gd-rs Arc / GD speed | Repeat gd-rs Arc / GD speed |
|---:|---:|---:|---:|---:|
| 16 | 1 | 50% | 0.947× | 0.939× |
| 16 | 8 | 90% | 0.989× | 0.810× |
| 128 | 1 | 90% | 4.364× | 4.340× |
| 128 | 8 | 50% | 3.290× | 3.287× |
| 128 | 8 | 90% | 2.446× | 3.351× |

At 128 bytes, eight workers and 90% selection, gd-rs Arc takes **3.232 ms** in the primary run and **2.356 ms** in the repeat. This demonstrates substantial run-to-run variability. Host scheduling, allocation layout and cache state are possible contributors; this experiment does not isolate them. Read the reported ratios as workload-specific diagnostics under contention, with the repeat measurements showing their practical uncertainty.

## Reproduction and validation

Sources: [Arc scaling harness](../../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../../src/table/shared_record.rs), [Rust comparison driver](../../../benches/filter_copy/driver.rs), [C++ comparison driver](../../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.

- Host `M6.local`; `Apple M6`; 32 GiB; 128-byte cache lines; niceness zero; no affinity.
- Compiler: `rustc 1.99.0 (b940084d7 2026-09-28)`; comparison C++: `Apple clang version 21.0.0 (clang-2100.3.34.2)`.
- Core sweep: 4 rotated rounds, 7 batches, calibrated to at least 50 ms; the Arc pipeline uses the public APIs described above.
- Full comparison: 6 rotated process rounds, 7 batches, at least 50 ms; native release builds; Rust thin LTO, C++ IPO, without sanitizers.
- Million-row source-destruction checks and boundary/ownership checks pass for every reported implementation, using an independent Python value oracle.
- Every timed process verifies the complete ordered result outside its timing. Reference counts return to one after Arc target cleanup. Source and executable fingerprints are retained and remained unchanged.
- Separate AddressSanitizer tests passed for Arc filtering, cleanup and process-clock FFI; no sanitizer timings enter these results. Leak detection was disabled because Apple ASan does not support it.
- Library tests cover ordering, duplicate handles, copy-on-write, final-owner destruction, empty input and predicate panics. Full repository CI and Rust 1.86 checks passed.

[Core raw data](../measurements/arc-scaling-m6.json), [GD comparison](../measurements/filter-copy-arc-m6.json), [confirmations](../measurements/filter-copy-arc-m6-confirmations.json), [additional 90%-selection repeat](../measurements/filter-copy-arc-m6-regression.json).

```sh
python3 benches/filter_copy/arc_scaling.py --strategies chunks-par-drop \
  --workers 1 2 4 6 8 12 --rounds 4 --samples 7 --sample-ms 50 --verify-full --allow-contended
python3 benches/filter_copy/compare.py --arc-parallel arc_chunks --rounds 6 --allow-contended
# Exact oracle/output paths and confirmation case arguments are in each raw file's invocation metadata.
```

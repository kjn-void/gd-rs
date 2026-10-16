# Interned string copying with entry handles

Interned columns use thin `Arc` entry handles. Copying a table, range or row list
clones those handles without hashing the strings or acquiring the lookup-cache
mutex. Entry reference counts track live cells; the last entry reference removes
its weak cache entry. Each entry owns an immutable `Arc<str>` payload and a weak
reference to its interner. Public string values, checked writes and selection
APIs are unchanged. Copies remain independently mutable and still take O(rows)
time; they are not constant-time selection handles.

This comparison uses four variants:

- **Default:** current gd-rs `CompactString` columns.
- **Baseline:** the interned implementation at `ba8c32a`, whose copies hash each
  string to update logical cell counts under the pool lock.
- **Entry:** current interned columns with reference-counted entry handles.
- **GD:** C++ GD `rstring` columns at `cb11cff90d05260a88d59a30c9da421cd4e19c34`.

Sources: [current interned storage](../../src/table/interned_string.rs),
[baseline interned storage](https://github.com/kjn-void/gd-rs/blob/ba8c32ab8fefbe1b90d3f009fecb1e33c8c1f7f3/src/table/interned_string.rs),
[Rust workload](../../benches/interned_strings/workload.rs),
[C++ workload](../../benches/cpp-reference/interned_strings.cpp),
[comparison runner](../../benches/interned_strings/compare.py), and
[methodology](../../benches/interned_strings/README.md).

## Reproduction and workload

The experiment uses 20,000 rows, one required string column, and 1, 16, 256,
4,096 or 20,000 distinct values. Strings have fixed lengths of 16 or 128 bytes,
with deterministic ASCII IDs repeated evenly in row order. It measures insertion,
complete table copying, replacement with existing values (`rotate`), and
replacement with entirely new unique values (`replace`). Input generation,
mutation fixtures, result destruction and full-content checks are outside the
timer. Insertion and copying include allocation and normal API overhead.

Build a baseline from a committed revision into a fresh directory, then run:

```sh
python3 benches/interned_strings/baseline.py ba8c32a \
  --output target/interned-strings/rebuilt-baseline
./benches/run_interned_strings.sh --wait-for-idle --rounds 4 --samples 5 \
  --baseline-dir target/interned-strings/rebuilt-baseline \
  --output target/interned-strings/entry-results.json
```

Both current and baseline Rust use release optimization level 3, thin LTO, one
codegen unit, native CPU targeting and no crate features. C++ uses C++20,
`-O3 -DNDEBUG -march=native -flto=thin`. Measurements are sequential, single-threaded
and use OS scheduling without affinity. Implementation order rotates over four
rounds, each containing five samples. The baseline builder and raw report record
source revisions, compiler versions, source hashes and executable hashes.
The [full raw report](measurements/interned-entry-copy-m1.json) contains all
accepted samples, memory results, rejected rounds and run-segment metadata.

The host is an Apple M1 MacBook Air (`MacBookAir10,1`), with eight physical/logical
CPU cores and 8 GiB RAM, running macOS 27.0.1. Rust is 1.99.0 with LLVM 23.1.1;
C++ uses Apple Clang 21.0.0. The full matrix spans October 15–16, 2026. The runner
waits for quiet periods and rejects complete rounds when its background-load checks detect
sustained unrelated activity. It also rejects rounds with excessive process
off-CPU time, sample duration or sample spread. The limits and all rejected
records are preserved in the raw report.

The initial run exposed system-suspension gaps inside native samples. The runner
was extended with interruption checks, and the checkpoint was audited before
resuming. Five previously accepted rounds were discarded and remeasured. Earlier
retained records were checked by duration and spread; the resumed segment also
records process wall/CPU time. The same native executables and workload sources
were used throughout, with hashes checked between segments and after completion.

C++ table copying uses its native copy constructor plus a schema-reference
increment in the benchmark adapter, compensating for the pinned constructor's
missing acquisition. No upstream source is modified. C++ copies its reference
pool; both Rust interned variants share payloads. All variants have independently
mutable cells. C++ does not have Rust's synchronized lookup cache.

## Copy time

Sources: [Rust workload](../../benches/interned_strings/workload.rs),
[C++ workload](../../benches/cpp-reference/interned_strings.cpp), and
[runner](../../benches/interned_strings/compare.py).

Milliseconds to copy all 20,000 rows, reported as the median of four round
medians (five samples per round). Lower is better.

| Bytes/string | Distinct | Default | Baseline | Entry | GD | Baseline / Entry |
|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 0.0281 | 0.1485 | 0.0505 | 0.0031 | 2.94× |
| 16 | 16 | 0.0276 | 0.1474 | 0.0498 | 0.0036 | 2.96× |
| 16 | 256 | 0.0278 | 0.1490 | 0.0498 | 0.0104 | 2.99× |
| 16 | 4,096 | 0.0281 | 0.1681 | 0.0505 | 0.1112 | 3.33× |
| 16 | 20,000 | 0.0279 | 0.1827 | 0.0511 | 0.5278 | 3.58× |
| 128 | 1 | 0.3393 | 0.3526 | 0.0511 | 0.0036 | 6.90× |
| 128 | 16 | 0.3238 | 0.3517 | 0.0498 | 0.0040 | 7.07× |
| 128 | 256 | 0.3275 | 0.3574 | 0.0504 | 0.0111 | 7.09× |
| 128 | 4,096 | 0.3330 | 0.3873 | 0.0509 | 0.1038 | 7.60× |
| 128 | 20,000 | 0.3309 | 0.4181 | 0.0519 | 0.4826 | 8.05× |

Entry copies take about 0.050–0.052 ms across this matrix: **2.94–3.58× faster**
than the baseline for 16-byte strings and **6.90–8.05× faster** for 128-byte strings.
They are insensitive to payload length because copying touches entry handles,
not string bytes. Default inline short strings still copy about 1.8× faster than
Entry; for allocated 128-byte strings, Entry is about 6.4–6.6× faster than Default.
GD copies a small repeated dictionary faster, while its time grows with the
number of distinct payloads it copies. These costs include cell-vector copying;
borrowed and owning selections have different semantics and are not measured here.

An independent four-round [copy confirmation](measurements/interned-entry-copy-m1-confirmation.json)
on October 16 repeated the one-value and all-unique endpoints with the same
[Rust](../../benches/interned_strings/workload.rs) and
[C++](../../benches/cpp-reference/interned_strings.cpp) workloads:

| Bytes/string | Distinct | Baseline ms | Entry ms | Baseline / Entry |
|---:|---:|---:|---:|---:|
| 16 | 1 | 0.1699 | 0.0579 | 2.93× |
| 16 | 20,000 | 0.1969 | 0.0543 | 3.63× |
| 128 | 1 | 0.3514 | 0.0521 | 6.74× |
| 128 | 20,000 | 0.4098 | 0.0513 | 7.98× |

The relative gains repeat despite variation in absolute runtimes between runs.
Reproduction command, after the initial build:

```sh
./benches/run_interned_strings.sh --skip-build --wait-for-idle \
  --baseline-dir target/interned-strings/rebuilt-baseline \
  --operations copy --unique 1 20000 --lengths 16 128 --rounds 4 --samples 5 \
  --output target/interned-strings/entry-copy-confirmation.json
```

## Insertion and mutation

Sources: [Rust workload](../../benches/interned_strings/workload.rs),
[C++ workload](../../benches/cpp-reference/interned_strings.cpp), and
[runner](../../benches/interned_strings/compare.py).

Insertion times, in milliseconds:

| Bytes/string | Distinct | Default | Baseline | Entry | GD |
|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 0.225 | 0.586 | 0.484 | 0.122 |
| 16 | 16 | 0.222 | 0.589 | 0.486 | 0.479 |
| 16 | 256 | 0.202 | 0.601 | 0.512 | 4.485 |
| 16 | 4,096 | 0.201 | 0.911 | 0.742 | 67.992 |
| 16 | 20,000 | 0.201 | 1.828 | 1.430 | 329.587 |
| 128 | 1 | 0.457 | 1.406 | 1.119 | 0.162 |
| 128 | 16 | 0.470 | 1.358 | 1.046 | 0.511 |
| 128 | 256 | 0.476 | 1.379 | 1.210 | 4.546 |
| 128 | 4,096 | 0.469 | 1.770 | 1.566 | 94.909 |
| 128 | 20,000 | 0.483 | 2.665 | 2.454 | 460.305 |

Mutation times, in milliseconds. `rotate` reuses existing dictionary values;
`replace` ends with 20,000 new distinct values regardless of the initial dictionary.
Each cell contains `rotate / replace`.

| Bytes/string | Initial distinct | Default | Baseline | Entry | GD |
|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 0.196 / 0.192 | 0.738 / 1.966 | 0.410 / 1.380 | 0.104 / 328.976 |
| 16 | 16 | 0.194 / 0.191 | 0.733 / 1.958 | 0.404 / 1.381 | 0.433 / 329.737 |
| 16 | 256 | 0.196 / 0.192 | 0.739 / 1.963 | 0.410 / 1.393 | 4.430 / 338.467 |
| 16 | 4,096 | 0.200 / 0.192 | 0.783 / 1.972 | 0.445 / 1.566 | 67.877 / 464.863 |
| 16 | 20,000 | 0.201 / 0.194 | 0.871 / 2.140 | 0.492 / 2.384 | 334.011 / 1027.903 |
| 128 | 1 | 0.611 / 0.588 | 1.781 / 3.003 | 1.096 / 2.425 | 0.154 / 450.575 |
| 128 | 16 | 0.617 / 0.596 | 1.779 / 3.019 | 1.087 / 2.319 | 0.493 / 450.721 |
| 128 | 256 | 0.614 / 0.607 | 1.835 / 3.173 | 1.140 / 2.546 | 4.439 / 481.382 |
| 128 | 4,096 | 0.634 / 0.615 | 1.931 / 3.032 | 1.182 / 2.654 | 93.400 / 660.864 |
| 128 | 20,000 | 0.623 / 0.605 | 2.032 / 2.852 | 1.238 / 3.403 | 457.606 / 1453.921 |

Entry insertion improves over the baseline throughout this matrix, and `rotate`
improves because removing a non-final cell no longer hashes or updates the cache.
Default strings remain faster for insertion and mutation. Replacing an initially
all-unique column exposes the cost of new entry allocations and final-entry
cleanup: Entry is about 11% slower than Baseline for 16-byte values and 19% slower
for 128-byte values in the full run. This tradeoff matters for high-cardinality,
write-heavy columns even when copying is faster.

A separate October 16 [replacement confirmation](measurements/interned-entry-copy-m1-replace-confirmation.json)
used the same [Rust](../../benches/interned_strings/workload.rs) and
[C++](../../benches/cpp-reference/interned_strings.cpp) workloads with four rounds
of five samples. It reproduced the all-unique regression at about 9% for short
strings and 15% for long strings:

| Bytes/string | Default ms | Baseline ms | Entry ms | GD ms |
|---:|---:|---:|---:|---:|
| 16 | 0.194 | 2.161 | 2.359 | 998.057 |
| 128 | 0.621 | 2.914 | 3.354 | 1,452.116 |

```sh
./benches/run_interned_strings.sh --skip-build --wait-for-idle \
  --baseline-dir target/interned-strings/rebuilt-baseline \
  --operations replace --unique 20000 --lengths 16 128 --rounds 4 --samples 5 \
  --output target/interned-strings/entry-replace-confirmation.json
```

GD's [reference lookup](../../external/gd/source/gd_table.cpp) scans its reference
collection; its larger insertion and mutation costs at high cardinality follow
that algorithm. The result applies to
the pinned GD `rstring` path and this workload, not to all C++ string layouts.
The benchmark does not test reads in isolation, parallel mutation, null-heavy
columns, variable-length strings, or application-level selection pipelines.

## Retained memory

Sources: [Rust workload](../../benches/interned_strings/workload.rs),
[Rust observer](../../benches/interned_strings/memory.rs),
[C++ workload](../../benches/cpp-reference/interned_strings.cpp), and
[C++ observer](../../benches/cpp-reference/interned_strings_memory.h).

Separate executables count retained allocator block sizes, including size-class
rounding, using `malloc_size` on macOS or `malloc_usable_size` on Linux. These
figures exclude allocator metadata, unused arena pages, stack, code, input
catalogues and the prebuilt schema. They are not peak RSS or allocation traffic.
The timing binaries have no allocation counters.

KiB (1,024 bytes), one populated table. The final two columns show additional
storage for an interned copy while its source remains alive:

| Bytes/string | Distinct | Default | Baseline | Entry | GD | Baseline copy extra | Entry copy extra |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 480.4 | 320.7 | 160.8 | 160.1 | 320.1 | 160.1 |
| 16 | 16 | 480.4 | 322.0 | 162.7 | 160.9 | 320.1 | 160.1 |
| 16 | 256 | 480.4 | 342.6 | 194.6 | 174.0 | 320.1 | 160.1 |
| 16 | 4,096 | 480.4 | 656.6 | 688.6 | 384.0 | 320.1 | 160.1 |
| 16 | 20,000 | 480.4 | 1,761.6 | 2,539.1 | 1,353.5 | 320.1 | 160.1 |
| 128 | 1 | 2,980.4 | 320.9 | 160.9 | 160.2 | 320.1 | 160.1 |
| 128 | 16 | 2,980.4 | 324.0 | 164.7 | 162.6 | 320.1 | 160.1 |
| 128 | 256 | 2,980.4 | 374.6 | 226.6 | 202.0 | 320.1 | 160.1 |
| 128 | 4,096 | 2,980.4 | 1,168.6 | 1,200.6 | 832.0 | 320.1 | 160.1 |
| 128 | 20,000 | 2,980.4 | 4,261.6 | 5,039.1 | 3,541.0 | 320.1 | 160.1 |

The entry layout halves incremental copy storage from about 320 to 160 KiB by
using eight-byte cell handles in place of sixteen-byte `Arc<str>` handles.
For one distinct 128-byte value, the table also falls from 321 to 161 KiB, about
94.6% below default storage. The tradeoff is an extra entry allocation per distinct
value. With 20,000 distinct 128-byte values, the entry table retains about 5,039 KiB,
18.2% more than the baseline interned table and 69.1% more than default strings.
Additional copies are smaller, so the total cost depends on how many copies live
at once. Shared allocations are counted once.

Appends between different interners can still share immutable text, but each
interner has its own entries. Deleting the last cell in one pool removes that
pool's entry even if another pool retains the same payload. No strong reference
cycle is formed between entries and caches.

## Correctness and scope

The [Rust workload](../../benches/interned_strings/workload.rs) and
[C++ workload](../../benches/cpp-reference/interned_strings.cpp) validate every
timed result against independently generated full-content digests. Both ordinary
and allocation-counting executables check result contents, unchanged source
contents, and a copy surviving its source. Verification mode also mutates the
result to check source independence. All 160 implementation/workload cases passed
verification and memory checks; all 3,200 accepted timed operations passed their checks.
The full run's memory results match the separate verification pass in all cases.

The [storage unit tests](../../src/table/interned_string.rs) cover required and
nullable copies while the lookup-cache mutex is locked, entry cleanup after
copy/drop/replacement, independent pools sharing a payload, and a last-entry
destructor racing with publication of a replacement. The
[integration tests](../../tests/table_interned_strings.rs) cover concurrent table,
range and row-list copies with mutation and source independence, alongside the
existing borrowed, owning and shared-selection behavior. The full CI script
passes formatting, Clippy, all-feature and minimal-feature tests, doctests,
documentation and the Rust 1.86 library check. No unsafe code was added.

This is a single-machine comparison of these storage paths. Fixed-length ASCII
payloads, one column, sequential operations and retained allocator block sizes
do not predict every application workload. Entry copying remains O(rows), and
its per-value allocation and extra read indirection should be considered when
choosing storage for mostly unique data.

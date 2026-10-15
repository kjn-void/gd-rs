# Interned string columns: memory and throughput

This comparison measures gd-rs's default `CompactString` column, its opt-in
`Arc<str>` interned column, and C++ GD's `rstring` reference column. It uses one
required string column and 20,000 rows, with 1, 16, 256, 4,096 or 20,000 distinct
values. The corresponding average rows per value are 20,000, 1,250, 78.1, 4.88 and
1. Strings are either 16 bytes (inline in `CompactString`) or 128 bytes (allocated).

Sources: [Rust workload](../../benches/interned_strings/workload.rs),
[C++ workload](../../benches/cpp-reference/interned_strings.cpp),
[runner and independent checksum oracle](../../benches/interned_strings/compare.py),
[full methodology](../../benches/interned_strings/README.md).

## Reproduction and scope

Measured on 2026-10-15 on an Apple M1 MacBook Air (`MacBookAir10,1`), eight logical
CPUs and 8 GiB RAM, Darwin 27.0.0 arm64. Rust was 1.99.0
(`b940084d7`, LLVM 23.1.1); C++ was Apple Clang 21.0.0
(`clang-2100.3.34.2`). Rust used the release profile, optimization level 3,
one codegen unit, thin LTO, native CPU targeting, and no crate features. C++ used
`-std=c++20 -O3 -DNDEBUG -march=native -flto=thin`. Both ran one thread, under OS
scheduling without affinity. Sanitizers were disabled for measurement.

The gd-rs worktree was based on `1126787f8e6b5dfbe964f6bb2f0cdd81a4d2c08f`
with the uncommitted interned-column and benchmark implementation. C++ GD was
`cb11cff90d05260a88d59a30c9da421cd4e19c34`. The raw report records SHA-256 hashes
of the Rust sources, benchmark sources and executables, complete build commands,
thermal state and background CPU samples. The runner verifies those hashes again
after measurement and checks that the C++ reference sources remain unchanged.
A Clippy allowance was subsequently added to the memory driver; rebuilding produced
byte-for-byte identical release executables. Both reports record this source-only
lint cleanup separately from the original measurement hashes.

```sh
./benches/run_interned_strings.sh --wait-for-idle
```

This builds both timing and memory executables. After that build, the measured
run can be repeated with `--skip-build --wait-for-idle`. Defaults are three rounds
of five individual samples; implementation order rotates across rounds. Timings
are medians of round medians, in milliseconds per whole 20,000-row operation.
Result destruction and full-content checks are outside the timer. Insertion and
copying include allocation; mutations include releasing replaced cells and use
independently populated tables created before timing. Input string catalogues
are prebuilt. All APIs include their usual validation and temporary value costs.

C++ copying uses the native copy constructor with a benchmark-side schema
reference increment to compensate for its missing acquisition in this pinned
revision. No upstream code is changed. Rust copies independently mutable cell
vectors and shares interned payloads; C++ copies its reference pool. These are
complete table copies, not immutable selection handles. The single-thread results
include Rust's synchronized interner costs; C++'s reference pool has no matching
internal synchronization.

Every executable's result is checked against a Python FNV-1a digest of every
expected row. Verification also checks that writing a result does not change its
source and that a copy survives destruction of its immediate source.

## Interpreting the memory figures

Sources: [Rust memory observer](../../benches/interned_strings/memory.rs),
[Rust workload](../../benches/interned_strings/workload.rs),
[C++ observer](../../benches/cpp-reference/interned_strings_memory.h) and
[C++ workload](../../benches/cpp-reference/interned_strings.cpp).

Separate executables measure retained allocator block bytes using macOS
`malloc_size` (Linux uses `malloc_usable_size`). They include size-class rounding,
but exclude allocator metadata, unused arena pages, stack, code, input catalogues
and the already-created schema. This is live heap storage, not peak RSS or total
allocation traffic. C++ counts `new`/`delete` storage, including the table and its
reference pool; Rust counts `System` allocations. Counters are absent from the
timing binaries. Shared payloads are counted once.
Only the raw report's uninstrumented `measurements` entries contribute to the
timing tables; durations in its instrumented `memory` entries are not used.

“Table” means one newly populated table. “Copy extra” is additional heap retained
by its copy with the original alive. “After new values” is an independently
populated result after replacing all rows with unique, previously unseen strings;
the original table remains alive but has a separate interner.


KiB (1,024 bytes), rounded to one decimal:

| Bytes/string | Distinct | Default table | Default copy extra | Interned table | Interned copy extra | GD table | GD copy extra |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 480.4 | 480.1 | 320.7 | 320.1 | 160.1 | 160.1 |
| 16 | 16 | 480.4 | 480.1 | 322.0 | 320.1 | 160.9 | 160.9 |
| 16 | 256 | 480.4 | 480.1 | 342.6 | 320.1 | 174.0 | 174.0 |
| 16 | 4,096 | 480.4 | 480.1 | 656.6 | 320.1 | 384.0 | 384.0 |
| 16 | 20,000 | 480.4 | 480.1 | 1,761.6 | 320.1 | 1,353.5 | 1,353.5 |
| 128 | 1 | 2,980.4 | 2,980.1 | 320.9 | 320.1 | 160.2 | 160.2 |
| 128 | 16 | 2,980.4 | 2,980.1 | 324.0 | 320.1 | 162.6 | 162.6 |
| 128 | 256 | 2,980.4 | 2,980.1 | 374.6 | 320.1 | 202.0 | 202.0 |
| 128 | 4,096 | 2,980.4 | 2,980.1 | 1,168.6 | 320.1 | 832.0 | 832.0 |
| 128 | 20,000 | 2,980.4 | 2,980.1 | 4,261.6 | 320.1 | 3,541.0 | 3,541.0 |

For 128-byte strings and one distinct value, Rust interning reduces the table's
retained storage by 89.2% and a copy's incremental storage by 89.3% relative to
default strings. Even at 4,096 distinct values it retains less than half as much
as the default table. At 20,000 distinct values it uses 43.0% more than default;
its hash table and reference-counted payloads outweigh deduplication. For unique
16-byte strings it uses 3.67 times the default table's memory.

C++ GD's eight-byte reference indices make its highly repeated columns smaller
than Rust's sixteen-byte `Arc<str>` cell handles. Default Rust strings use
24-byte descriptors, with the 16-byte text stored inline. Rust interned copies
always add approximately 320 KiB for cell handles here, irrespective of dictionary
size; C++ deep-copies its reference payloads along with the row buffer.
Sharing can therefore help copies even with unique values: for 128-byte strings,
the all-unique source plus one copy totals 4,582 KiB with interning versus
5,961 KiB by default, despite the larger interned source on its own.

After replacing every cell with a new unique value, the result retains:

| Bytes/string | Starting distinct | Default KiB | Interned KiB | GD KiB |
|---:|---:|---:|---:|---:|
| 16 | 1 | 480.4 | 1,761.6 | 1,353.5 |
| 16 | 16 | 480.4 | 1,761.6 | 1,354.2 |
| 16 | 256 | 480.4 | 1,761.6 | 1,365.5 |
| 16 | 4,096 | 480.4 | 1,761.6 | 1,545.5 |
| 16 | 20,000 | 480.4 | 1,761.6 | 2,547.0 |
| 128 | 1 | 2,980.4 | 4,261.6 | 3,541.2 |
| 128 | 16 | 2,980.4 | 4,261.6 | 3,543.5 |
| 128 | 256 | 2,980.4 | 4,261.6 | 3,581.0 |
| 128 | 4,096 | 2,980.4 | 4,261.6 | 4,181.0 |
| 128 | 20,000 | 2,980.4 | 4,261.6 | 6,922.0 |

Rust releases interner entries whose final associated cell is replaced, although
its hash-table capacity is retained. C++ GD's reference pool retains old entries
as new ones are added. With 20,000 initially unique 128-byte values, GD ends at
6,922 KiB after replacement, versus Rust interning at 4,262 KiB. Default strings
remain at 2,980 KiB. This workload turns every starting repetition level into an
all-unique result; it does not preserve the starting repetition ratio.


## Insertion, copying and mutation times

Sources: [Rust](../../benches/interned_strings/workload.rs),
[C++ GD](../../benches/cpp-reference/interned_strings.cpp), and
[raw samples, round ranges and metadata](measurements/interned-strings-m1.json).

Milliseconds per 20,000-row operation, median of three round medians. `Existing`
replaces each row with the next value in the original dictionary. For one distinct
value this sets the current value again. `New unique` replaces every row with a
new, unique string. Small differences, particularly sub-millisecond copies,
should be read with the round ranges and confirmation results below.

**16-byte strings**

| Distinct | Operation | Default Rust ms | Interned Rust ms | C++ GD ms |
|---:|---|---:|---:|---:|
| 1 | Insert | 0.227 | 0.609 | 0.127 |
| 1 | Copy | 0.0272 | 0.151 | 0.00308 |
| 1 | Existing | 0.197 | 0.734 | 0.105 |
| 1 | New unique | 0.194 | 1.96 | 327 |
| 16 | Insert | 0.225 | 0.613 | 0.504 |
| 16 | Copy | 0.0276 | 0.149 | 0.00371 |
| 16 | Existing | 0.197 | 0.734 | 0.441 |
| 16 | New unique | 0.191 | 1.97 | 329 |
| 256 | Insert | 0.197 | 0.636 | 4.57 |
| 256 | Copy | 0.0276 | 0.149 | 0.0105 |
| 256 | Existing | 0.201 | 0.736 | 4.4 |
| 256 | New unique | 0.192 | 1.95 | 336 |
| 4,096 | Insert | 0.198 | 0.94 | 67.6 |
| 4,096 | Copy | 0.0281 | 0.17 | 0.11 |
| 4,096 | Existing | 0.198 | 0.783 | 67.5 |
| 4,096 | New unique | 0.192 | 1.92 | 463 |
| 20,000 | Insert | 0.199 | 1.81 | 328 |
| 20,000 | Copy | 0.0281 | 0.181 | 0.524 |
| 20,000 | Existing | 0.197 | 0.823 | 327 |
| 20,000 | New unique | 0.192 | 2.05 | 981 |

**128-byte strings**

| Distinct | Operation | Default Rust ms | Interned Rust ms | C++ GD ms |
|---:|---|---:|---:|---:|
| 1 | Insert | 0.452 | 1.36 | 0.158 |
| 1 | Copy | 0.325 | 0.345 | 0.00346 |
| 1 | Existing | 0.615 | 1.76 | 0.154 |
| 1 | New unique | 0.627 | 3.04 | 457 |
| 16 | Insert | 0.462 | 1.42 | 0.52 |
| 16 | Copy | 0.325 | 0.344 | 0.00387 |
| 16 | Existing | 0.619 | 1.75 | 0.493 |
| 16 | New unique | 0.596 | 3.06 | 456 |
| 256 | Insert | 0.459 | 1.43 | 4.43 |
| 256 | Copy | 0.323 | 0.349 | 0.0104 |
| 256 | Existing | 0.616 | 1.79 | 4.49 |
| 256 | New unique | 0.602 | 3.06 | 470 |
| 4,096 | Insert | 0.465 | 1.74 | 92.8 |
| 4,096 | Copy | 0.331 | 0.39 | 0.0979 |
| 4,096 | Existing | 0.618 | 1.91 | 92.4 |
| 4,096 | New unique | 0.614 | 3.04 | 642 |
| 20,000 | Insert | 0.46 | 2.79 | 457 |
| 20,000 | Copy | 0.33 | 0.416 | 0.471 |
| 20,000 | Existing | 0.625 | 1.95 | 456 |
| 20,000 | New unique | 0.619 | 2.86 | 1,395 |

The shared payloads save memory but do not make this `Table::clone` constant time.
It still copies cell handles, increments `Arc` counts, and hashes each string to
update the pool's logical cell counts under a lock. See
[`InternedStringData::clone` and `Interner::retain_values`](../../src/table/interned_string.rs).
For repeated 128-byte values, the main run has default and interned copies around
0.33 and 0.35 ms respectively. For 16-byte values the inline default copies are
substantially faster than interning. These results do not support a copy-speed
advantage for the current interned implementation at these string lengths.

Writes through the normal public API are also more expensive with Rust interning.
For one distinct 128-byte value, insertion takes about 1.36 ms versus 0.45 ms by
default; replacing with existing values takes 1.76 ms versus 0.61 ms. With all
20,000 values unique, insertion takes 2.79 ms versus 0.46 ms. Hashing, synchronized
pool bookkeeping and atomic reference counts are included, as is temporary `Value`
construction. This does not benchmark a specialized bulk interning API.

C++ GD is fastest for a tiny dictionary but its
[`references::find`](../../external/gd/source/gd_table.cpp) scans the pool linearly.
All values in a case have the same byte length, so candidates cannot be rejected
by length alone. At 20,000 distinct 128-byte values, GD insertion takes about
457 ms versus 2.79 ms for Rust interning. Adding 20,000 new unique values to that
pool takes about 1,395 ms versus 2.86 ms. This is a comparison of these storage
implementations and public API paths, not a general Rust-versus-C++ result.


## Confirmation, variation and limits

Sources: [Rust](../../benches/interned_strings/workload.rs),
[C++ GD](../../benches/cpp-reference/interned_strings.cpp),
[main run](measurements/interned-strings-m1.json), and
[accepted endpoint rechecks](measurements/interned-strings-m1-confirmation.json).

The main run completed all 30 rounds: 360 processes, each with five samples,
for 1,800 accepted timed operations. An independent run rechecked one-distinct
and all-unique data at both lengths, using three samples per process. Three
endpoint cases completed two rounds; the all-unique 128-byte case completed one
accepted round. An additional round was rejected after sustained background load
returned. The recheck was stopped during the subsequent idle wait, so its raw
metadata deliberately records an incomplete two-round request and explains the
stop. All four endpoints have accepted rechecks: 252 additional timed operations.
Rejected samples are retained separately and do not contribute to any estimate.

The rechecks differ from the main medians by at most 14.7% across these endpoints;
all 128-byte endpoint results differ by at most 11.8%. For the closely matched
one-distinct 128-byte copy, default/interned times were 0.325/0.345 ms in the main
run and 0.334/0.346 ms in the recheck. This supports the memory finding, not a
copy-speed advantage. The large-cardinality C++ lookup cost and Rust's additional
write cost also repeated. The independent full memory pass reproduced every
reported byte count exactly.

The runner samples background process CPU time before and after each round. Two
consecutive samples above one core's aggregate CPU use cause an idle wait or
rejection; an isolated spike followed by a quiet confirmation is allowed. The
main run waited for quiet periods and rejected no rounds; the recheck rejected
one. The raw files retain these load samples. `pmset` reported no thermal or
performance warning, but OS scheduling, allocator reuse and brief activity still
cause variation. Main-run round medians for several fast cases vary by 25–35%;
the tiny C++ copy with 16 distinct long values spans 0.00371–0.00579 ms. Do not
interpret the displayed digits as stable precision or small gaps as universal.

All results come from a single machine and a single required string column.
There are no nulls, mixed string lengths, skewed distributions, multiple columns,
parallel writes or strings longer than 128 bytes. Input generation, initial
mutation fixtures, full-content checks and final result destruction are outside
timing; memory measurements omit allocator arenas and fragmentation. This is a
steady workload comparison, not an RSS, cold-start or application-latency study.

Choose interning here for repeated-payload memory savings and smaller incremental
copies, while budgeting for slower writes. Keep default strings for this workload's
write-heavy cases or standalone tables with unique values. Existing borrowed/owning
selections and shared mutable projections have different ownership costs and are
not measured here.

## Correctness checks

The 120 main cases pass the full-content Python oracle in both native timing and
memory executables. Each verification checks source independence after a result
write and a copy surviving destruction of its immediate source. The smaller
64-row matrix also passed all operations at both lengths and three repetition
levels. Rust's allocator wrapper tests allocation, alignment through 4,096 bytes,
zeroing, reallocation, content preservation and release; these tests also passed
under AddressSanitizer. Twelve C++ verification cases passed in a separate
AddressSanitizer build (`-O1 -g -fsanitize=address -fno-omit-frame-pointer`, without
LTO or `NDEBUG`). No sanitizer instrumentation was used for reported timings.

`./scripts/ci.sh` passed formatting, Clippy, all-features tests, minimal-features
tests, doctests, documentation and the Rust 1.86.0 minimum-version check
(`cargo +1.86.0 check --lib --no-default-features`). The `external/gd` source
fingerprint is unchanged.

# Cross-platform 10k-row price benchmark

Benchmark sources: [Rust typed-SoA workload](price_total_500k.rs) ·
[C++ aligned-AoS workload](cpp-reference/price_total_500k_benchmark.cpp) ·
[Linux build and run script](run_price_total_500k.sh)

This is the cache-resident companion to the
[500k-row report](price-total-500k.md). Both row counts use the same configurable
executables, formula, compiler settings, data layout, and validation:

```text
result = price * quantity + tax
```

Setup, table construction, slice selection, output allocation, and validation are
outside the reported timing samples. The result is written to a separate allocation
and consumed after every pass so the optimizer cannot remove the calculation.

## Working-set size

The hot working set is the input and output storage repeatedly touched by the timed
kernel. It excludes small table/vector descriptors, schema metadata, code, stacks,
and process runtime state:

```text
Rust SoA
  price Vec<f64>       10,000 × 8 =  80,000 bytes
  tax Vec<f64>         10,000 × 8 =  80,000 bytes
  quantity Vec<u32>    10,000 × 4 =  40,000 bytes
  result Vec<f64>      10,000 × 8 =  80,000 bytes
                                      -------------
                                      280,000 bytes = 273.44 KiB

C++ AoS
  PriceRow[10,000]     10,000 × 24 = 240,000 bytes
  result double[10k]   10,000 ×  8 =  80,000 bytes
                                      -------------
                                      320,000 bytes = 312.50 KiB
```

Both working sets exceed every tested L1D cache but fit in every tested L2: the Ky X1
and Cortex-A76 have 512 KiB available, Lion Cove has a private 3 MiB L2, and each
four-core Skymont cluster shares 4 MiB. The Apple M4 performance cluster has a
128 KiB L1D per core and shares 16 MiB of L2. Rust scans 12.5% fewer bytes because
its separate typed vectors contain no four-byte inter-row padding.

## Method

These measurements were made on 2026-07-19. Rust uses rustc 1.97.0 or 1.97.1 with no
`target-cpu` override. C++ uses GCC 13.3.0 on the Ky X1 and Core Ultra, GCC 15.2.0 on
the RK3588, and `-O3 -ffast-math -DNDEBUG` without `-march`. The separate Ky X1
compiler comparison uses Clang 18.1.3 with libstdc++ or libc++ 18.

The C++ binaries may contract multiply-add because of `-ffast-math`; Rust retains
its normal floating-point semantics and the inspected Rust loops use separate
multiply and add instructions. Checksums match the expected result in every run.

The Apple M4 addition uses rustc 1.97.1 and Apple Clang 17.0.0 on macOS 26.5, also
without a target-CPU override. macOS does not expose a supported `taskset` equivalent,
so these serial processes use foreground application scheduling rather than hard
affinity. `sysctl` reports four performance cores with a shared 16 MiB L2 and six
efficiency cores with a shared 4 MiB L2. The timings are consistent with performance-
core execution, but core placement was not directly observable.

The 10k fixture increases its pass counts to preserve the 500k experiment's logical
work:

| Phase | 500k passes | 10k passes | Logical rows |
|---|---:|---:|---:|
| warm-up | 16 | 800 | 8,000,000 |
| each timing sample | 512 | 25,600 | 256,000,000 |
| `perf` body | 4,096 | 204,800 | 2,048,000,000 |
| complete `perf` process | 4,112 | 205,600 | 2,056,000,000 |

Consequently, the 500k and 10k PMU totals have the same logical denominator: 6.168
billion field reads and 2.056 billion result writes. Process counters also include
setup, which is much smaller for 10k rows.

Reproduce one pinned run with:

```sh
./benches/run_price_total_500k.sh 4 10000
```

As in the 500k report, values are geometric means of three run medians. Every run
contains nine timing samples. PMU values are three-run `perf stat` averages with
events kept in small groups; event time enabled was at least 99% unless the counter
was not supported.

## CPU topology and affinity

Logical CPU numbering is specific to these machines:

| Host | Affinity | CPU/core type |
|---|---:|---|
| Ky X1 RISC-V | CPU 0 | Ky X1 RISC-V core |
| Rockchip RK3588 | CPU 4 | Arm Cortex-A76 performance core |
| Intel Core Ultra 5 225H | CPU 0 | Lion Cove performance core (P-core) |
| Intel Core Ultra 5 225H | CPU 4 | Skymont efficiency core (E-core) |
| Apple M4 | macOS foreground scheduler | Scheduler-managed; performance-core placement expected but not hard-pinned |

The full `lstopo --no-io --no-factorize` diagrams for the Linux hosts are shared with
the 500k report:

**Ky X1 — benchmark CPU 0**

![Ky X1 eight-core lstopo topology](topology/ky-x1.svg)

**RK3588 — benchmark Cortex-A76 CPU 4**

![RK3588 eight-core lstopo topology](topology/rk3588.svg)

**Core Ultra 5 225H — benchmark Lion Cove CPU 0 and Skymont CPU 4 separately**

![Core Ultra 5 225H fourteen-core lstopo topology](topology/core-ultra-225h.svg)

## Timing results

Median time per complete 10,000-row pass:

| Host and affinity | Rust SoA | C++ AoS | C++ AoS `restrict` | Fastest |
|---|---:|---:|---:|---:|
| Ky X1 RISC-V, CPU 0 | 107.344 us | 56.147 us | 65.060 us | C++ unrestricted |
| RK3588 Cortex-A76, CPU 4 | 7.289 us | 10.024 us | 9.707 us | Rust |
| Core Ultra 5 225H Lion Cove, CPU 0 | 2.688 us | 3.686 us | 3.678 us | Rust |
| Core Ultra 5 225H Skymont, CPU 4 | 2.279 us | 3.705 us | 3.927 us | Rust |
| Apple M4, macOS foreground scheduler | 2.177 us | 2.995 us | 2.987 us | Rust |

Rust has 33.2% more throughput than the faster GCC path on the A76, 36.8% more on
Lion Cove, and 62.5% more on Skymont. GCC unrestricted has 91.2% more throughput than
Rust on the Ky X1. Rust has 37.2% more throughput than the faster Apple Clang path on
the M4. The three Lion Cove Rust run medians were 2.574, 2.724, and 2.772 us; their
monotonic spread is retained in the geometric mean rather than selecting the most
favorable run. The corresponding `perf` time and cycle count agree with a roughly
2.68 us steady-state pass.

### Packed-vectorization A/B for Rust and C++

Benchmark sources: [Rust typed-SoA workload](price_total_500k.rs) ·
[C++ aligned-AoS workload](cpp-reference/price_total_500k_benchmark.cpp). The
scalar-oriented binaries use the same optimized profiles and portable targets as the
ordinary binaries, but disable their compilers' loop and SLP vectorizers:

```sh
RUSTFLAGS="-C no-vectorize-loops -C no-vectorize-slp" \
  cargo bench --bench price_total_500k --no-run

g++ -std=c++20 -O3 -ffast-math -DNDEBUG -fno-exceptions -fno-rtti \
  -fno-tree-loop-vectorize -fno-tree-slp-vectorize \
  benches/cpp-reference/price_total_500k_benchmark.cpp -o price_total_no_vector

clang++ -std=c++20 -O3 -ffast-math -DNDEBUG -fno-exceptions -fno-rtti \
  -fno-vectorize -fno-slp-vectorize \
  benches/cpp-reference/price_total_500k_benchmark.cpp -o price_total_no_vector_m4
```

Only the 10k fixture was run. Results use the same three run medians, nine samples per
run, and affinity or scheduler policy as the main table. C++ unrestricted and
`restrict` order was alternated between repetitions. Smaller is faster:

| Implementation | Cortex-A76 | Lion Cove | Skymont | Apple M4 |
|---|---:|---:|---:|---:|
| Rust SoA, ordinary | 7.289 us | 2.688 us | 2.279 us | 2.177 us |
| Rust SoA, vectorizers disabled | 13.517 us | 3.410 us | 5.022 us | 2.446 us |
| C++ AoS, ordinary | 10.024 us | 3.686 us | 3.705 us | 2.995 us |
| C++ AoS, vectorizers disabled | 10.034 us | 3.660 us | 3.714 us | 2.983 us |
| C++ AoS `restrict`, ordinary | 9.707 us | 3.678 us | 3.927 us | 2.987 us |
| C++ AoS `restrict`, vectorizers disabled | 9.609 us | 3.641 us | 3.712 us | 2.975 us |

| Implementation | Cortex-A76 disabled/ordinary | Lion Cove disabled/ordinary | Skymont disabled/ordinary | M4 disabled/ordinary |
|---|---:|---:|---:|---:|
| Rust SoA | 1.855× | 1.268× | 2.204× | 1.124× |
| C++ AoS | 1.001× | 0.993× | 1.002× | 0.996× |
| C++ AoS `restrict` | 0.990× | 0.990× | 0.945× | 0.996× |

The Lion Cove scalar run was repeated because its individual medians varied. The
first and repeat three-run geometric means were 3.410 and 3.412 us, respectively, so
the aggregate result is reproducible despite that per-run variation.

The M4 required a different stabilization step. An unconditioned first Rust run
inherited a post-build frequency state and produced a 1.714 us median, while later
runs settled near 2.15--2.17 us. Each reported M4 path was therefore preconditioned
with its own 2.048-billion-logical-row `perf` body immediately before timing. The
reported geometric means use ordinary Rust run medians of 2.144, 2.214, and 2.174 us,
and disabled-vectorizer medians of 2.416, 2.470, and 2.454 us. This is a sustained
per-kernel result rather than a transient post-idle or post-compilation peak.

Disassembly confirms that packed arithmetic is absent from the disabled calculation
loops. Portable Rust x86-64 retains mandatory scalar SSE2 floating-point
instructions and processes two rows per loop through independent
`cvtsi2sd`/`mulsd`/`addsd` chains; an `xorps` zero idiom clears each scalar register
but does not process multiple rows. Rust AArch64 processes one row per loop with
scalar `ucvtf d`, `fmul d`, `fadd d`, and `str d`.

Both C++ x86-64 variants become the same explicitly 16-row-unrolled sequence of
scalar `cvtsi2sd`, `mulsd`, `addsd`, and `movsd` operations. On AArch64, GCC retains
the 16-row unroll and schedules many scalar `ucvtf` and `fmadd` chains concurrently.
It also combines adjacent scalar memory operations into `ldp`/`stp` register pairs;
these transfer two scalar D registers but are not packed NEON arithmetic.

On M4, ordinary Rust handles 16 rows per main-loop iteration through eight NEON
two-lane `f64` groups. With the vectorizers disabled, Apple/LLVM still unrolls three
scalar rows and uses paired scalar loads and stores. Apple Clang's explicitly
16-row-unrolled timed C++ block is scalar in both builds; its ordinary binary also
contains a vectorized remainder path, but 10,000 rows divide evenly by 16 so that
remainder is not executed in this fixture.

This is not a literal removal of the architectures' SIMD/FP register files: scalar
floating point uses XMM registers on x86-64 and the scalar view of FP/NEON registers
on AArch64. The flags specifically prevent LLVM from turning the row loop into
packed, data-parallel work and apply the corresponding restriction to GCC.

The C++ ratios near 1.0 show that its AoS arithmetic was already scalar; disabling
vectorization mainly removes store grouping from the ordinary restricted x86-64
path, and that path was not faster to begin with. By contrast, the ordinary Rust SoA
loop performs genuinely packed arithmetic. With vectorization disabled, C++
`restrict` has 40.7% more throughput on the A76 and 35.3% more on Skymont, while Rust
retains 6.8% more throughput on Lion Cove and 21.6% more on M4.

This scalar-only cross-language result includes two intentional differences: C++
still requests a 16-row unroll through its checked-in pragma and `-ffast-math` permits
scalar FMA on AArch64, while Rust uses normal floating-point semantics and receives a
two-row x86-64 unroll, no Linux AArch64 unroll, or a three-row Apple AArch64 unroll.
It therefore demonstrates the value of Rust's ordinary SoA vectorization and C++'s
explicit instruction-level parallelism; it is not a layout-only comparison. Because
the working sets are L2-resident, the differences primarily measure execution width,
instruction count, and exposed independent work rather than DRAM bandwidth.

The M4's 1.124× Rust ratio is smaller than Lion Cove's 1.268× ratio, but timing alone
cannot separate clock/power behavior from microarchitecture. Privileged
`powermetrics` frequency data was unavailable, macOS core placement was not directly
observable, and the generated scalar schedules differ. The M4 result does establish
that Apple/LLVM exposes enough scalar instruction-level parallelism to retain 89% of
the vectorized throughput after sustained per-binary preconditioning; it does not by
itself prove that the core maintains the same clock in both modes.

### RISC-V GCC versus Clang

| Ky X1 compiler and standard library | C++ AoS | C++ AoS `restrict` | Faster variant |
|---|---:|---:|---:|
| GCC 13.3.0 | 56.147 us | 65.060 us | unrestricted |
| Clang 18.1.3 + libstdc++ | 78.534 us | 53.745 us | `restrict` |
| Clang 18.1.3 + libc++ 18 | 79.584 us | 52.112 us | `restrict` |

The two Clang calculation functions are independent of the standard library and
compile to byte-identical machine code for each aliasing variant. Their 1--3%
difference is therefore run-order/system variation, not a libc++ effect. Clang's
`restrict` version is the fastest measured 10k Ky X1 path: 7.7% more throughput than
GCC unrestricted and 106.0% more than Rust. Unlike the 500k result, where GCC
unrestricted won, keeping the 320,000-byte C++ working set in L2 makes Clang's more
aggressively interleaved restricted schedule the winner.

### Per-row scaling from 500k to 10k

The following ratios compare time per row. Values above ×1 mean the 10k fixture
processes each row faster than the 500k fixture:

| Host | Rust | C++ GCC AoS | C++ GCC AoS `restrict` |
|---|---:|---:|---:|
| Ky X1 | ×0.97 | ×1.23 | ×1.25 |
| Cortex-A76 | ×2.62 | ×2.21 | ×2.30 |
| Lion Cove | ×2.30 | ×1.93 | ×1.94 |
| Skymont | ×2.48 | ×1.81 | ×1.81 |

Cache residency substantially improves every AArch64 and x86-64 path. The scalar,
non-unrolled Rust loop on portable RISC-V is 3% slower per row at 10k, showing that
its 500k result was not primarily constrained by streaming the larger working set.
Both GCC paths improve on Ky X1; Clang restricted improves enough to overtake GCC.

## Load and store operations

Intel counts retired memory instructions/uops. The Cortex-A76 kernel exposes
`LD_SPEC` and `ST_SPEC`, which count speculatively executed memory instructions and
must not be compared numerically with Intel's retired events.

| CPU | Implementation | Loads | Loads / logical row | Stores | Stores / logical row |
|---|---|---:|---:|---:|---:|
| Lion Cove | Rust SoA | 3.089 B | 1.503 | 1.032 B | 0.502 |
| Lion Cove | C++ AoS | 6.171 B | 3.002 | 2.057 B | 1.001 |
| Lion Cove | C++ AoS `restrict` | 6.171 B | 3.002 | 1.029 B | 0.500 |
| Skymont | Rust SoA | 3.088 B | 1.502 | 1.031 B | 0.501 |
| Skymont | C++ AoS | 6.169 B | 3.000 | 2.056 B | 1.000 |
| Skymont | C++ AoS `restrict` | 6.169 B | 3.001 | 1.028 B | 0.500 |
| Cortex-A76 (`LD_SPEC`/`ST_SPEC`) | Rust SoA | 1.289 B | 0.627 | 0.516 B | 0.251 |
| Cortex-A76 (`LD_SPEC`/`ST_SPEC`) | C++ AoS | 4.119 B | 2.003 | 2.059 B | 1.001 |
| Cortex-A76 (`LD_SPEC`/`ST_SPEC`) | C++ AoS `restrict` | 4.120 B | 2.004 | 1.030 B | 0.501 |

The 10k totals are closer to the ideal steady-state rates than the 500k process
totals because table construction handles 490,000 fewer rows. Rust halves retired
x86-64 load operations relative to C++ and halves stores relative to unrestricted
C++. Restricted C++ combines two outputs per store but cannot turn its 24-byte AoS
input into contiguous typed vectors.

The Ky X1 has no usable dynamic PMU counters. Static inspection still gives three
scalar loads and one scalar store per row for Rust and C++; scheduling and unrolling,
not a reduced semantic access count, separate their timing results.

## Cycles and instructions

Process totals for 2.056 billion logical rows:

| CPU | Implementation | Cycles | Instructions | Instructions/cycle |
|---|---|---:|---:|---:|
| Cortex-A76 | Rust SoA | 3.294 B | 6.703 B | 2.035 |
| Cortex-A76 | C++ AoS | 4.536 B | 10.814 B | 2.384 |
| Cortex-A76 | C++ AoS `restrict` | 4.370 B | 9.785 B | 2.239 |
| Lion Cove | Rust SoA | 2.666 B | 10.814 B | 4.057 |
| Lion Cove | C++ AoS | 3.682 B | 12.862 B | 3.494 |
| Lion Cove | C++ AoS `restrict` | 3.702 B | 17.362 B | 4.690 |
| Skymont | Rust SoA | 2.013 B | 10.807 B | 5.368 |
| Skymont | C++ AoS | 3.270 B | 12.857 B | 3.932 |
| Skymont | C++ AoS `restrict` | 3.460 B | 17.355 B | 5.016 |

The outcome is cycles, not IPC in isolation. Restricted C++ has the highest Lion
Cove IPC but still consumes 39% more cycles than Rust. Skymont completes every path
in fewer cycles than Lion Cove in these measurements, with Rust using 24.5% fewer
cycles on Skymont than on Lion Cove.

## Cache and data-TLB counters

### Cortex-A76

| Implementation | L1D accesses | L1D refills | Refill rate | L2D accesses | L2D refills | Refill rate |
|---|---:|---:|---:|---:|---:|---:|
| Rust SoA | 3.607 B | 17.587 M | 0.488% | 1.561 B | 50,791 | 0.0033% |
| C++ AoS | 6.178 B | 34.907 M | 0.565% | 1.817 B | 179,040 | 0.0099% |
| C++ AoS `restrict` | 5.148 B | 35.094 M | 0.682% | 1.816 B | 172,690 | 0.0095% |

| Implementation | L1D-TLB accesses | L1D-TLB refills | Refill rate | Completed data-TLB walks |
|---|---:|---:|---:|---:|
| Rust SoA | 3.607 B | 8,886 | 0.00025% | 51 |
| C++ AoS | 6.174 B | 12,539 | 0.00020% | 88 |
| C++ AoS `restrict` | 5.148 B | 11,516 | 0.00022% | 89 |

The contrast with 500k is decisive: Rust's A76 L2 refills fall from 149.4 million to
50.8 thousand. C++ falls from approximately 272 million to 173--179 thousand. Once
loaded, both working sets remain resident in the private 512 KiB L2.

### Lion Cove P-core

| Implementation | L1 hits | L1 misses | Conditional miss | L2 hits | L2 misses | Conditional miss |
|---|---:|---:|---:|---:|---:|---:|
| Rust SoA | 1.956 B | 288.277 M | 12.847% | 301.573 M | 307 | 0.00010% |
| C++ AoS | 4.634 B | 205.574 M | 4.248% | 200.643 M | 880 | 0.00044% |
| C++ AoS `restrict` | 5.974 B | 45.389 M | 0.754% | 41.860 M | 826 | 0.00197% |

| Implementation | STLB hits | Completed data-TLB walks | L3 hits | L3 misses |
|---|---:|---:|---:|---:|
| Rust SoA | 1,310 | 268 | 257 | 17 |
| C++ AoS | 8,101 | 661 | 717 | 134 |
| C++ AoS `restrict` | 7,509 | 637 | 689 | 143 |

The high Rust L1 miss percentage again has a much smaller load-instruction
denominator. All three L2 miss totals are below one thousand across 2.056 billion
rows. L3 and TLB events are so sparse that their percentage variation is not useful;
the absolute totals establish that neither is a steady-state bottleneck.

### Skymont E-core

| Implementation | L1 hits | L1 misses | Conditional miss | L2 hits | L2 misses | Conditional miss |
|---|---:|---:|---:|---:|---:|---:|
| Rust SoA | 2.013 B | 1.070 B | 34.699% | 219.102 M | 216,451 | 0.0987% |
| C++ AoS | 5.702 B | 449.242 M | 7.303% | 60.101 M | 49,971 | 0.0831% |
| C++ AoS `restrict` | 5.513 B | 637.047 M | 10.358% | 94.623 M | 25,228 | 0.0267% |

| Implementation | STLB hits | Completed 4 KiB walks | Generic LLC references | Generic LLC misses |
|---|---:|---:|---:|---:|
| Rust SoA | 614,353 | 245 | 12,512 | 1,187 |
| C++ AoS | 419,139 | 401 | 25,016 | 5,078 |
| C++ AoS `restrict` | 624,416 | 398 | 22,905 | 2,380 |

No 2 MiB/4 MiB data-TLB walk completed in any Skymont run. The generic LLC counts
are tiny and have high relative variance because the working sets stay in L2; only
their absolute scale is meaningful. Rust's higher L1 miss rate does not prevent it
from using 38--42% fewer cycles than C++.

## Generated loops

Changing the runtime row count does not change the calculation functions. The 10k
executables therefore use the same steady-state loops documented in the 500k report:

| Target | Strategy |
|---|---|
| portable x86-64 | Rust SSE2, four rows per loop through two independent vector groups |
| portable AArch64 | Rust NEON, eight rows per loop through four independent vector groups |
| Apple `arm64-apple-darwin` | Rust NEON, 16 rows per loop through eight independent vector groups |
| portable RISC-V `rv64imafdc` | Rust scalar, one row per loop, no unrolling |

GCC and Clang use scalar RV64 FMA instructions for the AoS input and honor the
16-row unroll pragma. Clang unrestricted completes each row serially; `restrict`
lets it interleave many rows and postpone stores. GCC already interleaves work in its
unrestricted loop. With the 10k data resident in L2, Clang's restricted scheduling
becomes the fastest Ky X1 implementation.

## Interpretation

- The hot set is exactly 280,000 bytes for Rust and 320,000 bytes for C++ before
  negligible descriptors; both fit in every tested L2 but not L1D.
- Cache residency roughly doubles per-row throughput on A76, Lion Cove, and Skymont
  compared with 500k and almost eliminates L2, LLC, and page-walk traffic.
- Rust remains faster on AArch64 and x86-64 because SoA combines the smaller working
  set with straightforward packed SIMD loads and stores.
- Disabling LLVM's vectorizers makes the Rust loop 1.27× slower on Lion Cove, 1.85×
  slower on Cortex-A76, 2.20× slower on Skymont, and 1.12× slower on M4. Disabling
  the C++ compiler vectorizers changes the timed, already-scalar AoS paths by between
  -5.5% and +0.2%, confirming that packed arithmetic is a material part of the
  cache-resident Rust SoA advantage.
- Ky X1 remains a compiler/code-generation exception: portable Rust is scalar and
  non-unrolled, while the explicitly unrolled C++ variants expose more independent
  work. Clang restricted is fastest once the fixture resides in L2.
- Miss percentages must be reported with totals and denominators. Rust can have a
  higher L1 percentage while issuing half as many x86-64 load operations and taking
  substantially fewer cycles.
- The 10k and 500k results describe different cache regimes. Neither should be used
  alone as a universal language or table-layout comparison.

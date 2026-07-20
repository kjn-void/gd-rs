# RK3588 Cortex-A55 2k-row price benchmark

Benchmark sources: [Rust typed-SoA workload](price_total_500k.rs) ·
[C++ aligned-AoS workload](cpp-reference/price_total_500k_benchmark.cpp) ·
[Linux build and run script](run_price_total_500k.sh)

This is a cache-resident companion to the cross-platform
[10k-row report](price-total-10k.md). It runs only on CPU 0 of the Rockchip RK3588,
an in-order Arm Cortex-A55, to separate its private-L2 behavior from the 10k fixture
that spills into the shared L3. Both fixtures calculate:

```text
result = price * quantity + tax
```

Setup, table construction, typed-slice selection, output allocation, and checksum
validation are outside each reported timing sample. Every pass writes its result to
a separate allocation, which is consumed so the optimizer cannot remove the work.

## Working-set size

```text
Rust SoA
  price Vec<f64>       2,000 × 8 = 16,000 bytes
  tax Vec<f64>         2,000 × 8 = 16,000 bytes
  quantity Vec<u32>    2,000 × 4 =  8,000 bytes
  result Vec<f64>      2,000 × 8 = 16,000 bytes
                                     ------------
                                      56,000 bytes = 54.69 KiB

C++ AoS
  PriceRow[2,000]      2,000 × 24 = 48,000 bytes
  result double[2k]    2,000 ×  8 = 16,000 bytes
                                     ------------
                                      64,000 bytes = 62.50 KiB
```

Both hot sets exceed the Cortex-A55's private 32 KiB L1D but fit comfortably in its
private 128 KiB L2. The RK3588 also has a shared 3 MiB L3. Small vector and table
descriptors, schema metadata, code, stacks, and process runtime state are excluded.

## Method

Measurements were made on 2026-07-20 with every process pinned to CPU 0 using
`taskset`. The CPU frequency governor was `ondemand`; the reported maximum and the
observed frequency before and after the complete matrix were both 1.8 GHz. Rust used
rustc 1.97.1 with no `target-cpu` override. C++ used GCC 15.2.0 with
`-O3 -ffast-math -DNDEBUG` and no `-march` override. The C++ compiler may therefore
contract multiply-add while Rust preserves its ordinary floating-point semantics.
All checksums matched.

The ordinary and vectorizers-disabled binaries use the same optimized profiles. The
disabled variants add:

```sh
RUSTFLAGS="-C no-vectorize-loops -C no-vectorize-slp"

-fno-tree-loop-vectorize -fno-tree-slp-vectorize
```

Each program performs 4,000 warm-up passes, then nine timing samples of 128,000
passes each. Those counts retain the other price reports' 8-million-row warm-up and
256-million-row workload per sample. Results are geometric means of three run
medians. The six paths were reversed and mixed between repetitions to expose
run-order or frequency effects.

Reproduce an ordinary pinned run with:

```sh
./benches/run_price_total_500k.sh 0 2000
```

## Timing results

Benchmark sources: [Rust typed-SoA workload](price_total_500k.rs) ·
[C++ aligned-AoS workload](cpp-reference/price_total_500k_benchmark.cpp).

Smaller is faster. Each value is the time for one complete 2,000-row pass.

| Implementation | Time | Disabled/ordinary |
|---|---:|---:|
| Rust SoA, ordinary | 7.902 us | — |
| Rust SoA, vectorizers disabled | 19.289 us | 2.441× |
| C++ AoS, ordinary | 11.365 us | — |
| C++ AoS, vectorizers disabled | 11.369 us | 1.000× |
| C++ AoS `restrict`, ordinary | **6.516 us** | — |
| C++ AoS `restrict`, vectorizers disabled | 6.716 us | 1.031× |

The three independent run medians were:

| Implementation | Run 1 | Run 2 | Run 3 |
|---|---:|---:|---:|
| Rust ordinary | 7.899 us | 7.898 us | 7.908 us |
| Rust vectorizers disabled | 19.287 us | 19.293 us | 19.287 us |
| C++ ordinary | 11.363 us | 11.365 us | 11.368 us |
| C++ vectorizers disabled | 11.370 us | 11.367 us | 11.368 us |
| C++ `restrict` ordinary | 6.516 us | 6.513 us | 6.519 us |
| C++ `restrict` vectorizers disabled | 6.718 us | 6.713 us | 6.716 us |

GCC `restrict` has 21.3% more throughput than ordinary Rust and 74.4% more than
unrestricted C++. Packed Rust is 2.441× faster than Rust with LLVM's loop and SLP
vectorizers disabled. Disabling GCC's vectorizers changes unrestricted C++ by less
than 0.1%; its explicitly 16-row-unrolled main loop was already scalar.

## Effect of fitting in private L2

The following normalizes both fixtures by row count. Smaller is faster:

| Implementation | 2k time/row | 10k time/row | 2k improvement |
|---|---:|---:|---:|
| Rust ordinary | 3.951 ns | 4.314 ns | 8.4% |
| Rust vectorizers disabled | 9.645 ns | 9.737 ns | 0.9% |
| C++ ordinary | 5.683 ns | 5.997 ns | 5.2% |
| C++ vectorizers disabled | 5.684 ns | 5.998 ns | 5.2% |
| C++ `restrict` ordinary | 3.258 ns | 3.474 ns | 6.2% |
| C++ `restrict` vectorizers disabled | 3.358 ns | 3.523 ns | 4.7% |

Moving the complete hot set from shared L3 into private L2 improves ordinary paths
by 5–8%, not by multiples. The 10k A55 result was therefore not primarily limited
by the shared L3. Execution width, instruction scheduling, and exposed independent
work remain dominant.

The generated loops are identical to those inspected for 10k because both row counts
divide evenly by the C++ block size of 16. Ordinary Rust processes eight rows per
iteration through four two-lane NEON groups. Rust with vectorizers disabled processes
one scalar row per iteration. Both C++ paths use scalar `ucvtf` and `fmadd`; the
restricted loop postpones output and uses eight paired `stp` stores per 16 rows,
whereas the unrestricted loop intersperses the calculations with 16 individual
stores. That schedule remains particularly effective on the in-order Cortex-A55.

# libc memory-operation scaling

This report measures the platform implementations of `memcpy` and `memset` across the
Apple, Intel, AMD, Arm, and RISC-V systems used for the other performance reports. It
deliberately excludes handwritten and compiler-generated array loops: those results
depend strongly on vectorization, alias analysis, and store selection and are not a
stable proxy for the memory subsystem.

The checked-in
[`memops_benchmark.cpp`](../../benches/cpp-reference/memops_benchmark.cpp) fixture has
no Rust counterpart because it characterizes the benchmark hosts rather than GD or
`gd-rs` APIs.

## What the numbers mean

The benchmark reports three values:

- **`memcpy` payload GB/s**: destination bytes copied per second, which is the usual
  convention for a copy benchmark;
- **`memcpy` read+write GB/s**: twice the payload rate, counting one source read and
  one destination write per byte;
- **`memset` GB/s**: destination bytes initialized per second.

The read+write column is logical accounting, not a hardware counter. Cache-line
allocation, non-temporal stores, cache absorption, eviction, and deferred writeback
can make physical traffic differ. `memset` has no source-read input and therefore
must not be compared directly with the doubled `memcpy` value.

## Method

| Property | Value |
|---|---|
| Source buffer | 512,000,000 bytes |
| Destination buffer | 512,000,000 bytes |
| Combined allocation | 1.024 GB |
| Alignment | 64 bytes |
| Worker ownership | contiguous, disjoint byte ranges |
| Samples | nine calls per operation and process; median reported |
| Process repetitions | three in forward, reverse, forward configuration order |
| Compiler flags | `-O3 -march=native -std=c++20 -pthread` |
| Builtin policy | `-fno-builtin-memcpy -fno-builtin-memset` |
| Instrumentation | no sanitizers or hardware counters |

The no-builtin flags ensure that the measured loops call the platform libc instead of
letting the compiler replace the calls with its own inline loop. Worker threads are
created before timing and wait at a barrier. Timing includes barrier release and thread
joining. A final unmeasured `memcpy` and sampled checksum make the copied result
observable.

Build and run:

```sh
c++ -O3 -march=native -std=c++20 -pthread \
  -fno-builtin-memcpy -fno-builtin-memset \
  benches/cpp-reference/memops_benchmark.cpp -o /tmp/memops_benchmark

# Four pinned Linux workers. Use `-` instead of a CPU list on macOS.
/tmp/memops_benchmark 512000000 4 0,1,2,3
```

The Ky X1 GCC rejects `-march=native`, so that host uses the compiler's portable
`rv64imafdc` default. Every other host uses the flags above.

## Hosts

| Host | CPU and memory | Compiler and operating system | Affinity |
|---|---|---|---|
| Local M3 Max | 12 performance + 4 efficiency cores, 128 GB | Apple Clang 21.0.0, macOS 26.5.2 | scheduler-managed |
| Base M4 | 4 performance + 6 efficiency cores, 16 GB | Apple Clang 17.0.0, macOS 26.5.2 | scheduler-managed |
| Core Ultra 5 225H | CPUs 0–3 Lion Cove; CPUs 4–11 Skymont; CPUs 12–13 low-power efficiency cores, 32 GB DDR5-5600 | GCC 13.3.0, Linux 6.17.0 | exact Linux CPU masks |
| RK3588 | CPUs 0–3 Cortex-A55 at 1.8 GHz; CPUs 4–7 Cortex-A76 at 2.4 GHz, 16 GB | GCC 15.2.0, Linux 7.0.0 | exact Linux CPU masks |
| Ky X1 RISC-V | 8 cores at 1.6 GHz, 4 GB | GCC 13.3.0, Linux 6.6.63 | exact Linux CPU masks |
| Core i7-5775C | 4 cores/8 threads, 128 MiB L4, 2 × 8 GB dual-rank DDR3-1333 | GCC 13.3.0, Linux 6.8.0 | exact Linux CPU masks |
| Ryzen 9 3900X | 12 cores/24 threads, dual-channel DDR4-2666; WSL2 exposes 20 GB | GCC 15.2.0, WSL2 Linux 6.6.87 | exact virtual-CPU masks |

On Broadwell, CPUs 0–3 are separate physical cores and CPUs 4–7 are their respective
SMT siblings. On the Ryzen guest, even CPUs 0–22 represent the twelve physical cores
and the following odd CPU is each core's SMT sibling. Its figures characterize the
WSL2-visible path, not a native memory-controller run.

macOS provides no public equivalent to Linux's exact CPU affinity. The M3 Max and M4
worker counts are therefore scheduler-managed. The local M3 Max also ran the Codex
session conducting the measurements, so its figures are repeatable process medians
rather than an idle-system certification result.

## Peak results

All rates below are decimal GB/s. The placement columns identify the independently
best configuration for each libc function.

| Host | `memcpy` payload | `memcpy` read+write | Copy placement | `memset` | Set placement |
|---|---:|---:|---|---:|---|
| M3 Max | **111.8** | **223.7** | 8 workers | **125.1** | 8 workers |
| Base M4 | **53.5** | **106.9** | 6 workers | **116.2** | 8 workers |
| Core Ultra 5 225H | **34.9** | **69.9** | 4 Lion Cove | **55.9** | 4 Lion Cove |
| RK3588 | **9.4** | **18.8** | 2 Cortex-A76 | **25.3** | all 8 cores |
| Ky X1 RISC-V | **3.4** | **6.8** | all 8 cores | **7.6** | 1 core |
| Core i7-5775C | **8.8** | **17.6** | 1 core | **22.9** | 4 cores |
| Ryzen 9 3900X under WSL2 | **16.2** | **32.4** | 4 cores | **28.9** | all 24 threads |

### Single-core share of peak

For heterogeneous Linux hosts, “single core” means one performance-class core:
Lion Cove CPU 0 on the 225H and Cortex-A76 CPU 4 on RK3588. The Apple rows use one
normal-priority worker and remain scheduler-managed. The copy percentage is identical
whether payload or read+write accounting is used because the latter is exactly twice
the former.

| Host | 1-core `memcpy` | Peak `memcpy` | 1 core / peak | 1-core `memset` | Peak `memset` | 1 core / peak |
|---|---:|---:|---:|---:|---:|---:|
| M3 Max | 57.5 | 111.8 | **51.5%** | 110.2 | 125.1 | **88.1%** |
| Base M4 | 45.7 | 53.5 | **85.5%** | 73.5 | 116.2 | **63.2%** |
| Core Ultra 5 225H, Lion Cove | 25.5 | 34.9 | **73.1%** | 18.8 | 55.9 | **33.7%** |
| RK3588, Cortex-A76 | 8.6 | 9.4 | **91.3%** | 15.9 | 25.3 | **62.8%** |
| Ky X1 RISC-V | 3.24 | 3.40 | **95.5%** | 7.55 | 7.55 | **100.0%** |
| Core i7-5775C | 8.8 | 8.8 | **100.0%** | 12.3 | 22.9 | **53.7%** |
| Ryzen 9 3900X under WSL2 | 14.3 | 16.2 | **88.4%** | 20.3 | 28.9 | **70.3%** |

The percentages show that copy and fill can saturate at very different worker counts.
Broadwell and Ky X1 need only one core for peak or near-peak copy, while their
`memset` paths benefit materially from more workers. The 225H is the opposite extreme
for filling: one Lion Cove reaches only 33.7% of the four-core `memset` maximum.

## Scaling by host

### M3 Max

| Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---:|---:|---:|---:|
| 1 | 57.5 | 115.1 | 110.2 |
| 2 | 77.1 | 154.2 | 116.4 |
| 4 | 92.0 | 184.0 | 118.9 |
| 8 | **111.8** | **223.7** | **125.1** |
| 12 | 109.4 | 218.7 | 122.6 |
| 16 | 109.7 | 219.5 | 123.6 |

Copy throughput peaks at eight workers. `memset` is already 88.1% of its maximum with
one worker and changes little after two workers. Adding workers beyond eight does not
improve either operation.

### Base M4

| Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---:|---:|---:|---:|
| 1 | 45.7 | 91.5 | 73.5 |
| 2 | 52.1 | 104.1 | 73.5 |
| 4 | 51.6 | 103.3 | 73.2 |
| 6 | **53.5** | **106.9** | 105.8 |
| 8 | 53.1 | 106.2 | **116.2** |
| 10 | 52.9 | 105.8 | 112.3 |

Two workers are within 2.6% of maximum copy payload. `memset` behaves differently:
one to four workers remain near 73.5 GB/s, while six to eight workers reach a higher
throughput tier. Exact P/E placement is not observable.

### Core Ultra 5 225H

| Placement | Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---|---:|---:|---:|---:|
| Skymont | 1 | 18.6 | 37.2 | 14.2 |
| Skymont | 2 | 26.8 | 53.6 | 24.3 |
| Skymont | 4 | 25.9 | 51.8 | 26.6 |
| Skymont | 8 | 31.5 | 63.0 | 33.0 |
| Lion Cove | 1 | 25.5 | 51.1 | 18.8 |
| Lion Cove | 4 | **34.9** | **69.9** | **55.9** |
| Lion Cove + Skymont | 12 | 33.3 | 66.6 | 37.6 |

Four Lion Cove cores produce the best result for both operations. Dividing equal-size
ranges across the heterogeneous twelve-core set regresses both functions, especially
`memset`; the slower workers become stragglers after faster workers finish.

### RK3588

| Placement | Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---|---:|---:|---:|---:|
| Cortex-A55 | 4 | 9.2 | 18.5 | 22.8 |
| Cortex-A76 | 1 | 8.6 | 17.1 | 15.9 |
| Cortex-A76 | 2 | **9.4** | **18.8** | 17.2 |
| Cortex-A76 | 4 | 8.9 | 17.8 | 17.4 |
| Cortex-A55 + Cortex-A76 | 8 | 9.2 | 18.3 | **25.3** |

One or two A76 cores are sufficient for maximum copy throughput. `memset` continues
scaling across both core types and is 47.5% faster with all eight cores than with two
A76 cores.

### Ky X1 RISC-V

| Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---:|---:|---:|---:|
| 1 | 3.24 | 6.49 | **7.55** |
| 2 | 3.08 | 6.16 | 7.55 |
| 4 | 3.07 | 6.15 | 7.51 |
| 8 | **3.40** | **6.79** | 7.45 |

The libc copy path gains only 4.7% from one to eight workers, and `memset` is saturated
by one core. This host lacks usable performance counters, so the data cannot separate
libc implementation limits from the controller and DRAM ceiling.

### Core i7-5775C

| Placement | Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---|---:|---:|---:|---:|
| 1 physical core | 1 | **8.8** | **17.6** | 12.3 |
| 2 physical cores | 2 | 8.4 | 16.7 | 19.4 |
| 4 physical cores | 4 | 8.0 | 16.0 | **22.9** |
| 4 cores, all SMT threads | 8 | 7.7 | 15.5 | 22.4 |

One Broadwell core maximizes `memcpy`; additional workers compete for the same
throughput and gradually regress. `memset` instead scales through four physical cores.
SMT does not improve either maximum. The 1.024 GB allocation is more than seven times
the 128 MiB Crystalwell L4 and cannot reside wholly in that cache.

### Ryzen 9 3900X under WSL2

| Placement | Workers | `memcpy` payload | `memcpy` read+write | `memset` |
|---|---:|---:|---:|---:|
| Physical cores | 1 | 14.3 | 28.6 | 20.3 |
| Physical cores | 2 | 15.5 | 30.9 | 20.7 |
| Physical cores | 4 | **16.2** | **32.4** | 20.8 |
| Physical cores | 8 | 15.4 | 30.8 | 25.3 |
| Physical cores | 12 | 15.8 | 31.7 | 27.7 |
| All SMT threads | 24 | 15.1 | 30.3 | **28.9** |

Copy payload is effectively saturated by two to four physical cores. `memset` follows
a different curve and continues improving through all exposed threads. These figures
remain WSL2 results affected by Hyper-V topology, host memory allocation, mitigations,
and libc implementation details.

The 3900X result is not unexpectedly low for its memory configuration. Dual-channel
DDR4-2666 has a theoretical transfer rate of 42.7 GB/s. The peak logical `memcpy`
read+write rate of 32.4 GB/s is 75.9% of that ceiling. For comparison, Broadwell's
dual-channel DDR3-1333 ceiling is 21.3 GB/s, and its 17.6 GB/s logical copy rate is
82.5% of that ceiling. These percentages use the benchmark's logical byte accounting;
they are useful context but are not measured DRAM-bus utilization.

## Conclusions

- M3 Max delivers the highest copy payload at 111.8 GB/s and peaks around eight
  scheduler-managed workers.
- Base M4 reaches 85.5% of maximum copy payload with one worker and is effectively
  saturated with two.
- Four Lion Cove cores are substantially more effective than an equal static split
  over all 225H core types.
- RK3588 needs only two A76 cores for copy but uses all eight cores effectively for
  `memset`.
- Ky X1 and Broadwell saturate `memcpy` with one core or close to it; more threads do
  not automatically create more bandwidth.
- The Ryzen WSL2 path reaches 16.2 GB/s copy payload, equivalent to 32.4 GB/s of
  logical read+write traffic. That is 75.9% of the theoretical bandwidth of its
  dual-channel DDR4-2666 memory and is not an unexpectedly low result.

The reliable comparison is between the same libc function and byte-accounting
convention. Neither function should be described as a direct measurement of physical
RAM-bus traffic without memory-controller counters.

# STREAM-style memory bandwidth

This report measures how worker count affects sustained sequential memory throughput
on the base Apple M4 and Intel Core Ultra 5 225H systems used for the other performance
reports. The source is the standalone
[`stream_benchmark.cpp`](../../benches/cpp-reference/stream_benchmark.cpp) fixture.
It has no Rust counterpart because it measures the hosts rather than a GD or `gd-rs`
API.

The fixture implements the conventional STREAM Copy, Scale, Add, and Triad loops over
three aligned `double` arrays. It adds explicit worker-count scaling, Linux CPU
affinity, a synchronized start, median timing, and a final checksum. Each worker owns
a contiguous, disjoint range.

```text
Copy:   c[i] = a[i]                 1 read  + 1 write
Scale:  b[i] = 3.0 * c[i]           1 read  + 1 write
Add:    c[i] = a[i] + b[i]          2 reads + 1 write
Triad:  a[i] = b[i] + 3.0 * c[i]    2 reads + 1 write
```

The reported GB/s uses conventional STREAM logical-byte accounting: 16 bytes per
element for Copy and Scale and 24 bytes for Add and Triad. It is not a hardware-counter
measurement of physical DRAM traffic. In particular, cache-line allocation caused by
ordinary stores can move additional data that STREAM does not count.

## Method

| Property | Value |
|---|---|
| Elements per array | 64,000,000 `double` values |
| Bytes per array | 512 MB |
| Combined working set | 1.536 GB |
| Alignment | 64 bytes |
| Samples | nine kernel repetitions per process; median reported |
| Process repetitions | three; median of process medians below |
| Compiler flags | `-O3 -march=native -std=c++20 -pthread` |
| Instrumentation | no sanitizers or hardware counters |

The 225H host ran GCC 13.3.0 on Linux 6.17.0. CPUs 0–3 are its Lion Cove
performance cores, CPUs 4–11 are its two four-core Skymont clusters, and CPUs 12–13
are the low-power efficiency cores. Linux affinity binds every worker to the CPU at
the same position in the supplied list.

The M4 host ran Apple Clang 17.0.0 on macOS 26.5.2. It has four performance and six
efficiency cores, but macOS provides no public equivalent to Linux's exact CPU masks.
Normal-priority results are therefore scheduler-managed. The machine was otherwise
free of known unrelated load during both sets of measurements.

Build on either host:

```sh
c++ -O3 -march=native -std=c++20 -pthread \
  benches/cpp-reference/stream_benchmark.cpp -o /tmp/stream_benchmark
```

Examples:

```sh
# 225H: four Lion Cove cores
/tmp/stream_benchmark 64000000 4 0,1,2,3

# 225H: eight Skymont cores
/tmp/stream_benchmark 64000000 8 4,5,6,7,8,9,10,11

# M4: four scheduler-managed workers
/tmp/stream_benchmark 64000000 4 -
```

## Triad scaling

The following values are conventional STREAM GB/s. A dash means that the worker
count does not exist in that homogeneous core group.

| Workers | M4, normal scheduler | 225H Lion Cove only | 225H Skymont only |
|---:|---:|---:|---:|
| 1 | 96.1 | 15.7 | 31.1 |
| 2 | **97.7** | 19.0 | 35.8 |
| 3 | 97.4 | 18.7 | — |
| 4 | 97.2 | **25.7** | 35.4 |
| 6 | 96.8 | — | 40.8 |
| 8 | 96.6 | — | **45.6** |
| 10 | 96.5 | — | — |

Combining CPUs 0–11 produced **51.9 GB/s** with twelve workers. Including the two
low-power cores produced only **34.3 GB/s** with fourteen equally sized ranges. The
slowest workers become stragglers after the faster cores finish their ranges, and the
extra cores also change package power and frequency. This fixture deliberately keeps
the static STREAM partition instead of hiding that heterogeneous-core effect with
dynamic scheduling.

The Lion Cove process medians varied materially with run order despite repeating the
forward order, reverse order, and forward order: the one-, two-, three-, and four-core
Triad ranges were respectively 15.3–21.2, 15.7–19.8, 18.5–24.5, and 25.6–34.9 GB/s.
The table reports their medians and should not be interpreted more precisely than that.
The Skymont and M4 curves were substantially more stable.

## Best observed kernel medians

| Kernel | M4 result | M4 workers | 225H result | 225H workers | M4 / 225H |
|---|---:|---:|---:|---:|---:|
| Copy | 102.6 GB/s | 2 | 45.6 GB/s | 12 | 2.25× |
| Scale | 101.8 GB/s | 4 | 45.2 GB/s | 12 | 2.25× |
| Add | 97.9 GB/s | 2 | 52.0 GB/s | 12 | 1.88× |
| Triad | 97.7 GB/s | 2 | 51.9 GB/s | 12 | 1.88× |

For Triad, one normal-priority M4 worker reached 98.3% of the best two-worker result;
two workers are sufficient for practical saturation of this kernel. Eight Skymont
cores reached 87.9% of the 225H's best combined result, whereas four Lion Cove cores
reached 49.5%. The best measured 225H throughput required all twelve non-low-power
cores.

## M4 background scheduling probe

As an exploratory check, `taskpolicy -b` favored background scheduling:

| Workers | Triad |
|---:|---:|
| 1 | 18.6 GB/s |
| 2 | 24.5 GB/s |
| 4 | 25.3 GB/s |
| 6 | 18.0 GB/s |

This is not an exact efficiency-core affinity result. `taskpolicy -b` changes Darwin
background priority and supplies scheduling policy rather than a CPU mask. The
six-worker regression is repeatable for this policy, but it must not be interpreted as
a direct measurement of six dedicated M4 efficiency cores.

## Interpretation

The measured M4 is the base chip, for which Apple specifies **120 GB/s** unified-memory
bandwidth. Its one-worker Triad result is therefore unusually close to the host's
multithreaded STREAM ceiling. This is consistent with a performance core sustaining
enough independent cache misses and sequential prefetch traffic to keep this particular
memory system busy. Apple does not publish enough of the relevant miss-buffer and
prefetch implementation to assign the difference to one structure.

This does not establish that Apple M-series CPUs uniquely let one core saturate
memory, and the memory is not an ordinary pair of socketed DDR5 DIMMs. It also does
not generalize across the M4 family: Apple specifies
[120 GB/s for M4, 273 GB/s for M4 Pro, and up to 546 GB/s for M4 Max](https://www.apple.com/newsroom/2024/10/apple-introduces-m4-pro-and-m4-max/).
A single CPU core cannot be expected to saturate the much wider Pro and Max memory
systems.

The 225H has two DDR5-5600 64-bit DIMM channels, giving 89.6 GB/s theoretical transfer
rate before protocol and controller overhead. Its 51.9 GB/s conventional Triad result
does not by itself reveal physical bus utilization: ordinary write allocation can
make actual traffic higher than STREAM's 24-byte logical count, while static work
division, core frequency, prefetching, compiler code generation, and memory-level
parallelism can limit the loop before the theoretical transfer rate is reached.

The defensible conclusion is consequently narrow: **this base M4 executes this
sequential STREAM workload at nearly its observed maximum with one normal-priority
worker, while this 225H requires both performance and efficiency cores for its best
result**. It is not a general ISA limitation of Intel or AMD processors.

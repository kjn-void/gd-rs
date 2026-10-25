# Intermittent L1 misses and independent work

The standalone [C++ cache-latency fixture](../../benches/cpp-reference/cache_latency/cache_latency.cpp)
compares a shuffled pointer ring that fits in L1D with a larger ring that fits in
L2. It has no GD dependency or matched Rust counterpart. The
[CMake project](../../benches/cpp-reference/cache_latency/CMakeLists.txt) builds it
separately from the GD reference benchmarks.

Optional perf-stat FIFO control is implemented separately in
[`perf_control.h`](../../benches/cpp-reference/cache_latency/perf_control.h) and
[`perf_control.cpp`](../../benches/cpp-reference/cache_latency/perf_control.cpp).

## Build and verify

On the target Linux machine, run from the repository root:

```sh
cmake -S benches/cpp-reference/cache_latency -B /tmp/gd-cache-latency-build \
  -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/gd-cache-latency-build --parallel
ctest --test-dir /tmp/gd-cache-latency-build --output-on-failure
```

Release is the default configuration for single-configuration generators. It
enables C++20, `-O3`, and link-time optimization, without adding native architecture
flags. CMake checks that the compiler supports LTO. With a multi-configuration
generator, also pass `--config Release` to the build and `-C Release` to CTest.

For a native Linux build, CMake reads `getconf LEVEL1_DCACHE_LINESIZE` and supplies
the result as `L1_CACHE_BYTES`. The Linux kernel's `L1_CACHE_BYTES` and
`L1_CACHE_SHIFT` macros are not exported in userspace headers. An explicit
`-DL1_CACHE_BYTES=64` or `-DL1_CACHE_SHIFT=6` overrides detection; cross-compilation
and builds on other operating systems require an explicit target value. If both
are supplied, they must agree. `Node` alignment and size use this compile-time
value, while the hot ring remains 1 KiB.

## Workload and measurement

Each iteration follows one link: one cold-ring step followed by three hot-ring
steps. Each ring is a shuffled cycle with a separate cursor. Consecutive steps in
one ring depend on the previous load, but the two rings and the four integer
rotate/add recurrences are independent. Loaded indices contribute to a checksum.
The `work0`, `work1`, `work2`, and `work4` variants change the number of arithmetic
rounds per iteration. The `serial` control follows only the cold ring, exposing
dependent-load latency.

With 64-byte lines, the hot ring has 16 nodes. Cold rings of 8 and 80 KiB give total
data footprints of 9 and 81 KiB. Check the target's per-core L1D and L2 sizes before
choosing sizes, and use performance counters to establish where misses are served.

For the RK3588 host, pin to CPU 3 for Cortex-A55 and CPU 4 for Cortex-A76:

```sh
taskset -c 3 /tmp/gd-cache-latency-build/cache-latency work1 80 0.5
taskset -c 4 /tmp/gd-cache-latency-build/cache-latency work1 80 0.5
```

The existing verification checks ring construction and all kernels against a
reference implementation outside the timed region. The
[measurement runner](../../benches/cpp-reference/cache_latency/run_cache_latency.py)
collects user-mode PMU counters, randomizes trials, and records all C++ source and
binary hashes, toolchain, affinity, cache geometry, and seeds. It targets the RK3588 A55/A76
PMU names and expects a GCC build and working user perf access:

```sh
python3 benches/cpp-reference/cache_latency/run_cache_latency.py \
  --binary /tmp/gd-cache-latency-build/cache-latency \
  --output /tmp/gd-cache-latency-results \
  --reps 3 --seconds 0.35 --seeds 70274 982451653 73
```

The comparison tests how much independent work overlaps intermittent misses. An
A55/A76 comparison also changes pipeline width and other microarchitecture
details; use within-core working-set changes and the serialized control when
interpreting latency coverage.

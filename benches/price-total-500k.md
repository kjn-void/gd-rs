# Cross-platform 500k-row price benchmark

This benchmark compares the Rust table's typed structure-of-arrays columns with a
24-byte C++ array-of-structures row matching the corresponding GD row layout:

```text
Rust input:  Vec<f64> + Vec<f64> + Vec<u32> = 20 bytes/row
C++ input:   PriceRow                         = 24 bytes/row
Output:      contiguous double                =  8 bytes/row
```

Both calculate the same expression into a separate output allocation:

```text
result = price * quantity + tax
```

The C++ source provides both ordinary pointers and the same loop with a `__restrict`
aliasing contract. Setup, schema construction, and row insertion are outside the
reported timing samples. The longer `perf` mode makes initialization negligible
relative to the measured kernel, although process-level counters still include it.

Run portable baseline builds pinned to a Linux logical CPU with:

```sh
./benches/run_price_total_500k.sh 4
```

The runner deliberately unsets Rust architecture flags and gives C++ no `-march`
option. Set `PERF_EVENTS` to add three repeated `perf stat` runs per workload:

```sh
PERF_EVENTS='cycles:u,instructions:u,cache-references:u,cache-misses:u' \
  ./benches/run_price_total_500k.sh 4
```

Use host-PMU events for detailed cache-level and TLB reporting. Keep each event group
small enough to avoid multiplexing, and record the compiler versions, CPU affinity,
event names, event time-enabled percentage, and kernel perf permissions with results.

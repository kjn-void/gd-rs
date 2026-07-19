#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 CPU" >&2
    exit 2
fi

cpu=$1
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS \
    cargo bench --bench price_total_500k --no-run
rust_bin=$(
    find target/release/deps -maxdepth 1 -type f -name 'price_total_500k-*' \
        -executable -printf '%T@ %p\n' |
        sort -nr |
        head -n 1 |
        cut -d' ' -f2-
)
if [[ -z "$rust_bin" ]]; then
    echo "could not locate the Rust benchmark executable" >&2
    exit 1
fi

cxx=${CXX:-g++}
cpp_bin=target/price_total_500k_cpp
"$cxx" -std=c++20 -O3 -ffast-math -DNDEBUG -fno-exceptions -fno-rtti \
    ${CXXFLAGS_EXTRA:-} \
    benches/cpp-reference/price_total_500k_benchmark.cpp \
    -o "$cpp_bin"

run=(taskset -c "$cpu")

echo "Rust SoA timing"
"${run[@]}" "$rust_bin" timing
echo "C++ AoS unrestricted timing"
"${run[@]}" "$cpp_bin" unrestricted timing
echo "C++ AoS restricted timing"
"${run[@]}" "$cpp_bin" restricted timing

if [[ -n ${PERF_EVENTS:-} ]]; then
    perf_repeat=${PERF_REPEAT:-3}
    for workload in rust unrestricted restricted; do
        echo "perf: $workload"
        if [[ $workload == rust ]]; then
            command=("$rust_bin" perf)
        else
            command=("$cpp_bin" "$workload" perf)
        fi
        perf stat -r "$perf_repeat" -e "$PERF_EVENTS" -- \
            "${run[@]}" "${command[@]}"
    done
fi

#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cpp_source="$root/benches/cpp-reference"

: "${GD_TABLE_COPY_SOURCE_ROWS:=1000000}"
: "${GD_TABLE_COPY_PERCENT:=25}"
export GD_TABLE_COPY_SOURCE_ROWS GD_TABLE_COPY_PERCENT

cmake --preset release-native -S "$cpp_source"
cmake --build "$root/target/cpp-reference/release-native" \
    --target gd_cpp_reference_benchmarks

echo "Rust gd-rs table copies"
env -u CARGO_ENCODED_RUSTFLAGS \
    RUSTFLAGS="-C target-cpu=native" \
    cargo bench --manifest-path "$root/Cargo.toml" \
    --bench table_copy --no-default-features --features rayon

echo "C++ GD table copies"
"$root/target/cpp-reference/release-native/gd_cpp_reference_benchmarks" \
    --benchmark_filter='TableCopy/'

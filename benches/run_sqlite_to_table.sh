#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cpp_source="$root/benches/cpp-reference"

: "${GD_SQLITE_TO_TABLE_ROWS:=1000000}"
: "${GD_SQLITE_TO_TABLE_SEED:=0x6a09e667f3bcc909}"
export GD_SQLITE_TO_TABLE_ROWS GD_SQLITE_TO_TABLE_SEED

cmake --preset release-native -S "$cpp_source"
cmake --build "$root/target/cpp-reference/release-native" \
    --target gd_cpp_reference_benchmarks

echo "Rust gd-rs SQLite-to-table materialization"
env -u CARGO_ENCODED_RUSTFLAGS \
    RUSTFLAGS="-C target-cpu=native" \
    cargo bench --manifest-path "$root/Cargo.toml" \
    --bench sqlite_to_table --features sqlite

echo "C++ GD SQLite-to-table materialization"
"$root/target/cpp-reference/release-native/gd_cpp_reference_benchmarks" \
    --benchmark_filter='SQLite/ToTable/ThreeTables/GD'

#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cpp_source="$root/benches/cpp-reference"

cmake --preset release-native -S "$cpp_source"
cmake --build "$root/target/cpp-reference/release-native" \
    --target gd_cpp_reference_string_transport

echo "Rust borrowed buffer, exact-size owned storage, and String"
env -u CARGO_ENCODED_RUSTFLAGS \
    RUSTFLAGS="-C target-cpu=native" \
    cargo bench --manifest-path "$root/Cargo.toml" \
    --bench string_transport --no-default-features

echo "C++ GD string_transport and std::string (-flto)"
"$root/target/cpp-reference/release-native/gd_cpp_reference_string_transport" \
    --benchmark_repetitions=10 --benchmark_report_aggregates_only=true

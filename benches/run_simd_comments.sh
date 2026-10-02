#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cpp_source="$root/benches/cpp-reference"

cmake --preset release-native -S "$cpp_source"
cmake --build "$root/target/cpp-reference/release-native" \
    --target gd_cpp_reference_simd_comments

echo "Rust pack hybrid, normal gd-rs SoA, and standard library"
env -u CARGO_ENCODED_RUSTFLAGS \
    RUSTFLAGS="-C target-cpu=native" \
    cargo bench --manifest-path "$root/Cargo.toml" \
    --bench simd_comments --no-default-features -- all

echo "C++ GD hybrid, scalar-table diagnostic, and standard library"
"$root/target/cpp-reference/release-native/gd_cpp_reference_simd_comments" all

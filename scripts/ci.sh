#!/usr/bin/env bash
#
# Runs the checks required by AGENTS.md, docs/ai/validation.md, and
# docs/ai/pre-commit.md. This is the same sequence CI runs.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

step() {
    printf '\n==> %s\n' "$1"
}

msrv_skipped=0

step "cargo fmt --all -- --check"
cargo fmt --all -- --check

step "cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

step "cargo test --all-targets --all-features"
cargo test --all-targets --all-features

step "cargo test --lib --tests --no-default-features"
cargo test --lib --tests --no-default-features

step 'RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features'
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

step "cargo +1.86.0 check --lib --no-default-features (MSRV)"
if command -v rustup >/dev/null 2>&1 && rustup toolchain list 2>/dev/null | grep -q '^1\.86\.0'; then
    cargo +1.86.0 check --lib --no-default-features
else
    msrv_skipped=1
    printf 'WARNING: Rust 1.86.0 is not installed; the MSRV check was NOT run.\n'
fi

if ((msrv_skipped)); then
    printf '\nAll checks passed except the MSRV check, which was NOT run.\n'
    printf 'Install Rust 1.86.0 and re-run to verify the minimum supported version.\n'
else
    printf '\nAll checks passed.\n'
fi

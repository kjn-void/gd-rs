# Validation

## Minimum supported Rust version

The crate supports Rust 1.86. Do not use standard-library APIs, language features, or
dependency versions that require a newer compiler.

Before committing dependency, feature, or public-API changes, run an appropriate
Rust 1.86 check in addition to the normal current-toolchain tests. At minimum, verify
the library without default features:

```sh
cargo +1.86.0 check --lib --no-default-features
```

If Rust 1.86 is unavailable locally, report that explicitly rather than claiming MSRV
verification.

## Full suite and CI

Run every check in this document through the repository script:

```sh
./scripts/ci.sh
```

The script runs formatting, Clippy, documentation, all-features tests,
minimal-features tests, and the MSRV check in order, and reports explicitly when the
MSRV toolchain is unavailable. The same script runs in
`.github/workflows/ci.yml` on pushes and pull requests. CI installs both the stable
toolchain and Rust 1.86.0 before invoking the script.

## Static checks

Before committing Rust changes, run:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

These checks supplement, rather than replace, the two complete test commands required
by [`AGENTS.md`](../../AGENTS.md).

# Validation

## Minimum supported Rust version

The crate supports Rust 1.86. Do not use standard-library APIs, language features, or
dependency versions that require a newer compiler.

Before committing dependency, feature, or public-API changes, run an appropriate
Rust 1.86 check in addition to the normal current-toolchain tests. At minimum, verify
the library without default features:

```sh
cargo +1.86 check --lib --no-default-features
```

If Rust 1.86 is unavailable locally, report that explicitly rather than claiming MSRV
verification.

## Static checks

Before committing Rust changes, run:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

These checks supplement, rather than replace, the two complete test commands required
by [`AGENTS.md`](../../AGENTS.md).

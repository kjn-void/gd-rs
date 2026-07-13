# gd-rs

An idiomatic Rust port of the portable data and expression core of
[`codemopper/gd`](https://github.com/codemopper/gd).

The port is semantic rather than ABI-compatible. Rust enums replace manually tagged
C++ variants, lifetimes replace borrowed pointer flags, and typed columns replace the
packed row buffer. SQLite value binding and typed-table materialization are available
through the default `sqlite` feature; general database abstractions and other drivers
are excluded.
Command-line parsing, filesystem policy, console behavior, logging sinks, COM-like
application routing, and SQL construction remain at application boundaries and are
not reimplemented by this crate.

The implemented core currently provides:

- owned `Value` and borrowed `ValueRef` sum types;
- ordered `Arguments` with duplicate and positional entries;
- reusable lifetime-bound `ahash` indexes;
- validated schemas and typed column storage;
- borrowing row and column views;
- stable, lifetime-bound row ordering without moving table payloads;
- bounds-checked binary cursors, hex conversion, and byte search;
- UTF boundaries plus JSON, URI-component, and XML text conversion;
- compile-once expressions and scripts with bounded execution;
- loss-aware argument and table JSON, URI, and CSV formatting;
- checked SQLite parameter binding and typed-table materialization;
- GoogleTest/Google Benchmark characterization in the sibling C++ tree;
- Rust integration/property tests and Criterion benchmarks.

## Documentation

### API guide

- [Rust API guide index](docs/api/index.md) — Rust-oriented documentation translated
  from the original C++ API where applicable

### Core design and usage

- [Architecture](docs/high-level/architecture.md)
- [C++ and Rust usage examples](docs/high-level/examples.md)
- [Values](docs/high-level/value.md)
- [Arguments](docs/high-level/arguments.md)
- [Tables](docs/high-level/table.md)

### Supporting facilities

- [Binary data](docs/high-level/binary.md)
- [Text boundaries](docs/high-level/text.md)
- [Expressions and scripts](docs/high-level/expression.md)
- [Interchange formats](docs/high-level/format.md)
- [SQLite adapter](docs/high-level/sqlite.md)
- [Benchmark methodology and results](docs/high-level/performance.md)

### Porting record

- [Porting plan](docs/port/porting-plan.md)
- [Compatibility decisions](docs/port/compatibility.md)
- [Architecture postmortem](docs/port/postmortem.md)
- [C++ issues and audit](docs/port/cpp-gd-issues.md)
- [Source size and complexity](docs/port/source-stats.md)

Run the Rust checks with:

```sh
cargo fmt --all -- --check
cargo test --all-targets --all-features
cargo test --lib --tests --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
cargo bench
```

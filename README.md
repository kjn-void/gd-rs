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

Project documentation:

- [Architecture](docs/architecture.md)
- [C++ and Rust usage examples](docs/examples.md)
- [Values](docs/value.md)
- [Arguments](docs/arguments.md)
- [Tables](docs/table.md)
- [Binary data](docs/binary.md)
- [Text boundaries](docs/text.md)
- [Expressions and scripts](docs/expression.md)
- [Interchange formats](docs/format.md)
- [SQLite adapter](docs/sqlite.md)
- [Benchmark methodology and results](docs/performance.md)
- [Porting plan](docs/port/porting-plan.md)
- [Port compatibility](docs/port/compatibility.md)
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

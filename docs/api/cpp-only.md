# C++-only facilities

The C++ documentation covers more than the semantic core ported to `gd-rs`. Some
facilities disappear because Rust or Cargo already supplies the mechanism; others are
application policy or independent libraries that should not be coupled to values,
arguments, and tables.

| C++ topic | Rust mapping or decision |
|---|---|
| arena allocation and arena borrowing | No public allocator API. Owned values use ordinary Rust containers; borrowing is expressed with lifetimes. Add an arena crate only for a measured workload. |
| custom vector | `Vec<T>`, slices, and iterators cover storage and views. Typed table columns use internal vectors. |
| compiler macros and feature detection | Cargo features, `rust-version = "1.86"`, `cfg`, and build scripts replace the macro layer. |
| CLI options | Use an application-level parser such as `clap`; no command-line policy belongs in the data-model crate. |
| ODBC database | Not ported. The maintained adapter is SQLite behind the `sqlite` feature. |
| file utilities | Use `std::fs`, `std::io`, and ecosystem crates for mapping, watching, or platform-specific operations. |
| logger and logger macros | Use the application's `log` or `tracing` stack. The library does not select sinks or write during ordinary evaluation. |
| SQL query builder and SQL value formatter | Not ported. SQL dialect policy belongs near a database driver; bind `Arguments` as parameters instead of interpolating literals. |
| raw pointers and pointer-valued variants | Intentionally absent from the safe `Value` model. Store domain handles outside GD values or define an application-owned typed wrapper. |
| manual reference counts and ownership flags | Replaced by owned values, borrowing `ValueRef`/row/column views, and `Arc` when shared ownership is actually required. |

Several apparent omissions are instead folded into a smaller Rust API:

- C++ `variant`, `variant_view`, and type-number machinery map to `Value`, `ValueRef`,
  and `DataType` in [Values](values.md).
- Owned, borrowed, and shared argument forms map to `Arguments`, borrowing accessors,
  and ordinary `Arc<Arguments>` in [Arguments](arguments.md).
- Member tables and DTO tables map to the same typed `Table`; nullable columns or an
  application sidecar replace per-row dynamic arguments. See [Tables](tables.md).
- C++ table indexes map to an equality `ColumnIndex` and stable `RowOrder`, described
  in [Indexes and row ordering](indexes.md).
- The custom expression compiler maps to bounded Rhai programs in
  [Expressions](expressions.md).

This boundary is deliberate rather than a claim that these tasks never matter. A
future addition should have a Rust-specific contract, evidence that it is reused by
the library's intended consumers, and benchmarks where performance is part of the
motivation. Reproducing a C++ abstraction only because it exists would preserve its
surface area without necessarily preserving its value.

# Rust API guides

These guides adapt the useful material in `../gd/documentation` to the public
`gd-rs` API. They are not mechanical translations of C++ headers. Examples use
Rust ownership, standard containers, typed errors, and the behavior implemented by
this crate.

The C++ documentation sometimes describes multiple storage classes for the same
concept. Rust consolidates those into one owned type plus borrowed views and optional
indexes. C++-only facilities are listed in [C++-only facilities](cpp-only.md) rather
than being assigned fictional Rust equivalents.

## Guides

- [Types and dynamic values](values.md)
- [Arguments](arguments.md)
- [Tables and schemas](tables.md)
- [Table indexes and row ordering](indexes.md)
- [JSON, URI, and CSV formatting](formatting.md)
- [UTF and text boundaries](text.md)
- [Expressions and scripts](expressions.md)
- [SQLite integration](sqlite.md)
- [C++-only facilities and migration choices](cpp-only.md)

API items are re-exported from the `gd` crate root, so examples generally import
with `use gd::{...}` rather than naming private implementation modules.

## Source-document coverage

| C++ documentation topic | Rust destination | Disposition |
|---|---|---|
| type numbers, groups, and tags | [values](values.md) | replaced by `DataType` |
| owning variant | [values](values.md) | `Value` |
| borrowed variant view | [values](values.md) | `ValueRef<'a>` |
| owning and viewing argument pairs | [arguments](arguments.md) | consolidated into `Argument` and borrowing accessors |
| packed arguments | [arguments](arguments.md) | semantic behavior retained; packed live storage rejected |
| shared arguments | [arguments](arguments.md) | use ordinary ownership or application-level `Arc` |
| member table | [tables](tables.md) | consolidated into `Table` |
| DTO table | [tables](tables.md) | consolidated into `Table` |
| per-row argument table | [tables](tables.md) | fixed schema retained; dynamic row sidecars are application-level |
| table index | [indexes](indexes.md) | `ColumnIndex` uses hashing rather than sorted vectors |
| table I/O | [formatting](formatting.md) | JSON and CSV retained; unsupported formats documented |
| UTF-8 utilities | [text](text.md) | standard Rust text operations plus GD-specific boundaries |
| expression runtime | [expressions](expressions.md) | Rhai-backed compiler and evaluator |
| SQLite database wrapper | [SQLite](sqlite.md) | narrow `rusqlite` adapter |
| compiler feature macros | [C++-only facilities](cpp-only.md) | Cargo, `rust-version`, and `cfg` replace them |
| arenas and custom vectors | [C++-only facilities](cpp-only.md) | standard containers; specialize only after measurement |
| CLI parser | [C++-only facilities](cpp-only.md) | application concern |
| ODBC wrapper | [C++-only facilities](cpp-only.md) | intentionally excluded |
| file/path helpers | [C++-only facilities](cpp-only.md) | `std::fs`, `std::io`, and `std::path` |
| logger and logging macros | [C++-only facilities](cpp-only.md) | application concern |
| SQL query builder and SQL templating | [C++-only facilities](cpp-only.md) | intentionally excluded; bind parameters through the database driver |

## Contract boundaries

The port is semantic rather than ABI-compatible:

- numeric widths, insertion order, duplicate argument names, nulls, and supported
  conversions are explicit contracts;
- packed byte layouts, raw pointer values, manual allocation flags, COM-style
  interfaces, and C++ tag dispatch are not reproduced;
- borrowed values and indexes carry lifetimes, so mutation cannot invalidate them;
- formatting rejects inputs that cannot be represented without losing information;
- `SQLite` connection and transaction semantics remain those of `rusqlite`.

See the [compatibility decisions](../port/compatibility.md) for characterized
differences and the [architecture postmortem](../port/postmortem.md) for the design
rationale behind the three-object core.

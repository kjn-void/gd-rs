# Architecture

`gd-rs` is a semantic port of the portable core of `gd`. It preserves useful
observable behavior while replacing C++ layout coupling, manual variants, and raw
ownership flags with Rust types.

The `sqlite` feature is a narrow adapter from GD values and arguments to `rusqlite`
and from query rows to typed tables. Generic database interfaces, ODBC and other
drivers are excluded. Pure SQL construction is also excluded from this crate; it can
be considered as a separate package if golden-output tests establish a concrete need.

## Dependency direction

```mermaid
flowchart TD
    Value["DataType / Value / ValueRef"] --> Arguments["Arguments / ArgumentIndex"]
    Value --> Schema["Schema / ColumnSpec"]
    Schema --> Table["Table / Row / Column"]
    Value --> Table
    Table --> ColumnIndex["ColumnIndex"]
    Binary["Binary cursors / hex / byte search"]
    Text["Text and parsing"] --> Value
    Value --> Expression["Expression engine / context / program"]
    Arguments --> Formatting["Formatting and adapters"]
    Table --> Formatting
    Text --> Formatting
    Arguments --> SQLite["SQLite adapter (feature)"]
    SQLite --> Table
    SQLite --> Rusqlite["rusqlite"]
```

Dependencies point from concrete facilities toward the small value core. Only the
feature-gated SQLite adapter depends on a database API; no module depends on a global
logger or service locator.

Expression parsing and execution are delegated to Rhai behind a GD-value boundary.
This replaces the C++ tokenizer, postfix compiler, erased function-pointer registry,
and bytecode interpreter with one maintained component. The boundary deliberately
returns only `Value`; Rhai-only arrays, maps, function pointers, and custom objects
produce a typed error.

CLI parsing, filesystem and rotation policy, console rendering, logging sinks, and
COM-like request routing are application integration concerns. Rust applications
should use `clap`, `std::fs`/`std::path`, and purpose-built logging or routing crates
directly. Re-exporting those crates here would add coupling without a GD-specific
abstraction.

## Ownership model

```mermaid
flowchart LR
    Owned["Owned storage\nValue / Arguments / Table"] -->|"borrow"| View["ValueRef / Row / Column"]
    Owned -->|"immutable borrow"| Index["ArgumentIndex / ColumnIndex"]
    Index -->|"prevents mutation"| Stable["Stable borrowed keys and positions"]
```

Borrowed views carry lifetimes. An index borrows its source immutably, so the source
cannot be structurally mutated while offsets or borrowed keys are in use. This removes
the stale-pointer and stale-offset states possible in the C++ companion index types.

## Error and diagnostic policy

Expected failures use typed `Result` errors. The library has no global logger and does
not emit output as a side effect of normal API failures. If a later module has a
concrete need for spans or diagnostics, it can expose optional `tracing` integration;
the application remains responsible for selecting and configuring a subscriber.

## Hash policy

Hash-backed schemas and indexes use `ahash`. This assumes keys are trusted or otherwise
non-adversarial. Ordered sequences remain vectors, because `Arguments` must preserve
duplicate names, unnamed entries, and insertion order.

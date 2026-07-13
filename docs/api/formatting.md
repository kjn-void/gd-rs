# Formatting

The C++ formatting layer accepts callbacks, raw output modes, and several SQL-oriented
targets. `gd-rs` keeps a smaller set of deterministic serializers for the crate's
central data structures:

- `arguments_to_json` and `arguments_to_uri`;
- `table_to_json` and `row_order_to_json`;
- `table_to_csv`.

Every function returns an owned `String` and a `FormatError` when the source cannot be
represented without ambiguity or data loss.

## Arguments as JSON

```rust
use gd::{Arguments, arguments_to_json};

let mut arguments = Arguments::new();
arguments.push_named("name", "Ada");
arguments.push_named("active", true);
arguments.push_named("visits", 3_i64);

assert_eq!(
    arguments_to_json(&arguments).unwrap(),
    r#"{"name":"Ada","active":true,"visits":3}"#,
);
```

JSON object keys require every argument to be named and every name to be unique.
Positional arguments and duplicate names return errors instead of being silently
dropped. Values use natural JSON scalars; bytes are lower-case hexadecimal strings
and UUIDs use canonical text. Non-finite floats are rejected because JSON has no
portable representation for them.

## Arguments as a URI query

```rust
use gd::{Arguments, arguments_to_uri};

let mut arguments = Arguments::new();
arguments.push_named("q", "rust & c++");
arguments.push_named("tag", "table");
arguments.push_named("tag", "value");

assert_eq!(
    arguments_to_uri(&arguments).unwrap(),
    "q=rust%20%26%20c%2B%2B&tag=table&tag=value",
);
```

URI formatting requires names but deliberately retains duplicate keys and insertion
order. It returns the query component without a leading `?`. Null is an empty value.
Names and values use [`encode_percent_component`](text.md#percent-encoding).

## Tables as JSON

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, table_to_json};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::U64(1), Value::from("Ada")]).unwrap();

assert_eq!(
    table_to_json(&table).unwrap(),
    r#"[{"id":1,"name":"Ada"}]"#,
);
```

Each row becomes an object whose keys are primary schema names, in schema order.
`row_order_to_json` emits the same representation while following a `RowOrder` rather
than physical insertion order.

## Tables as CSV

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, table_to_csv};

let schema = Schema::new([
    ColumnSpec::new("name", DataType::String),
    ColumnSpec::new("note", DataType::String).nullable(true),
])
.unwrap();
let mut table = Table::new(schema);
table
    .push_row([Value::from("Ada"), Value::from("uses, commas")])
    .unwrap();
table.push_row([Value::from("Grace"), Value::Null]).unwrap();

assert_eq!(
    table_to_csv(&table, true).unwrap(),
    "name,note\nAda,\"uses, commas\"\nGrace,\n",
);
```

The Boolean parameter controls whether primary column names are written as a header.
The `csv` crate handles quoting and line endings. Null is an empty field, bytes are
lower-case hexadecimal, and UUIDs use canonical text.

These functions serialize complete in-memory values. Streaming output, parsing CSV,
custom callback formatting, SQL literals, and a CLI renderer are not current public
APIs. Use the underlying ecosystem crates when those policies belong to an
application.

# Formatting

The C++ formatting layer accepts callbacks, raw output modes, and several SQL-oriented
targets. `gd-rs` keeps a smaller set of deterministic serializers for the crate's
central data structures:

- `arguments_to_json` and `arguments_to_uri`;
- `table_to_json` and `row_order_to_json`;
- `table_to_csv`;
- `table_from_json` and `table_from_csv`, which read the table formats back.

Every writer returns an owned `String` and a `FormatError` when the source cannot be
represented without ambiguity or data loss. The readers return a new `Table` or an
`ImportError`.

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
than physical insertion order. Tombstoned rows are logically deleted and are therefore
omitted from both functions. Use `table_debug::print` or the physical row APIs when
retained tombstoned data must be inspected.

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
lower-case hexadecimal, and UUIDs use canonical text. Tombstoned rows are omitted
because they are logically deleted. A table with no columns returns
`FormatError::ZeroColumnTable`, because CSV has no distinct zero-field record. The
`csv` reader strips a leading UTF-8 BOM, so a first field that literally begins with
U+FEFF does not survive a round trip.

## Reading tables back

Neither table format carries column types, so the reader supplies the schema. Both
sides typically share it as code or as a `Schema` built from the same definition:

```rust
use std::sync::Arc;

use gd::{
    ColumnSpec, DataType, Schema, Table, Value, ValueRef, table_from_csv, table_from_json,
    table_to_csv, table_to_json,
};

let schema = Arc::new(
    Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("name", DataType::String).with_alias("display_name"),
        ColumnSpec::new("score", DataType::F64).nullable(true),
    ])
    .unwrap(),
);
let mut table = Table::new(Arc::clone(&schema));
table
    .push_row([Value::U64(u64::MAX), Value::from("Ada"), Value::F64(0.1 + 0.2)])
    .unwrap();
table.push_row([Value::U64(2), Value::from("Grace"), Value::Null]).unwrap();

let from_json = table_from_json(Arc::clone(&schema), &table_to_json(&table).unwrap()).unwrap();
let from_csv =
    table_from_csv(Arc::clone(&schema), &table_to_csv(&table, true).unwrap(), true).unwrap();
for imported in [&from_json, &from_csv] {
    assert_eq!(imported.cell(0, 0), Ok(ValueRef::U64(u64::MAX)));
    assert_eq!(imported.cell(0, 2), Ok(ValueRef::F64(0.1 + 0.2)));
    assert_eq!(imported.cell(1, 2), Ok(ValueRef::Null));
}

// Keys may be aliases in any order; a missing nullable field reads as null.
let edited = table_from_json(schema, r#"[{"display_name":"Linus","id":3}]"#).unwrap();
assert_eq!(edited.cell_named(0, "name"), Ok(ValueRef::String("Linus")));
assert_eq!(edited.cell(0, 2), Ok(ValueRef::Null));
```

Each field is decoded according to its column type, the inverse of the value mapping
used by the writers. Numbers are parsed from their exact text, so 64-bit integers and
floats round-trip without loss. A field that does not decode as the column type is
passed to ordinary schema validation as its natural value (a JSON number becomes
`I64`, `U64`, or `F64`; CSV text becomes `String`). A column converter may accept it;
otherwise the row fails with `TableError::TypeMismatch`.

Field names may be primary names or aliases. In JSON, and in CSV with a header, a
missing column reads as null, so only a nullable column may be omitted. Names that
match no column become row-local extras when the schema uses `UnknownFields::Store`
(JSON values keep their natural type, CSV values are strings) and return
`ImportError::UnknownField` otherwise. A name and its alias in the same object or
header return `ImportError::DuplicateField`. CSV without a header is positional and
must have exactly one field per column.

CSV cannot tell null from an empty string or empty byte sequence. An empty field reads
as null, except in a required string or byte column, where it reads as the empty
value. The CSV reader also accepts the `NaN` and `inf` text that `table_to_csv` writes
for non-finite floats.

Imported tables have no tombstones and no properties, because the formats carry
neither. Import may create row-local extras under `UnknownFields::Store`, but the table
writers emit only schema columns, so extras are not serialized and do not round-trip.
`ImportError::Row` reports the zero-based data row and the `TableError` that rejected
it; no partial table is returned.

These functions work on complete in-memory strings. Streaming input and output,
custom callback formatting, SQL literals, and a CLI renderer are not current public
APIs. Use the underlying ecosystem crates when those policies belong to an
application.

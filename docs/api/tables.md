# Tables and schemas

The C++ documentation describes member tables, DTO tables, and tables with per-row
argument sidecars. `gd-rs` consolidates the fixed-schema behavior into one `Table`:

- an immutable `Schema` describes names, aliases, types, and nullability;
- each column stores its primitive type directly in a contiguous vector;
- `Row` and `Column` are borrowing views;
- all row mutations validate the complete schema contract.

There is no distinction between a temporary DTO table and a long-lived member table.
Choose ownership and placement through ordinary Rust structs and function signatures.

## Defining a schema

```rust
use gd::{ColumnSpec, DataType, Schema};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String).with_alias("display_name"),
    ColumnSpec::new("score", DataType::I32).nullable(true),
])
.unwrap();

assert_eq!(schema.len(), 3);
assert_eq!(schema.column_index("display_name"), Some(1));

```

Primary names and aliases must be unique across different columns. `Schema::new`
returns `TableError::DuplicateColumnName` rather than selecting one ambiguous column.

A `DataType::Null` column is always nullable. Other columns are non-nullable unless
`nullable(true)` is specified.

## Constructing and appending

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String),
    ColumnSpec::new("score", DataType::I32).nullable(true),
])
.unwrap();
let mut table = Table::with_capacity(schema, 100);

let first = table
    .push_row([Value::U64(1), Value::from("Ada"), Value::I32(95)])
    .unwrap();
let second = table
    .push_row([Value::U64(2), Value::from("Grace"), Value::Null])
    .unwrap();

assert_eq!((first, second), (0, 1));
assert_eq!(table.row_count(), 2);

```

Use `push_row([Value; N])` when the width is known at the call site.
`push_row_vec(Vec<Value>)` consumes an existing runtime-width vector without creating a
second staging vector.

The entire row is checked before any column changes. Width, exact logical type, and
nullability errors therefore leave the table unchanged. Numeric widths are not
implicitly widened during insertion.

## Reading rows, columns, and cells

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::U64(1), Value::from("Ada")]).unwrap();
table.push_row([Value::U64(2), Value::from("Grace")]).unwrap();

assert_eq!(table.cell(0, 0).unwrap(), ValueRef::U64(1));
assert_eq!(
    table.cell_named(1, "name").unwrap(),
    ValueRef::String("Grace")
);

let first_row = table.row(0).unwrap();
assert_eq!(first_row.get_named("name"), Some(ValueRef::String("Ada")));

let names: Vec<_> = table
    .column_named("name")
    .unwrap()
    .iter()
    .map(|value| value.as_str().unwrap())
    .collect();
assert_eq!(names, ["Ada", "Grace"]);

```

`Table::rows` iterates borrowing row views. A `Row` performs positional or schema-name
lookup; a `Column` scans one contiguous typed storage vector. The views do not own or
copy cell payloads.

## Mutation

`set_cell` validates position, exact type, and nullability before changing storage.
`pop_row` removes the last cell from every column atomically with respect to the table
structure.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value};

let schema = Schema::new([ColumnSpec::new("count", DataType::I64)]).unwrap();
let mut table = Table::new(schema);
table.push_row([Value::I64(1)]).unwrap();
table.set_cell(0, 0, Value::I64(2)).unwrap();
assert_eq!(table.cell(0, 0).unwrap().to_i64(), Ok(2));
assert!(table.pop_row());
assert!(table.is_empty());
```

The current API does not insert or remove columns after construction. Build a new
schema and table when the data model changes.

## Storage model

Each logical column has a matching storage variant such as `Vec<Option<i64>>` or
`Vec<Option<CompactString>>`. This differs from the C++ packed row buffer even where
the older documentation calls that buffer columnar. The representation favors direct
typed scans and simple validity rules.

Null state currently uses `Option<T>` rather than a separate bitmap. The public API
does not expose this choice, so a measured future optimization can replace it without
changing callers.

Cloning a `Table` clones its schema and column data. To construct several empty tables
with the same layout, clone the `Schema` explicitly. The crate does not reproduce the
C++ manual schema reference count or promise concurrent mutation. Share immutable
tables with `Arc<Table>` only when an application needs shared ownership.

## Dynamic per-row fields

The C++ per-row-arguments table has no direct counterpart. `Table` deliberately keeps
one validated schema for every row. Depending on the domain, represent optional data
with nullable columns or keep a sidecar collection keyed by row identity:

```rust
use std::collections::HashMap;

use gd::Arguments;

let mut extra_by_id: HashMap<u64, Arguments> = HashMap::new();
let mut extra = Arguments::new();
extra.push_named("owner", "Ada");
extra_by_id.insert(42, extra);
```

Prefer a stable domain key over a row position if rows may be rebuilt or reordered.
For genuinely heterogeneous JSON-style objects, a table may not be the right live
container.

## Related operations

- [Indexes and row ordering](indexes.md) covers repeated equality lookup and stable
  sorted traversal.
- [Formatting](formatting.md) covers table JSON and CSV output.
- [SQLite](sqlite.md) materializes query results directly into typed tables.

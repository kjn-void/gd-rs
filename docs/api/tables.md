# Tables and schemas

The C++ documentation describes member tables, DTO tables, and tables with per-row
argument sidecars. `gd-rs` consolidates the fixed-schema behavior into one `Table`:

- an immutable `Schema` describes names, aliases, types, and nullability;
- each column stores its primitive type directly in a contiguous vector;
- an opt-in schema policy stores unknown names as lazy row-local extras;
- `Row` and `Column` are borrowing views;
- all fixed-row mutations validate the complete schema contract.

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

### Typed bulk column operations

Required fixed-width columns can be checked once and borrowed as an ordinary typed
slice. The supported element types are `bool`, the fixed-width integer and
floating-point primitives, and `Uuid`:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([ColumnSpec::new("requests", DataType::U64)]).unwrap();
let mut table = Table::new(schema);
for value in [500_u64, 1_000, 1_500, 2_000] {
    table.push_row([Value::U64(value)]).unwrap();
}

let values = table
    .column_named("requests")
    .unwrap()
    .as_slice::<u64>()
    .unwrap();

let doubled: Vec<_> = values.iter().map(|value| value * 2).collect();
assert_eq!(doubled, [1_000, 2_000, 3_000, 4_000]);

let large: Vec<_> = values
    .iter()
    .copied()
    .filter(|value| *value >= 1_500)
    .collect();
assert_eq!(large, [1_500, 2_000]);

let selected_rows: Vec<_> = values
    .iter()
    .enumerate()
    .filter_map(|(row, value)| (*value >= 1_500).then_some(row))
    .collect();
assert_eq!(selected_rows, [2, 3]);

let total = values
    .iter()
    .copied()
    .fold(0_u64, u64::saturating_add);
assert_eq!(total, 5_000);
```

These are the standard slice and `Iterator` `map`, `filter`, and `fold` operations;
the table does not wrap them in a second collection API. `as_slice` performs runtime
type and nullability checks once. It returns `ColumnSliceError::TypeMismatch` for the
wrong `T` and `ColumnSliceError::Nullable` when a column can contain nulls.

Keeping the returned type as `&[T]` makes the hot loop monomorphic and contiguous. It
also lets LLVM eliminate bounds checks and auto-vectorize suitable integer reductions
and element-wise transformations. For a table filter, retain row identity by using
`enumerate` and collecting positions as above.

`Table::column_pair_mut(source, target)` supports direct bulk transforms without
exposing the storage enum. It returns an immutable `Column` for `source` and a
`ColumnMut` for a distinct `target`; equal or out-of-range positions return `None`.
`ColumnMut::as_mut_slice::<T>` applies the same fixed-width type and nullability checks
as `Column::as_slice::<T>`, then returns `&mut [T]`. The two disjoint slices can be
zipped by ordinary sequential iterators or a parallel slice library.

When the column type is not known until runtime, `Column::for_each_value` retains a
`ValueRef` callback but dispatches the column's storage type and nullability only once:

```rust
let column = table.column_named("requests").unwrap();
let mut total = 0_u64;
column.for_each_value(|value| {
    if let ValueRef::U64(value) = value {
        total = total.saturating_add(value);
    }
});
```

Use `iter` when iterator composition or early termination matters. Use
`for_each_value` for a terminal dynamic scan, and `as_slice::<T>` when the caller knows
the fixed-width type.

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

Each logical column has a matching storage variant. Required columns use dense storage
such as `Vec<i64>` or `Vec<CompactString>`; nullable columns currently use
`Vec<Option<i64>>` or `Vec<Option<CompactString>>`. This differs from the C++ packed row
buffer even where the older documentation calls that buffer columnar. Required numeric
columns expose `&[T]` for direct bulk scans, while dynamic `ValueRef` iteration remains
available for code that does not know the type statically.

Nullable null state currently uses `Option<T>` rather than a separate bitmap. The
typed-slice API rejects nullable columns, so a measured future validity-bitmap
optimization can replace that internal representation without changing callers.

Cloning a `Table` clones its schema and column data. To construct several empty tables
with the same layout, clone the `Schema` explicitly. The crate does not reproduce the
C++ manual schema reference count or promise concurrent mutation. Share immutable
tables with `Arc<Table>` only when an application needs shared ownership.

## Dynamic per-row fields

Schemas reject unknown names by default. A schema can explicitly allow row-local
dynamic values without making its fixed typed columns mutable:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, TableError, UnknownFields, Value, ValueRef};

fn files() -> Result<Table, TableError> {
    let schema = Schema::new([
        ColumnSpec::new("path", DataType::String),
        ColumnSpec::new("size", DataType::U64),
    ])?
    .with_unknown_fields(UnknownFields::Store);
    let mut table = Table::new(schema);

    let row = table.push_row_with_extras(
        [Value::from(r"C:\data\entry.bin"), Value::U64(1_000)],
        [
            ("category", Value::from("binary")),
            ("region", Value::from("north")),
        ],
    )?;
    assert_eq!(table.cell_named(row, "category")?, ValueRef::String("binary"));

    table.set_named(row, "category", "archive")?;
    assert_eq!(table.cell_named(row, "category")?, ValueRef::String("archive"));
    Ok(table)
}
```

Known names still use typed column storage and exact type/null validation. A closed
schema allocates no extras sidecar. An open schema adds a parallel nullable-pointer
vector and creates each row's extras object only when the row receives an unknown
field; the first two values use inline storage. `cell_named` and `Row::get_named`
search fixed names first and then the row extras.

Extras are row metadata rather than logical columns. They are therefore absent from
`column_named`, table indexes, row ordering, fixed-schema row iteration, JSON, and CSV.
Promote a repeatedly scanned or serialized field to a real nullable column. Use an
external collection keyed by stable domain identity when the metadata should not be
owned by the table at all.

## Related operations

- [Indexes and row ordering](indexes.md) covers repeated equality lookup and stable
  sorted traversal.
- [Formatting](formatting.md) covers table JSON and CSV output.
- [SQLite](sqlite.md) materializes query results directly into typed tables.

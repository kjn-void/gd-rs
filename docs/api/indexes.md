# Indexes and row ordering

The C++ table index is centered on sorted index structures and binary search.
`gd-rs` exposes two smaller, borrowing operations with explicit costs:

- `ColumnIndex` builds a hash index for repeated equality lookup;
- `RowOrder` builds a stable permutation for ordered traversal.

Both borrow the source `Table`. Rust therefore prevents table mutation while stored
row positions or borrowed keys are in use.

## Equality indexes

```rust
use gd::{ColumnSpec, DataType, IndexKeyRef, Schema, Table, Value};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String).nullable(true),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::U64(1), Value::from("Ada")]).unwrap();
table.push_row([Value::U64(2), Value::from("Grace")]).unwrap();
table.push_row([Value::U64(3), Value::from("Ada")]).unwrap();
table.push_row([Value::U64(4), Value::Null]).unwrap();

let index = table.index(1).unwrap();
assert_eq!(index.rows(IndexKeyRef::from("Ada")), &[0, 2]);
assert!(index.rows(IndexKeyRef::from("missing")).is_empty());
assert_eq!(index.null_rows(), &[3]);
assert_eq!(index.distinct_key_count(), 2);
```

`rows` returns every matching original row position in insertion order; duplicate
keys are not collapsed. Null rows have their own accessor rather than a lookup key.

The supported column types are `Bool`, all signed and unsigned integer widths,
`String`, `Bytes`, and `Uuid`. Signed and unsigned keys remain separate domains.
`Null`, `F32`, and `F64` columns return `TableError::UnsupportedIndexType`; floating
point equality, especially around NaN and signed zero, needs an application policy.

Building an index takes O(r) expected time and O(r) additional space for `r` rows.
An expected lookup is O(1), plus the number of matching rows returned. The index is a
snapshot view: drop it, mutate the table, and build another one when data changes.

## Stable row ordering

```rust
use gd::{ColumnSpec, DataType, NullOrder, Schema, SortDirection, Table, Value};

let schema = Schema::new([
    ColumnSpec::new("name", DataType::String),
    ColumnSpec::new("score", DataType::I32).nullable(true),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::from("Ada"), Value::I32(10)]).unwrap();
table.push_row([Value::from("Grace"), Value::Null]).unwrap();
table.push_row([Value::from("Linus"), Value::I32(10)]).unwrap();
table.push_row([Value::from("Edsger"), Value::I32(5)]).unwrap();

let order = table
    .row_order_named("score", SortDirection::Descending, NullOrder::Last)
    .unwrap();
assert_eq!(order.positions(), &[0, 2, 3, 1]);

let names: Vec<_> = order
    .rows()
    .map(|row| row.get_named("name").unwrap().as_str().unwrap())
    .collect();
assert_eq!(names, ["Ada", "Linus", "Edsger", "Grace"]);
```

Equal values retain their original order. Null placement is independent of ascending
or descending direction. Floating-point columns are sortable and use Rust's total
ordering, so every NaN and signed-zero representation has a deterministic position.

Building a `RowOrder` takes O(r log r) time and O(r) space for its positions. It does
not copy cells or change the table; iteration after construction performs no further
allocation.

The current API orders by one column. For compound application-specific ordering,
collect row positions and sort them with a comparator over `Table::cell`, or add a
measured crate-level operation when that pattern becomes common.

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
Tombstoned rows are excluded from keys, null rows, and `distinct_key_count`; their
positions are reported by `tombstoned_rows`.

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
`positions` covers every physical row; `live_rows` skips tombstoned rows while keeping
the same key order.

Building a `RowOrder` takes O(r log r) time and O(r) space for its positions. It does
not copy cells or change the table; iteration after construction performs no further
allocation.

The current API orders by one column. For compound application-specific ordering,
collect row positions and sort them with a comparator over `Table::cell`, or add a
measured crate-level operation when that pattern becomes common.

## Indexed left joins

`left.left_join_rows(left_column, &right_index)` returns `(left_position,
Option<right_position>)` pairs. It preserves left source order and emits every
matching right row in right source order. Unmatched and null left keys produce one
`None` pair; null keys never match, including other nulls. Deleted rows on either
side are excluded. The two columns must have exactly the same supported logical
type; signed and unsigned types are not coerced.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value};
let schema = Schema::new([ColumnSpec::new("id", DataType::I64)])?;
let mut left = Table::new(schema.clone());
left.push_row([Value::I64(7)])?;
left.push_row([Value::I64(9)])?;
let mut right = Table::new(schema);
right.push_row([Value::I64(7)])?;
right.push_row([Value::I64(7)])?;
let index = right.index(0)?;
assert_eq!(left.left_join_rows(0, &index)?,
           [(0, Some(0)), (0, Some(1)), (1, None)]);
# Ok::<(), gd::TableError>(())
```

An index can be reused across joins. Building it is expected O(right rows); probing
is expected O(left rows + emitted matches). The returned vector owns positions, not
cells, and requires O(emitted matches) space. A many-to-many join can therefore
produce a large result. Inner joins can discard pairs with no right position.
Materializing selected payload columns is an explicit application step, allowing
computed columns and name-conflict policies without a query language.

The right index borrows its table and prevents mutation during use. Returned
positions do not keep that borrow alive: like `select_rows`, they must not be reused
after removals or compaction change either table's physical row positions.

## Composite indexes and joins

`table.composite_index([column_a, column_b, ...])` builds a
`CompositeIndex<'a, N>` over an ordered tuple of any fixed positive width. Component
kinds match `ColumnIndex`: booleans, signed/unsigned integer widths, strings, bytes,
and UUIDs. Null-typed and floating-point columns are rejected, even for an empty
table. Repeated columns are allowed; a zero-width key returns `EmptyIndexKey`.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value};
let schema = Schema::new([
    ColumnSpec::new("customer", DataType::String),
    ColumnSpec::new("order", DataType::I64),
])?;
let mut left = Table::new(schema.clone());
left.push_row([Value::from("Ada"), Value::I64(7)])?;
let mut right = Table::new(schema);
right.push_row([Value::from("Ada"), Value::I64(7)])?;
right.push_row([Value::from("Ada"), Value::I64(7)])?;
let index = right.composite_index([0, 1])?;
assert_eq!(index.rows(["Ada".into(), 7_i64.into()]), &[0, 1]);
assert_eq!(left.left_join_rows_composite([0, 1], &index)?,
           [(0, Some(0)), (0, Some(1))]);
# Ok::<(), gd::TableError>(())
```

Tuple order matters. Signed/unsigned keys are distinct; strings are compared as
strings, not converted from numbers. As in single-column probes, signed widths are
widened losslessly inside keys, but join components must have **exactly matching
logical types**, including widths. A null in any component excludes the row from
key buckets and prevents it from joining. `null_rows` reports those live rows;
`tombstoned_rows` reports all excluded deleted rows. `distinct_key_count` counts
complete unique tuples. Every duplicate match retains physical source order.

Keys borrow string/byte payloads. For fixed-size keys, construction takes expected O(rows × N) time and
storage proportional to keys and indexed row positions. Probing takes expected
O(N + returned matches) including consuming the returned match slice. Hashing and
comparing string/byte components additionally depend on their payload lengths. The source
borrow prevents invalidation; query values do not need to live as long as the index
or the returned position slice.

`left_join_composite(columns, &index)` is a lazy iterator yielding
`(Row, Option<Row>)` rather than allocating the result vector. `left_join(column,
&single_column_index)` offers the same shape for single-column joins. Both validate
column/type compatibility before iteration, exclude tombstones, expand duplicates,
and emit one unmatched `None` per null/missing left key. Both source tables and the
right index remain borrowed throughout iteration. `take`, `filter` and other
ordinary iterator operations can bound or transform output without copying cells.
No joins implicitly materialize payload columns or choose conflict-name policies.

`left_join_rows_composite` and the existing `left_join_rows` collect owned position
pairs with those same semantics. These snapshots do not keep either source borrowed
and become stale after removal/compaction. A many-to-many result can be much larger
than either input; use lazy iteration when every pair need not be stored.

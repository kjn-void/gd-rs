# Tables

`Table` combines an immutable `Schema` with one typed vector per column. `Row` and
`Column` are borrowing views; ordinary cell access returns `ValueRef`, while required
fixed-width columns can also expose a checked typed slice.

```mermaid
flowchart TD
    Schema["Schema\nColumnSpec + ahash name/alias map"] --> Table
    Table --> C0["required: Vec&lt;T0&gt;"]
    Table --> C1["nullable: Vec&lt;Option&lt;T1&gt;&gt;"]
    Table --> CN["required or nullable typed column"]
    Table --> Extras["optional Box&lt;RowExtras&gt; per row"]
    Table -->|"immutable borrow"| Index["ColumnIndex\ntyped AHashMap&lt;key, rows&gt;"]
    Table -->|"immutable borrow"| Order["RowOrder\nstable Vec&lt;row position&gt;"]
```

This is different from the C++ `table_column_buffer`, whose fixed payload buffer is
row-major despite “columnar” wording in its documentation.

## Schema and rows

Primary names and aliases are unique across different columns. Schema lookup uses
`ahash` and is expected O(name length). A row must have exactly the schema width.
Values must match the declared `DataType`; conversion is not implicit.

`push_row([Value; N])` consumes a fixed-width array without a staging allocation.
`push_row_vec(Vec<Value>)` handles runtime-width rows and consumes the existing vector.
Both validate the entire row before changing any column.

Nullable columns accept `Value::Null`; non-nullable columns reject it. Unlike C++
`row_add()`, an omitted value never becomes a non-null cell with uninitialized bytes.

## Open schemas

`UnknownFields::Reject` is the default. Applying
`with_unknown_fields(UnknownFields::Store)` keeps the fixed schema immutable but lets
individual rows own additional named `Value`s. `push_row_with_extras` declares fixed
and dynamic values atomically, while `set_named` updates either storage class through
one name-based API. Fixed names and aliases always take precedence.

Closed schemas store no sidecar. Open schemas add a vector parallel to the fixed
columns; each element is either a null pointer or points to one row's extras object:

```mermaid
flowchart LR
    Table["Table"] --> Fixed["fixed columns<br/>Vec&lt;ColumnStorage&gt;"]
    Fixed --> Path["path: Vec&lt;String&gt;<br/>row 0 / row 1 / row 2"]
    Fixed --> Size["size: Vec&lt;u64&gt;<br/>row 0 / row 1 / row 2"]

    Table --> Policy["extras storage selected by schema"]
    Policy --> Closed["Reject: Disabled<br/>no per-row allocation"]
    Policy --> Sidecar["Store: Vec&lt;Option&lt;Box&lt;RowExtras&gt;&gt;&gt;"]
    Sidecar --> Slot0["row 0<br/>None / null pointer"]
    Sidecar --> Slot1["row 1<br/>Some / Box pointer"]
    Sidecar --> Slot2["row 2<br/>Some / Box pointer"]

    Slot1 --> Heap1["heap: RowExtras::Inline<br/>SmallVec inline capacity 2<br/>(category, String: binary)<br/>(region, String: north)"]
    Slot2 --> Heap2["heap: RowExtras::Inline<br/>SmallVec inline capacity 2<br/>(category, U64: 7)"]

    Heap1 -. "fifth unique field" .-> Hashed["RowExtras::Hashed<br/>AHashMap&lt;CompactString, Value&gt;"]
```

The diagram also shows that the same extra name can have a different `Value` type in
another row. It has no shared column storage or schema-level type contract.

Extras storage is schema-aware. A closed schema allocates no pointer vector. In an open
schema every row has one nullable pointer slot; rows without extras allocate no
`RowExtras` object, and the first two extras remain inline in the allocated row object.
Rows stay in the compact representation through four fields, then promote to an
`AHashMap` on the fifth unique name. They are deliberately excluded from column scans,
indexes, ordering, and fixed-schema formatting because they do not form homogeneous
columns.

## Null storage

Required columns use dense `Vec<T>` storage because the schema and atomic row
validation guarantee that every committed row contains a value. Nullable columns use
`Vec<Option<T>>`; for example, `Option<i64>` occupies 16 bytes on the current target.
A separate validity bitmap could reduce nullable-column memory, but would add another
allocation and more indexing logic. The public API exposes dense required values as a
slice, not the internal storage enum, so nullable representation can still change.

## Typed bulk column operations

`Column::as_slice::<T>` checks the runtime schema type and nullability once, then
returns the required column as `&[T]`. It supports Boolean, fixed-width integer,
floating-point, and UUID columns. A wrong type returns `ColumnSliceError::TypeMismatch`;
a nullable column returns `ColumnSliceError::Nullable`.

The slice uses standard iterator operations rather than table-specific versions of
`map`, `filter`, and `fold`:

```rust
let values = table.column_named("requests").unwrap().as_slice::<u64>()?;

let doubled: Vec<u64> = values.iter().map(|value| value * 2).collect();
let large: Vec<u64> = values.iter().copied().filter(|value| *value >= 1_000).collect();
let selected_rows: Vec<usize> = values
    .iter()
    .enumerate()
    .filter_map(|(row, value)| (*value >= 1_000).then_some(row))
    .collect();
let total = values.iter().copied().fold(0_u64, u64::saturating_add);
```

This moves dynamic dispatch out of the hot loop. The compiler sees a monomorphic,
contiguous slice, which is the form most suitable for bounds-check elimination and
auto-vectorization. Filtering preserves table correspondence by collecting row
positions; callers that only need values can use ordinary `filter` directly.

For code that does not know the column type until runtime, `Column::for_each_value`
matches storage type and nullability once, then calls a `ValueRef` closure for every
cell. This avoids the repeated storage dispatch and bounds check in `Column::iter`
without fragmenting the dynamic value API. It is a terminal operation; use `iter` for
composable or short-circuiting traversal, and `as_slice::<T>` for an explicitly typed
loop.

## Views and indexes

A dynamic column scan yields `ValueRef`; a required fixed-width scan can instead walk
its typed slice directly. Row iteration assembles a borrowing view across columns.
`ColumnIndex` borrows the table, uses a typed `AHashMap`, preserves
duplicate row positions, and tracks null rows separately. Boolean, integer, string,
byte, and UUID columns are indexable. Floating-point indexes are rejected until NaN
and signed-zero equality have an explicit policy.

## Ordered rows

`row_order` and `row_order_named` return `RowOrder`, a stable permutation of original
row positions. The table is neither copied nor mutated. `SortDirection` controls
non-null values, while `NullOrder` independently places nulls first or last. Equal
keys retain insertion order.

Integer, Boolean, string, byte, and UUID columns use their ordinary total order.
Floating-point columns use `total_cmp`, which gives deterministic positions to NaNs
and distinguishes negative and positive zero. A `RowOrder` immutably borrows its
source, preventing row positions from becoming stale during iteration.

This replaces destructive selection and bubble sorts with standard stable sorting of
row indexes. Constructing an order takes **O(r log r)** time and **O(r)** space; it
does not move payloads from unrelated columns. The C++ algorithms take **O(r²)**
comparisons and may move complete rows after comparisons.

## Complexity

| Operation | Expected time | Extra space |
|---|---:|---:|
| positional cell read/write | O(1) | none |
| schema name/alias lookup | O(name length) | none per lookup |
| unknown row-field lookup | O(name length + extras) through four fields; expected O(name length) after promotion | none per lookup |
| append complete row | O(columns) | payload ownership only |
| append row with extras | expected O(columns + extras) | owned extra names and values |
| pop last row | O(columns) | none |
| column scan | O(rows) | none |
| build column index | O(rows) | O(rows) |
| indexed equality lookup | O(key length) | none per lookup |
| build stable row order | O(rows log rows) | O(rows) |
| iterate ordered rows | O(rows) | none after construction |

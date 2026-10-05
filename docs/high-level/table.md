# Tables

`Table` combines a shared immutable `Arc<Schema>` with one typed vector per column.
`Row` and `Column` are borrowing views; ordinary cell access returns `ValueRef`, while
required fixed-width columns can also expose a checked typed slice. Independent
tables with the same layout can share all schema metadata without sharing row data.

```mermaid
flowchart TD
    Schema["Arc&lt;Schema&gt;\nColumnSpec + converters + name/alias map"] --> Table
    Table --> C0["required: Vec&lt;T0&gt;"]
    Table --> C1["nullable: Vec&lt;Option&lt;T1&gt;&gt;"]
    Table --> CN["required or nullable typed column"]
    Table --> Extras["optional Box&lt;RowExtras&gt; per row"]
    Table --> Tombstones["optional tombstone flags\nallocated on first deletion"]
    Table --> Properties["table-wide Arguments properties"]
    Table -->|"immutable borrow"| Index["ColumnIndex\ntyped AHashMap&lt;key, rows&gt;"]
    Table -->|"immutable borrow"| Order["RowOrder\nstable Vec&lt;row position&gt;"]
```

This is different from the C++ `table_column_buffer`, whose fixed payload buffer is
row-major despite “columnar” wording in its documentation.

## Schema and rows

Primary names and aliases are unique across different columns. Schema lookup uses
`ahash` and is expected O(name length). A row must have exactly the schema width.
Values must match the declared `DataType` unless that column explicitly attaches a
named `ColumnConverter`. A converter handles only mismatched non-null input and its
output is validated before storage.

`Table::new` and `Table::with_capacity` accept either an owned `Schema` or a shared
`Arc<Schema>`. Cloning a table or its `schema_arc` handle increments standard atomic
ownership; schema metadata is released only after the last handle is dropped.

`push_row([Value; N])` consumes a fixed-width array without a staging allocation.
`push_row_vec(Vec<Value>)` handles runtime-width rows and consumes the existing vector.
Both convert and validate the entire row before changing any column.

`copy_range` and `copy_rows` construct independent row storage while sharing the
source table's immutable schema. A contiguous range copies each typed vector slice in
bulk. An arbitrary row selection gathers values one column at a time into consecutive
destination positions, preserving selection order and duplicates. This keeps table
selection aligned with the SoA layout instead of transposing every source row through
dynamic `Value` objects.

With the `rayon` feature, `par_copy_range` and `par_copy_rows` submit each fixed column
as an independent task. The current Rayon pool controls concurrency, so workers can
dynamically steal uneven column tasks. Row-local extras are copied after the fixed
columns because they are a single row-oriented sidecar rather than one store per
declared column.

Nullable columns accept `Value::Null`; non-nullable columns reject it. Unlike C++
`row_add()`, an omitted value never becomes a non-null cell with uninitialized bytes.

Table-wide properties use uniquely named dynamic `Value`s in insertion order. They
are independent of rows, typed columns, and open-schema extras. Row-selection copies
clone the properties together with the selected data.

### Concurrent construction

`ConcurrentTableBuilder` separates parallel row production from the final `Table`.
Producers share `&ConcurrentTableBuilder`; every row is converted, validated, and assembled before
it is published as one concurrent-vector element. This avoids a partially appended
row if validation fails or another producer observes the collection concurrently.

```mermaid
flowchart LR
    P0["producer 0<br/>validate complete row"] --> Rows
    P1["producer 1<br/>validate complete row"] --> Rows
    PN["producer N<br/>validate complete row"] --> Rows
    Rows["temporary concurrent rows<br/>SmallVec&lt;Value&gt; + RowExtras"]
    Rows -->|"consume into_table()"| T["new Table"]
    Rows -->|"append_to(&mut table)"| Existing["existing Table<br/>preserve old rows"]
    T --> C0["Vec&lt;T0&gt;"]
    T --> C1["Vec&lt;T1&gt;"]
    T --> CN["Vec&lt;TN&gt;"]
    Existing --> EC["extend existing<br/>typed Vec columns"]
```

The temporary layout is deliberately row-oriented: one reservation publishes the
whole logical row rather than independently extending several columns and risking
different lengths. Most rows of up to eight values keep their staging values inline.
`into_table` consumes the builder and performs one row-to-column transpose into a new
dense SoA table. `append_to` performs the same move into an existing compatible table,
preserving its row positions and returning the appended range. Pending rows and boxed
extras are moved rather than cloned or reallocated.

This is a construction boundary, not a concurrently mutable `Table`. Concurrent row
order follows scheduling, live cell mutation is not exposed, and no producer may
remain borrowed when the builder is consumed. `append_to` takes `&mut Table`, making
the final merge exclusive and ensuring all typed columns remain the same length. Once
converted or appended, typed column slices and `RowsMut::split_at` provide the existing
safe parallel-processing paths.

```rust
use std::thread;

use gd::{ColumnSpec, ConcurrentTableBuilder, DataType, Schema, Value};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("square", DataType::U64),
])
.unwrap();
let builder = ConcurrentTableBuilder::new(schema);

thread::scope(|scope| {
    for shard in 0_u64..4 {
        let builder = &builder;
        scope.spawn(move || {
            let rows = (0_u64..100).map(|offset| {
                let id = shard * 100 + offset;
                [Value::U64(id), Value::U64(id * id)]
            });
            builder.extend_rows(rows).unwrap();
        });
    }
});

let table = builder.into_table();
assert_eq!(table.row_count(), 400);
```

### Shared-schema and row memory

Sharing the schema separates the one-time metadata cost from each table's row storage:

```mermaid
flowchart LR
    Schema["Arc&lt;Schema&gt;<br/>columns + name/alias AHashMap"]
    Schema --> A["Table A<br/>own typed columns"]
    Schema --> B["Table B<br/>own typed columns"]
    Schema --> C["Table C<br/>own typed columns"]
```

The following 64-bit M3 release-layout comparison uses five required columns in this
order: `u8`, `u64`, `bool`, `u8`, and `i32`. It excludes allocator bookkeeping, null
metadata, open-schema extras, and unused capacity. C++ GD rounds every cell slot to a
four-byte boundary:

```text
C++ packed row (24 bytes)

offset  0  u8    [value][3 bytes padding]
offset  4  u64   [8-byte value]
offset 12  bool  [value][3 bytes padding]
offset 16  u8    [value][3 bytes padding]
offset 20  i32   [4-byte value]
```

The Rust table stores five independent, correctly aligned vectors:

```text
Rust SoA storage (15 bytes per populated row)

Vec<u8>    1 byte  x capacity
Vec<u64>   8 bytes x capacity
Vec<bool>  1 byte  x capacity
Vec<u8>    1 byte  x capacity
Vec<i32>   4 bytes x capacity
```

Rust's standard `Vec<bool>` stores ordinary one-byte `bool` elements; unlike C++
`std::vector<bool>`, it is not bit-packed. It therefore exposes normal `&[bool]` and
`&mut [bool]` slices that Rayon can partition directly.

Measured fixed layouts are 104 bytes for a C++ internal table and 120 bytes for the
Arc-backed Rust `Table` on the same 64-bit target (`std::mem::size_of::<Table>()`).
Rust additionally keeps five 40-byte `ColumnStorage` values in the table's
column-descriptor allocation. For capacity `C`, excluding the shared schema and
allocator bookkeeping:

| Implementation | Fixed per table | Row capacity | Total per table |
|---|---:|---:|---:|
| C++ internal table | 104 bytes | 24C bytes | `104 + 24C` |
| Rust `Table` | 120 + 200 bytes | 15C bytes | `320 + 15C` |

The C++ representation is smaller for very small equal-capacity tables; the Rust
representation crosses below it at about 24 rows because it avoids per-cell padding.
The shared schema was approximately 800 requested bytes for C++ and 1,016 requested
bytes for Rust in this fixture. Rust's larger one-time schema includes the `AHashMap`
used for expected constant-time name and alias lookup. Real tiny-table totals also
depend on allocation rounding and growth policy: Rust has one descriptor allocation
plus one allocation per typed column, while C++ has one packed row allocation.

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

## Tombstoned rows

A row can be logically deleted without being removed. `tombstone_row` records one
flag at the row's physical position, so the payload, the schema, and every other row
position stay unchanged. `restore_row` clears one flag and `restore_all_rows` clears
every flag at once. The flag vector is allocated on the first tombstone and dropped
when no tombstoned row remains, so tables that never delete a row pay no metadata
cost. The first deletion after the vector is dropped initializes one flag per physical
row, which is O(rows); after that, each deletion and restoration is O(1).

Physical and live views are deliberately separate:

- `row_count`, `rows`, `cell`, `set_cell`, `row_mut`, and the debug helpers remain
  physical; a tombstoned row keeps its payload and can still be read, written, or
  inspected;
- `live_row_count` and `live_rows` exclude tombstoned rows; `Row::is_tombstoned` and
  `tombstoned_rows` expose the metadata directly;
- `ColumnIndex` excludes tombstoned rows from keys, null rows, and distinct-key counts,
  and reports their positions through `ColumnIndex::tombstoned_rows`;
- `RowOrder::positions` covers every physical row while `RowOrder::live_rows` skips
  tombstoned rows;
- `table_to_json`, `table_to_csv`, and `row_order_to_json` omit tombstoned rows because
  they serialize logical content.

Row-selection copies carry flags with the selected positions: `copy_range` and
`copy_rows`, including their parallel variants, keep copied rows in their original
logical state while sharing the immutable schema. `pop_row` removes the last physical
row whether or not it is tombstoned. `ConcurrentTableBuilder` publishes live rows, and
`append_to` preserves the destination table's existing tombstones.

`compact` is the only way to reclaim tombstoned rows. It removes them from every
column and from the extras sidecar in one order-preserving pass, clears the tombstone
metadata, and returns a `RowCompaction` that maps pre-compaction positions to their new
positions. Tombstoned slots are never reused by appends: a reused slot would make a
position recorded earlier silently name another row, and it would discard a restorable
row without an explicit request.

## Typed bulk column operations

`Column::as_slice::<T>` checks the runtime schema type and nullability once, then
returns the required column as `&[T]`. It supports Boolean, fixed-width integer,
floating-point, and UUID columns. A wrong type returns `ColumnSliceError::TypeMismatch`;
a nullable column returns `ColumnSliceError::Nullable`.

The slice uses standard iterator operations rather than table-specific versions of
`map`, `filter`, and `fold`:

```rust
# use gd::{ColumnSpec, DataType, Schema, Table, Value};
# let schema = Schema::new([ColumnSpec::new("requests", DataType::U64)]).unwrap();
# let mut table = Table::new(schema);
# table.push_row([Value::U64(1_000)]).unwrap();
let values = table.column_named("requests").unwrap().as_slice::<u64>()?;

let doubled: Vec<u64> = values.iter().map(|value| value * 2).collect();
let large: Vec<u64> = values.iter().copied().filter(|value| *value >= 1_000).collect();
let selected_rows: Vec<usize> = values
    .iter()
    .enumerate()
    .filter_map(|(row, value)| (*value >= 1_000).then_some(row))
    .collect();
let total = values.iter().copied().fold(0_u64, u64::saturating_add);
# Ok::<(), Box<dyn std::error::Error>>(())
```

For an element-wise transform, `Table::columns_io` borrows any number of immutable
inputs and mutable outputs together. Each view can then be converted to a typed slice
with one type/nullability check:

```rust
# use gd::{ColumnSpec, DataType, Schema, Table, Value};
# let schema = Schema::new(["left", "right", "sum", "product"].map(|name| ColumnSpec::new(name, DataType::U32))).unwrap();
# let mut table = Table::new(schema);
# table.push_row([Value::U32(3), Value::U32(4), Value::U32(0), Value::U32(0)]).unwrap();
let ([left, right], [sum, product]) = table.columns_io([0, 1], [2, 3])?;
let left = left.as_slice::<u32>()?;
let right = right.as_slice::<u32>()?;
let sum = sum.as_mut_slice::<u32>()?;
let product = product.as_mut_slice::<u32>()?;

for (((left, right), sum), product) in left.iter().zip(right).zip(sum).zip(product) {
    *sum = left.saturating_add(*right);
    *product = left.saturating_mul(*right);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Input positions may repeat because they are shared borrows. Output positions must be
unique and cannot also occur as inputs; `columns_io` validates those rules and bounds
before returning any view. The common one-input, one-output case has a specialized
zero-allocation path:

```rust
# use gd::{ColumnSpec, DataType, Schema, Table, Value};
# let schema = Schema::new([ColumnSpec::new("arg", DataType::U32), ColumnSpec::new("result", DataType::U32)]).unwrap();
# let mut table = Table::new(schema);
# table.push_row([Value::U32(3), Value::U32(0)]).unwrap();
let (args, results) = table.column_pair_mut(0, 1).unwrap();
let args = args.as_slice::<u32>()?;
let results = results.as_mut_slice::<u32>()?;

for (&arg, result) in args.iter().zip(results) {
    *result = arg.saturating_mul(arg);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`column_pair_mut` validates the two positions and applies `split_at_mut` directly;
`columns_io` pays its small generalized setup cost only when a kernel needs more
columns. Since both return ordinary `&[T]` and `&mut [T]`, applications may use Rayon
for parallel transforms. The optional `rayon` feature additionally provides a safely
partitioned row-wise adapter for heterogeneous transforms.

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

## Parallel processing with Rayon

Rayon operates on the same borrowing interfaces as sequential code. It does not need
access to `Table` internals and does not make a table globally mutable from several
threads.

### Column-wise transform

For homogeneous work, borrow all participating required columns once and pass their
ordinary slices to Rayon's tuple `MultiZip` implementation:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value};
use rayon::prelude::*;

let schema = Schema::new([
    ColumnSpec::new("arg", DataType::U32),
    ColumnSpec::new("scale", DataType::U32),
    ColumnSpec::new("bias", DataType::U32),
    ColumnSpec::new("result", DataType::U32),
    ColumnSpec::new("even", DataType::Bool),
])
.unwrap();
let mut table = Table::with_capacity(schema, 10_000);
for arg in 0_u32..10_000 {
    table
        .push_row([
            Value::U32(arg),
            Value::U32(3),
            Value::U32(1),
            Value::U32(0),
            Value::Bool(false),
        ])
        .unwrap();
}

let ([args, scales, biases], [results, even]) =
    table.columns_io([0, 1, 2], [3, 4]).unwrap();
let args = args.as_slice::<u32>().unwrap();
let scales = scales.as_slice::<u32>().unwrap();
let biases = biases.as_slice::<u32>().unwrap();
let results = results.as_mut_slice::<u32>().unwrap();
let even = even.as_mut_slice::<bool>().unwrap();

(args, scales, biases, results, even)
    .into_par_iter()
    .for_each(|(&arg, &scale, &bias, result, even)| {
        *result = arg.saturating_mul(scale).saturating_add(bias);
        *even = *result % 2 == 0;
    });
```

```mermaid
flowchart TD
    Select["columns_io([arg, scale, bias], [result, even])<br/>validates bounds and aliasing"]
    Select --> Inputs["shared inputs<br/>&amp;[u32] x 3"]
    Select --> Outputs["exclusive outputs<br/>&amp;mut [u32] + &amp;mut [bool]"]
    Inputs --> MultiZip["Rayon MultiZip<br/>one row tuple per iteration"]
    Outputs --> MultiZip
    MultiZip --> W0["worker 0<br/>rows 0..mid"]
    MultiZip --> W1["worker 1<br/>rows mid..end"]
```

Rayon supports tuple zips through twelve participants. More inputs can be indexed by
row while parallel iteration is driven by the output slices, or several zips can be
nested. This is an iterator-level arity convenience, not a table restriction:
`columns_io` uses const-generic arrays and does not impose a fixed input/output count.

Rayon shares every immutable input and divides every mutable output into matching,
non-overlapping ranges, so two workers cannot receive mutable access to the same cell.
The outstanding borrows also prevent structural table operations until the parallel
transform finishes.

### Row-wise transform

Enable the crate's `rayon` feature when one operation needs heterogeneous cells from
the same row:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([
    ColumnSpec::new("arg", DataType::U32),
    ColumnSpec::new("result", DataType::U32),
])
.unwrap();
let mut table = Table::with_capacity(schema, 10_000);
for arg in 0_u32..10_000 {
    table.push_row([Value::U32(arg), Value::U32(0)]).unwrap();
}

table.par_for_each_row_mut(256, |mut row| {
    let ValueRef::U32(arg) = row.get_named("arg").unwrap() else {
        unreachable!()
    };
    row.set_named("result", arg.saturating_mul(arg)).unwrap();
});
```

```mermaid
flowchart TD
    Rows["RowsMut over the whole table"]
    Rows --> Split["split every column and extras<br/>at the same row boundary"]
    Split --> Left["RowsMut: 0..mid<br/>disjoint mutable slices"]
    Split --> Right["RowsMut: mid..end<br/>disjoint mutable slices"]
    Left --> W0["Rayon worker 0<br/>RowMut values in left half"]
    Right --> W1["Rayon worker 1<br/>RowMut values in right half"]
```

`RowsMut` applies `split_at_mut` to every column at the same logical row and splits the
optional extras sidecar there too. Each recursive half therefore owns a disjoint set
of rows across all storage allocations. The grain size (`256` above) is the splitting
threshold: larger ranges are halved until they fit, so scheduled ranges never exceed
it and can be about half as large. Row-wise access performs dynamic cell work;
typed column slices remain preferable when the operation is homogeneous.

## Views and indexes

A dynamic column scan yields `ValueRef`; a required fixed-width scan can instead walk
its typed slice directly. Row iteration assembles a borrowing view across columns.
`RowMut` similarly assembles disjoint mutable cell references for one row. `RowsMut`
can split all column slices and the open-schema sidecar at one common row boundary;
its halves can therefore be sent to scoped threads without a table lock or unsafe
aliasing. With the optional `rayon` feature, `par_for_each_row_mut` performs this
partitioning recursively using a caller-selected grain-size threshold. Typed column
slices remain the lower-overhead interface for homogeneous bulk operations.
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
| tombstone or restore one row | O(1) once the flag vector exists; O(rows) for the first deletion after it is dropped | flag vector of one byte per physical row |
| iterate live rows | O(rows) | none |
| compact tombstoned rows | O(rows × columns); O(1) without tombstones | removed-position list; column capacity retained |
| pop last row | O(columns) | none |
| column scan | O(rows) | none |
| build column index | O(rows) | O(rows) |
| indexed equality lookup | O(key length) | none per lookup |
| build stable row order | O(rows log rows) | O(rows) |
| iterate ordered rows | O(rows) | none after construction |

## Relational workflow building blocks

Predicate selection (`select_rows`, `filter_rows`), projection (`project`, `select`),
and indexed left-join row pairs (`left_join_rows`) compose into materialized data
processing pipelines. Filtering skips deleted rows; explicit position-based
selection and projection preserve them. Combined row/column selection gathers only
the requested columns, avoiding a full-width filtered intermediate. Joins reuse a
borrowing `ColumnIndex` and expose duplicate and missing matches explicitly.

The [order-workflow benchmark](order-workflow.md) exercises database import,
validation, two equality joins across three tables, clean/audit materialization,
and independent parameterized outputs, including concurrent variant generation.

## Combining and viewing tables

Mapped append stages an entire source batch before extending the destination.
`append` maps positions, `append_named` resolves destination names/aliases, and
`append_mapped` uses explicit `ColumnMapping` entries. Matching storage layouts are
cloned as typed vectors; conversion/nullability changes use per-cell preparation.
Extras are validated before commit, then prepared columns and sidecars are moved
into destination vectors. Existing destination cells are not cloned. The guarantee
is atomicity for returned validation errors, with converter side effects and
allocation failures outside that guarantee. Physical rows and tombstones are
preserved; destination properties are not merged with source properties.

`CompositeIndex<N>` extends equality indexing to ordered tuples. It uses borrowed
`IndexKeyRef` arrays in a hash map and separate duplicate-position buckets. Looking
up a temporary query key returns a bucket borrowed from the index, independent of
the query key's lifetime. Any null component excludes a live row from the key map;
deleted rows are tracked separately. Component type and null semantics remain
explicit rather than using string conversion to define equality.

`TableSelection` owns metadata and borrows cell storage. Selection predicates and
projections compose without copying cells; JSON/CSV serializers read the view
straight from the source. An owned table is created only by `materialize`. Used
views prevent source mutation at compile time, so their positions cannot become
stale through compaction. `SelectedRow` exposes projected cells with source row
positions and access to row-local extras.

Single-column and composite join iterators borrow both source tables and the index,
yielding borrowed row pairs lazily. They avoid allocating an entire expanded join
result and support ordinary iterator limits. The older position-vector joins remain
available for application code that deliberately wants an owned snapshot.

These APIs add no unsafe Rust and do not introduce concurrent mutation of `Table`.
Existing table-copy and workflow performance figures remain measurements of their
original benchmark paths; no performance improvement is claimed for the new APIs
without a corresponding measurement.

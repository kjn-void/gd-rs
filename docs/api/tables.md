# Tables and schemas

The C++ documentation describes member tables, DTO tables, and tables with per-row
argument sidecars. `gd-rs` consolidates the fixed-schema behavior into one `Table`:

- an immutable, shareable `Schema` describes names, aliases, types, and nullability;
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

### Sharing one schema between tables

`Table::new` and `Table::with_capacity` accept either an owned `Schema` or an
`Arc<Schema>`. Use an `Arc` when many independent tables have the same layout; each
table then stores one cloned handle instead of copying the column metadata and name
map:

```rust
use std::sync::Arc;

use gd::{ColumnSpec, DataType, Schema, Table};

let schema = Arc::new(
    Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("enabled", DataType::Bool),
    ])
    .unwrap(),
);

let first = Table::new(Arc::clone(&schema));
let second = Table::with_capacity(Arc::clone(&schema), 100);

assert!(std::ptr::eq(first.schema(), second.schema()));
assert!(Arc::ptr_eq(&schema, &first.schema_arc()));
```

The schema remains immutable. Row and column storage is still owned independently by
each table.

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

`Table::columns_io(inputs, outputs)` supports direct bulk transforms without exposing
the storage enum. Its const-generic position arrays can select any number of immutable
`Column` inputs and mutable `ColumnMut` outputs. Input positions may repeat; outputs
must be unique and cannot overlap an input. Invalid selections return a descriptive
`ColumnSelectionError`.

For the common one-input, one-output case, `column_pair_mut` is a specialized
zero-allocation path. It validates the two positions and uses `split_at_mut` directly,
without constructing the generalized selection request:

```rust
use rayon::prelude::*;

let (args, results) = table.column_pair_mut(0, 1).unwrap();
let args = args.as_slice::<u32>().unwrap();
let results = results.as_mut_slice::<u32>().unwrap();

args.par_iter()
    .zip(results.par_iter_mut())
    .for_each(|(&arg, result)| *result = arg.saturating_mul(arg));
```

Use `column_pair_mut` for unary transforms and `columns_io` once a kernel has multiple
inputs or outputs.

`ColumnMut::as_mut_slice::<T>` applies the same fixed-width type and nullability checks
as `Column::as_slice::<T>`, then returns `&mut [T]`. The disjoint slices can be zipped
by ordinary sequential iterators or a parallel slice library.

For example, an application can add `rayon = "1.12"` to its dependencies and run a
parallel source-to-target transform directly over the borrowed columns:

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

let mut table = Table::with_capacity(schema, 1_000_000);
for arg in 0_u32..1_000_000 {
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

(args, scales, biases, &mut *results, &mut *even)
    .into_par_iter()
    .for_each(|(&arg, &scale, &bias, result, even)| {
        *result = arg.saturating_mul(scale).saturating_add(bias);
        *even = *result % 2 == 0;
    });

assert_eq!(results[12], 37);
assert!(!even[12]);
```

Rayon's `MultiZip` implementation handles tuple zips through twelve participants.
Larger kernels can drive parallel iteration from their outputs and index additional
immutable inputs by row, or nest zips. `columns_io` itself has no fixed arity. Rayon
partitions the ordinary slices so workers receive non-overlapping mutable elements;
`gd-rs` provides the checked column borrows and leaves scheduling to Rayon.

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

`row_mut` provides the same checked mutation through one borrowing row view. This is
useful when one operation reads or changes several differently typed fields:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, UnknownFields, Value, ValueRef};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String),
])
.unwrap()
.with_unknown_fields(UnknownFields::Store);

let mut table = Table::new(schema);
table
    .push_row([Value::U64(7), Value::from("Ada")])
    .unwrap();

let mut row = table.row_mut(0).unwrap();
assert_eq!(row.get_named("name"), Some(ValueRef::String("Ada")));
row.set_named("name", "Grace").unwrap();
row.set_named("language", "COBOL").unwrap();
assert_eq!(row.get_named("language"), Some(ValueRef::String("COBOL")));
```

The mutable view cannot add or remove fixed columns or rows. Declared fields retain
the schema's exact type/null checks; an unknown name is accepted only by an open
schema and remains local to that row.

### Parallel row mutation

Enable the optional `rayon` feature to apply heterogeneous row logic in parallel:

```toml
[dependencies]
gd-rs = { version = "0.1", features = ["rayon"] }
```

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

assert_eq!(table.cell_named(12, "result"), Ok(ValueRef::U32(144)));
```

The grain size (`256` here) is the smallest independently scheduled range. Internally,
`rows_mut` divides every typed column and the optional extras sidecar at identical row
boundaries. The resulting `RowsMut` halves own disjoint mutable slices, so Rayon needs
neither a table lock nor unsafe aliasing. Callers that manage their own scoped threads
can use `table.rows_mut().split_at(mid)` directly.

Each `RowMut` assembles dynamic cell references for one row (inline for schemas of up
to eight columns), so it is intended for genuinely row-oriented, heterogeneous work.
For a uniform transform, `columns_io` and typed slices avoid that per-row dynamic
dispatch and remain the preferred bulk-performance API.

The current API does not insert or remove columns after construction. Build a new
schema and table when the data model changes.

## Debug printing

`table_debug` provides the same four diagnostic views as GD's C++ table debug
helpers. Rust has no function overloading, so the C++ `print(table, count)` overload
is named `print_rows`:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, table_debug};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String).with_alias("display_name"),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::U64(7), Value::from("Ada")]).unwrap();
table.push_row([Value::U64(8), Value::from("Grace")]).unwrap();

assert_eq!(table_debug::print(&table), "7, Ada\n8, Grace\n");
assert_eq!(table_debug::print_rows(&table, 1), "7, Ada\n");
assert_eq!(table_debug::print_row(&table, 1), "8, Grace\n");
assert_eq!(
    table_debug::print_column(&table),
    "[(0) id,u64,8] [(1) name (display_name),string,0]"
);
```

Rows are rendered in schema order with `", "` separators, `null` text, and a final
newline. A requested row count is clamped to the available rows; an invalid row uses
GD's `Max row is:N` diagnostic. Column output contains position, primary name,
optional alias, Rust logical type, and fixed payload width (`0` for variable-width
types).

These functions intentionally use the public `Table`, `Row`, `Schema`, and `ValueRef`
views. They do not expose column storage internals, and they omit open-schema extras
because those values are row-local fields rather than fixed columns. The ordinary
`Debug` implementation remains a structural developer view and is not a substitute
for this stable, selected output.

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

Cloning a `Table` shares its immutable schema and clones its column data. Constructing
several empty tables from clones of the same `Arc<Schema>` shares metadata while
leaving every table's rows independent. `schema_arc` obtains another shared handle
from an existing table. This uses standard atomic `Arc` ownership rather than the C++
manual schema reference count; it does not make mutable table contents shared.

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

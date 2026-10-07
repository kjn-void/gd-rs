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

### Fixed-capacity string buffers

Ordinary `DataType::String` columns retain `CompactString` storage: short UTF-8
values fit inside their descriptors, and longer values own separate allocations.
Use `ColumnSpec::fixed_string(name, capacity)` to select fixed-capacity storage.
The logical type remains `DataType::String`. Each string column owns one byte
buffer and an offset/length descriptor per physical row; every row reserves
`capacity` bytes, including null and empty cells. Capacity is measured in UTF-8
bytes. Zero capacity or a capacity above `isize::MAX` panics at schema construction.

```rust
use gd::{ColumnSpec, DataType, FixedStringError, Schema, Table, Value};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::fixed_string("name", 32).nullable(true),
])?;
let mut table = Table::new(schema);
table.push_row([Value::U64(1), Value::from("Åsa")])?;
table.push_row([Value::U64(2), Value::Null])?;

let names = table.column(1).unwrap().fixed_strings().unwrap();
assert_eq!(names.capacity(), 32);
assert_eq!(names.get(0), Some(Some("Åsa")));
assert_eq!(names.get(1), Some(None));
assert_eq!(names.get(2), None);

let (_, output) = table.column_pair_mut(0, 1).unwrap();
let mut names = output.fixed_strings_mut().unwrap();
names.set(1, Some(""))?; // Empty and null are distinct.
assert_eq!(names.get(1), Some(Some("")));
assert!(matches!(
    names.set(0, Some(&"x".repeat(33))),
    Err(FixedStringError::TooLong { capacity: 32, actual: 33 })
));
assert_eq!(names.get(0), Some(Some("Åsa"))); // Failed writes preserve the value.
# Ok::<(), Box<dyn std::error::Error>>(())
```

`ColumnSpec::fixed_string_capacity()` reports this policy. Dynamic table and row
writes reject oversized values with `TableError::StringTooLong`, including values
produced by converters. Complete-row insertion and table append remain atomic.
Appending between ordinary strings and fixed buffers, or between different slot
capacities, validates and copies into the destination layout.

`Column::fixed_strings()` and `ColumnMut::fixed_strings_mut()` return `None` for
other storage layouts. `FixedStrings::iter()` borrows strings without allocating.
`FixedStringsMut::set()` returns `FixedStringError` for bounds, size, or nullability
errors before changing a cell. It bypasses input conversion because its inputs
already have the exact logical string type. `cell_mut()` and `iter_mut()` expose
`FixedStringCellMut`, whose `set()` replaces a value and `as_str_mut()` supports
safe operations that preserve byte length, such as ASCII case conversion.

`FixedStringsMut::split_at(mid)` partitions both index descriptors and byte slots
into disjoint ranges that can be sent to scoped workers. Positions in either view
are relative to that view; splitting outside its length panics. The existing
`RowsMut` splitting and Rayon row mutation also support these buffers. Views
include tombstoned physical rows. Copies own independent buffers; compaction and
gather rebuild offsets, while replacing a string preserves its slot offset.

Ordinary string columns additionally support
`as_slice::<compact_str::CompactString>()` and
`as_nullable_slice::<compact_str::CompactString>()`, plus the corresponding mutable
slice methods. These expose the existing descriptors without changing the storage
representation. Requesting these slices from fixed-buffer strings returns
`ColumnSliceError::FixedStringBuffer`; use the fixed-string views instead.

### Explicit column conversion

Table writes remain strict unless a column declares a named `ColumnConverter`. The
converter runs only when a non-null input has a different logical type. Exact-type
values keep the ordinary fast path, and nullability is never bypassed.

`ColumnConverter::new` takes a stable policy name and a `Send + Sync` closure. The
closure receives a borrowed `ValueRef` and either returns an owned value or a
`ColumnConversionError`. Attach the converter with `ColumnSpec::with_converter`:

```rust
use gd::{
    ColumnConversionError, ColumnConverter, ColumnSpec, DataType, Schema, Table, TableError,
    Value, ValueRef,
};

let parse_u32 = ColumnConverter::new("decimal-u32", |value| match value {
    ValueRef::String(text) => text
        .parse::<u32>()
        .map(Value::U32)
        .map_err(|error| ColumnConversionError::new(error.to_string())),
    _ => Err(ColumnConversionError::new("expected decimal text")),
});
let schema = Schema::new([
    ColumnSpec::new("id", DataType::U32).with_converter(parse_u32.clone()),
    ColumnSpec::new("attempts", DataType::U32).with_converter(parse_u32),
    ColumnSpec::new("name", DataType::String),
])
.unwrap();
let mut table = Table::new(schema);

table
    .push_row([
        Value::from("42"),
        Value::from("3"),
        Value::from("Ada"),
    ])
    .unwrap();
assert_eq!(table.cell(0, 0), Ok(ValueRef::U32(42)));
assert_eq!(table.cell(0, 1), Ok(ValueRef::U32(3)));

// Exact U32 inputs bypass the converter.
table
    .push_row([Value::U32(7), Value::U32(1), Value::from("Grace")])
    .unwrap();

// A failed conversion rejects the complete row.
let error = table
    .push_row([
        Value::from("not a number"),
        Value::from("4"),
        Value::from("Linus"),
    ])
    .unwrap_err();
assert!(matches!(
    error,
    TableError::ConversionFailed { column: 0, .. }
));
assert_eq!(table.row_count(), 2);
```

Converter output is checked against the column's type and nullability before any
column changes. A converter error becomes `TableError::ConversionFailed`; returning a
value of the wrong type remains a `TypeMismatch`, and returning null for a required
column remains `NullNotAllowed`.

Conversion is part of every dynamic fixed-cell write path: `push_row`,
`push_row_vec`, `push_row_with_extras`, `set_cell`, `set_named`, mutable `RowMut`
writes, and `ConcurrentTableBuilder` insertion. Complete-row writes remain atomic: a
later conversion failure does not append values already converted earlier in that
row.

Their names are semantic identities for structural schema equality. Independently
constructed converters with the same name must therefore implement the same contract.
Use different names when two converters intentionally apply different policies, even
if they produce the same destination type.

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
nullability errors therefore leave the table unchanged. Columns without converters
do not implicitly widen or parse values during insertion.

### Table properties

Properties are insertion-ordered, uniquely named dynamic values describing the table
as a whole. They are independent of row-local extras and are preserved when a table is
cloned or copied with `copy_range`, `copy_rows`, or their parallel variants:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([ColumnSpec::new("id", DataType::U64)]).unwrap();
let mut table = Table::new(schema);
table.push_row([Value::U64(1)]).unwrap();

assert_eq!(table.set_property("source", "sqlite"), None);
assert_eq!(table.set_property("version", 1_u32), None);
assert_eq!(table.property("source"), Some(ValueRef::String("sqlite")));

let copied = table.copy_range(0..1).unwrap();
assert_eq!(copied.property("version"), Some(ValueRef::U32(1)));
```

`set_property` replaces an existing value and returns it. `contains_property` checks
for a name; `remove_property` and `clear_properties` remove values, while `properties`
exposes an immutable `Arguments` view in insertion order. Row JSON and CSV formatters
serialize rows only; they do not include table properties.

### Constructing rows concurrently

`ConcurrentTableBuilder` lets multiple threads validate and publish complete rows
without putting one application mutex around a `Table`. Each successful call returns
the row position assigned by the concurrent collector. Scheduling determines that
order, so retain the returned position when insertion order matters.

```rust
use std::thread;

use gd::{ColumnSpec, ConcurrentTableBuilder, DataType, Schema, Value};

let schema = Schema::new([
    ColumnSpec::new("arg", DataType::U32),
    ColumnSpec::new("result", DataType::U64),
])
.unwrap();
let builder = ConcurrentTableBuilder::new(schema);

thread::scope(|scope| {
    for shard in 0_u32..4 {
        let builder = &builder;
        scope.spawn(move || {
            for offset in 0_u32..250 {
                let arg = shard * 250 + offset;
                builder
                    .push_row([Value::U32(arg), Value::U64(u64::from(arg) * 3)])
                    .unwrap();
            }
        });
    }
});

assert_eq!(builder.row_count(), 1_000);

// Freezing consumes the collector and transposes its temporary rows into the
// same dense typed columns used by an ordinarily constructed Table.
let table = builder.into_table();
let args = table.column(0).unwrap().as_slice::<u32>().unwrap();
let results = table.column(1).unwrap().as_slice::<u64>().unwrap();
assert!(args
    .iter()
    .zip(results)
    .all(|(&arg, &result)| result == u64::from(arg) * 3));
```

To add concurrently produced rows after rows already stored in a table, construct the
builder from the table's shared schema and perform one exclusive final merge:

```rust
use gd::{ColumnSpec, ConcurrentTableBuilder, DataType, Schema, Table, Value};

let schema = Schema::new([
    ColumnSpec::new("arg", DataType::U32),
    ColumnSpec::new("result", DataType::U64),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::U32(1), Value::U64(1)]).unwrap();

let builder = ConcurrentTableBuilder::new(table.schema_arc());
builder
    .extend_rows([
        [Value::U32(2), Value::U64(4)],
        [Value::U32(3), Value::U64(9)],
    ])
    .unwrap();

let appended = builder.append_to(&mut table).unwrap();
assert_eq!(appended, 1..3);
assert_eq!(table.row_count(), 3);
```

`append_to` accepts structurally equal schemas, even when they are held by different
`Arc`s. It returns `TableError::SchemaMismatch` without changing the destination when
column definitions or the unknown-field policy differ. Existing rows keep their
positions, and the returned range identifies the newly appended rows.

`push_row_vec` accepts runtime-width rows, `push_row_with_extras` supports open
schemas, and `extend_rows` validates an entire batch before publishing it as one
consecutive range. A failed row or batch changes nothing.

The builder intentionally does not expose a live `Table` or concurrent cell updates.
Its temporary representation is row-oriented so all values and extras for one row
become visible together. `into_table` requires ownership of the builder, while
`append_to` additionally borrows the destination table exclusively for the transpose.
After either operation, use the ordinary mutable table API or partition typed columns
and rows with Rayon.

### Copying row selections

`copy_range` creates a new table from one contiguous source range. It shares the
immutable schema and copies each typed column as one contiguous slice. For required
fixed-width columns, this is one allocation and bulk copy per column; values are not
converted through `Value` or a row view.

`copy_rows` accepts arbitrary source positions and gathers each column into a dense
destination column. Selection order and duplicate positions are preserved:

With the `rayon` feature, `par_copy_range` and `par_copy_rows` submit one independent
task per fixed column. The current Rayon pool controls concurrency, allowing callers
to limit workers without statically grouping uneven columns. The sequential methods
can remain faster for small selections or narrow schemas.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("score", DataType::F32),
])
.unwrap();
let mut source = Table::new(schema);
for id in 0_u64..5 {
    source
        .push_row([Value::U64(id), Value::F32(id as f32 * 0.5)])
        .unwrap();
}

let range = source.copy_range(1..4).unwrap();
assert_eq!(range.row_count(), 3);
assert_eq!(range.cell(0, 0).unwrap(), ValueRef::U64(1));

let selected = source.copy_rows(&[4, 1, 4]).unwrap();
assert_eq!(selected.row_count(), 3);
assert_eq!(selected.cell(0, 0).unwrap(), ValueRef::U64(4));
assert_eq!(selected.cell(1, 0).unwrap(), ValueRef::U64(1));
assert_eq!(selected.cell(2, 0).unwrap(), ValueRef::U64(4));
```

Both methods clone nullable values and open-schema row extras along with their fixed
columns. Invalid positions return `TableError` without constructing a partial result.

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

Fixed-width columns can be checked once and borrowed as ordinary typed slices:
required columns expose `&[T]`, and nullable columns expose `&[Option<T>]`.
The supported element types are `bool`, the fixed-width integer and
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

Use `Column::as_nullable_slice::<T>` for nullable storage, including columns whose
current values are all populated. It returns `&[Option<T>]` without allocating or
converting cells through `ValueRef`; `None` represents null. A required column
returns `ColumnSliceError::Required`, and a wrong type returns `TypeMismatch`.
These checks depend on the schema, not on the observed values.

Nullable columns support the same checked bulk mutation through
`ColumnMut::as_nullable_mut_slice::<T>`:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([
    ColumnSpec::new("amount", DataType::I64).nullable(true),
])
.unwrap();
let mut table = Table::new(schema);
table.push_row([Value::I64(25)]).unwrap();
table.push_row([Value::Null]).unwrap();
assert_eq!(
    table.column(0).unwrap().as_nullable_slice::<i64>().unwrap(),
    &[Some(25), None],
);

let (_, [amounts]) = table.columns_io([], [0]).unwrap();
for amount in amounts.as_nullable_mut_slice::<i64>().unwrap() {
    *amount = amount.map(|value| value * 2);
}
assert_eq!(table.cell(0, 0), Ok(ValueRef::I64(50)));
assert_eq!(table.cell(1, 0), Ok(ValueRef::Null));
```

Both required and nullable slices include every physical row, including tombstoned
rows. A slice mutation changes values, not deletion flags. For live-row filtering,
use `select_rows` with a predicate that indexes the typed slice by `Row::position`,
or explicitly consult tombstones when collecting positions yourself.

For required columns, the returned `&[T]` makes the hot loop monomorphic and contiguous. It
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

# use gd::{ColumnSpec, DataType, Schema, Table, Value};
# let schema = Schema::new([ColumnSpec::new("arg", DataType::U32), ColumnSpec::new("result", DataType::U32)]).unwrap();
# let mut table = Table::new(schema);
# table.push_row([Value::U32(3), Value::U32(0)]).unwrap();
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
# use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};
# let schema = Schema::new([ColumnSpec::new("requests", DataType::U64)]).unwrap();
# let mut table = Table::new(schema);
# table.push_row([Value::U64(7)]).unwrap();
let column = table.column_named("requests").unwrap();
let mut total = 0_u64;
column.for_each_value(|value| {
    if let ValueRef::U64(value) = value {
        total = total.saturating_add(value);
    }
});
```

Use `iter` when iterator composition or early termination matters. Use
`for_each_value` for a terminal dynamic scan, and `as_slice::<T>` or
`as_nullable_slice::<T>` when the caller knows the fixed-width type.

## Mutation

`set_cell` applies the selected column's converter when needed, then validates
position, type, and nullability before changing storage.
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

### Tombstoning rows

A tombstone logically deletes a row while retaining its payload and physical position.
Tombstone metadata is allocated on the first deletion and released when no tombstoned
row remains, so tables that never delete a row pay nothing:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("name", DataType::String),
])
.unwrap();
let mut table = Table::new(schema);
for (id, name) in [(1_u64, "Ada"), (2, "Grace"), (3, "Linus")] {
    table.push_row([Value::U64(id), Value::from(name)]).unwrap();
}

assert!(table.tombstone_row(1).unwrap());
assert!(!table.tombstone_row(1).unwrap()); // already tombstoned
assert_eq!(table.row_count(), 3);
assert_eq!(table.live_row_count(), 2);
assert_eq!(table.tombstone_count(), 1);
assert_eq!(table.tombstoned_rows().collect::<Vec<_>>(), [1]);

// The retained row is still readable and writable at its physical position.
assert_eq!(table.cell_named(1, "name"), Ok(ValueRef::String("Grace")));
let live: Vec<_> = table
    .live_rows()
    .map(|row| row.position())
    .collect();
assert_eq!(live, [0, 2]);

assert!(table.restore_row(1).unwrap());
assert_eq!(table.live_row_count(), 3);
assert_eq!(table.restore_all_rows(), 0);
```

`row_count`, `is_empty`, `rows`, `cell`, `set_cell`, and `row_mut` remain physical;
`live_row_count`, `live_rows`, indexes, and JSON/CSV output use the live view.
`Row::is_tombstoned` reports the flag through a row view, and `RowMut::is_tombstoned`
does the same for mutable row processing. `is_tombstoned`, `tombstone_row`, and
`restore_row` return `TableError::RowOutOfBounds` for an invalid position.

Tombstone flags travel with copied rows. `copy_range`, `copy_rows`,
`par_copy_range`, and `par_copy_rows` preserve the logical state of every copied
position, including selection order and duplicates. `pop_row` removes the last
physical row whether or not it is tombstoned. `ConcurrentTableBuilder` always
publishes live rows, and `append_to` leaves the destination's existing tombstones
untouched.

`table_to_json`, `table_to_csv`, and `row_order_to_json` omit tombstoned rows.
`table_debug::print` and related helpers remain physical so retained data stays
inspectable.

### Compacting tombstoned rows

`compact` physically removes every tombstoned row. Surviving rows keep their relative
order and move down to close the gaps, so positions after the first removed row
change. The returned `RowCompaction` translates positions recorded before the call:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

let schema = Schema::new([ColumnSpec::new("id", DataType::U64)]).unwrap();
let mut table = Table::new(schema);
for id in 0_u64..5 {
    table.push_row([Value::U64(id)]).unwrap();
}
table.tombstone_row(1).unwrap();
table.tombstone_row(3).unwrap();

let compaction = table.compact();
assert_eq!(compaction.removed_rows(), &[1, 3]);
assert_eq!(compaction.new_position(0), Some(0));
assert_eq!(compaction.new_position(1), None); // removed
assert_eq!(compaction.new_position(4), Some(2));

assert_eq!(table.row_count(), 3);
assert_eq!(table.tombstone_count(), 0);
assert_eq!(table.cell(2, 0), Ok(ValueRef::U64(4)));
```

Compaction is final: removed rows cannot be restored. Row-local extras move with their
rows, table properties are unchanged, and column capacity is retained. Without
tombstones, `compact` returns an empty mapping without touching column storage.
`ColumnIndex` and `RowOrder` borrow the table, so none can survive compaction; rebuild
them afterwards. `RowCompaction::new_position` is a binary search over the removed
positions, and `previous_row_count` reports the physical row count before the call.

gd-rs deliberately provides compaction instead of reusing tombstoned slots. Reusing a
slot would silently give an old position a different row and would end restorability
implicitly; compaction makes both changes explicit in one call and keeps columns dense.

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
the schema's conversion and validation policy; an unknown name is accepted only by an
open schema and remains local to that row.

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

The grain size (`256` here) is the splitting threshold: ranges larger than it are
halved until they fit, so scheduled ranges never exceed it and can be about half as
large. Internally, `rows_mut` divides every typed column and the optional extras
sidecar at identical row boundaries. The resulting `RowsMut` halves own disjoint mutable slices, so Rayon needs
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

Known names still use typed column storage and schema-directed conversion and
validation. A closed schema allocates no extras sidecar. An open schema adds a parallel nullable-pointer
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

## Filtering and projection

`select_rows(predicate)` visits live rows once, in source order, and returns their
physical positions. `filter_rows(predicate)` gathers those rows into a new table,
sharing the immutable schema and copying values, extras, and properties. Tombstoned
rows never reach either predicate.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};
let mut source = Table::new(Schema::new([
    ColumnSpec::new("id", DataType::I64),
    ColumnSpec::new("name", DataType::String),
])?);
source.push_row([Value::I64(1), Value::from("Ada")])?;
source.push_row([Value::I64(2), Value::from("Åsa")])?;
let positions = source.select_rows(|row| row.get(0) == Some(ValueRef::I64(2)));
let selected = source.select(&positions, &[1, 0])?;
assert_eq!(selected.cell(0, 0)?, ValueRef::String("Åsa"));
# Ok::<(), gd::TableError>(())
```

`project(columns)` copies all physical rows of the selected columns.
`select(rows, columns)` gathers both dimensions without a full-width intermediate.
Both preserve selection order, column specifications (including aliases and
converters), table properties, extras, and deletion flags. Duplicate row positions
are allowed; duplicate columns are rejected because schema names must be unique.
An empty projection retains its row count. An invalid position returns `TableError`
before anything is copied. Results own their cell data and may be edited independently.

Positions are snapshots, not durable row IDs: do not reuse them after row removal or
compaction. Projection preserves row-local extras even when declared columns are
omitted; it is not a data-redaction API.

## Atomic mapped append

`append(&source)` appends by position and requires equal widths.
`append_named(&source)` resolves each destination's primary name and optional alias
through source schema lookup. `append_mapped` accepts explicit
`ColumnMapping::new(source_column, destination_column)` entries:

```rust
use gd::{ColumnMapping, ColumnSpec, DataType, Schema, Table, Value};

let mut source = Table::new(Schema::new([
    ColumnSpec::new("name", DataType::String),
    ColumnSpec::new("id", DataType::I64),
])?);
source.push_row([Value::from("Ada"), Value::I64(7)])?;
let mut destination = Table::new(Schema::new([
    ColumnSpec::new("id", DataType::I64),
    ColumnSpec::new("name", DataType::String),
    ColumnSpec::new("note", DataType::String).nullable(true),
])?);
assert_eq!(destination.append_mapped(&source, &[
    ColumnMapping::new(1, 0),
    ColumnMapping::new(0, 1),
])?, 0..1);
assert_eq!(destination.cell(0, 2)?.to_owned(), Value::Null);
# Ok::<(), gd::TableError>(())
```

Policies are explicit:

- Source rows include tombstones; payloads, deletion flags and row-local extras are
  copied. Existing destination row positions remain unchanged. The returned range
  identifies every appended physical row.
- Destination properties remain unchanged; source properties are not merged.
- A source column may feed multiple destinations. Each destination may be mapped
  only once. Unmapped destinations must be nullable, including for an empty source,
  and receive nulls. Unmapped fixed source columns are omitted.
- Named mapping accepts source aliases and destination aliases. If a destination's
  primary name and alias resolve to different source columns, the batch fails with
  `AmbiguousColumnMapping` instead of choosing one silently.
- Exact-type values bypass converters. Other non-null inputs require the existing
  named destination converter. Converted output and nullability are checked.
  Mismatched declared types without a converter fail even for an empty source,
  except that a `Null`-typed source supplies nulls. Nullable-to-required mappings
  check actual values, so an empty source has no nulls to reject.
- Extras are preserved, not promoted into fixed columns. Supplying nonempty extras
  to a closed destination schema, or an extra name colliding with a destination
  name/alias, rejects the batch.

Compatible columns are cloned directly as typed vectors. Columns requiring conversion
or a nullability change are staged cell by cell. Extras are prepared and checked
before typed vectors and sidecars are extended by moves. A returned error leaves the
destination unchanged, even if a late source value fails conversion. Converter
side effects cannot be rolled back, and allocation failure has normal Rust behavior.
Peak staging space is proportional to the appended data, not existing destination
size; append does not clone the destination.

To append live rows only, use a live selection and materialize it before appending:
`destination.append(&source.filter_view(|_| true).materialize()?)?`.

## Borrowed selections

`select_view(rows, columns)` borrows selected physical rows and columns.
`project_view(columns)` borrows all physical rows with a projection, while
`filter_view(predicate)` selects matching live rows and retains all fixed columns.
These return `TableSelection<'a>`: it owns row/column positions and projected schema
metadata but borrows the source cells, extras and properties.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef, selection_to_json};

let mut table = Table::new(Schema::new([
    ColumnSpec::new("id", DataType::I64),
    ColumnSpec::new("name", DataType::String),
])?);
table.push_row([Value::I64(7), Value::from("Ada")])?;
let filtered = table.filter_view(|row| row.get(0) == Some(ValueRef::I64(7)));
let names = filtered.project(&[1])?;
assert_eq!(selection_to_json(&names)?, "[{\"name\":\"Ada\"}]");
let independent = names.materialize()?;
assert_eq!(independent.column_count(), 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`TableSelection::rows` includes selected tombstones and repeated positions;
`live_rows` excludes tombstones. `filter_rows` composes a live-row predicate over
projected `SelectedRow` views. `project` uses positions relative to the current
projection. Columns cannot repeat because the resulting schema would have duplicate
names. An empty projection retains row count and row-local extras.

`SelectedRow::get` uses projected column positions; `position` returns the original
physical row position. Named lookup recognizes projected names/aliases and extras,
but hides omitted fixed columns. `table`, `positions`, `columns` and `schema` expose
the source and selection metadata for explicit application integration.

The view's source borrow prevents mutation or compaction while the view is used.
This is stronger than the snapshot positions returned by `select_rows`. Selection
construction allocates O(selected rows + selected columns) metadata and copies no
cell payloads. `materialize` performs the existing column-wise gather and copies
properties, extras and deletion flags without rerunning converters.

`selection_to_json` and `selection_to_csv` export the projection directly, skip
tombstones, and preserve row order/duplicates. They use the ordinary table formatting
rules and omit extras/properties. Empty projections produce JSON objects with no
fields; CSV rejects them with `ZeroColumnTable`.

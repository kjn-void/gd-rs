# Table API safety comparison: `gd-rs` and `gd`

This audit compares the Rust table API in this repository with the C++ table
family in `../gd`, with emphasis on mistakes that can cause undefined behavior
(UB), memory corruption, or a process crash. It is a source audit of
`gd-rs` commit `cc0ed9ea5b4c3849fa8709186399abed1e9fbfcc` and `gd` commit
`cb11cff90d05260a88d59a30c9da421cd4e19c34`.

The APIs are not method-for-method equivalents. Rust has one fixed-schema
[`Table`](../../src/table.rs#L142), an immutable [`Schema`](../../src/table/schema.rs#L207),
and borrowing row, column, index, and ordering views. C++ has several related
types: `table_column_buffer` (also exposed as `gd::table::dto::table`),
`gd::table::table`, `gd::table::arguments::table`, and separate index helpers.
The C++ API is broader and exposes substantially more low-level state.

## Conclusion

The Rust API is materially safer and substantially harder to misuse.
Ordinary mistakes such as a bad row width, wrong value type, null in a required
column, missing name, out-of-range position, overlapping mutable selection, or
stale view are rejected by `Result`/`Option`, or by the borrow checker. No route
to UB was found through the safe Rust table API, and the table implementation
contains no `unsafe` block.

The C++ API has a debug/release safety cliff. Many preconditions are `assert`s,
so a debug build aborts but a release build continues into pointer arithmetic,
unchecked indexing, shifts, or typed pointer dereferences. Several hazards do
not require exotic use: selecting a missing column by name, requesting the
wrong template type, using a 64-bit value at a four-byte-aligned row offset,
using more columns than the selected null bitmap supports, keeping a borrowed
view across growth, copying `gd::table::table`, or copying an
`arguments::table` that holds row-local extras can result in UB.

| Area | Rust `Table` | C++ table family |
| --- | --- | --- |
| Invalid row, column, or name | `Result`/`Option`; table remains valid | Commonly `assert`; abort in debug, possible UB in release |
| Type mismatch | Runtime `TableError` or `ColumnSliceError` | Template/raw access trusts the caller; wrong type can be UB |
| Row initialization | Exact width and all cells validated before insertion | Some overloads accept partial rows and leave omitted payload uninitialized |
| Alignment | Per-type `Vec<T>` storage supplies `T` alignment | Packed rows align fields to four bytes, including eight-byte values |
| Null state | `Option<T>` per nullable column | Fixed 32/64-bit masks; unchecked shift count |
| View lifetime | Encoded in Rust lifetimes | Raw pointers and non-owning views can outlive or be invalidated by storage |
| Schema ownership | Private, immutable, shared by `Arc` | Public/mutable metadata and manual, non-atomic reference counting |
| Copying | Derived deep value copy plus safe `Arc` clone | `gd::table::table` omits a reference-count increment in its normal copy path; `arguments::table` also `memcpy`s row-extras handles without adding references |
| Concurrent access | Rust `Send`/`Sync` and borrowing rules gate access | Public mutable state and non-atomic reference count require caller discipline |
| Remaining crash surface | Documented panics, allocation failure, or user callback panic | Assertions, allocation failure, plus numerous UB paths |

## Concrete mistakes Rust catches but C++ does not

The C++ snippets below are minimal illustrations of the reviewed public
operations. Exact construction syntax can vary among the C++ table variants;
the dangerous operation and its implementation are linked in each case.

### 1. A missing column name

Rust reports the error without touching memory:

```rust
let error = table.cell_named(0, "naem").unwrap_err();
assert!(matches!(error, TableError::ColumnNotFound(_)));
```

[`Table::cell_named`](../../src/table.rs#L520) resolves the name through the
validated schema and returns `ColumnNotFound` when it is absent.

C++ converts the failed lookup into an unsigned index:

```cpp
// "naem" is a typo and is not in the schema.
auto value = table.cell_get_variant_view(0, "naem");
```

[`column_get_index`](../../../gd/source/gd_table_column-buffer.cpp#L1002) asserts
that the signed result is not `-1`, then casts it to `unsigned`. In a debug build
this normally aborts. With assertions disabled, the index becomes `UINT_MAX`
and is subsequently used to access column metadata and row storage. That is an
out-of-bounds access and therefore UB; a crash is only one possible outcome.

### 2. A wrong typed read

Rust checks the requested slice type once:

```rust
let column = table.column(0).unwrap(); // schema type is U32
let error = column.as_slice::<u64>().unwrap_err();
assert!(matches!(error, ColumnSliceError::TypeMismatch { .. }));
```

[`Column::as_slice`](../../src/table/views.rs#L175) returns a typed error when
`T` does not match the schema and returns only correctly aligned storage when it
does match.

C++ makes the requested C++ type an unchecked promise by the caller:

```cpp
// Column 0 was declared as uint32.
uint64_t value = table.cell_get<uint64_t>(0, 0);
```

The template implementation is a direct
[`*(TYPE*)cell_get(...)`](../../../gd/source/gd_table_column-buffer.h#L1632).
There is no check that `TYPE` matches the column. In this example it reads an
eight-byte `uint64_t` object from four-byte storage. The read can extend outside
the cell or allocation and does not point at a live `uint64_t` object: UB.

### 3. A correctly typed but misaligned 64-bit read

Even a matching C++ type can be unsafe. Consider a packed row with `uint32`
followed by `uint64`:

```cpp
gd::table::dto::table table;
table.column_add("uint32", "tag");
table.column_add("uint64", "value");
table.prepare();
table.row_add({uint32_t{1}, uint64_t{2}});

uint64_t value = table.cell_get<uint64_t>(0, 1);
```

The C++ layout rounds each field only to `sizeof(uint32_t)`, so `value` may be
at byte offset 4. The typed getter then dereferences a `uint64_t*` at that
address. A pointer not aligned for `uint64_t` makes the dereference UB even on a
processor that happens to tolerate unaligned loads. The ordinary variant-view
read has the same issue when it dereferences
[`uint64_t*`](../../../gd/source/gd_table_table.cpp#L1766).

Rust uses a separate typed allocation for each column. The equivalent read is
aligned by construction:

```rust
let values: &[u64] = table.column(1).unwrap().as_slice::<u64>()?;
let value = values[0];
```

The only possible failure at the API boundary is the checked type/nullability
error. Bounds checking on `values[0]` can panic, but it cannot become UB in safe
Rust.

### 4. Too many columns for the null bitmap

The C++ flags explicitly describe null masks for at most 32 or 64 columns, but
the cell operations do not enforce the limit:

```cpp
gd::table::dto::table table(
    1, gd::table::table_column_buffer::eTableFlagNull32);
for (unsigned i = 0; i < 33; ++i) {
    table.column_add("uint32", std::to_string(i));
}
table.prepare();
table.row_add(gd::table::tag_null{});
table.cell_set_null(0, 32);
```

[`cell_set_null`](../../../gd/source/gd_table_column-buffer.h#L1663) evaluates
`(uint32_t{1} << uColumn)`. Shifting a 32-bit value by 32 is UB. The 64-bit mode
has the analogous failure at column 64. Debug assertions check the row and the
presence of *a* null flag, but not the column against the mask width.

Rust has no global bitmap-width contract. Nullability belongs to each schema
column and nullable storage uses `Option<T>`:

```rust
let result = table.set_cell(0, 32, Value::Null);
// Ok for a nullable column, NullNotAllowed for a required one, or
// ColumnOutOfBounds when column 32 does not exist. No unchecked shift occurs.
```

### 5. A partial row that leaves an omitted value uninitialized

Rust requires exactly one value per schema column and validates the entire row
before changing any column:

```rust
let mut table = Table::new(Schema::new([
    ColumnSpec::new("id", DataType::U64),
    ColumnSpec::new("score", DataType::U64).nullable(true),
])?);

let error = table.push_row([Value::U64(1)]).unwrap_err();
assert!(matches!(error, TableError::RowWidth { expected: 2, actual: 1 }));
assert_eq!(table.row_count(), 0);
```

[`push_row`](../../src/table.rs#L416) stages conversion and validation before
publishing the row.

The corresponding C++ overload explicitly accepts fewer values than columns:

```cpp
gd::table::dto::table table(1, gd::table::tag_null{});
table.column_add("uint64", "id");
table.column_add("uint64", "score");
table.prepare();

table.row_add({uint64_t{1}}); // "score" is omitted
auto score = table.cell_get<uint64_t>(0, 1);
```

The overload checks only `values.size() <= column_count`, calls plain
[`row_add()`](../../../gd/source/gd_table_column-buffer.cpp#L1333), and writes
the supplied prefix. Plain row addition is documented not to mark new cells
null ([header](../../../gd/source/gd_table_column-buffer.h#L1377)). In release
builds `prepare()` does not initialize the payload bytes; only debug builds
`memset` them ([allocation](../../../gd/source/gd_table_column-buffer.cpp#L1305)).
The omitted cell is therefore marked non-null but has an indeterminate payload.
Reading it as `uint64_t` is UB under the language rules applicable to the
project, in addition to producing unpredictable data on implementations where
the read appears to work.

The similar-looking `row_add(tag_null)` and selected-column overloads do
initialize null state. Safety therefore depends on choosing the correct member
from a large overload set.

### 6. A view kept across mutation

Rust attaches the table borrow to every `ValueRef`, row, column, index, and
ordering view:

```rust,compile_fail
let name = table.cell_named(0, "name")?; // borrows table
table.push_row(new_row)?;                // cannot mutably borrow while `name` lives
println!("{name:?}");
```

The compiler rejects this program. The user can end the borrow before mutation,
copy the value, or rebuild the view afterward.

C++ returns non-owning views and raw pointers with no lifetime relationship:

```cpp
gd::variant_view name = table.cell_get_variant_view(0, "name");
table.row_add(many_rows); // growth can replace the packed allocation/reference store
use(name);                // may now read dangling storage
```

Growth or destruction can invalidate the view, and subsequent use is
use-after-free UB. The same problem applies to string indexes because
[`index_string`](../../../gd/source/gd_table_index.h#L153) stores
`std::string_view` keys borrowed from table storage. Rust's
[`ColumnIndex`](../../src/table/index.rs#L98) borrows the table, so mutation and
destruction are statically excluded while the index is usable.

### 7. Copying the internal C++ `table`

Rust's `Table` derives `Clone`; columns and row values are copied while the
immutable schema is shared by `Arc`:

```rust
let second = first.clone();
drop(first);
assert_eq!(second.row_count(), 1); // remains valid
```

The ordinary copy path for `gd::table::table` copies the schema pointer but does
not increment its manual reference count:

```cpp
gd::table::table second;
{
    gd::table::table first = make_table();
    second = first;
} // first releases the shared columns object

auto count = second.get_column_count(); // dangling m_pcolumns: use-after-free UB
```

[`common_construct(const table&)`](../../../gd/source/gd_table_table.cpp#L270)
assigns `m_pcolumns` without calling `add_reference`. The column-only copy path
immediately below it *does* call `add_reference`, making the omission in the
normal path clear. The destructor releases the pointer
([header](../../../gd/source/gd_table_table.h#L1364)). Depending on destruction
order, this can also become a double release. The same omission exists in
`gd::table::arguments::table`.

This issue does not apply to `table_column_buffer`, which owns its column vector
directly; it is specific to the related internal table types and is one reason
the C++ type family must not be treated as interchangeable.

### 8. Copying row-local extras in `gd::table::arguments::table`

Rust row-local extras are owned per row
([`ExtrasStorage::Enabled`](../../src/table/storage.rs#L81) holds
`Option<Box<RowExtras>>`), so a cloned or row-copied table receives an
independent deep copy:

```rust
let mut second = first.clone();
second.set_named(0, "note", "changed")?;
drop(first);
assert_eq!(second.cell_named(0, "note")?, ValueRef::String("changed"));
```

When `eTableFlagArguments` is set, the C++ arguments table stores a
`gd::argument::shared::arguments` handle *in place* inside each row's metadata
block. That handle is a single pointer to a manually reference-counted buffer.
The full copy path duplicates the whole data-plus-metadata allocation with
`memcpy` and never adds a reference for the copied handles:

```cpp
gd::table::arguments::table second;
{
    gd::table::arguments::table first( 10, gd::table::arguments::table::eTableFlagAll );
    // ... add columns, prepare, add a row ...
    first.cell_set_argument( 0, "note", "value" ); // row 0 gets a shared buffer
    second = first;                                 // handle bytes copied, count unchanged
} // first's destructor releases row 0's buffer

auto args = second.row_get_arguments( 0 ); // dangling buffer: use-after-free UB
```

[`common_construct(const table&)`](../../../gd/source/gd_table_arguments.cpp#L276)
and the `tag_body` copy
([line 332](../../../gd/source/gd_table_arguments.cpp#L332)) `memcpy` the
buffer returned by `size_reserved_total()`, which includes the per-row metadata
where [`row_get_arguments_meta`](../../../gd/source/gd_table_arguments.h#L1310)
places the handle. The copy constructor and copy assignment both reach this path
([header](../../../gd/source/gd_table_arguments.h#L342)). Each table's
[destructor](../../../gd/source/gd_table_arguments.cpp#L226) then calls
[`erase_arguments_s`](../../../gd/source/gd_table_arguments.cpp#L4502), which
runs [`buffer_delete`](../../../gd/source/gd_arguments_shared.h#L1693) and
releases every row buffer. Every buffer is released once per table, even though
it was referenced only once, so destroying the second table is a double
release. This is separate from the missing `m_pcolumns` increment in section 7:
fixing only the column reference count still leaves the rows unsafe to copy.

This finding comes from reading the source; it has not been reproduced under a
sanitizer.

## Other safety-relevant differences

### Encapsulation and lifecycle

Rust keeps schema, storage, and row count private and constructs column storage
from an already validated schema ([`Table` fields](../../src/table.rs#L142)).
Duplicate names and aliases are rejected by [`Schema::new`](../../src/table/schema.rs#L220).
The schema cannot change behind existing rows.

The C++ API uses a two-phase `column_add` then `prepare` lifecycle. It also
publishes the raw data pointers, sizes, counts, flags, references, names, and
column vector as public members
([`table_column_buffer` fields](../../../gd/source/gd_table_column-buffer.h#L1264)).
Column fields and the shared column vector/reference count are public too.
Changing any of these after preparation can make the schema disagree with the
packed allocation. Later otherwise-normal cell access then performs invalid
pointer arithmetic or out-of-bounds access.

The `const` C++ subscript operators also `const_cast` the table and return a
mutable cell proxy. Writing through that proxy when the underlying object is
actually `const` is UB. Rust does not provide a mutable view from `&Table`.

### Aliasing and concurrency

Rust refuses to produce overlapping mutable columns. `columns_io` reports an
input/output overlap or duplicate output, and `column_pair_mut` returns `None`
for the same column ([implementation](../../src/table.rs#L617)). Row-range
splitting similarly creates disjoint borrows.

C++ exposes mutable raw pointers from table storage and mutable metadata. Its
shared-column reference count is a plain `int`, with unsynchronized increment
and decrement
([implementation](../../../gd/source/gd_table_column.h#L307)). Concurrent
copy/drop is a data race, which is itself UB, and public mutation allows data
races or aliasing violations without a narrow unsafe boundary.

### Raw serialization

`table_column_buffer` exposes binary read/write operations through a
`std::byte*` without a buffer length. The reader trusts serialized counts and
sizes while copying into allocations derived from the current schema. A
truncated or malicious buffer can therefore cause out-of-bounds reads or
writes. Rust's table API does not expose an equivalent unchecked binary table
deserializer; its text formatters use length-aware `Read`/`Write`-style library
interfaces.

### Panics still possible in Rust

Safe does not mean impossible to terminate the process. The reviewed Rust API
documents these table-specific panic cases:

- `RowsMut::split_at(mid)` panics when `mid > len`.
- Rayon `par_for_each(0, ...)` panics because zero is not a valid grain size.
- `ConcurrentTableBuilder` insertion panics if the concurrent vector's maximum
  capacity is exhausted.
- A user converter or callback can panic, indexing a returned Rust slice can
  panic, and calling `unwrap` on any reported error can panic.
- Allocation failure may abort the process, depending on the allocator/panic
  configuration.

These are crash/availability risks, not UB through the safe API. Bounds checks
and assertions execute in release builds unless a user deliberately selects an
abort behavior; they do not disappear like C++ `assert`.

The most notable mistake-resistance trade-off is `UnknownFields::Store`: it
intentionally turns an unknown name into a row-local extra, so a typo may be
accepted. The default is `UnknownFields::Reject`. Custom `ColumnConverter`s can
also implement incorrect business semantics or panic, although their output is
still checked for type and nullability before table mutation.

## Risk assessment

| API | Memory-safety risk | Likelihood of accidental misuse | Typical invalid-input outcome |
| --- | --- | --- | --- |
| Rust `Table`, safe calls | Low | Low | Typed error, `None`, or compile error |
| Rust documented panic operations | No UB found; availability risk | Low to medium | Panic/abort |
| C++ `table_column_buffer` / `dto::table` | High without a strict wrapper | High because overload and release behavior matter | Debug abort; release UB, corruption, or silent bad data |
| C++ `table` / `arguments::table` | Critical for copying/ownership until fixed | Medium even in ordinary RAII code | Use-after-free/double release UB |
| C++ raw members, pointers, serialization | Critical | High if exposed to general callers or untrusted bytes | Arbitrary UB/corruption |
| SQLite-to-Rust-table adapter | Low | Low | `SqliteError`; no row returned or the import stops with a valid prefix table kept private |
| SQLite-to-C++-table adapter | Critical for text/conversion paths | Medium to high | Silent coercion, partial row, allocation/deallocation UB, or conversion UB |

The Rust restrictions do impose work: complete rows must be supplied, borrowed
views must be dropped before mutation, and indexes must be rebuilt after a
mutation. Those are precisely the state transitions that are implicit and easy
to get wrong in C++.

## SQLite import and implicit column conversion

SQLite makes conversion policy especially important because each cell has a
runtime storage class independent of the column declaration. A `TEXT` cell can
contain `"1234"` even when an application expects `U32`.

### Rust: no implicit `String` to `U32` conversion

The general Rust table rejects the mismatch by default:

```rust
let schema = Schema::new([ColumnSpec::new("count", DataType::U32)])?;
let mut table = Table::new(schema);

let error = table.push_row([Value::from("1234")]).unwrap_err();
assert!(matches!(error, TableError::TypeMismatch {
    expected: DataType::U32,
    actual: DataType::String,
    ..
}));
assert_eq!(table.row_count(), 0);
```

Conversion must be enabled deliberately on the column. Its failure is reported,
its output is revalidated, and the row is not published:

```rust
let parse_u32 = ColumnConverter::new("parse-u32", |value| match value {
    ValueRef::String(text) => text
        .parse::<u32>()
        .map(Value::U32)
        .map_err(|error| ColumnConversionError::new(error.to_string())),
    _ => Err(ColumnConversionError::new("expected decimal text")),
});
let schema = Schema::new([
    ColumnSpec::new("count", DataType::U32).with_converter(parse_u32),
])?;
let mut table = Table::new(schema);

table.push_row([Value::from("1234")])?;
let error = table.push_row([Value::from("not-a-number")]).unwrap_err();
assert!(matches!(error, TableError::ConversionFailed { .. }));
assert_eq!(table.row_count(), 1);
```

This follows [`prepare_cell`](../../src/table.rs#L900): exact types use the fast
path, other types require an explicit converter, converter errors are preserved,
and the result must exactly match the column type and nullability. All cells are
prepared before [`prepare_row`](../../src/table.rs#L921) permits publication.

The SQLite adapter is stricter still. Given this database:

```sql
CREATE TABLE raw_count(count TEXT NOT NULL);
INSERT INTO raw_count VALUES ('1234'), ('not-a-number');
```

an explicit `U32` result schema does **not** silently parse the text:

```rust
let schema = Schema::new([ColumnSpec::new("count", DataType::U32)])?;
let result = database.query_table_with_schema(
    "SELECT count FROM raw_count",
    &Arguments::new(),
    schema,
);
assert!(matches!(result, Err(SqliteError::ColumnType { .. })));
```

[`query_table_with_schema`](../../src/sqlite.rs#L307) checks result width, reads
each SQLite value into a staged `Vec<Value>`, and only then calls the atomic table
insertion. [`typed_value`](../../src/sqlite.rs#L630) accepts SQLite `INTEGER` for
`U32`, range-checks it with `u32::try_from`, and rejects SQLite `TEXT`. It also
rejects a non-`0`/`1` Boolean, invalid UTF-8/UUID, null in a required column, and
the wrong storage class. Textual-number parsing therefore has to be an explicit
application step (or an intentional SQL expression with SQLite's own documented
conversion semantics).

The inferred Rust import is also defensive: if one result column contains both
an integer and text in different rows, [`query_table`](../../src/sqlite.rs#L247)
returns `SqliteError::MixedColumnType` instead of guessing a common type.

### C++: `tag_convert` is implicit, unchecked, and non-atomic

The C++ database adapter reads cursor values and calls table row insertion with
`tag_convert`
([`database::to_table`](../../../gd/source/database/gd_database_io.cpp#L33)).
For a predeclared destination with a `uint32` column named `count`, the data flow
for the same SQLite rows is effectively:

```cpp
// SQLite cursor rows: TEXT "1234", then TEXT "not-a-number".
gd::table::dto::table destination(10, gd::table::tag_full_meta{});
destination.column_add("uint32", "count");
destination.prepare();

auto result = gd::database::to_table(cursor_interface, &destination);
```

The first value is implicitly converted to `uint32`. That convenience hides a
serious defect in the conversion implementation:

```cpp
uint32_t v_; // uninitialized
std::from_chars(first, last, v_); // return value ignored
converted.assign(v_);
```

This is the actual pattern in
[`variant::convert_to_s`](../../../gd/source/gd_variant.cpp#L1222). `"1234"`
normally succeeds, but `"not-a-number"`, an empty string, or an out-of-range
decimal leaves `v_` uninitialized. Reading it in `assign` is UB. A partially
numeric string such as `"1234x"` can be accepted as `1234` because the returned
end pointer is not checked.

Other numeric conversions use C-style casts without checking whether the value
fits. For example, converting signed `-1` to `uint32` produces `4294967295`, and
narrowing a large integer can wrap. Those particular conversions are defined by
C++ for the unsigned target, but are silent data corruption from the perspective
of a range-constrained column.

Even when `convert_to` returns `false`, C++
[`cell_set(..., tag_convert)`](../../../gd/source/gd_table_column-buffer.cpp#L2398)
returns `void` and does not report the failure. Row insertion has already
published the row and converts cells one at a time
([selected-column path](../../../gd/source/gd_table_column-buffer.cpp#L1385)).
Earlier cells may contain converted values while a failed cell remains null, or
remains uninitialized when null metadata is disabled. Nevertheless,
`database::to_table` reaches its unconditional success return. The operation is
therefore neither checked nor atomic.

There is an additional UB independent of conversion success when importing
SQLite `TEXT` or `BLOB`. The C++ cursor record allocates variable-sized buffers
with `new uint8_t[]` but stores them in `std::unique_ptr<uint8_t>` rather than
`std::unique_ptr<uint8_t[]>`
([allocation](../../../gd/source/gd_database_record.cpp#L108),
[member type](../../../gd/source/gd_database_record.h#L137)). Destroying or
resizing the cursor record uses scalar `delete` for an array allocation. That
allocation/deallocation mismatch is UB and can surface as heap corruption or a
crash after an otherwise ordinary `SELECT text_column ...` to table import.

### Conversion-policy comparison

| Conversion case | Rust general `Table` | Rust SQLite import | C++ `tag_convert` / SQLite-to-table |
| --- | --- | --- | --- |
| `"1234"` -> `U32` | Rejected unless column has an explicit converter | Rejected as SQLite `TEXT`; caller must opt into a transformation | Implicitly accepted |
| `"not-a-number"` -> `U32` | `ConversionFailed`; no row change | `ColumnType`; no returned table | May read an uninitialized integer: UB |
| `"1234x"` -> `U32` | Normal `str::parse` converter rejects trailing text | Rejected as `TEXT` | May silently accept prefix `1234` |
| `-1_i64` -> `U32` | Rejected unless a custom converter chooses semantics | `ValueOutOfRange` | Wraps to `4294967295` |
| `4_294_967_296_i64` -> `U32` | Rejected unless custom converter chooses semantics | `ValueOutOfRange` | Narrows/wraps |
| `NULL` -> required `U32` | `NullNotAllowed`; no row change | `SqliteError::Table`; no returned table | May remain null despite required intent, or uninitialized without null metadata |
| Failure in cell 2 of a row | Entire row rejected before mutation | Import returns an error; partially built table is not returned | Row already published; cell 1 remains changed |

## Recommendations for the C++ API

1. Replace safety-critical `assert`s with release-mode validation that returns
   an error; keep assertions only for invariants already proved by checked code.
2. Remove or deprecate the unchecked typed `cell_get<T>` API. Return a checked
   result and use `memcpy` into a properly aligned `T` only after verifying the
   schema type and byte size.
3. Align every field to `alignof(T)`, or move to per-column typed storage.
4. Reject schemas wider than their selected null bitmap, or replace the bitmap
   contract with dynamically sized/per-column null storage.
5. Make every row insertion exact-width and atomic. Initialize the complete row
   before publishing it; require explicit null/default values.
6. Make prepared schema and layout state private and immutable. Do not expose
   mutable buffer pointers or row counts from the general table API.
7. Fix the missing reference-count increments and replace the manual count with
   `std::shared_ptr<const columns>` (or value ownership). Do not copy
   `gd::table::table` until this is corrected. Copy row-local extras in
   `arguments::table` by adding a reference (or deep-copying) for each row
   handle instead of duplicating the metadata bytes with `memcpy`.
8. Give indexes ownership of string keys or explicit invalidation/version
   checks. Document all other view invalidation rules.
9. Replace pointer-only deserialization with a bounded byte span and validate
   every count, size, multiplication, and remaining input length before copying.
10. Run C++ table tests in both assertion-enabled and `NDEBUG` configurations
    under AddressSanitizer and UndefinedBehaviorSanitizer. Sanitizers improve
    detection but do not make the API safe.

Until those changes land, the safest migration boundary is a small C++ wrapper
that constructs and prepares schemas once, requires exact complete rows,
forbids schema mutation and raw access after preparation, retains all validation
in release builds, owns values returned to callers, and does not expose the
internal `table` copy operations.

## Validation performed

- Inspected the public table types and their implementations in both trees,
  including schemas, insertion, cell access, typed views, null handling,
  ownership/copying, indexes, ordering, concurrent mutation, and serialization.
- Searched the Rust table module and its submodules for `unsafe`; none was
  found.
- Ran `cargo test --all-features --test table`: 29 tests passed.
- Ran `cargo test --all-features --test sqlite`: 10 tests passed.
- Compared behavior with assertions enabled and disabled at the source level;
  no C++ files were modified.

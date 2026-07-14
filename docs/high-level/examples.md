# C++ and Rust usage examples

This guide shows equivalent call-site patterns for the characterized C++ `gd`
library and `gd-rs`. It is a semantic translation guide, not an ABI mapping. The
examples use the public APIs exercised by the C++ GoogleTest suite and the Rust
integration tests.

| C++ | Rust | Main difference |
|---|---|---|
| `gd::variant` | `Value` | Owned sum type instead of a numeric tag and allocation flags |
| `gd::variant_view` | `ValueRef<'a>` | The Rust borrow cannot outlive or be mutably aliased with its source |
| `gd::argument::arguments` | `Arguments` | Both preserve order, duplicate names, and unnamed values |
| `gd::table::table_column_buffer` | `Table` | Rust stores one typed vector per column behind an immutable `Schema` |

## Variant and values

### Owned values

C++ constructs `gd::variant` through overloaded constructors. The object stores a
runtime type number and manages dynamic payload ownership through internal flags.

```cpp
#include <cassert>
#include <cstdint>
#include <string>

#include "gd_variant.h"

void cpp_owned_values()
{
   std::string source = "alpha";
   gd::variant text(source);
   gd::variant count(std::int32_t{42});

   source.assign("changed");

   // The variant copied and owns the original string.
   assert(text.is_string());
   assert(text.as_string() == "alpha");
   assert(count.as_int() == 42);
   assert(count.type_number() == gd::variant_type::eTypeNumberInt32);
}
```

Rust represents the type and payload together in the `Value` enum. The compiler
generates and checks the discriminant; there is no separate ownership flag.

```rust
use gd::{DataType, Value};

fn rust_owned_values() {
    let mut source = String::from("alpha");
    let text = Value::from(source.clone());
    let count = Value::from(42_i32);

    source.replace_range(.., "changed");

    assert_eq!(text.as_str(), Ok("alpha"));
    assert_eq!(count, Value::I32(42));
    assert_eq!(count.data_type(), DataType::I32);

    match count {
        Value::I32(number) => assert_eq!(number, 42),
        _ => unreachable!(),
    }
}
```

Primitive widths are preserved in both libraries. Rust expresses them as distinct
variants such as `I8`, `I32`, `U64`, and `F64`, so a pattern match can inspect the
payload without separately checking a numeric tag.

### Borrowed values

The C++ view relies on the caller keeping the source storage alive and stable.

```cpp
#include <cassert>
#include <string>
#include <string_view>

#include "gd_variant_view.h"

void cpp_borrowed_value()
{
   std::string source = "alpha";
   gd::variant_view borrowed{std::string_view(source)};

   assert(borrowed.as_string_view() == "alpha");

   // Reallocation, destruction, or an incompatible mutation of source while
   // borrowed is retained would leave the view dangling.
}
```

`ValueRef<'a>` carries the source lifetime. Safe Rust prevents the source from being
destroyed or mutably borrowed while the view is still used.

```rust
use gd::{Value, ValueRef};

fn rust_borrowed_value() {
    let source = String::from("alpha");
    let borrowed = ValueRef::from(source.as_str());

    assert_eq!(borrowed.as_str(), Ok("alpha"));

    // Make an independent value when it must outlive the source.
    let owned: Value = borrowed.to_owned();
    assert_eq!(owned.as_str(), Ok("alpha"));
}
```

Borrowing a `Value` itself is also O(1):

```rust
let owned = gd::Value::from("alpha");
let borrowed = owned.as_ref();
assert_eq!(borrowed.as_str(), Ok("alpha"));
```

### Conversion

The C++ conversion API mutates the variant's stored type and payload.

```cpp
gd::variant value{std::int32_t{42}};
value.convert(gd::variant_type::eTypeCDouble);
assert(value.type_number() == gd::variant_type::eTypeNumberDouble);
assert(value.as_double() == 42.0);
```

Rust conversion accessors leave the original value unchanged and return a typed
error for a type or range mismatch.

```rust
use gd::{Value, ValueError};

fn rust_value_conversion() -> Result<(), ValueError> {
    let value = Value::from(42_i32);
    assert_eq!(value.to_f64()?, 42.0);
    assert_eq!(value, Value::I32(42));

    assert!(Value::from(u64::MAX).to_i64().is_err());
    assert!(Value::from("42").to_i64().is_err());
    Ok(())
}
```

Text is not silently parsed as a number. Parse it at the application boundary and
then construct the intended `Value` variant.

## Arguments

### Construction and lookup

Both containers are ordered sequences rather than ordinary maps. Named and unnamed
entries can coexist.

```cpp
#include <cassert>
#include <cstdint>

#include "gd_arguments.h"

void cpp_arguments()
{
   gd::argument::arguments values;
   values.append("name", "Ada");
   values.append("age", std::int32_t{37});
   values.append(std::uint64_t{99});

   assert(values.size() == 3);
   assert(values["name"].as_string_view() == "Ada");
   assert(values["age"].as_int() == 37);
   assert(values[2].as_uint64() == 99);
}
```

```rust
use gd::{Arguments, Value};

fn rust_arguments() {
    let mut values = Arguments::new();
    values.push_named("name", "Ada");
    values.push_named("age", 37_i32);
    values.push_positional(99_u64);

    assert_eq!(values.len(), 3);
    assert_eq!(values.get_named("name").unwrap().value().as_str(), Ok("Ada"));
    assert_eq!(values.get_named("age").unwrap().value(), &Value::I32(37));
    assert_eq!(values.get(2).unwrap().name(), None);
    assert_eq!(values.get(2).unwrap().value(), &Value::U64(99));
}
```

The Rust accessors return `Option`; missing names and positions do not depend on an
assertion or sentinel value.

### Duplicate names

Occurrence order is part of both APIs. In C++, `find` returns a position in the
packed buffer and a static helper decodes its value.

```cpp
gd::argument::arguments values;
values.append("item", std::int32_t{10});
values.append("item", std::int32_t{20});
values.append("item", std::int32_t{30});

const auto* second = values.find("item", 1);
assert(second != nullptr);
assert(gd::argument::arguments::get_argument_s(second).as_int() == 20);
assert(values.find("item", 3) == nullptr);
```

Rust returns the complete `Argument`, including its name and owned `Value`.

```rust
use gd::{Argument, Arguments, Value};

let values: Arguments = [10_i32, 20, 30]
    .into_iter()
    .map(|value| Argument::named("item", value))
    .collect();

assert_eq!(
    values.get_nth_named("item", 1).unwrap().value(),
    &Value::I32(20),
);
assert!(values.get_nth_named("item", 3).is_none());
```

### Repeated lookup with an index

C++ named lookup scans and decodes the packed entries. Rust keeps linear lookup as
the small-container path and offers a borrowing `ahash` index for repeated lookup.

```rust
use gd::{Arguments, Value};

let mut values = Arguments::new();
values.push_named("item", 10_i32);
values.push_positional(false);
values.push_named("item", 20_i32);
values.push_named("other", 30_i32);

{
    let index = values.index();
    assert_eq!(index.positions("item"), &[0, 2]);

    // values.push_named("later", 40_i32); // does not compile: index is used below
    assert_eq!(
        index.get_nth_named("item", 1).unwrap().value(),
        &Value::I32(20),
    );
}

values.push_named("later", 40_i32); // valid after the index is dropped
```

The immutable borrow prevents mutation from making the indexed positions or borrowed
names stale. Building the index is expected O(n); positional access remains O(1).

### Storage ownership

C++ can place the packed argument representation in caller-owned storage:

```cpp
#include <array>
#include <cassert>
#include <cstddef>

std::array<std::byte, 256> storage{};
gd::argument::arguments values(storage);
values.append("enabled", true);
assert(!values.is_owner());
```

Rust `Arguments` owns a `Vec<Argument>`. Borrowing and shared ownership are expressed
by references, `Arc`, or an application-level wrapper rather than an ownership bit in
the encoded data. Removing an entry preserves the remaining order in both libraries;
it is O(n) because later entries shift.

## Table

### Schema construction and rows

The C++ column buffer builds schema metadata from type-name strings and must be
prepared before rows are added.

```cpp
#include <cassert>
#include <cstdint>
#include <stdexcept>
#include <string_view>
#include <tuple>
#include <vector>

#include "gd_table_column-buffer.h"

using CppColumn = std::tuple<std::string_view, unsigned, std::string_view>;

gd::table::table_column_buffer cpp_people_table()
{
   gd::table::table_column_buffer table;
   table.column_add(std::vector<CppColumn>{{"uint64", 0, "id"},
                                           {"string", 32, "name"},
                                           {"bool", 0, "active"}},
                    gd::table::tag_type_name{});

   const auto prepared = table.prepare();
   if(!prepared.first) throw std::runtime_error(prepared.second);

   table.row_add({std::uint64_t{8}, "Grace", false});
   table.row_add({std::uint64_t{7}, "Ada", true});

   assert(table.get_row_count() == 2);
   assert(table.get_column_count() == 3);
   assert(table.cell_get_variant_view(1, "id").as_uint64() == 7);
   assert(table.cell_get_variant_view(1, "name").as_string_view() == "Ada");
   return table;
}
```

Rust constructs a validated `Schema` from `DataType` values. `Table` stores one typed
vector per column, and every row is validated before any column changes.

```rust
use gd::{ColumnSpec, DataType, Schema, Table, TableError, Value, ValueRef};

fn rust_people_table() -> Result<Table, TableError> {
    let schema = Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("name", DataType::String).with_alias("display_name"),
        ColumnSpec::new("active", DataType::Bool),
    ])?;
    let mut table = Table::new(schema);

    table.push_row([
        Value::U64(8),
        Value::from("Grace"),
        Value::Bool(false),
    ])?;
    table.push_row([
        Value::U64(7),
        Value::from("Ada"),
        Value::Bool(true),
    ])?;

    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_named(1, "id")?, ValueRef::U64(7));
    assert_eq!(
        table.cell_named(1, "display_name")?,
        ValueRef::String("Ada"),
    );
    Ok(table)
}
```

Primary names and aliases must be unique. `push_row([Value; N])` consumes a fixed-size
array without a staging allocation; `push_row_vec` accepts runtime-width input.

### Open rows and custom fields

The C++ argument-backed table redirects an unknown cell name into row-local argument
storage. The named-row vector is explicit here because the bare nested initializer is
ambiguous with positional `row_set` overloads under Clang.

```cpp
using NamedValue = std::pair<std::string_view, gd::variant_view>;

gd::table::arguments::table files(gd::table::tag_full_meta{});
files.column_prepare();
files.column_add("rstring", 0, "path");
files.column_add("rstring", 0, "name");
files.column_add("uint64", 0, "size");
files.prepare();

for(unsigned row_index = 0; row_index < 4; ++row_index)
{
   const auto row = files.row_add_one();
   files.row_set(row,
                 std::vector<NamedValue>{
                    {"path", gd::variant_view("C:\\data\\files\\entry.bin")},
                    {"name", gd::variant_view("entry.bin")},
                    {"size", gd::variant_view(std::uint64_t{1000} + row_index)}},
                 gd::table::tag_convert{});
   files.cell_set(row, "custom_file_category",
                  gd::variant_view(row_index % 2 == 0 ? "binary" : "text"));
}

assert(files.cell_get_variant_view(0, "custom_file_category").as_string_view()
       == "binary");
```

Rust keeps the fixed schema columnar and makes the same fallback an explicit schema
policy. Fixed and extra values can be appended atomically:

```rust
use gd::{
    ColumnSpec, DataType, Schema, Table, TableError, UnknownFields, Value, ValueRef,
};

fn rust_files_table() -> Result<Table, TableError> {
    let schema = Schema::new([
        ColumnSpec::new("path", DataType::String),
        ColumnSpec::new("name", DataType::String),
        ColumnSpec::new("size", DataType::U64),
    ])?
    .with_unknown_fields(UnknownFields::Store);
    let mut files = Table::new(schema);

    for row_index in 0..4_u64 {
        let category = if row_index % 2 == 0 { "binary" } else { "text" };
        files.push_row_with_extras(
            [
                Value::from(r"C:\data\files\entry.bin"),
                Value::from("entry.bin"),
                Value::U64(1_000 + row_index),
            ],
            [("custom_file_category", Value::from(category))],
        )?;
    }

    assert_eq!(
        files.cell_named(0, "custom_file_category")?,
        ValueRef::String("binary"),
    );
    Ok(files)
}
```

In both APIs the custom name is row metadata, not a column available for column scans
or indexes. The Rust storage owns ordinary `Value`s and avoids the packed C++ sidecar
copy path that the sanitizer characterization currently reports as unsafe.

### Nulls and invalid rows

The C++ null-enabled table requires callers to set every omitted cell deliberately.
An empty row is otherwise marked non-null even though its fixed payload was not
initialized by `row_add()`. That behavior is documented as a defect and should not be
copied into new call sites.

Rust puts nullability in each column specification and represents null as a normal
sum-type case:

```rust
use gd::{ColumnSpec, DataType, Schema, Table, TableError, Value};

fn rust_nullable_table() -> Result<(), TableError> {
    let schema = Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("score", DataType::I32).nullable(true),
    ])?;
    let mut table = Table::new(schema);

    table.push_row([Value::U64(1), Value::I32(10)])?;
    table.push_row([Value::U64(2), Value::Null])?;

    // The wrong integer width is rejected without partially appending a row.
    assert!(table.push_row([Value::I64(3), Value::I32(30)]).is_err());
    assert_eq!(table.row_count(), 2);
    Ok(())
}
```

Non-nullable columns reject `Value::Null`. Wrong widths, wrong types, and row-width
mismatches are `TableError` values rather than implicit conversions. Parse or convert
application input before constructing the row.

### Rows and columns as borrowed views

C++ returns `variant_view` cells into the table's storage:

```cpp
auto table = cpp_people_table();
const gd::variant_view name = table.cell_get_variant_view(1, "name");
assert(name.as_string_view() == "Ada");
```

Rust `Row`, `Column`, and `ValueRef` carry the table borrow:

```rust
use gd::TableError;

fn rust_table_views() -> Result<(), TableError> {
    let table = rust_people_table()?;

    let row = table.row(1).unwrap();
    assert_eq!(row.get_named("name").unwrap().as_str(), Ok("Ada"));

    let names = table.column_named("name").unwrap();
    let collected: Vec<_> = names
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(collected, ["Grace", "Ada"]);
    Ok(())
}
```

The compiler prevents table mutation while one of these views remains in use.

### Indexing repeated values

Rust can build a typed, borrowing equality index for a column. Duplicate keys retain
all matching row positions.

```rust
use gd::{IndexKeyRef, TableError};

fn rust_table_index() -> Result<(), TableError> {
    let table = rust_people_table()?;
    let names = table.index(1)?;

    assert_eq!(names.rows(IndexKeyRef::from("Ada")), &[1]);
    assert_eq!(names.rows(IndexKeyRef::from("missing")), &[] as &[usize]);
    Ok(())
}
```

The index immutably borrows the table, preventing stale keys after mutation. The C++
index classes require separate population and sorting, retain borrowed string views,
and currently accept a non-equal `lower_bound` result as a match, so their lookup
behavior is not reproduced in this example.

### Ordering rows

C++ selection sorting physically reorders complete rows:

```cpp
auto table = cpp_people_table();
table.sort(0, true, gd::table::tag_sort_selection{});

assert(table.cell_get_variant_view(0, "id").as_uint64() == 7);
assert(table.cell_get_variant_view(1, "id").as_uint64() == 8);
```

Rust returns a stable permutation and leaves the table unchanged. Null placement is
independent of ascending or descending value order.

```rust
use gd::{NullOrder, SortDirection, TableError, ValueRef};

fn rust_table_order() -> Result<(), TableError> {
    let table = rust_people_table()?;
    let order = table.row_order_named(
        "id",
        SortDirection::Ascending,
        NullOrder::Last,
    )?;

    assert_eq!(order.positions(), &[1, 0]);
    assert_eq!(
        order.rows().next().unwrap().get_named("id"),
        Some(ValueRef::U64(7)),
    );
    assert_eq!(table.cell_named(0, "id")?, ValueRef::U64(8)); // original order
    Ok(())
}
```

Constructing `RowOrder` takes O(rows log rows) time and O(rows) additional positions.
Iteration is O(rows) and does not move unrelated column payloads.

## Common call-site translations

| C++ call | Rust call |
|---|---|
| `gd::variant(value)` | `Value::from(value)` or a specific `Value::I32(value)` variant |
| `variant.type_number()` | `value.data_type()` or an enum pattern match |
| `variant_view(source)` | `ValueRef::from(source)` or `value.as_ref()` |
| `variant.convert(type)` | checked accessor such as `value.to_i64()` or explicit application conversion |
| `arguments.append(name, value)` | `arguments.push_named(name, value)` |
| `arguments.append(value)` | `arguments.push_positional(value)` |
| `arguments.find(name, occurrence)` | `arguments.get_nth_named(name, occurrence)` |
| repeated linear argument lookup | `let index = arguments.index()` |
| `table.column_add(...); table.prepare()` | `Schema::new(...)` followed by `Table::new(schema)` |
| `table.row_add({...})` | `table.push_row([...])?` |
| `table.cell_get_variant_view(row, name)` | `table.cell_named(row, name)?` |
| destructive `table.sort(...)` | borrowing `table.row_order_named(...)` |

See [dynamic values](value.md), [arguments](arguments.md), and [tables](table.md) for
the complete contracts and complexity notes. Intentional behavior differences are
listed in [port compatibility](../port/compatibility.md).

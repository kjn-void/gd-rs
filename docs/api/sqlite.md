# SQLite

The default `sqlite` Cargo feature provides `SqliteDatabase`, a narrow adapter between
`Arguments`, `Value`, typed `Table` storage, and `rusqlite`. It owns one connection and
delegates SQL syntax, transactions, and connection behavior to SQLite.

Disable it with `default-features = false` when the crate is used without a database:

```toml
[dependencies]
gd-rs = { version = "0.1", default-features = false }
```

## Execute with parameters

```rust
use gd::{Arguments, SqliteDatabase};

let database = SqliteDatabase::open_in_memory().unwrap();
database
    .execute_batch("create table person (id integer, name text)")
    .unwrap();

let mut parameters = Arguments::new();
parameters.push_named("id", 1_i64);
parameters.push_named("name", "Ada");
database
    .execute(
        "insert into person (id, name) values (:id, :name)",
        &parameters,
    )
    .unwrap();
```

A statement and its arguments must be entirely named or entirely positional. Bare
argument names match `:name`, `@name`, and `$name`. Duplicate, missing, unused, mixed,
or incorrectly counted parameters are rejected before execution. `u64` values above
`i64::MAX` are also rejected because SQLite integers are signed.

GD values bind as follows:

| GD value | SQLite storage |
|---|---|
| null | `NULL` |
| Boolean and integers | `INTEGER` |
| floats | `REAL` |
| string | `TEXT` |
| bytes and UUID | `BLOB` |

UUID parameters use their 16-byte representation.

## Infer a result table

```rust
use gd::{Arguments, DataType, SqliteDatabase};

let database = SqliteDatabase::open_in_memory().unwrap();
let table = database
    .query_table(
        "select 1 as id, 'Ada' as name",
        &Arguments::new(),
    )
    .unwrap();

assert_eq!(table.schema().column(0).unwrap().data_type(), DataType::I64);
assert_eq!(table.schema().column(1).unwrap().data_type(), DataType::String);
```

Inference maps SQLite `INTEGER`, `REAL`, `TEXT`, and `BLOB` to `I64`, `F64`, `String`,
and `Bytes`. A column containing only nulls becomes `Null`; inferred columns are
nullable. SQLite permits a different storage class in every row, but a result column
with mixed non-null classes is rejected rather than coerced unpredictably.

This convenience path buffers O(rows × columns) `Value` discriminants before it can
construct typed columns. Owned string and blob payloads are moved into the result,
not copied.

## Supply an exact schema

```rust
use gd::{Arguments, ColumnSpec, DataType, Schema, SqliteDatabase};

let database = SqliteDatabase::open_in_memory().unwrap();
let schema = Schema::new([
    ColumnSpec::new("enabled", DataType::Bool),
    ColumnSpec::new("count", DataType::U16),
])
.unwrap();

let table = database
    .query_table_with_schema("select 1, 42", &Arguments::new(), schema)
    .unwrap();
assert_eq!(table.row_count(), 1);
```

The explicit path streams rows directly into the requested typed columns and uses
only O(columns) staging space beyond the returned table. Integers are range-checked;
Boolean columns accept only 0 or 1; integer results may become floats; UUID columns
accept either a 16-byte blob or parseable UUID text. Result width, storage class,
UTF-8, UUID, range, and nullability mismatches are errors.

## Connection access

`connection` and `connection_mut` expose `rusqlite::Connection` for transactions,
prepared statement reuse, pragmas, backup APIs, and other advanced operations.
`from_connection` wraps an existing connection, while `into_connection` returns it.
`last_insert_rowid` and `is_autocommit` forward common connection state.

Unlike the broader C++ database layer, this crate currently has no ODBC driver,
generic database interface, retry/logging policy, cursor wrapper, or SQL builder.
Bind parameters rather than generating SQL literals, and use `rusqlite` directly for
features outside the adapter's deliberately small contract.

# SQLite adapter

The default `sqlite` feature provides `SqliteDatabase`, a small adapter between GD
values, arguments, typed tables, and `rusqlite`. It is not a driver-neutral database
layer. Applications can access the wrapped `rusqlite::Connection` for transactions,
configuration, and APIs that do not need GD conversion.

Disable the adapter and its dependency with `--no-default-features`.

## Parameters

`execute`, `query_table`, and `query_table_with_schema` accept `Arguments`. A call
must use exactly one parameter mode:

- positional arguments bind `?`/`?NNN` slots in order;
- named arguments bind `:name`, `@name`, or `$name`; an argument key may include or
  omit the prefix;
- named and positional values cannot be mixed in one argument collection or SQL
  statement;
- duplicate, missing, extra, and count-mismatched parameters are errors.

Booleans and integer variants bind as SQLite `INTEGER`, floats as `REAL`, strings as
`TEXT`, bytes as `BLOB`, UUIDs as 16-byte blobs, and null as `NULL`. SQLite integers
are signed 64-bit values, so a `U64` above `i64::MAX` is rejected.

## Query materialization

`query_table` infers a nullable schema from runtime storage classes:

| SQLite class | GD type |
|---|---|
| `NULL` only | `Null` |
| `INTEGER` | `I64` |
| `REAL` | `F64` |
| `TEXT` | `String` |
| `BLOB` | `Bytes` |

SQLite permits different storage classes in one result column. Inference rejects a
column whose non-null rows change class rather than silently converting or losing
data. It buffers **O(rows × columns)** value discriminants before building the table;
owned text and blob payloads are moved into typed columns.

`query_table_with_schema` takes a caller-supplied `Schema` and stages one row at a
time, using **O(columns)** temporary space in addition to the returned table. Integer
widths and unsigned values are range-checked. Boolean columns accept integer 0 or 1.
UUID columns accept text recognized by the `uuid` crate or a 16-byte blob. Integer-to-float and
`F64`-to-`F32` conversion can round. Table nullability rules are enforced unchanged.

Both paths require valid UTF-8 for SQLite `TEXT` values.

## Transactions and errors

The adapter returns `SqliteError`; it does not log expected failures. Engine errors,
parameter-policy errors, conversion failures, and table/schema failures remain
distinguishable variants.

Use `connection_mut().transaction()` for native `rusqlite` transactions. The wrapper
also exposes immutable/mutable connection access, ownership recovery with
`into_connection`, `last_insert_rowid`, and autocommit state.

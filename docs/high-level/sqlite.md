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
are signed 64-bit values, so a `U64` above `i64::MAX` is rejected. SQLite has no NaN
representation and stores a bound NaN as `NULL`, so binding NaN is rejected instead of
silently changing the value; infinities bind as ordinary `REAL` values. `execute`
rejects statements that produce result columns before binding, so a rejected
`RETURNING` write never reaches the database.

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
time in a reusable vector, using **O(columns)** temporary space in addition to the
returned table. Complete-row validation precedes mutation; owned payloads move into
the typed columns while the same staging allocation serves the next row. Integer
widths and unsigned values are range-checked. Boolean columns accept integer 0 or 1.
UUID columns accept text recognized by the `uuid` crate or a 16-byte blob. Integer-to-float and
`F64`-to-`F32` conversion can round; a finite `F64` outside the `F32` range is rejected
rather than silently becoming an infinity. Table nullability rules are enforced unchanged.

Both paths require valid UTF-8 for SQLite `TEXT` values.

`schema_for_table` constructs a schema from a table's declared column metadata.
`load_table` combines that discovery with `SELECT *` and explicit-schema streaming.
Exact-width numeric declarations use the `INTEGER_I8`/`INTEGER_U8` family through 64
bits and `REAL_F32`/`REAL_F64`; ordinary SQLite declarations retain their conventional
`I64`, `F64`, `String`, and `Bytes` mappings. Unknown declarations fail explicitly.
Generated columns that `SELECT *` returns are included; hidden virtual-table columns are
not. `NOT NULL` is preserved, and a primary key is non-nullable only when SQLite reports
`NOT NULL` or the column is an `INTEGER PRIMARY KEY` rowid alias. Rowid-table primary
keys such as `TEXT PRIMARY KEY` and composite keys stay nullable because SQLite permits
stored `NULL` values in them.

## Transactions and errors

The adapter returns `SqliteError`; it does not log expected failures. Engine errors,
parameter-policy errors, conversion failures, and table/schema failures remain
distinguishable variants.

Use `connection_mut().transaction()` for native `rusqlite` transactions. The wrapper
also exposes immutable/mutable connection access, ownership recovery with
`into_connection`, `last_insert_rowid`, and autocommit state.

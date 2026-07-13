//! `SQLite` integration for GD values, arguments, and typed tables.

use std::{fmt, path::Path, str::Utf8Error};

use ahash::{AHashMap, AHashSet};
use compact_str::CompactString;
use rusqlite::{Statement, types::ToSqlOutput, types::ValueRef as SqlValueRef};
use thiserror::Error;
use uuid::Uuid;

use crate::{Arguments, ColumnSpec, DataType, Schema, Table, TableError, Value};

/// The maintained `SQLite` connection type used by [`SqliteDatabase`].
pub type SqliteConnection = rusqlite::Connection;

/// The underlying `SQLite` engine error type.
pub type SqliteEngineError = rusqlite::Error;

/// One of `SQLite`'s five runtime storage classes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SqliteStorageClass {
    /// SQL `NULL`.
    Null,
    /// A signed 64-bit integer.
    Integer,
    /// A 64-bit floating-point number.
    Real,
    /// Text bytes tagged by `SQLite` as text.
    Text,
    /// Arbitrary bytes.
    Blob,
}

impl SqliteStorageClass {
    /// Returns `SQLite`'s conventional uppercase name for this storage class.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Null => "NULL",
            Self::Integer => "INTEGER",
            Self::Real => "REAL",
            Self::Text => "TEXT",
            Self::Blob => "BLOB",
        }
    }
}

impl fmt::Display for SqliteStorageClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A failure while binding GD values or materializing `SQLite` results.
#[derive(Debug, Error)]
pub enum SqliteError {
    /// `SQLite` rejected an operation.
    #[error(transparent)]
    Engine(#[from] SqliteEngineError),
    /// A result did not satisfy the target table schema.
    #[error(transparent)]
    Table(#[from] TableError),
    /// Named and positional arguments were mixed in one collection.
    #[error("SQLite arguments must be either entirely named or entirely positional")]
    MixedArguments,
    /// Named and positional placeholders were mixed in one statement.
    #[error("SQLite statement mixes named and positional placeholders")]
    MixedStatementParameters,
    /// The argument mode does not match the statement placeholder mode.
    #[error("SQLite statement expects {expected} parameters, but arguments are {actual}")]
    ParameterMode {
        /// The mode used by the SQL statement.
        expected: &'static str,
        /// The mode used by the supplied arguments.
        actual: &'static str,
    },
    /// A positional parameter count differs from the statement count.
    #[error("SQLite statement expects {expected} parameters, found {actual}")]
    ParameterCount {
        /// Number of bind slots in the statement.
        expected: usize,
        /// Number of supplied arguments.
        actual: usize,
    },
    /// A named argument occurs more than once.
    #[error("duplicate SQLite parameter: {0}")]
    DuplicateParameter(CompactString),
    /// A statement placeholder has no supplied value.
    #[error("missing SQLite parameter: {0}")]
    MissingParameter(CompactString),
    /// A supplied named value is not used by the statement.
    #[error("unknown SQLite parameter: {0}")]
    UnknownParameter(CompactString),
    /// An unsigned GD integer cannot be represented by `SQLite`'s signed integer.
    #[error("unsigned value {value} exceeds SQLite's signed integer range")]
    UnsignedOutOfRange {
        /// The rejected value.
        value: u64,
    },
    /// A query returned a different number of columns than the requested schema.
    #[error("SQLite query returns {actual} columns, schema contains {expected}")]
    ColumnCount {
        /// Number of schema columns.
        expected: usize,
        /// Number of result columns.
        actual: usize,
    },
    /// One inferred result column used more than one non-null `SQLite` storage class.
    #[error("SQLite column {column} changes type from {first} to {actual}")]
    MixedColumnType {
        /// Zero-based result column.
        column: usize,
        /// First non-null storage class.
        first: SqliteStorageClass,
        /// Later, incompatible storage class.
        actual: SqliteStorageClass,
    },
    /// A storage class cannot be converted to the requested GD type.
    #[error("SQLite column {column} is {actual}, expected {expected}")]
    ColumnType {
        /// Zero-based result column.
        column: usize,
        /// Requested GD type.
        expected: DataType,
        /// `SQLite`'s runtime storage class.
        actual: SqliteStorageClass,
    },
    /// A numeric or UUID payload is outside the requested GD type's domain.
    #[error("SQLite column {column} value {value} is outside {target}")]
    ValueOutOfRange {
        /// Zero-based result column.
        column: usize,
        /// Display form of the rejected payload.
        value: String,
        /// Requested GD type.
        target: DataType,
    },
    /// `SQLite` text bytes are not valid UTF-8.
    #[error("SQLite column {column} contains invalid UTF-8: {source}")]
    InvalidUtf8 {
        /// Zero-based result column.
        column: usize,
        /// UTF-8 validation detail.
        #[source]
        source: Utf8Error,
    },
    /// A UUID result is neither a 16-byte blob nor text accepted by the UUID parser.
    #[error("SQLite column {column} contains an invalid UUID: {source}")]
    InvalidUuid {
        /// Zero-based result column.
        column: usize,
        /// UUID parser detail.
        #[source]
        source: uuid::Error,
    },
}

/// An owned `SQLite` connection with GD-aware parameter and result adapters.
///
/// The wrapper delegates connection, transaction, and SQL semantics to `rusqlite`.
/// Its GD-specific responsibility is loss-checked `Value` binding and conversion of
/// query results into typed [`Table`] storage. It does not log or retry operations.
pub struct SqliteDatabase {
    connection: SqliteConnection,
}

impl SqliteDatabase {
    /// Opens or creates a `SQLite` database at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`SqliteError::Engine`] when `SQLite` cannot open the path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteError> {
        Ok(Self {
            connection: SqliteConnection::open(path)?,
        })
    }

    /// Opens a private in-memory `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns [`SqliteError::Engine`] if `SQLite` initialization fails.
    pub fn open_in_memory() -> Result<Self, SqliteError> {
        Ok(Self {
            connection: SqliteConnection::open_in_memory()?,
        })
    }

    /// Wraps an existing `rusqlite` connection.
    #[must_use]
    pub const fn from_connection(connection: SqliteConnection) -> Self {
        Self { connection }
    }

    /// Executes one statement with either named or positional GD arguments.
    ///
    /// Bare named argument keys match `:name`, `@name`, or `$name` placeholders.
    /// Duplicate names, extra values, missing values, mixed modes, and `u64` values
    /// above `i64::MAX` are rejected before execution.
    ///
    /// # Errors
    ///
    /// Returns a parameter-policy error or [`SqliteError::Engine`].
    pub fn execute(&self, sql: &str, arguments: &Arguments) -> Result<usize, SqliteError> {
        let mut statement = self.connection.prepare(sql)?;
        bind_arguments(&mut statement, arguments)?;
        Ok(statement.raw_execute()?)
    }

    /// Executes one or more SQL statements without parameters.
    ///
    /// # Errors
    ///
    /// Returns [`SqliteError::Engine`] if any statement fails. `SQLite` controls
    /// whether prior statements remain committed; use a transaction for atomicity.
    pub fn execute_batch(&self, sql: &str) -> Result<(), SqliteError> {
        Ok(self.connection.execute_batch(sql)?)
    }

    /// Executes a query and infers a nullable GD schema from its runtime values.
    ///
    /// Integer, real, text, and blob storage become `I64`, `F64`, `String`, and
    /// `Bytes`. A column containing only nulls becomes `Null`. Because `SQLite` permits
    /// different storage classes in one column, a later non-null class differing from
    /// the first is an error. Use [`Self::query_table_with_schema`] for explicit
    /// coercion into Boolean, narrower numeric, unsigned, or UUID columns.
    ///
    /// This convenience path buffers **O(rows x columns)** `Value` discriminants
    /// before constructing the typed columns. Owned string/blob payloads are moved,
    /// not copied, into the table.
    ///
    /// # Errors
    ///
    /// Returns parameter, `SQLite`, UTF-8, mixed-column-type, or schema errors.
    pub fn query_table(&self, sql: &str, arguments: &Arguments) -> Result<Table, SqliteError> {
        let mut statement = self.connection.prepare(sql)?;
        let column_count = statement.column_count();
        let names: Vec<String> = statement
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        bind_arguments(&mut statement, arguments)?;

        let mut inferred = vec![None; column_count];
        let mut buffered = Vec::new();
        let mut rows = statement.raw_query();
        while let Some(row) = rows.next()? {
            let mut values = Vec::with_capacity(column_count);
            for (column, inferred_type) in inferred.iter_mut().enumerate() {
                let raw = row.get_ref(column)?;
                let storage = storage_class(raw);
                if storage != SqliteStorageClass::Null {
                    match *inferred_type {
                        None => *inferred_type = Some(storage),
                        Some(first) if first != storage => {
                            return Err(SqliteError::MixedColumnType {
                                column,
                                first,
                                actual: storage,
                            });
                        }
                        Some(_) => {}
                    }
                }
                values.push(inferred_value(column, raw)?);
            }
            buffered.push(values);
        }
        drop(rows);
        drop(statement);

        let columns = names.into_iter().zip(inferred).map(|(name, storage)| {
            ColumnSpec::new(name, storage.map_or(DataType::Null, inferred_data_type)).nullable(true)
        });
        let schema = Schema::new(columns)?;
        let mut table = Table::with_capacity(schema, buffered.len());
        for row in buffered {
            table.push_row_vec(row)?;
        }
        Ok(table)
    }

    /// Streams query rows directly into a caller-supplied typed schema.
    ///
    /// Integer results are range-checked for the requested integer width. Integers
    /// may also become `F32`/`F64`; Boolean columns accept only 0 or 1. UUID columns
    /// accept a 16-byte blob or UUID text. Nullability is enforced by [`Table`].
    /// This path uses **O(columns)** staging space per row plus the returned table.
    ///
    /// # Errors
    ///
    /// Returns an error for parameter mismatch, result width, storage-class mismatch,
    /// range failure, invalid UTF-8/UUID, or a table nullability violation.
    pub fn query_table_with_schema(
        &self,
        sql: &str,
        arguments: &Arguments,
        schema: Schema,
    ) -> Result<Table, SqliteError> {
        let mut statement = self.connection.prepare(sql)?;
        let actual = statement.column_count();
        if actual != schema.len() {
            return Err(SqliteError::ColumnCount {
                expected: schema.len(),
                actual,
            });
        }
        bind_arguments(&mut statement, arguments)?;

        let mut table = Table::new(schema);
        let mut rows = statement.raw_query();
        while let Some(row) = rows.next()? {
            let values = table
                .schema()
                .iter()
                .enumerate()
                .map(|(column, spec)| typed_value(column, spec.data_type(), row.get_ref(column)?))
                .collect::<Result<Vec<_>, SqliteError>>()?;
            table.push_row_vec(values)?;
        }
        Ok(table)
    }

    /// Returns the wrapped connection for advanced read-only operations.
    #[must_use]
    pub const fn connection(&self) -> &SqliteConnection {
        &self.connection
    }

    /// Returns the wrapped connection mutably, including for transactions.
    pub const fn connection_mut(&mut self) -> &mut SqliteConnection {
        &mut self.connection
    }

    /// Consumes the wrapper and returns the underlying connection.
    #[must_use]
    pub fn into_connection(self) -> SqliteConnection {
        self.connection
    }

    /// Returns the rowid of the most recent successful insert on this connection.
    #[must_use]
    pub fn last_insert_rowid(&self) -> i64 {
        self.connection.last_insert_rowid()
    }

    /// Returns whether the connection is currently outside a transaction.
    #[must_use]
    pub fn is_autocommit(&self) -> bool {
        self.connection.is_autocommit()
    }
}

impl fmt::Debug for SqliteDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteDatabase")
            .field("is_autocommit", &self.is_autocommit())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParameterMode {
    None,
    Positional,
    Named,
}

impl ParameterMode {
    const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Positional => "positional",
            Self::Named => "named",
        }
    }
}

fn bind_arguments(statement: &mut Statement<'_>, arguments: &Arguments) -> Result<(), SqliteError> {
    let statement_mode = statement_parameter_mode(statement)?;
    let argument_mode = argument_mode(arguments)?;
    if statement_mode != argument_mode {
        return Err(SqliteError::ParameterMode {
            expected: statement_mode.name(),
            actual: argument_mode.name(),
        });
    }

    match statement_mode {
        ParameterMode::None => Ok(()),
        ParameterMode::Positional => bind_positional(statement, arguments),
        ParameterMode::Named => bind_named(statement, arguments),
    }
}

fn statement_parameter_mode(statement: &Statement<'_>) -> Result<ParameterMode, SqliteError> {
    let mut named = false;
    let mut positional = false;
    for index in 1..=statement.parameter_count() {
        match statement.parameter_name(index) {
            Some(name) if !name.starts_with('?') => named = true,
            _ => positional = true,
        }
    }
    match (named, positional) {
        (false, false) => Ok(ParameterMode::None),
        (false, true) => Ok(ParameterMode::Positional),
        (true, false) => Ok(ParameterMode::Named),
        (true, true) => Err(SqliteError::MixedStatementParameters),
    }
}

fn argument_mode(arguments: &Arguments) -> Result<ParameterMode, SqliteError> {
    let named = arguments.iter().any(|argument| argument.name().is_some());
    let positional = arguments.iter().any(|argument| argument.name().is_none());
    match (named, positional) {
        (false, false) => Ok(ParameterMode::None),
        (false, true) => Ok(ParameterMode::Positional),
        (true, false) => Ok(ParameterMode::Named),
        (true, true) => Err(SqliteError::MixedArguments),
    }
}

fn bind_positional(
    statement: &mut Statement<'_>,
    arguments: &Arguments,
) -> Result<(), SqliteError> {
    let expected = statement.parameter_count();
    if arguments.len() != expected {
        return Err(SqliteError::ParameterCount {
            expected,
            actual: arguments.len(),
        });
    }
    for (position, argument) in arguments.iter().enumerate() {
        let value = sql_value_ref(argument.value())?;
        statement.raw_bind_parameter(position + 1, ToSqlOutput::Borrowed(value))?;
    }
    Ok(())
}

fn bind_named(statement: &mut Statement<'_>, arguments: &Arguments) -> Result<(), SqliteError> {
    let mut supplied = AHashMap::with_capacity(arguments.len());
    for argument in arguments.iter() {
        let name = normalized_parameter_name(argument.name().ok_or(SqliteError::MixedArguments)?);
        if supplied.insert(name, argument.value()).is_some() {
            return Err(SqliteError::DuplicateParameter(name.into()));
        }
    }

    let mut used = AHashSet::with_capacity(statement.parameter_count());
    for index in 1..=statement.parameter_count() {
        let raw_name = statement
            .parameter_name(index)
            .ok_or(SqliteError::MixedStatementParameters)?;
        let name = normalized_parameter_name(raw_name);
        let (supplied_name, value) = supplied
            .get_key_value(name)
            .ok_or_else(|| SqliteError::MissingParameter(name.into()))?;
        used.insert(*supplied_name);
        statement.raw_bind_parameter(index, ToSqlOutput::Borrowed(sql_value_ref(value)?))?;
    }
    if let Some(unused) = supplied.keys().find(|name| !used.contains(**name)) {
        return Err(SqliteError::UnknownParameter((*unused).into()));
    }
    Ok(())
}

fn normalized_parameter_name(name: &str) -> &str {
    name.strip_prefix([':', '@', '$']).unwrap_or(name)
}

fn sql_value_ref(value: &Value) -> Result<SqlValueRef<'_>, SqliteError> {
    Ok(match value {
        Value::Null => SqlValueRef::Null,
        Value::Bool(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::I8(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::I16(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::I32(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::I64(value) => SqlValueRef::Integer(*value),
        Value::U8(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::U16(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::U32(value) => SqlValueRef::Integer(i64::from(*value)),
        Value::U64(value) => SqlValueRef::Integer(
            i64::try_from(*value).map_err(|_| SqliteError::UnsignedOutOfRange { value: *value })?,
        ),
        Value::F32(value) => SqlValueRef::Real(f64::from(*value)),
        Value::F64(value) => SqlValueRef::Real(*value),
        Value::String(value) => SqlValueRef::Text(value.as_bytes()),
        Value::Bytes(value) => SqlValueRef::Blob(value),
        Value::Uuid(value) => SqlValueRef::Blob(value.as_bytes()),
    })
}

const fn storage_class(value: SqlValueRef<'_>) -> SqliteStorageClass {
    match value {
        SqlValueRef::Null => SqliteStorageClass::Null,
        SqlValueRef::Integer(_) => SqliteStorageClass::Integer,
        SqlValueRef::Real(_) => SqliteStorageClass::Real,
        SqlValueRef::Text(_) => SqliteStorageClass::Text,
        SqlValueRef::Blob(_) => SqliteStorageClass::Blob,
    }
}

const fn inferred_data_type(storage: SqliteStorageClass) -> DataType {
    match storage {
        SqliteStorageClass::Null => DataType::Null,
        SqliteStorageClass::Integer => DataType::I64,
        SqliteStorageClass::Real => DataType::F64,
        SqliteStorageClass::Text => DataType::String,
        SqliteStorageClass::Blob => DataType::Bytes,
    }
}

fn inferred_value(column: usize, value: SqlValueRef<'_>) -> Result<Value, SqliteError> {
    Ok(match value {
        SqlValueRef::Null => Value::Null,
        SqlValueRef::Integer(value) => Value::I64(value),
        SqlValueRef::Real(value) => Value::F64(value),
        SqlValueRef::Text(value) => Value::from(
            std::str::from_utf8(value)
                .map_err(|source| SqliteError::InvalidUtf8 { column, source })?,
        ),
        SqlValueRef::Blob(value) => Value::from(value),
    })
}

// Explicit floating schemas request SQLite-style numeric coercion. Integer-to-float
// and f64-to-f32 conversions may round, just as a SQLite REAL/CAST operation may.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn typed_value(
    column: usize,
    expected: DataType,
    value: SqlValueRef<'_>,
) -> Result<Value, SqliteError> {
    if matches!(value, SqlValueRef::Null) {
        return Ok(Value::Null);
    }
    match expected {
        DataType::Null => Err(column_type(column, expected, value)),
        DataType::Bool => match value {
            SqlValueRef::Integer(0) => Ok(Value::Bool(false)),
            SqlValueRef::Integer(1) => Ok(Value::Bool(true)),
            SqlValueRef::Integer(value) => Err(out_of_range(column, value, expected)),
            _ => Err(column_type(column, expected, value)),
        },
        DataType::I8 => integer_value(column, expected, value, |value| {
            i8::try_from(value).map(Value::I8)
        }),
        DataType::I16 => integer_value(column, expected, value, |value| {
            i16::try_from(value).map(Value::I16)
        }),
        DataType::I32 => integer_value(column, expected, value, |value| {
            i32::try_from(value).map(Value::I32)
        }),
        DataType::I64 => match value {
            SqlValueRef::Integer(value) => Ok(Value::I64(value)),
            _ => Err(column_type(column, expected, value)),
        },
        DataType::U8 => integer_value(column, expected, value, |value| {
            u8::try_from(value).map(Value::U8)
        }),
        DataType::U16 => integer_value(column, expected, value, |value| {
            u16::try_from(value).map(Value::U16)
        }),
        DataType::U32 => integer_value(column, expected, value, |value| {
            u32::try_from(value).map(Value::U32)
        }),
        DataType::U64 => integer_value(column, expected, value, |value| {
            u64::try_from(value).map(Value::U64)
        }),
        DataType::F32 => match value {
            SqlValueRef::Integer(value) => Ok(Value::F32(value as f32)),
            SqlValueRef::Real(value) => Ok(Value::F32(value as f32)),
            _ => Err(column_type(column, expected, value)),
        },
        DataType::F64 => match value {
            SqlValueRef::Integer(value) => Ok(Value::F64(value as f64)),
            SqlValueRef::Real(value) => Ok(Value::F64(value)),
            _ => Err(column_type(column, expected, value)),
        },
        DataType::String => match value {
            SqlValueRef::Text(value) => Ok(Value::from(
                std::str::from_utf8(value)
                    .map_err(|source| SqliteError::InvalidUtf8 { column, source })?,
            )),
            _ => Err(column_type(column, expected, value)),
        },
        DataType::Bytes => match value {
            SqlValueRef::Blob(value) => Ok(Value::from(value)),
            _ => Err(column_type(column, expected, value)),
        },
        DataType::Uuid => match value {
            SqlValueRef::Blob(value) => Uuid::from_slice(value)
                .map(Value::Uuid)
                .map_err(|source| SqliteError::InvalidUuid { column, source }),
            SqlValueRef::Text(value) => {
                let text = std::str::from_utf8(value)
                    .map_err(|source| SqliteError::InvalidUtf8 { column, source })?;
                Uuid::parse_str(text)
                    .map(Value::Uuid)
                    .map_err(|source| SqliteError::InvalidUuid { column, source })
            }
            _ => Err(column_type(column, expected, value)),
        },
    }
}

fn integer_value<T, F>(
    column: usize,
    expected: DataType,
    value: SqlValueRef<'_>,
    convert: F,
) -> Result<Value, SqliteError>
where
    F: FnOnce(i64) -> Result<Value, T>,
{
    match value {
        SqlValueRef::Integer(value) => convert(value).map_err(|_| SqliteError::ValueOutOfRange {
            column,
            value: value.to_string(),
            target: expected,
        }),
        _ => Err(column_type(column, expected, value)),
    }
}

fn column_type(column: usize, expected: DataType, value: SqlValueRef<'_>) -> SqliteError {
    SqliteError::ColumnType {
        column,
        expected,
        actual: storage_class(value),
    }
}

fn out_of_range(column: usize, value: i64, target: DataType) -> SqliteError {
    SqliteError::ValueOutOfRange {
        column,
        value: value.to_string(),
        target,
    }
}

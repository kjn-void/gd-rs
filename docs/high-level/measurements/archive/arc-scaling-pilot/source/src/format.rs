//! Serialization of arguments and tables into standard interchange formats.

use std::string::FromUtf8Error;

use ahash::AHashSet;
use compact_str::CompactString;
use thiserror::Error;

use crate::text::push_percent_component;
use crate::{Arguments, RowOrder, Table, TableError, TableSelection, ValueRef, encode_hex};

/// An interchange-format or representability error.
#[derive(Debug, Error)]
pub enum FormatError {
    /// An object or URI pair requires a name for every argument.
    #[error("argument {position} is unnamed and cannot be represented")]
    UnnamedArgument {
        /// Insertion position of the unnamed argument.
        position: usize,
    },
    /// A JSON object cannot preserve duplicate argument names reliably.
    #[error("duplicate argument name cannot be represented as a JSON object: {0}")]
    DuplicateArgumentName(CompactString),
    /// JSON numbers do not admit NaN or infinity.
    #[error("non-finite floating-point value cannot be represented as JSON")]
    NonFiniteFloat,
    /// JSON serialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// CSV writing failed.
    #[error(transparent)]
    Csv(#[from] csv::Error),
    /// A table with no columns has no distinct CSV record shape.
    #[error("a table with no columns cannot be represented as CSV")]
    ZeroColumnTable,
    /// Finalizing the CSV writer failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// CSV output unexpectedly contained invalid UTF-8.
    #[error(transparent)]
    InvalidUtf8(#[from] FromUtf8Error),
    /// The table rejected a row position while formatting.
    #[error(transparent)]
    Table(#[from] TableError),
}

/// Serializes named, uniquely keyed arguments as a JSON object.
///
/// Value variants become natural JSON scalars. Byte values are lowercase hex
/// strings and UUIDs are canonical strings. JSON object member ordering is not
/// part of this contract.
///
/// # Errors
///
/// Returns [`FormatError::UnnamedArgument`] or
/// [`FormatError::DuplicateArgumentName`] rather than silently dropping
/// information. Non-finite floats return [`FormatError::NonFiniteFloat`].
pub fn arguments_to_json(arguments: &Arguments) -> Result<String, FormatError> {
    let mut names = AHashSet::with_capacity(arguments.len());
    let mut output = Vec::new();
    output.push(b'{');
    for (position, argument) in arguments.iter().enumerate() {
        let name = argument
            .name()
            .ok_or(FormatError::UnnamedArgument { position })?;
        if !names.insert(name) {
            return Err(FormatError::DuplicateArgumentName(name.into()));
        }
        if position > 0 {
            output.push(b',');
        }
        serde_json::to_writer(&mut output, name)?;
        output.push(b':');
        write_json_value(&mut output, argument.value_ref())?;
    }
    output.push(b'}');
    Ok(String::from_utf8(output)?)
}

/// Serializes named arguments as URI query pairs without a leading `?`.
///
/// Insertion order and duplicate names are preserved. Names and scalar value
/// text use [`crate::encode_percent_component`]. Null is represented by an empty value.
///
/// # Errors
///
/// Returns [`FormatError::UnnamedArgument`] because a positional value has no
/// lossless query-pair representation.
pub fn arguments_to_uri(arguments: &Arguments) -> Result<String, FormatError> {
    let mut output = String::new();
    for (position, argument) in arguments.iter().enumerate() {
        let name = argument
            .name()
            .ok_or(FormatError::UnnamedArgument { position })?;
        if position > 0 {
            output.push('&');
        }
        push_percent_component(&mut output, name);
        output.push('=');
        write_uri_value(&mut output, argument.value_ref());
    }
    Ok(output)
}

/// Serializes live table rows as a JSON array of objects keyed by primary column
/// name.
///
/// Aliases are lookup conveniences and are not emitted. Byte values are lowercase
/// hex strings and UUIDs are canonical strings. Tombstoned rows are excluded
/// because they are logically deleted, and row-local extras are not serialized
/// because only schema columns are emitted.
///
/// # Errors
///
/// Returns [`FormatError::NonFiniteFloat`] for NaN or infinity, or a JSON writer
/// error.
pub fn table_to_json(table: &Table) -> Result<String, FormatError> {
    rows_to_json(
        table,
        (0..table.row_count()).filter(|&row| !table.row_is_tombstoned(row)),
        None,
    )
}

/// Serializes live rows in a [`RowOrder`] as a JSON array of objects.
///
/// Tombstoned rows are excluded because they are logically deleted.
///
/// # Errors
///
/// Returns [`FormatError::NonFiniteFloat`] for NaN or infinity, or a JSON writer
/// error.
pub fn row_order_to_json(order: &RowOrder<'_>) -> Result<String, FormatError> {
    let table = order.table();
    rows_to_json(
        table,
        order
            .positions()
            .iter()
            .copied()
            .filter(|&row| !table.row_is_tombstoned(row)),
        None,
    )
}

/// Serializes live selected rows and projected columns directly to JSON.
///
/// Source order/duplicates and projected names are retained. Extras and properties
/// are omitted like table JSON output; cells are never materialized into a table.
///
/// # Errors
///
/// Returns a non-finite float or JSON/UTF-8 writer error.
pub fn selection_to_json(selection: &TableSelection<'_>) -> Result<String, FormatError> {
    rows_to_json(
        selection.table(),
        selection.live_rows().map(crate::SelectedRow::position),
        Some(selection.columns()),
    )
}

/// Serializes live selected rows and projected columns directly to CSV.
///
/// Uses the ordinary table CSV rules without first copying cell payloads.
///
/// # Errors
///
/// Returns a zero-column, CSV, I/O or UTF-8 writer error.
pub fn selection_to_csv(
    selection: &TableSelection<'_>,
    headers: bool,
) -> Result<String, FormatError> {
    rows_to_csv(
        selection.schema(),
        selection.live_rows().map(crate::SelectedRow::iter),
        headers,
    )
}

/// Serializes a table's live rows as RFC 4180-style CSV.
///
/// When `headers` is true, primary column names form the first record. Nulls are
/// empty fields. Strings are quoted by the `csv` crate when required; bytes are
/// lowercase hex and UUIDs use canonical text. Tombstoned rows are excluded
/// because they are logically deleted, and row-local extras are not serialized
/// because only schema columns are emitted. A table with no columns is rejected
/// because CSV cannot distinguish an empty record from one empty field.
///
/// # Errors
///
/// Returns [`FormatError::ZeroColumnTable`] for a table with no columns, or a CSV,
/// I/O, or unexpected UTF-8 finalization error.
pub fn table_to_csv(table: &Table, headers: bool) -> Result<String, FormatError> {
    rows_to_csv(
        table.schema(),
        table.live_rows().map(crate::Row::iter),
        headers,
    )
}

fn rows_to_csv<'a>(
    schema: &crate::Schema,
    rows: impl IntoIterator<Item = impl Iterator<Item = ValueRef<'a>>>,
    headers: bool,
) -> Result<String, FormatError> {
    if schema.is_empty() {
        return Err(FormatError::ZeroColumnTable);
    }
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    if headers {
        writer.write_record(schema.iter().map(crate::ColumnSpec::name))?;
    }
    for row in rows {
        for value in row {
            write_csv_value(&mut writer, value)?;
        }
        writer.write_record(None::<&[u8]>)?;
    }
    writer.flush()?;
    let bytes = writer
        .into_inner()
        .map_err(csv::IntoInnerError::into_error)?;
    Ok(String::from_utf8(bytes)?)
}

fn write_csv_value<W: std::io::Write>(
    writer: &mut csv::Writer<W>,
    value: ValueRef<'_>,
) -> Result<(), csv::Error> {
    match value {
        ValueRef::Null => writer.write_field(b""),
        ValueRef::Bool(false) => writer.write_field(b"false"),
        ValueRef::Bool(true) => writer.write_field(b"true"),
        ValueRef::I8(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::I16(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::I32(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::I64(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::U8(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::U16(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::U32(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::U64(value) => writer.write_field(itoa::Buffer::new().format(value)),
        ValueRef::F32(value) => writer.write_field(ryu::Buffer::new().format(value)),
        ValueRef::F64(value) => writer.write_field(ryu::Buffer::new().format(value)),
        ValueRef::String(value) => writer.write_field(value),
        ValueRef::Bytes(value) => writer.write_field(encode_hex(value)),
        ValueRef::Uuid(value) => writer.write_field(value.to_string()),
    }
}

fn rows_to_json(
    table: &Table,
    rows: impl IntoIterator<Item = usize>,
    columns: Option<&[usize]>,
) -> Result<String, FormatError> {
    let mut output = Vec::new();
    output.push(b'[');
    let mut first_row = true;
    for row in rows {
        if !first_row {
            output.push(b',');
        }
        first_row = false;
        output.push(b'{');
        let row = table.row(row).ok_or(TableError::RowOutOfBounds {
            row,
            row_count: table.row_count(),
        })?;
        for column in 0..columns.map_or(table.column_count(), <[usize]>::len) {
            let source_column = columns.map_or(column, |columns| columns[column]);
            let spec =
                table
                    .schema()
                    .column(source_column)
                    .ok_or(TableError::ColumnOutOfBounds {
                        column: source_column,
                        column_count: table.column_count(),
                    })?;
            let value = row
                .get(source_column)
                .ok_or(TableError::InternalInvariant {
                    detail: "JSON output cell is missing",
                })?;
            if column > 0 {
                output.push(b',');
            }
            serde_json::to_writer(&mut output, spec.name())?;
            output.push(b':');
            write_json_value(&mut output, value)?;
        }
        output.push(b'}');
    }
    output.push(b']');
    Ok(String::from_utf8(output)?)
}

fn write_json_value(output: &mut Vec<u8>, value: ValueRef<'_>) -> Result<(), FormatError> {
    match value {
        ValueRef::Null => output.extend_from_slice(b"null"),
        ValueRef::Bool(value) => serde_json::to_writer(output, &value)?,
        ValueRef::I8(value) => serde_json::to_writer(output, &value)?,
        ValueRef::I16(value) => serde_json::to_writer(output, &value)?,
        ValueRef::I32(value) => serde_json::to_writer(output, &value)?,
        ValueRef::I64(value) => serde_json::to_writer(output, &value)?,
        ValueRef::U8(value) => serde_json::to_writer(output, &value)?,
        ValueRef::U16(value) => serde_json::to_writer(output, &value)?,
        ValueRef::U32(value) => serde_json::to_writer(output, &value)?,
        ValueRef::U64(value) => serde_json::to_writer(output, &value)?,
        ValueRef::F32(value) if value.is_finite() => serde_json::to_writer(output, &value)?,
        ValueRef::F64(value) if value.is_finite() => serde_json::to_writer(output, &value)?,
        ValueRef::F32(_) | ValueRef::F64(_) => return Err(FormatError::NonFiniteFloat),
        ValueRef::String(value) => serde_json::to_writer(output, value)?,
        ValueRef::Bytes(value) => serde_json::to_writer(output, &encode_hex(value))?,
        ValueRef::Uuid(value) => serde_json::to_writer(output, &value.to_string())?,
    }
    Ok(())
}

fn write_uri_value(output: &mut String, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => {}
        ValueRef::Bool(false) => output.push_str("false"),
        ValueRef::Bool(true) => output.push_str("true"),
        ValueRef::I8(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::I16(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::I32(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::I64(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::U8(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::U16(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::U32(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::U64(value) => output.push_str(itoa::Buffer::new().format(value)),
        ValueRef::F32(value) => output.push_str(ryu::Buffer::new().format(value)),
        ValueRef::F64(value) => output.push_str(ryu::Buffer::new().format(value)),
        ValueRef::String(value) => push_percent_component(output, value),
        ValueRef::Bytes(value) => output.push_str(&encode_hex(value)),
        ValueRef::Uuid(value) => {
            let mut buffer = uuid::Uuid::encode_buffer();
            output.push_str(value.hyphenated().encode_lower(&mut buffer));
        }
    }
}

//! Human-readable table diagnostics corresponding to GD's table debug helpers.

use crate::{Table, ValueRef, encode_hex};

/// Prints every fixed-schema row in insertion order.
///
/// Cells are separated by `", "`, nulls are written as `"null"`, and every
/// row ends with a newline. Row-local extra fields are not included because
/// they are not schema columns.
#[must_use]
pub fn print(table: &Table) -> String {
    print_rows(table, table.row_count())
}

/// Prints at most `count` fixed-schema rows in insertion order.
///
/// A count larger than [`Table::row_count`] is clamped to the available rows,
/// matching the C++ `print(table, count)` overload.
#[must_use]
pub fn print_rows(table: &Table, count: usize) -> String {
    let mut output = String::new();
    for row in table.rows().take(count) {
        push_row(&mut output, row.iter());
    }
    output
}

/// Prints the fixed-schema column definitions in positional order.
///
/// Each entry contains the column index, primary name, optional alias, logical
/// type, and fixed payload width. Variable-width types use width zero, matching
/// GD's column diagnostic convention. gd-rs does not print a `reference` marker:
/// columns own their storage and [`ValueRef`] represents only a checked borrow.
#[must_use]
pub fn print_column(table: &Table) -> String {
    let mut output = String::new();
    for (position, column) in table.schema().iter().enumerate() {
        if position > 0 {
            output.push(' ');
        }
        output.push_str("[(");
        output.push_str(itoa::Buffer::new().format(position));
        output.push_str(") ");
        output.push_str(column.name());
        if let Some(alias) = column.alias() {
            output.push_str(" (");
            output.push_str(alias);
            output.push(')');
        }
        output.push(',');
        output.push_str(column.data_type().name());
        output.push(',');
        output.push_str(
            itoa::Buffer::new().format(column.data_type().fixed_width().unwrap_or_default()),
        );
        output.push(']');
    }
    output
}

/// Prints one fixed-schema row, or GD's row-limit diagnostic when it is absent.
#[must_use]
pub fn print_row(table: &Table, row: usize) -> String {
    let Some(row) = table.row(row) else {
        let mut output = String::from("Max row is:");
        output.push_str(itoa::Buffer::new().format(table.row_count()));
        output.push('\n');
        return output;
    };

    let mut output = String::new();
    push_row(&mut output, row.iter());
    output
}

fn push_row<'a>(output: &mut String, values: impl IntoIterator<Item = ValueRef<'a>>) {
    for (column, value) in values.into_iter().enumerate() {
        if column > 0 {
            output.push_str(", ");
        }
        push_value(output, value);
    }
    output.push('\n');
}

fn push_value(output: &mut String, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => output.push_str("null"),
        ValueRef::Bool(false) => output.push('0'),
        ValueRef::Bool(true) => output.push('1'),
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
        ValueRef::String(value) => output.push_str(value),
        ValueRef::Bytes(value) => output.push_str(&encode_hex(value)),
        ValueRef::Uuid(value) => output.push_str(&value.to_string()),
    }
}

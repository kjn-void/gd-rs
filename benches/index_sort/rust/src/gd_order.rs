//! Public gd-rs row-order API with German short strings and ordinary long strings.

use super::SourceRow;
use gd::{ColumnSpec, DataType, Schema, Table, Value};

pub fn from_source(rows: &[SourceRow]) -> Table {
    let schema = Schema::new([
        ColumnSpec::new("C1", DataType::U64),
        ColumnSpec::german_string("C2"),
        ColumnSpec::new("C3", DataType::F64),
        ColumnSpec::new("C4", DataType::F64).nullable(true),
        ColumnSpec::new("C5", DataType::I32).nullable(true),
        ColumnSpec::new("C6", DataType::U32).nullable(true),
        ColumnSpec::new("C7", DataType::String),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, rows.len());
    for row in rows {
        table
            .push_row([
                Value::U64(row.c1),
                row.c2.as_str().into(),
                Value::F64(row.c3),
                Value::F64(row.c4),
                Value::I32(row.c5),
                Value::U32(row.c6),
                row.c7.as_str().into(),
            ])
            .unwrap();
    }
    table
}

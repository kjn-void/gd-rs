//! Integration tests for the opt-in German/Umbra string column layout.

#![cfg(feature = "german-strings")]

use compact_str::CompactString;
use gd::{
    ColumnSliceError, ColumnSpec, DataType, GermanStringError, IndexKeyRef, NullOrder, Schema,
    SortDirection, Table, Value, ValueRef, table_from_csv, table_from_json, table_to_csv,
    table_to_json,
};

fn string_schema(nullable: bool) -> Schema {
    Schema::new([ColumnSpec::german_string("text").nullable(nullable)]).unwrap()
}

#[test]
fn german_columns_round_trip_inline_heap_null_and_utf8_values() {
    let mut table = Table::new(string_schema(true));
    let values = [
        "",
        "abc",
        "abcdefghijkl",
        "a longer value",
        "Åsa",
        "😀",
        "line\n2",
        "a\0b",
    ];
    for value in values {
        table.push_row([Value::from(value)]).unwrap();
    }
    table.push_row([Value::Null]).unwrap();

    for (row, value) in values.into_iter().enumerate() {
        assert_eq!(table.cell(row, 0), Ok(ValueRef::String(value)));
    }
    assert_eq!(table.cell(values.len(), 0), Ok(ValueRef::Null));

    let strings = table.column(0).unwrap().german_strings().unwrap();
    assert_eq!(strings.len(), values.len() + 1);
    assert_eq!(strings.get(0), Some(Some("")));
    assert_eq!(strings.get(2), Some(Some("abcdefghijkl")));
    assert_eq!(strings.get(values.len()), Some(None));
    assert_eq!(strings.get(values.len() + 1), None);
    assert_eq!(
        strings.iter().collect::<Vec<_>>(),
        values
            .iter()
            .copied()
            .map(Some)
            .chain([None])
            .collect::<Vec<_>>()
    );

    {
        let (_, [column]) = table.columns_io([], [0]).unwrap();
        let mut strings = column.german_strings_mut().unwrap();
        strings.set(0, Some("changed")).unwrap();
        strings.set(1, Some("abcdefghijklmnop")).unwrap();
        strings.set(2, None).unwrap();
        let row_count = strings.len();
        assert_eq!(
            strings.set(row_count, Some("out of bounds")),
            Err(GermanStringError::RowOutOfBounds {
                row: row_count,
                row_count,
            })
        );
    }
    assert_eq!(table.cell(0, 0), Ok(ValueRef::String("changed")));
    assert_eq!(table.cell(1, 0), Ok(ValueRef::String("abcdefghijklmnop")));
    assert_eq!(table.cell(2, 0), Ok(ValueRef::Null));

    let required = Table::new(string_schema(false));
    assert!(
        required
            .column(0)
            .unwrap()
            .german_strings()
            .unwrap()
            .get(0)
            .is_none()
    );
    let mut required = required;
    required.push_row(["value".into()]).unwrap();
    let (_, [column]) = required.columns_io([], [0]).unwrap();
    let mut strings = column.german_strings_mut().unwrap();
    assert_eq!(strings.set(0, None), Err(GermanStringError::NullNotAllowed));
}

#[test]
fn german_order_and_index_keep_interior_nul_values_distinct() {
    let values = [
        "a",
        "a\0",
        "abc",
        "abc\0",
        "a longer value",
        "a longer value\0",
        "é",
        "😀",
    ];
    let mut table = Table::new(string_schema(false));
    for value in values {
        table.push_row([Value::from(value)]).unwrap();
    }

    let order = table
        .row_order(0, SortDirection::Ascending, NullOrder::Last)
        .unwrap();
    let mut expected: Vec<_> = (0..values.len()).collect();
    expected.sort_by(|&left, &right| values[left].cmp(values[right]));
    assert_eq!(order.positions(), expected);

    let index = table.index(0).unwrap();
    assert_eq!(index.distinct_key_count(), values.len());
    assert_eq!(index.rows(IndexKeyRef::from("a")), &[0]);
    assert_eq!(index.rows(IndexKeyRef::from("a\0")), &[1]);
    assert_eq!(index.rows(IndexKeyRef::from("abc")), &[2]);
    assert_eq!(index.rows(IndexKeyRef::from("abc\0")), &[3]);
}

#[test]
fn german_columns_work_with_row_mut_and_interchange() {
    let mut table = Table::new(string_schema(true));
    table.push_row(["first".into()]).unwrap();
    table.push_row(["a longer value".into()]).unwrap();
    table.row_mut(0).unwrap().set(0, "changed".into()).unwrap();
    assert_eq!(table.cell(0, 0), Ok(ValueRef::String("changed")));

    let json = table_to_json(&table).unwrap();
    let from_json = table_from_json(table.schema_arc(), &json).unwrap();
    assert_eq!(from_json.cell(0, 0), Ok(ValueRef::String("changed")));
    assert_eq!(from_json.cell(1, 0), Ok(ValueRef::String("a longer value")));

    let csv = table_to_csv(&table, true).unwrap();
    let from_csv = table_from_csv(table.schema_arc(), &csv, true).unwrap();
    assert_eq!(from_csv.cell(0, 0), Ok(ValueRef::String("changed")));
    assert_eq!(from_csv.cell(1, 0), Ok(ValueRef::String("a longer value")));
}

#[test]
fn german_columns_copy_append_project_and_compact_independently() {
    let schema = Schema::new([
        ColumnSpec::german_string("text"),
        ColumnSpec::new("id", DataType::U64),
    ])
    .unwrap();
    let mut source = Table::new(schema.clone());
    for (id, text) in ["first", "a longer value", "third"].into_iter().enumerate() {
        source
            .push_row([Value::from(text), Value::U64(id as u64)])
            .unwrap();
    }
    source.tombstone_row(1).unwrap();

    let copied = source.copy_rows(&[2, 1, 0]).unwrap();
    assert_eq!(copied.cell(0, 0), Ok(ValueRef::String("third")));
    assert_eq!(copied.cell(2, 0), Ok(ValueRef::String("first")));
    assert!(copied.is_tombstoned(1).unwrap());

    let projected = source.project(&[0]).unwrap();
    assert_eq!(projected.column_count(), 1);
    assert_eq!(projected.cell(1, 0), Ok(ValueRef::String("a longer value")));

    let mut target = Table::new(schema);
    target.push_row(["target".into(), Value::U64(9)]).unwrap();
    assert_eq!(target.append(&source).unwrap(), 1..4);
    assert_eq!(target.cell(1, 0), Ok(ValueRef::String("first")));
    assert_eq!(target.cell(2, 0), Ok(ValueRef::String("a longer value")));
    assert!(target.is_tombstoned(2).unwrap());

    source.set_cell(0, 0, "changed".into()).unwrap();
    assert_eq!(target.cell(1, 0), Ok(ValueRef::String("first")));

    let compaction = target.compact();
    assert_eq!(compaction.removed_rows(), &[2]);
    assert_eq!(target.row_count(), 3);
    assert_eq!(target.cell(2, 0), Ok(ValueRef::String("third")));
}

#[test]
fn german_storage_coexists_with_other_string_layouts() {
    let schema = Schema::new([
        ColumnSpec::fixed_string("fixed", 16),
        ColumnSpec::german_string("german"),
        ColumnSpec::interned_string("interned"),
        ColumnSpec::new("ordinary", DataType::String),
    ])
    .unwrap();
    let mut source = Table::new(schema.clone());
    source
        .push_row([
            "fixed".into(),
            "german value".into(),
            "shared".into(),
            "ordinary".into(),
        ])
        .unwrap();
    let mut target = Table::new(schema);
    target
        .push_row(["a".into(), "b".into(), "c".into(), "d".into()])
        .unwrap();
    target.append(&source).unwrap();

    assert_eq!(target.cell(1, 0), Ok(ValueRef::String("fixed")));
    assert_eq!(target.cell(1, 1), Ok(ValueRef::String("german value")));
    assert_eq!(target.cell(1, 2), Ok(ValueRef::String("shared")));
    assert_eq!(target.cell(1, 3), Ok(ValueRef::String("ordinary")));
    assert!(target.column(1).unwrap().german_strings().is_some());
    assert!(target.column(0).unwrap().german_strings().is_none());
    assert_eq!(
        target.column(1).unwrap().as_slice::<CompactString>(),
        Err(ColumnSliceError::FixedStringBuffer)
    );
}

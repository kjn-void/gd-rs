//! Public selection, projection, and join contracts.
use gd::{ColumnSpec, DataType, Schema, Table, TableError, UnknownFields, Value, ValueRef};
use proptest::prelude::*;

fn keys(values: &[Option<i64>]) -> Table {
    let schema = Schema::new([ColumnSpec::new("key", DataType::I64).nullable(true)]).unwrap();
    let mut table = Table::new(schema);
    for value in values {
        table
            .push_row([value.map_or(Value::Null, Value::I64)])
            .unwrap();
    }
    table
}

#[test]
fn join_preserves_duplicates_order_nulls_and_live_rows() {
    let mut left = keys(&[Some(3), Some(1), None, Some(2), Some(4)]);
    let mut right = keys(&[Some(1), None, Some(3), Some(1), Some(4)]);
    left.tombstone_row(4).unwrap();
    right.tombstone_row(2).unwrap();
    assert_eq!(
        left.left_join_rows(0, &right.index(0).unwrap()).unwrap(),
        [(0, None), (1, Some(0)), (1, Some(3)), (2, None), (3, None)]
    );
    assert_eq!(
        left.left_join_rows(0, &keys(&[]).index(0).unwrap())
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        keys(&[])
            .left_join_rows(0, &right.index(0).unwrap())
            .unwrap(),
        [] as [(usize, Option<usize>); 0]
    );
    assert!(matches!(
        left.left_join_rows(1, &right.index(0).unwrap()),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    let strings = Table::new(Schema::new([ColumnSpec::new("key", DataType::String)]).unwrap());
    assert!(matches!(
        left.left_join_rows(0, &strings.index(0).unwrap()),
        Err(TableError::TypeMismatch { .. })
    ));
}

#[test]
fn selection_owns_values_and_preserves_metadata() {
    let schema = Schema::new([
        ColumnSpec::new("id", DataType::I64),
        ColumnSpec::new("name", DataType::String)
            .nullable(true)
            .with_alias("label"),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let mut table = Table::new(schema);
    table.push_row([Value::I64(1), Value::from("Ada")]).unwrap();
    table.push_row([Value::I64(2), Value::Null]).unwrap();
    table.push_row([Value::I64(3), Value::from("Åsa")]).unwrap();
    table
        .set_named(0, "extra", Value::from("retained"))
        .unwrap();
    table.set_property("source", Value::from("fixture"));
    table.tombstone_row(1).unwrap();
    let mut calls = 0;
    let filtered = table
        .filter_rows(|row| {
            calls += 1;
            row.get(0) == Some(ValueRef::I64(1))
        })
        .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(filtered.row_count(), 1);
    assert_eq!(
        filtered.cell_named(0, "extra").unwrap(),
        ValueRef::String("retained")
    );
    assert_eq!(filtered.properties(), table.properties());
    assert!(std::sync::Arc::ptr_eq(
        &table.schema_arc(),
        &filtered.schema_arc()
    ));
    let mut projected = table.select(&[2, 0, 2, 1], &[1, 0]).unwrap();
    assert_eq!(projected.schema().column_index("label"), Some(0));
    assert_eq!(projected.live_row_count(), 3);
    assert!(projected.row(3).unwrap().is_tombstoned());
    projected.set_cell(1, 0, Value::from("changed")).unwrap();
    assert_eq!(table.cell(0, 1).unwrap(), ValueRef::String("Ada"));
    assert_eq!(projected.cell(0, 0).unwrap(), ValueRef::String("Åsa"));
    assert_eq!(table.project(&[]).unwrap().row_count(), 3);
    assert_eq!(table.project(&[1]).unwrap().live_row_count(), 2);
    assert!(table.filter_rows(|_| false).unwrap().is_empty());
    assert!(matches!(
        table.project(&[0, 0]),
        Err(TableError::DuplicateColumnName(_))
    ));
    assert!(matches!(
        table.project(&[2]),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    assert!(matches!(
        table.select(&[3], &[0]),
        Err(TableError::RowOutOfBounds { .. })
    ));
}

proptest! {
    #[test]
    fn indexed_join_agrees_with_nested_loop(
        left_values in prop::collection::vec(prop::option::of(-5_i64..5), 0..40),
        right_values in prop::collection::vec(prop::option::of(-5_i64..5), 0..40),
    ) {
        let left = keys(&left_values);
        let right = keys(&right_values);
        let mut expected = Vec::new();
        for (l, key) in left_values.iter().enumerate() {
            let before = expected.len();
            for (r, other) in right_values.iter().enumerate() {
                if key.is_some() && key == other { expected.push((l, Some(r))); }
            }
            if expected.len() == before { expected.push((l, None)); }
        }
        prop_assert_eq!(left.left_join_rows(0, &right.index(0).unwrap()).unwrap(), expected);
    }
}

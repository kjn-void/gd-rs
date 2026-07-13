//! Integration and property tests for schemas, typed columns, and column indexes.

use gd::{
    ColumnSpec, DataType, IndexKeyRef, NullOrder, Schema, SortDirection, Table, TableError, Value,
    ValueRef,
};
use proptest::prelude::*;

fn people_schema() -> Schema {
    Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("name", DataType::String).with_alias("display_name"),
        ColumnSpec::new("score", DataType::I32).nullable(true),
    ])
    .unwrap()
}

#[test]
fn preserves_schema_rows_names_aliases_and_nulls() {
    let mut table = Table::new(people_schema());
    table
        .push_row([Value::from(7_u64), Value::from("Ada"), Value::from(42_i32)])
        .unwrap();
    table
        .push_row([Value::from(8_u64), Value::from("Grace"), Value::Null])
        .unwrap();

    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 3);
    assert_eq!(
        table.cell_named(0, "display_name"),
        Ok(ValueRef::String("Ada"))
    );
    assert_eq!(table.cell_named(1, "score"), Ok(ValueRef::Null));
    assert_eq!(
        table.row(0).unwrap().iter().collect::<Vec<_>>(),
        vec![ValueRef::U64(7), ValueRef::String("Ada"), ValueRef::I32(42)]
    );
}

#[test]
fn rejects_invalid_rows_atomically() {
    let mut table = Table::new(people_schema());

    assert_eq!(
        table.push_row([Value::from(7_u64)]),
        Err(TableError::RowWidth {
            expected: 3,
            actual: 1,
        })
    );
    assert_eq!(
        table.push_row([Value::from(7_i64), Value::from("Ada"), Value::from(1_i32)]),
        Err(TableError::TypeMismatch {
            column: 0,
            expected: DataType::U64,
            actual: DataType::I64,
        })
    );
    assert_eq!(
        table.push_row([Value::Null, Value::from("Ada"), Value::from(1_i32)]),
        Err(TableError::NullNotAllowed { column: 0 })
    );
    assert_eq!(table.row_count(), 0);
    assert!(table.column(0).unwrap().is_empty());
}

#[test]
fn rejects_duplicate_names_and_aliases() {
    let error = Schema::new([
        ColumnSpec::new("id", DataType::U64).with_alias("key"),
        ColumnSpec::new("key", DataType::String),
    ])
    .unwrap_err();
    assert_eq!(error, TableError::DuplicateColumnName("key".into()));
}

#[test]
fn column_index_preserves_duplicate_and_null_rows() {
    let mut table = Table::new(people_schema());
    for (id, name, score) in [
        (1, "Ada", Value::I32(10)),
        (2, "Grace", Value::Null),
        (3, "Ada", Value::I32(20)),
    ] {
        table
            .push_row([Value::U64(id), Value::from(name), score])
            .unwrap();
    }

    let names = table.index(1).unwrap();
    assert_eq!(names.rows(IndexKeyRef::from("Ada")), &[0, 2]);
    assert_eq!(names.rows(IndexKeyRef::from("missing")), &[] as &[usize]);

    let scores = table.index(2).unwrap();
    assert_eq!(scores.rows(IndexKeyRef::from(20_i32)), &[2]);
    assert_eq!(scores.null_rows(), &[1]);
}

#[test]
fn row_order_is_stable_and_places_nulls_explicitly() {
    let mut table = Table::new(people_schema());
    for (id, name, score) in [
        (1, "first ten", Value::I32(10)),
        (2, "null", Value::Null),
        (3, "twenty", Value::I32(20)),
        (4, "second ten", Value::I32(10)),
    ] {
        table
            .push_row([Value::U64(id), Value::from(name), score])
            .unwrap();
    }

    let ascending = table
        .row_order_named("score", SortDirection::Ascending, NullOrder::Last)
        .unwrap();
    assert_eq!(ascending.positions(), &[0, 3, 2, 1]);
    assert_eq!(
        ascending
            .rows()
            .map(|row| row.get_named("name").unwrap())
            .collect::<Vec<_>>(),
        [
            ValueRef::String("first ten"),
            ValueRef::String("second ten"),
            ValueRef::String("twenty"),
            ValueRef::String("null")
        ]
    );

    let descending = table
        .row_order(2, SortDirection::Descending, NullOrder::First)
        .unwrap();
    assert_eq!(descending.positions(), &[1, 2, 0, 3]);
}

#[test]
fn floating_order_uses_total_cmp() {
    let schema = Schema::new([ColumnSpec::new("value", DataType::F64).nullable(true)]).unwrap();
    let mut table = Table::new(schema);
    for value in [
        Value::F64(f64::NAN),
        Value::F64(-0.0),
        Value::F64(0.0),
        Value::Null,
    ] {
        table.push_row([value]).unwrap();
    }
    let order = table
        .row_order(0, SortDirection::Ascending, NullOrder::Last)
        .unwrap();
    assert_eq!(order.positions(), &[1, 2, 0, 3]);
}

proptest! {
    #[test]
    fn typed_column_round_trips(values in prop::collection::vec(any::<i64>(), 0..512)) {
        let schema = Schema::new([ColumnSpec::new("value", DataType::I64)]).unwrap();
        let mut table = Table::with_capacity(schema, values.len());
        for value in &values {
            table.push_row([Value::I64(*value)]).unwrap();
        }

        let actual: Vec<_> = table
            .column(0)
            .unwrap()
            .iter()
            .map(|value| value.to_i64().unwrap())
            .collect();
        prop_assert_eq!(actual, values);
    }

    #[test]
    fn row_order_matches_stable_slice_sort(values in prop::collection::vec(any::<i64>(), 0..512)) {
        let schema = Schema::new([ColumnSpec::new("value", DataType::I64)]).unwrap();
        let mut table = Table::with_capacity(schema, values.len());
        for value in &values {
            table.push_row([Value::I64(*value)]).unwrap();
        }
        let order = table
            .row_order(0, SortDirection::Ascending, NullOrder::Last)
            .unwrap();
        let mut expected: Vec<_> = (0..values.len()).collect();
        expected.sort_by_key(|position| values[*position]);
        prop_assert_eq!(order.positions(), expected);
    }
}

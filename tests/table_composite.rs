//! Composite key indexes and lazy join contracts.

use gd::{ColumnSpec, DataType, IndexKeyRef, Row, Schema, Table, TableError, Value};
use proptest::prelude::*;
use uuid::Uuid;

fn pairs(values: &[(Option<i64>, Option<i64>, bool)]) -> Table {
    let mut table = Table::new(
        Schema::new([
            ColumnSpec::new("a", DataType::I64).nullable(true),
            ColumnSpec::new("b", DataType::I64).nullable(true),
        ])
        .unwrap(),
    );
    for &(a, b, deleted) in values {
        let row = table
            .push_row([
                a.map_or(Value::Null, Value::I64),
                b.map_or(Value::Null, Value::I64),
            ])
            .unwrap();
        if deleted {
            table.tombstone_row(row).unwrap();
        }
    }
    table
}

#[test]
fn composite_index_preserves_duplicates_and_separates_null_and_deleted_rows() {
    let table = pairs(&[
        (Some(1), Some(2), false),
        (Some(1), Some(3), false),
        (Some(1), Some(2), false),
        (None, Some(2), false),
        (Some(1), None, false),
        (None, None, true),
        (Some(1), Some(2), true),
    ]);
    let index = table.composite_index([0, 1]).unwrap();
    assert_eq!(index.columns(), &[0, 1]);
    assert!(std::ptr::eq(index.table(), std::ptr::from_ref(&table)));
    assert_eq!(index.rows([1_i64.into(), 2_i64.into()]), &[0, 2]);
    assert_eq!(index.rows([1_i64.into(), 4_i64.into()]), [] as [usize; 0]);
    assert_eq!(index.rows([1_u64.into(), 2_i64.into()]), [] as [usize; 0]);
    assert_eq!(index.distinct_key_count(), 2);
    assert_eq!(index.null_rows(), &[3, 4]);
    assert_eq!(index.tombstoned_rows(), &[5, 6]);
    let repeated = table.composite_index([0, 0]).unwrap();
    assert_eq!(repeated.rows([1_i64.into(), 1_i64.into()]), &[0, 1, 2, 4]);
    let one = table.composite_index([0]).unwrap();
    assert_eq!(
        one.rows([1_i64.into()]),
        table.index(0).unwrap().rows(1_i64.into())
    );
}

#[test]
fn mixed_keys_borrow_payloads_and_lookup_results_outlive_temporary_query_text() {
    let uuid = Uuid::from_u128(3);
    let values = [
        Value::from("customer"),
        Value::I8(-7),
        Value::I16(-7),
        Value::I32(-7),
        Value::I64(-7),
        Value::U8(7),
        Value::U16(7),
        Value::U32(7),
        Value::U64(u64::MAX),
        Value::Bool(true),
        Value::from(vec![1_u8, 2]),
        Value::from(uuid),
    ];
    let mut table = Table::new(
        Schema::new(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| ColumnSpec::new(format!("c{i}"), v.data_type())),
        )
        .unwrap(),
    );
    table.push_row(values.clone()).unwrap();
    let index = table
        .composite_index([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11])
        .unwrap();
    let matches;
    {
        let text = String::from("customer");
        let bytes = vec![1, 2];
        matches = index.rows([
            text.as_str().into(),
            (-7_i8).into(),
            (-7_i16).into(),
            (-7_i32).into(),
            (-7_i64).into(),
            7_u8.into(),
            7_u16.into(),
            7_u32.into(),
            u64::MAX.into(),
            true.into(),
            bytes.as_slice().into(),
            uuid.into(),
        ]);
    }
    assert_eq!(matches, &[0]);
}

#[test]
fn joins_expand_duplicates_keep_unmatched_rows_and_offer_lazy_payload_access() {
    let left = pairs(&[
        (Some(1), Some(2), false),
        (Some(1), Some(2), true),
        (None, Some(2), false),
        (Some(3), Some(2), false),
    ]);
    let right = pairs(&[
        (Some(1), Some(2), false),
        (Some(1), Some(2), false),
        (Some(1), Some(2), true),
        (None, Some(2), false),
    ]);
    let index = right.composite_index([0, 1]).unwrap();
    let expected = vec![(0, Some(0)), (0, Some(1)), (2, None), (3, None)];
    assert_eq!(
        left.left_join_rows_composite([0, 1], &index).unwrap(),
        expected
    );
    let lazy: Vec<_> = left
        .left_join_composite([0, 1], &index)
        .unwrap()
        .map(|(l, r)| (l.position(), r.map(Row::position)))
        .collect();
    assert_eq!(lazy, expected);
    assert_eq!(
        left.left_join_composite([0, 1], &index)
            .unwrap()
            .take(1)
            .count(),
        1
    );
    let single = right.index(0).unwrap();
    let lazy: Vec<_> = left
        .left_join(0, &single)
        .unwrap()
        .map(|(l, r)| (l.position(), r.map(Row::position)))
        .collect();
    assert_eq!(lazy, left.left_join_rows(0, &single).unwrap());
    let empty = pairs(&[]);
    assert_eq!(
        left.left_join_rows_composite([0, 1], &empty.composite_index([0, 1]).unwrap())
            .unwrap(),
        vec![(0, None), (2, None), (3, None)]
    );
}

#[test]
fn keys_and_join_components_are_validated_even_without_rows() {
    let empty = pairs(&[]);
    assert!(matches!(
        empty.composite_index([]),
        Err(TableError::EmptyIndexKey)
    ));
    assert!(matches!(
        empty.composite_index([0, 2]),
        Err(TableError::ColumnOutOfBounds { column: 2, .. })
    ));
    for data_type in [DataType::Null, DataType::F32, DataType::F64] {
        let table = Table::new(Schema::new([ColumnSpec::new("a", data_type)]).unwrap());
        assert!(
            matches!(table.composite_index([0]),Err(TableError::UnsupportedIndexType(t)) if t == data_type)
        );
    }
    let narrow = Table::new(
        Schema::new([
            ColumnSpec::new("a", DataType::I32),
            ColumnSpec::new("b", DataType::I64),
        ])
        .unwrap(),
    );
    let index = empty.composite_index([0, 1]).unwrap();
    assert!(matches!(
        narrow.left_join_rows_composite([0, 1], &index),
        Err(TableError::TypeMismatch {
            column: 0,
            expected: DataType::I64,
            actual: DataType::I32
        })
    ));
    assert!(matches!(
        empty.left_join_rows_composite([2, 1], &index),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    assert_eq!(
        index.rows([IndexKeyRef::String("1"), 2_i64.into()]),
        [] as [usize; 0]
    );
}

proptest! {
    #[test]
    fn composite_join_matches_nested_loop_model(
        l in prop::collection::vec((prop::option::of(-3_i64..3),prop::option::of(-3_i64..3),any::<bool>()),0..35),
        r in prop::collection::vec((prop::option::of(-3_i64..3),prop::option::of(-3_i64..3),any::<bool>()),0..35)
    ) {
        let left = pairs(&l); let right = pairs(&r);
        let index = right.composite_index([0,1]).unwrap();
        let mut expected = Vec::new();
        for (position,&(a,b,deleted)) in l.iter().enumerate() {
            if deleted { continue; }
            let before = expected.len();
            for (other,&(x,y,deleted)) in r.iter().enumerate() {
                if !deleted && a.is_some() && b.is_some() && (a,b) == (x,y) { expected.push((position,Some(other))); }
            }
            if expected.len() == before { expected.push((position,None)); }
        }
        prop_assert_eq!(left.left_join_rows_composite([0,1],&index).unwrap(),expected);
    }
}

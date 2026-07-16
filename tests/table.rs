//! Integration and property tests for schemas, typed columns, and column indexes.

use gd::{
    ColumnSliceError, ColumnSpec, DataType, IndexKeyRef, NullOrder, Schema, SortDirection, Table,
    TableError, UnknownFields, Value, ValueRef,
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
fn unknown_row_fields_are_an_explicit_schema_policy() {
    let strict_schema = Schema::new([
        ColumnSpec::new("path", DataType::String),
        ColumnSpec::new("name", DataType::String),
        ColumnSpec::new("size", DataType::U64),
    ])
    .unwrap();
    let mut strict = Table::new(strict_schema.clone());
    let strict_row = strict
        .push_row([
            Value::from("C:\\test\\file.txt"),
            Value::from("file.txt"),
            Value::U64(12_345),
        ])
        .unwrap();
    assert_eq!(
        strict.set_named(strict_row, "path2", "C:\\test\\file2.txt"),
        Err(TableError::ColumnNotFound("path2".into()))
    );

    let open_schema = strict_schema.with_unknown_fields(UnknownFields::Store);
    assert_eq!(open_schema.unknown_fields(), UnknownFields::Store);
    let mut table = Table::new(open_schema);
    let row = table
        .push_row([
            Value::from("C:\\test\\file.txt"),
            Value::from("file.txt"),
            Value::U64(12_345),
        ])
        .unwrap();
    table
        .set_named(row, "path2", "C:\\test\\file2.txt")
        .unwrap();

    assert_eq!(
        table.cell_named(row, "path"),
        Ok(ValueRef::String("C:\\test\\file.txt"))
    );
    assert_eq!(
        table.cell_named(row, "path2"),
        Ok(ValueRef::String("C:\\test\\file2.txt"))
    );
    table
        .set_named(row, "path2", "C:\\test\\updated.txt")
        .unwrap();
    assert_eq!(
        table.row(row).unwrap().get_named("path2"),
        Some(ValueRef::String("C:\\test\\updated.txt"))
    );
    table.set_named(row, "size", 54_u64).unwrap();
    assert_eq!(table.cell_named(row, "size"), Ok(ValueRef::U64(54)));
    assert_eq!(
        table.set_named(row, "size", 54_i64),
        Err(TableError::TypeMismatch {
            column: 2,
            expected: DataType::U64,
            actual: DataType::I64,
        })
    );
    assert_eq!(
        table.push_row_with_extras(
            [
                Value::from("C:\\test\\other.txt"),
                Value::from("other.txt"),
                Value::U64(10),
            ],
            [("path", "not-an-extra")],
        ),
        Err(TableError::ExtraFieldConflictsWithColumn("path".into()))
    );
    assert_eq!(table.row_count(), 1);
    assert!(table.pop_row());
    assert!(table.is_empty());
}

fn custom_file_field_workload_uses_open_schema() {
    let schema = Schema::new([
        ColumnSpec::new("path", DataType::String),
        ColumnSpec::new("name", DataType::String),
        ColumnSpec::new("size", DataType::U64),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let mut files = Table::new(schema);

    for row_index in 0..100_usize {
        files
            .push_row_with_extras(
                [
                    Value::from("C:\\data\\files\\entry.bin"),
                    Value::from("entry.bin"),
                    Value::U64(1_000 + row_index as u64),
                ],
                [
                    (
                        "custom_file_category",
                        Value::from(if row_index % 2 == 0 { "binary" } else { "text" }),
                    ),
                    (
                        "custom_file_region",
                        Value::from(if row_index % 3 == 0 { "north" } else { "south" }),
                    ),
                ],
            )
            .unwrap();
    }

    for row_index in 0..100_usize {
        assert_eq!(
            files.cell_named(row_index, "custom_file_category"),
            Ok(ValueRef::String(if row_index % 2 == 0 {
                "binary"
            } else {
                "text"
            }))
        );
        assert_eq!(
            files.cell_named(row_index, "custom_file_region"),
            Ok(ValueRef::String(if row_index % 3 == 0 {
                "north"
            } else {
                "south"
            }))
        );
    }
}

fn custom_user_field_workload_uses_open_schema() {
    let schema = Schema::new([
        ColumnSpec::new("user_id", DataType::U64),
        ColumnSpec::new("username", DataType::String),
        ColumnSpec::new("email", DataType::String),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let mut users = Table::new(schema);

    for row_index in 0..100_usize {
        users
            .push_row_with_extras(
                [
                    Value::U64(10_000 + row_index as u64),
                    Value::from("demo-user"),
                    Value::from("demo@example.com"),
                ],
                [
                    (
                        "custom_user_tier",
                        Value::from(if row_index % 4 == 0 {
                            "gold"
                        } else {
                            "standard"
                        }),
                    ),
                    (
                        "custom_user_channel",
                        Value::from(if row_index % 5 == 0 {
                            "partner"
                        } else {
                            "direct"
                        }),
                    ),
                ],
            )
            .unwrap();
    }

    for row_index in 0..100_usize {
        assert_eq!(
            users.cell_named(row_index, "custom_user_tier"),
            Ok(ValueRef::String(if row_index % 4 == 0 {
                "gold"
            } else {
                "standard"
            }))
        );
        assert_eq!(
            users.cell_named(row_index, "custom_user_channel"),
            Ok(ValueRef::String(if row_index % 5 == 0 {
                "partner"
            } else {
                "direct"
            }))
        );
    }
}

fn custom_metric_field_workload_uses_open_schema() {
    let schema = Schema::new([
        ColumnSpec::new("service", DataType::String),
        ColumnSpec::new("epoch", DataType::U64),
        ColumnSpec::new("requests", DataType::U64),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let mut metrics = Table::new(schema);

    for row_index in 0..100_usize {
        metrics
            .push_row_with_extras(
                [
                    Value::from("api-service"),
                    Value::U64(1_700_000_000 + row_index as u64),
                    Value::U64(500 + row_index as u64),
                ],
                [
                    (
                        "custom_metric_bucket",
                        Value::from(if row_index % 10 < 5 { "low" } else { "high" }),
                    ),
                    (
                        "custom_metric_window",
                        Value::from(if row_index % 2 == 0 { "day" } else { "night" }),
                    ),
                ],
            )
            .unwrap();
    }

    for row_index in 0..100_usize {
        assert_eq!(
            metrics.cell_named(row_index, "custom_metric_bucket"),
            Ok(ValueRef::String(if row_index % 10 < 5 {
                "low"
            } else {
                "high"
            }))
        );
        assert_eq!(
            metrics.cell_named(row_index, "custom_metric_window"),
            Ok(ValueRef::String(if row_index % 2 == 0 {
                "day"
            } else {
                "night"
            }))
        );
    }
}

#[test]
fn custom_row_field_workloads_use_open_schemas() {
    custom_file_field_workload_uses_open_schema();
    custom_user_field_workload_uses_open_schema();
    custom_metric_field_workload_uses_open_schema();
}

#[test]
fn open_schema_promotes_many_fields_without_changing_lookup_or_update() {
    let schema = Schema::new([ColumnSpec::new("id", DataType::U64)])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store);
    let mut table = Table::new(schema);
    let row = table.push_row([Value::U64(1)]).unwrap();

    for field in 0..100_u64 {
        table
            .set_named(row, &format!("field_{field:03}"), field)
            .unwrap();
    }
    for field in 0..100_u64 {
        assert_eq!(
            table.cell_named(row, &format!("field_{field:03}")),
            Ok(ValueRef::U64(field))
        );
    }

    table.set_named(row, "field_050", 500_u64).unwrap();
    assert_eq!(table.cell_named(row, "field_050"), Ok(ValueRef::U64(500)));
    assert_eq!(table.cell_named(row, "id"), Ok(ValueRef::U64(1)));
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

#[test]
fn required_fixed_width_columns_expose_typed_slices() {
    let mut table = Table::new(people_schema());
    table
        .push_row([Value::U64(7), Value::from("Ada"), Value::I32(42)])
        .unwrap();
    table
        .push_row([Value::U64(8), Value::from("Grace"), Value::Null])
        .unwrap();

    assert_eq!(
        table.column_named("id").unwrap().as_slice::<u64>(),
        Ok(&[7, 8][..])
    );
    assert_eq!(
        table.column_named("id").unwrap().as_slice::<i64>(),
        Err(ColumnSliceError::TypeMismatch {
            expected: DataType::I64,
            actual: DataType::U64,
        })
    );
    assert_eq!(
        table.column_named("score").unwrap().as_slice::<i32>(),
        Err(ColumnSliceError::Nullable {
            data_type: DataType::I32,
        })
    );
}

#[test]
fn distinct_required_columns_support_typed_bulk_transforms() {
    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U32),
    ])
    .unwrap();
    let mut table = Table::new(schema);
    table.push_row([Value::U32(7), Value::U32(0)]).unwrap();
    table.push_row([Value::U32(11), Value::U32(0)]).unwrap();

    let (source, target) = table.column_pair_mut(0, 1).unwrap();
    let source = source.as_slice::<u32>().unwrap();
    let target = target.as_mut_slice::<u32>().unwrap();
    for (source, target) in source.iter().zip(target) {
        *target = source * 2;
    }

    assert_eq!(
        table.column_named("result").unwrap().as_slice::<u32>(),
        Ok(&[14, 22][..])
    );

    let (source, target) = table.column_pair_mut(1, 0).unwrap();
    let source = source.as_slice::<u32>().unwrap();
    let target = target.as_mut_slice::<u32>().unwrap();
    for (source, target) in source.iter().zip(target) {
        *target = source + 1;
    }
    assert_eq!(
        table.column_named("arg").unwrap().as_slice::<u32>(),
        Ok(&[15, 23][..])
    );

    let (_, target) = table.column_pair_mut(0, 1).unwrap();
    assert_eq!(
        target.as_mut_slice::<i32>(),
        Err(ColumnSliceError::TypeMismatch {
            expected: DataType::I32,
            actual: DataType::U32,
        })
    );
    assert!(table.column_pair_mut(0, 0).is_none());
    assert!(table.column_pair_mut(0, 2).is_none());
}

#[test]
fn for_each_value_matches_column_iteration() {
    let mut table = Table::new(people_schema());
    table
        .push_row([Value::U64(7), Value::from("Ada"), Value::I32(42)])
        .unwrap();
    table
        .push_row([Value::U64(8), Value::from("Grace"), Value::Null])
        .unwrap();

    for column in 0..table.column_count() {
        let column = table.column(column).unwrap();
        let expected = column.iter().collect::<Vec<_>>();
        let mut visited = Vec::new();
        column.for_each_value(|value| visited.push(value));
        assert_eq!(visited, expected);
    }
}

#[test]
fn typed_slices_use_standard_map_filter_and_fold_operations() {
    let schema = Schema::new([ColumnSpec::new("count", DataType::I64)]).unwrap();
    let mut table = Table::new(schema);
    for value in [1_i64, 10, 20, 5] {
        table.push_row([Value::I64(value)]).unwrap();
    }

    let values = table
        .column_named("count")
        .unwrap()
        .as_slice::<i64>()
        .unwrap();
    assert_eq!(
        values.iter().map(|value| value * 2).collect::<Vec<_>>(),
        [2, 20, 40, 10]
    );
    assert_eq!(
        values
            .iter()
            .copied()
            .filter(|value| *value >= 10)
            .collect::<Vec<_>>(),
        [10, 20]
    );
    assert_eq!(
        values
            .iter()
            .enumerate()
            .filter_map(|(row, value)| (*value >= 10).then_some(row))
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(values.iter().copied().fold(0_i64, i64::saturating_add), 36);
}

proptest! {
    #[test]
    fn typed_column_round_trips(values in prop::collection::vec(any::<i64>(), 0..512)) {
        let schema = Schema::new([ColumnSpec::new("value", DataType::I64)]).unwrap();
        let mut table = Table::with_capacity(schema, values.len());
        for value in &values {
            table.push_row([Value::I64(*value)]).unwrap();
        }

        prop_assert_eq!(
            table.column(0).unwrap().as_slice::<i64>().unwrap(),
            values.as_slice()
        );
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

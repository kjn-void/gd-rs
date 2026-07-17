//! Integration and property tests for schemas, typed columns, and column indexes.

use gd::{
    ColumnSelectionError, ColumnSliceError, ColumnSpec, ConcurrentTableBuilder, DataType,
    IndexKeyRef, NullOrder, Schema, SortDirection, Table, TableError, UnknownFields, Value,
    ValueRef, table_debug,
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
fn table_debug_helpers_match_gd_row_and_column_diagnostics() {
    let mut table = Table::new(people_schema().with_unknown_fields(UnknownFields::Store));
    table
        .push_row_with_extras(
            [Value::U64(7), Value::from("Ada"), Value::I32(42)],
            [("language", Value::from("COBOL"))],
        )
        .unwrap();
    table
        .push_row([Value::U64(8), Value::from("Grace"), Value::Null])
        .unwrap();

    assert_eq!(table_debug::print(&table), "7, Ada, 42\n8, Grace, null\n");
    assert_eq!(table_debug::print_rows(&table, 1), "7, Ada, 42\n");
    assert_eq!(
        table_debug::print_rows(&table, usize::MAX),
        table_debug::print(&table)
    );
    assert_eq!(table_debug::print_row(&table, 1), "8, Grace, null\n");
    assert_eq!(table_debug::print_row(&table, 2), "Max row is:2\n");
    assert_eq!(
        table_debug::print_column(&table),
        "[(0) id,u64,8] [(1) name (display_name),string,0] [(2) score,i32,4]"
    );
}

#[test]
fn table_debug_formats_dynamic_scalar_variants() {
    let schema = Schema::new([
        ColumnSpec::new("flag", DataType::Bool),
        ColumnSpec::new("ratio", DataType::F64),
        ColumnSpec::new("payload", DataType::Bytes),
    ])
    .unwrap();
    let mut table = Table::new(schema);
    table
        .push_row([
            Value::Bool(true),
            Value::F64(1.25),
            Value::from(&b"\x00\xff"[..]),
        ])
        .unwrap();

    assert_eq!(table_debug::print(&table), "1, 1.25, 00ff\n");
}

#[test]
fn concurrent_builder_publishes_complete_rows_and_freezes_to_typed_columns() {
    const THREADS: usize = 8;
    const ROWS_PER_THREAD: usize = 500;

    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U64),
    ])
    .unwrap();
    let builder = ConcurrentTableBuilder::new(schema);

    std::thread::scope(|scope| {
        for thread in 0..THREADS {
            let builder = &builder;
            scope.spawn(move || {
                for offset in 0..ROWS_PER_THREAD {
                    let arg = u32::try_from(thread * ROWS_PER_THREAD + offset).unwrap();
                    builder
                        .push_row([Value::U32(arg), Value::U64(u64::from(arg) * 3)])
                        .unwrap();
                }
            });
        }
    });

    assert_eq!(builder.row_count(), THREADS * ROWS_PER_THREAD);
    let table = builder.into_table();
    let args = table.column(0).unwrap().as_slice::<u32>().unwrap();
    let results = table.column(1).unwrap().as_slice::<u64>().unwrap();
    let mut seen = vec![false; THREADS * ROWS_PER_THREAD];
    for (&arg, &result) in args.iter().zip(results) {
        assert_eq!(result, u64::from(arg) * 3);
        seen[arg as usize] = true;
    }
    assert!(seen.into_iter().all(|value| value));
}

#[test]
fn concurrent_builder_validates_batches_atomically_and_preserves_extras() {
    let schema = people_schema().with_unknown_fields(UnknownFields::Store);
    let builder = ConcurrentTableBuilder::new(schema);

    let range = builder
        .extend_rows([
            [Value::U64(1), Value::from("Ada"), Value::I32(10)],
            [Value::U64(2), Value::from("Grace"), Value::Null],
        ])
        .unwrap();
    assert_eq!(range, 0..2);

    let error = builder
        .extend_rows([
            [Value::U64(3), Value::from("Linus"), Value::I32(30)],
            [Value::I64(4), Value::from("Edsger"), Value::I32(40)],
        ])
        .unwrap_err();
    assert_eq!(
        error,
        TableError::TypeMismatch {
            column: 0,
            expected: DataType::U64,
            actual: DataType::I64,
        }
    );
    assert_eq!(builder.row_count(), 2);

    let extra_row = builder
        .push_row_with_extras(
            [Value::U64(3), Value::from("Margaret"), Value::I32(30)],
            [("language", Value::from("COBOL"))],
        )
        .unwrap();
    assert_eq!(extra_row, 2);

    let table = builder.into_table();
    assert_eq!(table.row_count(), 3);
    assert_eq!(
        table.cell_named(extra_row, "language"),
        Ok(ValueRef::String("COBOL"))
    );
}

#[test]
fn concurrent_builder_appends_to_compatible_existing_table() {
    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U64),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let mut table = Table::new(schema);
    table.push_row([Value::U32(1), Value::U64(1)]).unwrap();

    // Use a separately constructed but structurally equal schema.
    let builder_schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U64),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let builder = ConcurrentTableBuilder::new(builder_schema);
    builder.push_row([Value::U32(2), Value::U64(4)]).unwrap();
    builder
        .push_row_with_extras(
            [Value::U32(3), Value::U64(9)],
            [("label", Value::from("nine"))],
        )
        .unwrap();

    assert_eq!(builder.append_to(&mut table), Ok(1..3));
    assert_eq!(
        table.column(0).unwrap().as_slice::<u32>().unwrap(),
        &[1, 2, 3]
    );
    assert_eq!(
        table.column(1).unwrap().as_slice::<u64>().unwrap(),
        &[1, 4, 9]
    );
    assert_eq!(table.cell_named(2, "label"), Ok(ValueRef::String("nine")));
}

#[test]
fn concurrent_builder_rejects_incompatible_destination_atomically() {
    let builder_schema = Schema::new([ColumnSpec::new("value", DataType::U32)])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store);
    let builder = ConcurrentTableBuilder::new(builder_schema);
    builder.push_row([Value::U32(7)]).unwrap();

    let table_schema = Schema::new([ColumnSpec::new("value", DataType::U32)]).unwrap();
    let mut table = Table::new(table_schema);
    table.push_row([Value::U32(3)]).unwrap();

    assert_eq!(
        builder.append_to(&mut table),
        Err(TableError::SchemaMismatch)
    );
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.cell(0, 0), Ok(ValueRef::U32(3)));
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
fn column_io_selection_validates_aliasing_and_preserves_order() {
    let schema = Schema::new([
        ColumnSpec::new("left", DataType::U32),
        ColumnSpec::new("right", DataType::U32),
        ColumnSpec::new("sum", DataType::U32),
        ColumnSpec::new("product", DataType::U32),
    ])
    .unwrap();
    let mut table = Table::new(schema);
    table
        .push_row([Value::U32(2), Value::U32(3), Value::U32(0), Value::U32(0)])
        .unwrap();
    table
        .push_row([Value::U32(5), Value::U32(7), Value::U32(0), Value::U32(0)])
        .unwrap();

    let ([left, right], [sum, product]) = table.columns_io([0, 1], [2, 3]).unwrap();
    let left = left.as_slice::<u32>().unwrap();
    let right = right.as_slice::<u32>().unwrap();
    let sum = sum.as_mut_slice::<u32>().unwrap();
    let product = product.as_mut_slice::<u32>().unwrap();
    for (((left, right), sum), product) in left.iter().zip(right).zip(&mut *sum).zip(&mut *product)
    {
        *sum = left + right;
        *product = left * right;
    }
    assert_eq!(sum, &[5, 12]);
    assert_eq!(product, &[6, 35]);

    let ([first, repeated], []) = table.columns_io([1, 1], []).unwrap();
    assert!(std::ptr::eq(
        first.as_slice::<u32>().unwrap(),
        repeated.as_slice::<u32>().unwrap()
    ));

    assert_eq!(
        table.columns_io([0], [0]).unwrap_err(),
        ColumnSelectionError::InputOutputOverlap { column: 0 }
    );
    assert_eq!(
        table.columns_io([], [2, 2]).unwrap_err(),
        ColumnSelectionError::DuplicateOutput { column: 2 }
    );
    assert_eq!(
        table.columns_io([4], []).unwrap_err(),
        ColumnSelectionError::ColumnOutOfBounds {
            column: 4,
            column_count: 4,
        }
    );
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

#[test]
fn mutable_row_reads_and_replaces_fixed_and_extra_values() {
    let schema = people_schema().with_unknown_fields(UnknownFields::Store);
    let mut table = Table::new(schema);
    table
        .push_row([Value::U64(7), Value::from("Ada"), Value::I32(42)])
        .unwrap();

    let mut row = table.row_mut(0).unwrap();
    assert_eq!(row.position(), 0);
    assert_eq!(row.len(), 3);
    assert!(!row.is_empty());
    assert_eq!(row.get(0), Some(ValueRef::U64(7)));
    assert_eq!(row.get_named("display_name"), Some(ValueRef::String("Ada")));

    row.set(0, Value::U64(8)).unwrap();
    row.set_named("name", "Grace").unwrap();
    row.set_named("language", "COBOL").unwrap();
    assert_eq!(row.get_named("language"), Some(ValueRef::String("COBOL")));
    assert_eq!(
        row.set(0, Value::I64(8)),
        Err(TableError::TypeMismatch {
            column: 0,
            expected: DataType::U64,
            actual: DataType::I64,
        })
    );
    drop(row);

    assert_eq!(table.cell_named(0, "id"), Ok(ValueRef::U64(8)));
    assert_eq!(table.cell_named(0, "name"), Ok(ValueRef::String("Grace")));
    assert_eq!(
        table.cell_named(0, "language"),
        Ok(ValueRef::String("COBOL"))
    );
    assert!(table.row_mut(1).is_none());
}

#[test]
fn split_mutable_rows_can_run_on_scoped_threads() {
    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U32),
    ])
    .unwrap()
    .with_unknown_fields(UnknownFields::Store);
    let mut table = Table::with_capacity(schema, 100);
    for arg in 0_u32..100 {
        table.push_row([Value::U32(arg), Value::U32(0)]).unwrap();
    }

    let (left, right) = table.rows_mut().split_at(40);
    assert_eq!((left.start(), left.len()), (0, 40));
    assert_eq!((right.start(), right.len()), (40, 60));
    std::thread::scope(|scope| {
        scope.spawn(move || {
            left.for_each(|mut row| {
                let ValueRef::U32(arg) = row.get(0).unwrap() else {
                    unreachable!()
                };
                row.set(1, Value::U32(arg * 2)).unwrap();
                row.set_named("partition", "left").unwrap();
            });
        });
        scope.spawn(move || {
            right.for_each(|mut row| {
                let ValueRef::U32(arg) = row.get_named("arg").unwrap() else {
                    unreachable!()
                };
                row.set_named("result", arg * 3).unwrap();
                row.set_named("partition", "right").unwrap();
            });
        });
    });

    for row in 0..100 {
        let multiplier = if row < 40 { 2 } else { 3 };
        assert_eq!(
            table.cell_named(row, "result"),
            Ok(ValueRef::U32(u32::try_from(row).unwrap() * multiplier))
        );
        assert_eq!(
            table.cell_named(row, "partition"),
            Ok(ValueRef::String(if row < 40 { "left" } else { "right" }))
        );
    }
}

#[cfg(feature = "rayon")]
#[test]
fn parallel_column_projection_supports_multiple_inputs_and_outputs() {
    use rayon::prelude::*;

    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("scale", DataType::U32),
        ColumnSpec::new("bias", DataType::U32),
        ColumnSpec::new("result", DataType::U32),
        ColumnSpec::new("even", DataType::Bool),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, 10_000);
    for arg in 0_u32..10_000 {
        table
            .push_row([
                Value::U32(arg),
                Value::U32(3),
                Value::U32(1),
                Value::U32(0),
                Value::Bool(false),
            ])
            .unwrap();
    }

    let ([args, scales, biases], [results, even]) = table.columns_io([0, 1, 2], [3, 4]).unwrap();
    let args = args.as_slice::<u32>().unwrap();
    let scales = scales.as_slice::<u32>().unwrap();
    let biases = biases.as_slice::<u32>().unwrap();
    let results = results.as_mut_slice::<u32>().unwrap();
    let even = even.as_mut_slice::<bool>().unwrap();

    (args, scales, biases, results, even)
        .into_par_iter()
        .for_each(|(&arg, &scale, &bias, result, even)| {
            *result = arg.saturating_mul(scale).saturating_add(bias);
            *even = *result % 2 == 0;
        });

    for arg in [0_u32, 1, 12, 9_999] {
        let expected = arg * 3 + 1;
        assert_eq!(table.cell(arg as usize, 3), Ok(ValueRef::U32(expected)));
        assert_eq!(
            table.cell(arg as usize, 4),
            Ok(ValueRef::Bool(expected % 2 == 0))
        );
    }
}

#[cfg(feature = "rayon")]
#[test]
fn parallel_mutable_rows_transform_fixed_columns() {
    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U32),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, 10_000);
    for arg in 0_u32..10_000 {
        table.push_row([Value::U32(arg), Value::U32(0)]).unwrap();
    }

    table.par_for_each_row_mut(256, |mut row| {
        let ValueRef::U32(arg) = row.get(0).unwrap() else {
            unreachable!()
        };
        row.set(1, Value::U32(arg.saturating_mul(arg))).unwrap();
    });

    for arg in [0_u32, 1, 12, 9_999] {
        assert_eq!(
            table.cell(arg as usize, 1),
            Ok(ValueRef::U32(arg.saturating_mul(arg)))
        );
    }
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

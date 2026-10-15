//! Interned storage compatibility, ownership, import and mutation contracts.

use std::sync::Arc;

use compact_str::CompactString;
use gd::{
    ColumnConverter, ColumnMapping, ColumnSliceError, ColumnSpec, ConcurrentTableBuilder, DataType,
    IndexKeyRef, InternedStringError, NullOrder, Schema, SortDirection, Table, TableError, Value,
    ValueRef, selection_to_json, shared_selection_to_json, table_from_csv, table_from_json,
    table_to_csv, table_to_json,
};
use proptest::prelude::*;

const LONG: &str = "a repeated string long enough to require allocated storage";

fn single(nullable: bool) -> Table {
    Table::new(
        Schema::new([ColumnSpec::interned_string("text")
            .with_alias("display")
            .nullable(nullable)])
        .unwrap(),
    )
}

fn values(table: &Table) -> Vec<Value> {
    table
        .rows()
        .map(|row| row.get(0).unwrap().to_owned())
        .collect()
}

fn text(table: &Table, row: usize) -> &str {
    table.cell(row, 0).unwrap().as_str().unwrap()
}

// Existing exhaustive matches remain valid: interned storage uses the same
// non-CompactString-layout error as fixed buffer storage.
fn classify(error: ColumnSliceError) -> &'static str {
    match error {
        ColumnSliceError::TypeMismatch { .. } => "type",
        ColumnSliceError::FixedStringBuffer => "layout",
        ColumnSliceError::Nullable { .. } => "nullable",
        ColumnSliceError::Required { .. } => "required",
    }
}

#[test]
fn opt_in_preserves_logical_types_default_slices_and_slice_error_variants() {
    let ordinary = ColumnSpec::new("text", DataType::String);
    let interned = ColumnSpec::interned_string("text");
    assert_eq!(interned.data_type(), DataType::String);
    assert!(interned.is_interned_string());
    assert!(!ordinary.is_interned_string());
    assert!(!ColumnSpec::fixed_string("text", 8).is_interned_string());
    assert_eq!(interned.fixed_string_capacity(), None);
    assert_ne!(
        Schema::new([ordinary.clone()]).unwrap(),
        Schema::new([interned]).unwrap()
    );
    let mut ordinary = Table::new(Schema::new([ordinary]).unwrap());
    ordinary.push_row([LONG.into()]).unwrap();
    assert_eq!(
        ordinary
            .column(0)
            .unwrap()
            .as_slice::<CompactString>()
            .unwrap()[0],
        LONG
    );
    assert!(ordinary.column(0).unwrap().interned_strings().is_none());
    let (_, [column]) = ordinary.columns_io([], [0]).unwrap();
    column.as_mut_slice::<CompactString>().unwrap()[0] = "ordinary mutation".into();
    assert_eq!(text(&ordinary, 0), "ordinary mutation");

    for nullable in [false, true] {
        let mut table = single(nullable);
        table.push_row([LONG.into()]).unwrap();
        let column = table.column(0).unwrap();
        assert_eq!(
            classify(column.as_slice::<CompactString>().unwrap_err()),
            "layout"
        );
        assert_eq!(
            classify(column.as_nullable_slice::<CompactString>().unwrap_err()),
            "layout"
        );
        assert_eq!(classify(column.as_slice::<u64>().unwrap_err()), "type");
        assert!(column.fixed_strings().is_none());
        let (_, [column]) = table.columns_io([], [0]).unwrap();
        assert_eq!(
            classify(column.as_mut_slice::<CompactString>().unwrap_err()),
            "layout"
        );
        let (_, [column]) = table.columns_io([], [0]).unwrap();
        assert_eq!(
            classify(column.as_nullable_mut_slice::<CompactString>().unwrap_err()),
            "layout"
        );
    }
}

#[test]
fn equal_utf8_values_share_payloads_and_checked_writes_preserve_other_cells() {
    let mut table = single(true);
    for value in [
        Value::from(LONG),
        Value::from(LONG),
        Value::Null,
        Value::from(""),
        Value::from("å🙂\0tail"),
    ] {
        table.push_row([value]).unwrap();
    }
    assert_eq!(text(&table, 0).as_ptr(), text(&table, 1).as_ptr());
    let view = table.column(0).unwrap().interned_strings().unwrap();
    assert_eq!(view.len(), 5);
    assert!(!view.is_empty());
    assert_eq!(view.get(2), Some(None));
    assert_eq!(view.get(3), Some(Some("")));
    assert_eq!(view.get(4), Some(Some("å🙂\0tail")));
    assert_eq!(view.get(5), None);
    assert_eq!(view.iter().next_back(), Some(Some("å🙂\0tail")));
    let mut scanned = Vec::new();
    table
        .column(0)
        .unwrap()
        .for_each_value(|value| scanned.push(value.to_owned()));
    assert_eq!(scanned, values(&table));
    let (_, [column]) = table.columns_io([], [0]).unwrap();
    let mut view = column.interned_strings_mut().unwrap();
    assert_eq!(
        view.set(usize::MAX, Some("rejected")),
        Err(InternedStringError::RowOutOfBounds {
            row: usize::MAX,
            row_count: 5
        })
    );
    view.set(0, Some("changed")).unwrap();
    view.set(2, Some("å🙂\0tail")).unwrap();
    assert_eq!(view.get(1), Some(Some(LONG)));
    assert_eq!(view.get(0), Some(Some("changed")));
    assert_eq!(text(&table, 2).as_ptr(), text(&table, 4).as_ptr());
    table
        .row_mut(0)
        .unwrap()
        .set_named("display", LONG)
        .unwrap();
    assert_eq!(text(&table, 0).as_ptr(), text(&table, 1).as_ptr());
    let mut required = single(false);
    required.push_row([LONG.into()]).unwrap();
    let (_, [column]) = required.columns_io([], [0]).unwrap();
    assert_eq!(
        column.interned_strings_mut().unwrap().set(0, None),
        Err(InternedStringError::NullNotAllowed)
    );
    assert!(matches!(
        required.set_cell(0, 0, Value::Null),
        Err(TableError::NullNotAllowed { .. })
    ));
    assert!(matches!(
        required.push_row([Value::I64(7)]),
        Err(TableError::TypeMismatch { .. })
    ));
    assert_eq!(values(&required), [Value::from(LONG)]);
}

#[test]
fn copies_and_shared_views_keep_payloads_alive_and_cells_independent() {
    let mut table = single(true);
    for value in [
        Value::from(LONG),
        Value::Null,
        Value::from("other"),
        Value::from(LONG),
    ] {
        table.push_row([value]).unwrap();
    }
    table.tombstone_row(1).unwrap();
    table.set_property("owner", "source");
    let mut copied = table.copy_rows(&[3, 1, 0]).unwrap();
    let range = table.copy_range(2..4).unwrap();
    let projected = table.project(&[0]).unwrap();
    let cloned = table.clone();
    let source_pointer = text(&table, 0).as_ptr();
    for (other, row) in [(&copied, 0), (&range, 1), (&projected, 0), (&cloned, 0)] {
        assert_eq!(source_pointer, text(other, row).as_ptr());
    }
    copied.set_cell(0, 0, "new".into()).unwrap();
    assert_eq!(text(&table, 0), LONG);
    copied.compact();
    assert_eq!(values(&copied), [Value::from("new"), Value::from(LONG)]);
    assert_eq!(copied.property("owner"), Some(ValueRef::String("source")));
    assert!(copied.pop_row());
    assert_eq!(values(&copied), [Value::from("new")]);
    let immutable = table.into_shared();
    let view = immutable.project(&[0]).unwrap();
    assert_eq!(
        view.row(0)
            .unwrap()
            .get(0)
            .unwrap()
            .as_str()
            .unwrap()
            .as_ptr(),
        source_pointer
    );
    let snapshot = view.materialize().unwrap();
    let mutable = snapshot.into_shared_mut();
    mutable.set_cell(0, 0, "shared mutation").unwrap();
    assert_eq!(view.row(0).unwrap().get(0), Some(ValueRef::String(LONG)));
    drop(immutable);
    assert_eq!(
        shared_selection_to_json(&view).unwrap(),
        table_to_json(&projected).unwrap()
    );
}

#[test]
fn every_string_layout_append_pair_preserves_destination_policy_and_atomicity() {
    fn spec(layout: usize, nullable: bool) -> ColumnSpec {
        match layout {
            0 => ColumnSpec::new("text", DataType::String),
            1 => ColumnSpec::fixed_string("text", 128),
            _ => ColumnSpec::interned_string("text"),
        }
        .nullable(nullable)
    }
    for from in 0..3 {
        for to in 0..3 {
            for source_nullable in [false, true] {
                for target_nullable in [false, true] {
                    let mut source =
                        Table::new(Schema::new([spec(from, source_nullable)]).unwrap());
                    source.push_row([LONG.into()]).unwrap();
                    source.push_row([LONG.into()]).unwrap();
                    if source_nullable {
                        source.push_row([Value::Null]).unwrap();
                    }
                    source.tombstone_row(1).unwrap();
                    let mut target = Table::new(Schema::new([spec(to, target_nullable)]).unwrap());
                    target.push_row(["seed".into()]).unwrap();
                    let result = target.append(&source);
                    if source_nullable && !target_nullable {
                        assert!(matches!(result, Err(TableError::NullNotAllowed { .. })));
                        assert_eq!(values(&target), [Value::from("seed")]);
                    } else {
                        assert_eq!(result.unwrap(), 1..1 + source.row_count());
                        assert_eq!(&values(&target)[1..], values(&source));
                        assert!(target.is_tombstoned(2).unwrap());
                        if to == 2 {
                            assert_eq!(text(&target, 1).as_ptr(), text(&target, 2).as_ptr());
                        }
                    }
                    assert_eq!(
                        target.column(0).unwrap().interned_strings().is_some(),
                        to == 2
                    );
                    assert_eq!(target.column(0).unwrap().fixed_strings().is_some(), to == 1);
                }
            }
        }
    }
}

#[test]
fn mapped_append_deduplicates_and_releases_staged_data_on_later_validation_failure() {
    let mut source = Table::new(
        Schema::new([
            ColumnSpec::interned_string("text"),
            ColumnSpec::new("number", DataType::String),
        ])
        .unwrap(),
    );
    source.push_row([LONG.into(), "wrong".into()]).unwrap();
    let mut target = Table::new(
        Schema::new([
            ColumnSpec::interned_string("text"),
            ColumnSpec::new("number", DataType::I64),
        ])
        .unwrap(),
    );
    target.push_row(["original".into(), 1_i64.into()]).unwrap();
    assert!(matches!(
        target.append(&source),
        Err(TableError::TypeMismatch { column: 1, .. })
    ));
    assert_eq!(target.row_count(), 1);
    assert_eq!(text(&target, 0), "original");
    let mut two = Table::new(
        Schema::new([
            ColumnSpec::interned_string("a"),
            ColumnSpec::interned_string("b"),
        ])
        .unwrap(),
    );
    two.append_mapped(
        &source,
        &[ColumnMapping::new(0, 0), ColumnMapping::new(0, 1)],
    )
    .unwrap();
    assert_eq!(two.cell(0, 0).unwrap(), two.cell(0, 1).unwrap());
    assert!(two.schema().iter().all(ColumnSpec::is_interned_string));
}

#[test]
fn imports_converters_indexes_ordering_and_exports_use_string_contents() {
    let schema =
        Arc::new(Schema::new([ColumnSpec::interned_string("text").nullable(true)]).unwrap());
    let table = table_from_json(
        Arc::clone(&schema),
        r#"[{"text":"z"},{"text":"a"},{"text":"a"},{"text":null}]"#,
    )
    .unwrap();
    assert_eq!(text(&table, 1).as_ptr(), text(&table, 2).as_ptr());
    assert_eq!(
        table.index(0).unwrap().rows(IndexKeyRef::String("a")),
        &[1, 2]
    );
    assert_eq!(
        table
            .composite_index([0])
            .unwrap()
            .rows([IndexKeyRef::String("a")]),
        &[1, 2]
    );
    assert_eq!(
        table
            .row_order(0, SortDirection::Ascending, NullOrder::First)
            .unwrap()
            .positions(),
        &[3, 1, 2, 0]
    );
    assert_eq!(
        selection_to_json(&table.project_view(&[0]).unwrap()).unwrap(),
        table_to_json(&table).unwrap()
    );
    let csv = table_to_csv(&table, true).unwrap();
    let imported = table_from_csv(Arc::clone(&schema), &csv, true).unwrap();
    assert_eq!(values(&table), values(&imported));
    let policy = ColumnConverter::new("number-to-string", |value| {
        Ok(Value::from(value.to_i64().unwrap().to_string()))
    });
    let mut converted = Table::new(
        Schema::new([ColumnSpec::interned_string("text").with_converter(policy)]).unwrap(),
    );
    converted.push_row([Value::I64(7)]).unwrap();
    converted.push_row([Value::I64(7)]).unwrap();
    assert_eq!(text(&converted, 0).as_ptr(), text(&converted, 1).as_ptr());
}

#[test]
fn disjoint_typed_and_row_views_can_intern_from_scoped_threads() {
    let mut table = single(true);
    for _ in 0..8 {
        table.push_row([LONG.into()]).unwrap();
    }
    {
        let (_, [column]) = table.columns_io([], [0]).unwrap();
        let (mut left, mut right) = column.interned_strings_mut().unwrap().split_at(4);
        std::thread::scope(|scope| {
            scope.spawn(move || {
                for row in 0..left.len() {
                    left.set(row, Some("parallel shared text")).unwrap();
                }
            });
            scope.spawn(move || {
                for row in 0..right.len() {
                    right.set(row, Some("parallel shared text")).unwrap();
                }
            });
        });
    }
    for row in 0..8 {
        assert_eq!(text(&table, row).as_ptr(), text(&table, 0).as_ptr());
    }
    let (mut left, mut right) = table.rows_mut().split_at(3);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for row in 0..left.len() {
                left.row_mut(row)
                    .unwrap()
                    .set(0, "row update".into())
                    .unwrap();
            }
        });
        scope.spawn(move || {
            for row in 0..right.len() {
                right
                    .row_mut(row)
                    .unwrap()
                    .set(0, "row update".into())
                    .unwrap();
            }
        });
    });
    for row in 0..8 {
        assert_eq!(text(&table, row).as_ptr(), text(&table, 0).as_ptr());
    }
    let builder = ConcurrentTableBuilder::new(table.schema_arc());
    std::thread::scope(|scope| {
        for _ in 0..2 {
            let builder = &builder;
            scope.spawn(move || {
                for _ in 0..8 {
                    builder.push_row([LONG.into()]).unwrap();
                }
            });
        }
    });
    let built = builder.into_table();
    assert_eq!(built.row_count(), 16);
    assert_eq!(text(&built, 0).as_ptr(), text(&built, 15).as_ptr());
}

#[cfg(feature = "rayon")]
#[test]
fn rayon_copies_and_parallel_mutation_preserve_interning() {
    let mut source = single(false);
    for _ in 0..64 {
        source.push_row([LONG.into()]).unwrap();
    }
    let mut copy = source
        .par_copy_rows(&(0..64).rev().collect::<Vec<_>>())
        .unwrap();
    copy.par_for_each_row_mut(8, |mut row| {
        row.set(0, "parallel replacement".into()).unwrap();
    });
    assert_eq!(text(&source, 0), LONG);
    for row in 0..64 {
        assert_eq!(text(&copy, row).as_ptr(), text(&copy, 0).as_ptr());
    }
    assert_eq!(source.par_copy_range(3..9).unwrap().row_count(), 6);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_schema_loading_keeps_interned_storage() {
    let db = gd::SqliteDatabase::open_in_memory().unwrap();
    let table = db
        .query_table_with_schema(
            "SELECT 'same' AS text UNION ALL SELECT 'same'",
            &gd::Arguments::new(),
            Schema::new([ColumnSpec::interned_string("text")]).unwrap(),
        )
        .unwrap();
    assert!(table.column(0).unwrap().interned_strings().is_some());
    assert_eq!(text(&table, 0).as_ptr(), text(&table, 1).as_ptr());
}

proptest! {
    #[test]
    fn mixed_updates_copies_and_compaction_match_owned_values(
        inputs in prop::collection::vec(prop::option::of("[a-c]{0,8}"), 1..30),
        updates in prop::collection::vec((any::<usize>(), prop::option::of("[a-c]{0,8}")), 0..40),
    ) {
        let mut table = single(true);
        let original: Vec<_> = inputs.iter().map(|value| value.as_deref().map_or(Value::Null, Value::from)).collect();
        let mut expected = inputs;
        for value in &expected { table.push_row([value.as_deref().map_or(Value::Null, Value::from)]).unwrap(); }
        let copy = table.clone();
        for (row, value) in updates {
            let row = row % expected.len();
            table.row_mut(row).unwrap().set(0, value.as_deref().map_or(Value::Null, Value::from)).unwrap();
            expected[row] = value;
        }
        for row in (0..expected.len()).step_by(2) { table.tombstone_row(row).unwrap(); }
        table.compact();
        let expected: Vec<_> = expected.into_iter().enumerate().filter(|(row, _)| row % 2 == 1).map(|(_, value)| value.map_or(Value::Null, Value::from)).collect();
        prop_assert_eq!(values(&table), expected);
        prop_assert_eq!(values(&copy), original);
    }
}

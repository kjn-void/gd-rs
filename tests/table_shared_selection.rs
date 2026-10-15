//! Owning selection lifetimes, composition, metadata and export contracts.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use gd::{
    ColumnConverter, ColumnSpec, DataType, FormatError, Schema, SharedTableSelection, Table,
    TableError, UnknownFields, Value, ValueRef, selection_to_csv, selection_to_json,
    shared_selection_to_csv, shared_selection_to_json, table_to_csv, table_to_json,
};
use proptest::prelude::*;

const ADA: &str = "Ada Lovelace customer record with heap storage";

fn source() -> Table {
    let mut table = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::I64),
            ColumnSpec::new("name", DataType::String).with_alias("display"),
            ColumnSpec::fixed_string("fixed", 48).nullable(true),
            ColumnSpec::new("blob", DataType::Bytes).nullable(true),
        ])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store),
    );
    table
        .push_row_with_extras(
            [1_i64.into(), ADA.into(), "Åsa".into(), vec![1_u8, 2].into()],
            [("extra", "row extra")],
        )
        .unwrap();
    table
        .push_row([
            2_i64.into(),
            "Grace Hopper customer record".into(),
            Value::Null,
            Value::Null,
        ])
        .unwrap();
    table.tombstone_row(1).unwrap();
    table
        .push_row([
            3_i64.into(),
            "Linus Torvalds customer record".into(),
            "λ".into(),
            vec![3_u8, 4].into(),
        ])
        .unwrap();
    table.set_property("owner", "source");
    table
}

#[test]
fn handles_retain_the_source_and_share_cells_and_cloned_metadata() {
    let mut table = Arc::new(source());
    let weak = Arc::downgrade(&table);
    let shared = SharedTableSelection::new(Arc::clone(&table));
    assert!(std::ptr::eq(shared.table(), table.as_ref()));
    assert!(std::ptr::eq(shared.schema(), table.schema()));
    let a = shared.project(&[0, 1, 2, 3]).unwrap();
    let b = shared.project(&[1, 2, 3]).unwrap();
    let cloned = a.clone();
    assert!(std::ptr::eq(a.schema(), cloned.schema()));
    assert_eq!(a.columns().as_ptr(), cloned.columns().as_ptr());

    let row_a = a.row(0).unwrap();
    let row_b = b.row(0).unwrap();
    for (left, right) in [(1, 0), (2, 1)] {
        assert_eq!(
            row_a.get(left).unwrap().as_str().unwrap().as_ptr(),
            row_b.get(right).unwrap().as_str().unwrap().as_ptr()
        );
    }
    assert_eq!(
        row_a.get(3).unwrap().as_bytes().unwrap().as_ptr(),
        row_b.get(2).unwrap().as_bytes().unwrap().as_ptr()
    );

    assert!(Arc::get_mut(&mut table).is_none());
    let detached = Arc::make_mut(&mut table);
    detached.set_cell(0, 1, "detached copy".into()).unwrap();
    detached.compact();
    assert_eq!(table.row_count(), 2);
    assert_eq!(shared.row_count(), 3);
    assert_eq!(b.row(0).unwrap().get(0), Some(ValueRef::String(ADA)));

    drop(table);
    drop(shared);
    drop(a);
    drop(cloned);
    assert!(weak.upgrade().is_some());
    assert_eq!(b.row(0).unwrap().get(0), Some(ValueRef::String(ADA)));
    let empty = b.select(&[], &[]).unwrap();
    drop(b);
    assert!(weak.upgrade().is_some()); // Even an empty selection retains its source.
    drop(empty);
    assert!(weak.upgrade().is_none());
}

#[test]
fn projections_and_row_selection_compose_in_selection_relative_coordinates() {
    let shared = source().into_shared();
    let selected = shared.select(&[2, 0, 0, 1], &[3, 1, 0]).unwrap();
    assert_eq!(selected.positions().collect::<Vec<_>>(), [2, 0, 0, 1]);
    assert_eq!(selected.columns(), &[3, 1, 0]);
    assert_eq!(selected.row_count(), 4);
    assert_eq!(selected.column_count(), 3);
    assert_eq!(selected.positions().rev().collect::<Vec<_>>(), [1, 0, 0, 2]);
    assert_eq!(selected.rows().len(), 4);
    assert_eq!(selected.rows().next_back().unwrap().position(), 1);
    assert!(selected.row(4).is_none());
    assert!(selected.row(usize::MAX).is_none());
    assert!(selected.column(3).is_none());

    let names = selected.project(&[1]).unwrap();
    assert_eq!(names.columns(), &[1]);
    let row = names.row(1).unwrap();
    assert_eq!(row.len(), 1);
    assert!(!row.is_empty());
    assert_eq!(row.get_named("display"), Some(ValueRef::String(ADA)));
    assert_eq!(row.get_named("extra"), Some(ValueRef::String("row extra")));
    assert_eq!(row.get_named("id"), None);
    assert_eq!(row.get(1), None);
    assert_eq!(row.iter().collect::<Vec<_>>(), [ValueRef::String(ADA)]);
    assert_eq!(names.column(0).unwrap().alias(), Some("display"));

    let repeated = selected.select(&[3, 1, 1], &[2, 1]).unwrap();
    assert_eq!(repeated.positions().collect::<Vec<_>>(), [1, 0, 0]);
    assert_eq!(repeated.columns(), &[0, 1]);
    assert!(repeated.row(0).unwrap().is_tombstoned());
    let mut seen = Vec::new();
    let filtered = repeated.filter_rows(|row| {
        seen.push(row.position());
        row.get(0) == Some(ValueRef::I64(1))
    });
    assert_eq!(seen, [0, 0]); // Deleted rows never reach the predicate.
    assert_eq!(filtered.positions().collect::<Vec<_>>(), [0, 0]);
    assert!(std::ptr::eq(filtered.schema(), repeated.schema()));
    assert_eq!(filtered.columns().as_ptr(), repeated.columns().as_ptr());
}

#[test]
fn ranges_and_indexed_slices_preserve_order_and_allow_empty_end_ranges() {
    let shared = source().into_shared();
    let range = shared.slice(1..3).unwrap().slice(1..2).unwrap();
    assert_eq!(range.positions().collect::<Vec<_>>(), [2]);
    assert_eq!(range.row(0).unwrap().get(0), Some(ValueRef::I64(3)));
    let indexed = shared.select(&[2, 0, 1, 0], &[1]).unwrap();
    let slice = indexed.slice(1..4).unwrap().slice(1..3).unwrap();
    assert!(std::ptr::eq(slice.schema(), indexed.schema()));
    assert_eq!(slice.columns().as_ptr(), indexed.columns().as_ptr());
    assert_eq!(slice.positions().collect::<Vec<_>>(), [1, 0]);
    assert_eq!(slice.live_rows().count(), 1);
    assert_eq!(slice.project(&[0]).unwrap().positions().len(), 2);
    assert!(slice.slice(2..2).unwrap().is_empty());
    assert_eq!(slice.select(&[1, 0], &[]).unwrap().row_count(), 2);

    for rows in [
        3..4,
        usize::MAX..usize::MAX,
        std::ops::Range { start: 1, end: 0 },
    ] {
        assert_eq!(
            range.slice(rows.clone()).unwrap_err(),
            TableError::RowRangeOutOfBounds {
                start: rows.start,
                end: rows.end,
                row_count: 1,
            }
        );
    }
}

#[test]
fn bounds_duplicates_and_zero_column_selections_follow_borrowed_contracts() {
    let shared = source().into_shared().project(&[1]).unwrap();
    assert_eq!(
        shared.select(&[3], &[0]).unwrap_err(),
        TableError::RowOutOfBounds {
            row: 3,
            row_count: 3
        }
    );
    assert_eq!(
        shared.select(&[usize::MAX], &[0]).unwrap_err(),
        TableError::RowOutOfBounds {
            row: usize::MAX,
            row_count: 3
        }
    );
    assert_eq!(
        shared.project(&[1]).unwrap_err(),
        TableError::ColumnOutOfBounds {
            column: 1,
            column_count: 1
        }
    );
    assert!(matches!(
        shared.project(&[0, 0]),
        Err(TableError::DuplicateColumnName(_))
    ));
    assert!(matches!(
        shared.select(&[], &[0, 0]),
        Err(TableError::DuplicateColumnName(_))
    ));

    let empty_columns = shared.select(&[0, 1], &[]).unwrap();
    assert!(!empty_columns.is_empty());
    assert!(empty_columns.schema().is_empty());
    let row = empty_columns.row(0).unwrap();
    assert!(row.is_empty());
    assert_eq!(row.get_named("display"), None);
    assert_eq!(row.get_named("extra"), Some(ValueRef::String("row extra")));
    assert_eq!(shared_selection_to_json(&empty_columns).unwrap(), "[{}]");
    assert!(matches!(
        shared_selection_to_csv(&empty_columns, false),
        Err(FormatError::ZeroColumnTable)
    ));
    assert_eq!(empty_columns.materialize().unwrap().row_count(), 2);
    let empty_rows = shared.select(&[], &[0]).unwrap();
    assert!(empty_rows.is_empty());
    assert_eq!(empty_rows.positions().len(), 0);
    assert!(empty_rows.row(0).is_none());
    assert_eq!(shared_selection_to_json(&empty_rows).unwrap(), "[]");

    let mut no_columns = Table::new(Schema::new([]).unwrap());
    no_columns.push_row([]).unwrap();
    let no_columns = no_columns.into_shared();
    assert_eq!(no_columns.row_count(), 1);
    assert_eq!(no_columns.materialize().unwrap().row_count(), 1);
    let empty = Table::new(Schema::new([]).unwrap()).into_shared();
    assert!(empty.slice(0..0).unwrap().is_empty());
    assert!(empty.row(0).is_none());
}

#[test]
fn materialization_is_independent_and_retains_extras_properties_nulls_and_tombstones() {
    let shared = source()
        .into_shared()
        .select(&[1, 0, 0], &[2, 1, 3])
        .unwrap();
    let mut copied = shared.materialize().unwrap();
    assert_eq!(copied.schema(), shared.schema());
    assert_eq!(copied.row_count(), 3);
    assert!(copied.is_tombstoned(0).unwrap());
    assert_eq!(copied.cell(0, 0).unwrap(), ValueRef::Null);
    assert_eq!(copied.cell(0, 2).unwrap(), ValueRef::Null);
    assert_eq!(copied.property("owner"), Some(ValueRef::String("source")));
    copied.set_named(1, "extra", "changed").unwrap();
    copied.set_cell(1, 0, "changed".into()).unwrap();
    copied.set_cell(1, 1, "changed".into()).unwrap();
    copied.set_cell(1, 2, vec![9_u8].into()).unwrap();
    copied.set_property("owner", "copy");
    let original = shared.row(1).unwrap();
    assert_eq!(original.get(0), Some(ValueRef::String("Åsa")));
    assert_eq!(original.get(1), Some(ValueRef::String(ADA)));
    assert_eq!(original.get(2), Some(ValueRef::Bytes(&[1, 2])));
    assert_eq!(
        original.get_named("extra"),
        Some(ValueRef::String("row extra"))
    );
    assert_eq!(
        shared.table().property("owner"),
        Some(ValueRef::String("source"))
    );
    assert_eq!(copied.cell(2, 1).unwrap(), ValueRef::String(ADA));
    copied.compact();
    assert_eq!(shared.positions().collect::<Vec<_>>(), [1, 0, 0]);
    assert_eq!(shared.row_count(), 3);
}

#[test]
fn materialization_preserves_converters_without_executing_them() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let converter = ColumnConverter::new("text-to-id", move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(Value::I64(7))
    });
    let mut table = Table::new(
        Schema::new([ColumnSpec::new("id", DataType::I64)
            .with_alias("key")
            .with_converter(converter)])
        .unwrap(),
    );
    table.push_row(["seven".into()]).unwrap();
    let selection = table
        .into_shared()
        .project(&[0])
        .unwrap()
        .select(&[0, 0], &[0])
        .unwrap();
    let mut materialized = selection.materialize().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        materialized.schema().column(0).unwrap().alias(),
        Some("key")
    );
    materialized.push_row(["seven".into()]).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn shared_and_borrowed_exports_match_materialized_tables_and_format_errors() {
    let selection = source()
        .into_shared()
        .select(&[2, 0, 1, 0], &[3, 2, 1])
        .unwrap();
    let borrowed = selection.as_borrowed();
    assert!(std::ptr::eq(borrowed.table(), selection.table()));
    assert!(std::ptr::eq(borrowed.schema(), selection.schema()));
    assert_eq!(borrowed.positions(), &[2, 0, 1, 0]);
    let copied = selection.materialize().unwrap();
    assert_eq!(
        shared_selection_to_json(&selection).unwrap(),
        selection_to_json(&borrowed).unwrap()
    );
    assert_eq!(
        shared_selection_to_json(&selection).unwrap(),
        table_to_json(&copied).unwrap()
    );
    for headers in [false, true] {
        assert_eq!(
            shared_selection_to_csv(&selection, headers).unwrap(),
            selection_to_csv(&borrowed, headers).unwrap()
        );
        assert_eq!(
            shared_selection_to_csv(&selection, headers).unwrap(),
            table_to_csv(&copied, headers).unwrap()
        );
    }
    let mut floats = Table::new(Schema::new([ColumnSpec::new("value", DataType::F64)]).unwrap());
    floats.push_row([f64::INFINITY.into()]).unwrap();
    assert!(matches!(
        shared_selection_to_json(&floats.into_shared()),
        Err(FormatError::NonFiniteFloat)
    ));
}

#[test]
fn immutable_selections_can_be_moved_and_shared_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SharedTableSelection>();
    let shared = source().into_shared();
    let names = shared.project(&[1]).unwrap();
    let worker_names = names.clone();
    let worker = std::thread::spawn(move || {
        assert_eq!(
            worker_names.row(0).unwrap().get(0),
            Some(ValueRef::String(ADA))
        );
        shared_selection_to_json(&worker_names).unwrap()
    });
    drop(shared);
    assert_eq!(
        worker.join().unwrap(),
        shared_selection_to_json(&names).unwrap()
    );
}

proptest! {
    #[test]
    fn composed_shared_selections_match_an_independent_row_model(
        values in prop::collection::vec(any::<i64>(), 1..40),
        positions in prop::collection::vec(any::<usize>(), 1..40),
        start in any::<usize>(),
        end in any::<usize>(),
    ) {
        let mut table = Table::new(Schema::new([ColumnSpec::new("value", DataType::I64)]).unwrap());
        for &value in &values { table.push_row([value.into()]).unwrap(); }
        let positions: Vec<_> = positions.iter().map(|&p| p % values.len()).collect();
        let (start, end) = (start % (positions.len() + 1), end % (positions.len() + 1));
        let range = start.min(end)..start.max(end);
        let expected: Vec<_> = positions[range.clone()].iter().map(|&p| values[p]).filter(|v| v % 2 == 0).collect();
        let view = table.into_shared().select(&positions, &[0]).unwrap()
            .slice(range).unwrap().project(&[0]).unwrap()
            .filter_rows(|row| row.get(0).unwrap().to_i64().unwrap() % 2 == 0);
        let actual: Vec<_> = view.rows().map(|row| row.get(0).unwrap().to_i64().unwrap()).collect();
        prop_assert_eq!(actual, expected.clone());
        let materialized = view.materialize().unwrap();
        let actual: Vec<_> = materialized.rows().map(|row| row.get(0).unwrap().to_i64().unwrap()).collect();
        prop_assert_eq!(actual, expected);
    }
}

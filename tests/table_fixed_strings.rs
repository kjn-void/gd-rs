//! Fixed string storage invariants and integration with ordinary table operations.

use gd::{
    ColumnConverter, ColumnSpec, ConcurrentTableBuilder, DataType, FixedStringError, NullOrder,
    Schema, SortDirection, Table, TableError, Value, ValueRef,
};
use proptest::prelude::*;

fn table(nullable: bool) -> Table {
    Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::U64),
            ColumnSpec::fixed_string("text", 8).nullable(nullable),
        ])
        .unwrap(),
    )
}

#[test]
fn utf8_null_empty_and_rejected_writes_are_atomic() {
    let mut t = table(true);
    for (id, text) in [Value::from("åäö🙂"), Value::Null, Value::from("")]
        .into_iter()
        .enumerate()
    {
        // Ten UTF-8 bytes exceed the eight-byte slot, despite only four characters.
        let result = t.push_row([Value::U64(id as u64), text]);
        if id == 0 {
            assert!(matches!(
                result,
                Err(TableError::StringTooLong {
                    actual: 10,
                    capacity: 8,
                    column: 1
                })
            ));
        } else {
            result.unwrap();
        }
    }
    assert_eq!(t.row_count(), 2);
    t.set_cell(0, 1, Value::from("å🙂")).unwrap();
    assert!(t.set_cell(0, 1, Value::from("123456789")).is_err());
    assert_eq!(t.cell(0, 1).unwrap(), ValueRef::String("å🙂"));
    let mut view = t
        .column_pair_mut(0, 1)
        .unwrap()
        .1
        .fixed_strings_mut()
        .unwrap();
    assert_eq!(view.get(0), Some(Some("å🙂")));
    assert_eq!(view.get(1), Some(Some("")));
    assert_eq!(view.get(2), None);
    assert_eq!(
        view.set(0, Some("123456789")),
        Err(FixedStringError::TooLong {
            capacity: 8,
            actual: 9
        })
    );
    view.set(0, None).unwrap();
    assert_eq!(view.get(0), Some(None));
    assert!(matches!(
        view.set(2, Some("x")),
        Err(FixedStringError::RowOutOfBounds { .. })
    ));
    let mut required = table(false);
    required
        .push_row([Value::U64(1), Value::from("12345678")])
        .unwrap();
    assert_eq!(
        required
            .column_pair_mut(0, 1)
            .unwrap()
            .1
            .fixed_strings_mut()
            .unwrap()
            .set(0, None),
        Err(FixedStringError::NullNotAllowed)
    );
    assert_eq!(
        required
            .row_mut(0)
            .unwrap()
            .set(1, Value::from("too long!")),
        Err(TableError::StringTooLong {
            column: 1,
            capacity: 8,
            actual: 9
        })
    );
    assert_eq!(required.cell(0, 1).unwrap(), ValueRef::String("12345678"));
}

#[test]
fn gather_append_compact_and_pop_preserve_offsets_and_ownership() {
    let mut t = table(true);
    for (i, text) in [Some("alpha"), None, Some("åä"), Some(""), Some("omega")]
        .into_iter()
        .enumerate()
    {
        t.push_row([Value::U64(i as u64), text.map_or(Value::Null, Value::from)])
            .unwrap();
    }
    let mut copy = t.copy_rows(&[4, 1, 4, 2]).unwrap();
    copy.set_cell(0, 1, Value::from("changed")).unwrap();
    assert_eq!(t.cell(4, 1).unwrap(), ValueRef::String("omega"));
    let range = t.copy_range(1..4).unwrap();
    copy.append(&range).unwrap();
    copy.tombstone_row(1).unwrap();
    copy.tombstone_row(4).unwrap();
    copy.compact();
    let values: Vec<_> = copy
        .column(1)
        .unwrap()
        .fixed_strings()
        .unwrap()
        .iter()
        .collect();
    assert_eq!(
        values,
        [
            Some("changed"),
            Some("omega"),
            Some("åä"),
            Some("åä"),
            Some("")
        ]
    );
    assert!(copy.pop_row());
    copy.push_row([Value::U64(99), Value::from("new")]).unwrap();
    assert_eq!(copy.cell(4, 1).unwrap(), ValueRef::String("new"));
    let mut compact = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::U64),
            ColumnSpec::new("text", DataType::String).nullable(true),
        ])
        .unwrap(),
    );
    compact.append(&t).unwrap();
    let mut small = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::U64),
            ColumnSpec::fixed_string("text", 3).nullable(true),
        ])
        .unwrap(),
    );
    assert!(small.append(&compact).is_err());
    assert_eq!(small.row_count(), 0);
    let mut back = table(true);
    back.append(&compact).unwrap();
    assert_eq!(back.cell(2, 1).unwrap(), ValueRef::String("åä"));
}

#[test]
fn split_views_and_rows_mut_are_safe_for_scoped_workers() {
    let mut t = table(false);
    for i in 0..17 {
        t.push_row([Value::U64(i), Value::from("abc")]).unwrap();
    }
    let view = t
        .column_pair_mut(0, 1)
        .unwrap()
        .1
        .fixed_strings_mut()
        .unwrap();
    let (mut left, right) = view.split_at(3);
    let (mut middle, mut right) = right.split_at(7);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for mut cell in left.iter_mut() {
                cell.as_str_mut().unwrap().make_ascii_uppercase();
            }
        });
        scope.spawn(move || {
            for i in 0..middle.len() {
                middle.set(i, Some("åäö")).unwrap();
            }
        });
        scope.spawn(move || {
            right.set(0, Some("right")).unwrap();
        });
    });
    assert_eq!(t.cell(0, 1).unwrap(), ValueRef::String("ABC"));
    assert_eq!(t.cell(9, 1).unwrap(), ValueRef::String("åäö"));
    assert_eq!(t.cell(10, 1).unwrap(), ValueRef::String("right"));
    let (left, right) = t.rows_mut().split_at(5);
    std::thread::scope(|scope| {
        scope.spawn(move || {
            left.for_each(|mut row| {
                row.set(1, Value::from("left")).unwrap();
            });
        });
        scope.spawn(move || {
            right.for_each(|mut row| {
                row.set(1, Value::from("right")).unwrap();
            });
        });
    });
    assert_eq!(t.cell(4, 1).unwrap(), ValueRef::String("left"));
    assert_eq!(t.cell(5, 1).unwrap(), ValueRef::String("right"));
}

#[test]
fn conversion_builder_sort_index_and_format_use_logical_strings() {
    let converter = ColumnConverter::new("u64-text", |value| {
        Ok(Value::from(match value {
            ValueRef::U64(n) => n.to_string(),
            _ => return Err("expected u64".into()),
        }))
    });
    let schema = Schema::new([ColumnSpec::fixed_string("text", 4)
        .nullable(true)
        .with_converter(converter)])
    .unwrap();
    let builder = ConcurrentTableBuilder::new(schema);
    builder.push_row([Value::U64(9)]).unwrap();
    builder.push_row([Value::Null]).unwrap();
    builder.push_row([Value::from("abc")]).unwrap();
    assert!(builder.push_row([Value::U64(12345)]).is_err());
    let t = builder.into_table();
    assert_eq!(t.row_count(), 3);
    assert_eq!(t.cell(0, 0).unwrap(), ValueRef::String("9"));
    let order = t
        .row_order(0, SortDirection::Ascending, NullOrder::First)
        .unwrap();
    assert_eq!(order.positions(), &[1, 0, 2]);
    assert!(gd::table_to_json(&t).unwrap().contains("abc"));
    assert_eq!(
        t.index(0).unwrap().rows(gd::IndexKeyRef::String("abc")),
        &[2]
    );
}

#[test]
fn ordinary_string_slices_and_storage_errors_are_explicit() {
    use compact_str::CompactString;
    use gd::ColumnSliceError;

    let mut ordinary = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::U64),
            ColumnSpec::new("text", DataType::String).nullable(true),
        ])
        .unwrap(),
    );
    ordinary.push_row([Value::U64(1), Value::Null]).unwrap();
    ordinary
        .column_pair_mut(0, 1)
        .unwrap()
        .1
        .as_nullable_mut_slice::<CompactString>()
        .unwrap()[0] = Some("hello".into());
    assert_eq!(
        ordinary
            .column(1)
            .unwrap()
            .as_nullable_slice::<CompactString>()
            .unwrap()[0]
            .as_deref(),
        Some("hello")
    );
    let fixed = table(false);
    assert_eq!(
        fixed.column(1).unwrap().as_slice::<CompactString>(),
        Err(ColumnSliceError::FixedStringBuffer)
    );
    assert!(matches!(
        fixed.column(1).unwrap().as_slice::<u64>(),
        Err(ColumnSliceError::TypeMismatch { .. })
    ));
    let view = ordinary.column(1).unwrap();
    assert!(view.fixed_strings().is_none());
    assert!(std::panic::catch_unwind(|| ColumnSpec::fixed_string("bad", 0)).is_err());
}

#[test]
fn trusted_utf8_borrows_handle_shorter_replacements_and_split_slots() {
    let mut t =
        Table::new(Schema::new([ColumnSpec::fixed_string("text", 9).nullable(true)]).unwrap());
    for text in ["å🙂xyz", "🙂åabc", "abcé🙂"] {
        t.push_row([Value::from(text)]).unwrap();
    }
    let view = t.columns_io([], [0]).unwrap().1.into_iter().next().unwrap();
    let (mut empty, remainder) = view.fixed_strings_mut().unwrap().split_at(0);
    assert!(empty.cell_mut(0).is_none());
    let (mut left, mut right) = remainder.split_at(1);
    // Old continuation bytes remain in each slot's unused suffix. Only the
    // new prefix is a string; treating the entire slot as UTF-8 would be wrong.
    left.set(0, Some("x")).unwrap();
    let mut cell = left.cell_mut(0).unwrap();
    assert_eq!(cell.get(), Some("x"));
    cell.as_str_mut().unwrap().make_ascii_uppercase();
    assert_eq!(cell.get(), Some("X"));
    right.set(0, None).unwrap();
    assert_eq!(right.cell_mut(0).unwrap().get(), None);
    assert!(right.cell_mut(0).unwrap().as_str_mut().is_none());
    right.set(0, Some("")).unwrap();
    assert_eq!(right.as_view().get(0), Some(Some("")));
    let mut cell = right.cell_mut(1).unwrap();
    cell.as_str_mut().unwrap().make_ascii_uppercase();
    assert_eq!(cell.get(), Some("ABCé🙂"));
    assert!(cell.set(Some("🙂🙂🙂")).is_err());
    assert_eq!(cell.get(), Some("ABCé🙂"));

    let mut copy = t.copy_rows(&[2, 0, 1, 2]).unwrap();
    copy.append(&t).unwrap();
    copy.tombstone_row(1).unwrap();
    copy.compact();
    assert!(copy.pop_row());
    copy.push_row([Value::from("🙂éåx")]).unwrap();
    assert_eq!(
        copy.column(0)
            .unwrap()
            .fixed_strings()
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        [
            Some("ABCé🙂"),
            Some(""),
            Some("ABCé🙂"),
            Some("X"),
            Some(""),
            Some("🙂éåx")
        ]
    );
    assert_eq!(t.cell(2, 0).unwrap(), ValueRef::String("ABCé🙂"));
}

proptest! {
    #[test]
    fn fixed_and_ordinary_tables_agree_after_edits_and_gathers(
        edits in prop::collection::vec((0usize..9, prop::option::of("[a-zåäö🙂]{0,16}")), 0..60),
        rows in prop::collection::vec(0usize..9, 0..30),
    ) {
        let mut fixed = Table::new(Schema::new([ColumnSpec::fixed_string("text", 64).nullable(true)]).unwrap());
        let mut ordinary = Table::new(Schema::new([ColumnSpec::new("text", DataType::String).nullable(true)]).unwrap());
        for _ in 0..9 { fixed.push_row([Value::Null]).unwrap(); ordinary.push_row([Value::Null]).unwrap(); }
        for (row, text) in edits {
            let value = text.map_or(Value::Null, Value::from);
            fixed.set_cell(row, 0, value.clone()).unwrap();
            ordinary.set_cell(row, 0, value).unwrap();
        }
        prop_assert_eq!(gd::table_to_json(&fixed).unwrap(), gd::table_to_json(&ordinary).unwrap());
        let mut copy = fixed.copy_rows(&rows).unwrap();
        let reference = ordinary.copy_rows(&rows).unwrap();
        prop_assert_eq!(gd::table_to_json(&copy).unwrap(), gd::table_to_json(&reference).unwrap());
        copy.append(&ordinary).unwrap();
        for row in (0..copy.row_count()).step_by(2) { copy.tombstone_row(row).unwrap(); }
        let before = gd::table_to_json(&copy).unwrap();
        copy.compact();
        prop_assert_eq!(before, gd::table_to_json(&copy).unwrap());
        let view = copy.columns_io([], [0]).unwrap().1.into_iter().next().unwrap().fixed_strings_mut().unwrap();
        let len = view.len();
        let (left, right) = view.split_at(len);
        prop_assert_eq!(left.len(), len);
        prop_assert!(right.is_empty());
    }
}

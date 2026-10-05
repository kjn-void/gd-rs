//! Atomic mapped append and metadata contracts.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use gd::{
    ColumnConverter, ColumnMapping, ColumnSpec, DataType, Schema, Table, TableError, UnknownFields,
    Value, ValueRef,
};
use proptest::prelude::*;
use uuid::Uuid;

fn cells(table: &Table) -> Vec<Vec<Value>> {
    table
        .rows()
        .map(|row| row.iter().map(ValueRef::to_owned).collect())
        .collect()
}

#[test]
fn positional_append_covers_every_storage_kind_and_preserves_metadata() {
    let values = vec![
        Value::Null,
        Value::Bool(true),
        Value::I8(-1),
        Value::I16(-2),
        Value::I32(-3),
        Value::I64(-4),
        Value::U8(1),
        Value::U16(2),
        Value::U32(3),
        Value::U64(u64::MAX),
        Value::F32(1.25),
        Value::F64(2.5),
        Value::from("long string requiring independent storage"),
        Value::from(vec![1_u8, 2]),
        Value::from(Uuid::from_u128(7)),
    ];
    for nullable in [false, true] {
        let schema = Schema::new(values.iter().enumerate().map(|(i, value)| {
            ColumnSpec::new(format!("c{i}"), value.data_type()).nullable(nullable)
        }))
        .unwrap()
        .with_unknown_fields(UnknownFields::Store);
        let mut source = Table::new(schema.clone());
        source.push_row_vec(values.clone()).unwrap();
        source.set_named(0, "extra", "hello").unwrap();
        source.push_row_vec(values.clone()).unwrap();
        source.tombstone_row(1).unwrap();
        source.set_property("owner", "source");
        let mut target = Table::new(schema);
        target.push_row_vec(values.clone()).unwrap();
        target.tombstone_row(0).unwrap();
        target.set_property("owner", "target");
        assert_eq!(target.append(&source).unwrap(), 1..3);
        assert_eq!(
            cells(&target),
            vec![values.clone(), values.clone(), values.clone()]
        );
        assert_eq!(target.tombstoned_rows().collect::<Vec<_>>(), [0, 2]);
        assert_eq!(target.cell_named(1, "extra"), Ok(ValueRef::String("hello")));
        assert_eq!(target.property("owner"), Some(ValueRef::String("target")));
        source.set_named(0, "extra", "changed").unwrap();
        source.set_cell(0, 12, "changed".into()).unwrap();
        assert_eq!(target.cell_named(1, "extra"), Ok(ValueRef::String("hello")));
        assert_eq!(target.cell(1, 12), Ok(values[12].as_ref()));
    }
}

#[test]
fn mapped_append_reorders_reuses_and_fills_nullable_destinations() {
    let mut source = Table::new(
        Schema::new([
            ColumnSpec::new("name", DataType::String),
            ColumnSpec::new("id", DataType::I64),
        ])
        .unwrap(),
    );
    source.push_row(["Ada".into(), 7_i64.into()]).unwrap();
    let mut target = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::I64).nullable(true),
            ColumnSpec::new("name", DataType::String),
            ColumnSpec::new("copy", DataType::String).nullable(true),
            ColumnSpec::new("optional", DataType::Bool).nullable(true),
        ])
        .unwrap(),
    );
    assert_eq!(
        target
            .append_mapped(
                &source,
                &[
                    ColumnMapping::new(1, 0),
                    ColumnMapping::new(0, 1),
                    ColumnMapping::new(0, 2)
                ]
            )
            .unwrap(),
        0..1
    );
    assert_eq!(
        cells(&target),
        vec![vec![7_i64.into(), "Ada".into(), "Ada".into(), Value::Null]]
    );
}

#[test]
fn named_append_resolves_aliases_and_rejects_ambiguous_matches() {
    let mut source = Table::new(
        Schema::new([
            ColumnSpec::new("old_name", DataType::I64).with_alias("id"),
            ColumnSpec::new("discard", DataType::Bool),
        ])
        .unwrap(),
    );
    source.push_row([7_i64.into(), true.into()]).unwrap();
    let mut target = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::I64),
            ColumnSpec::new("missing", DataType::String).nullable(true),
        ])
        .unwrap(),
    );
    target.append_named(&source).unwrap();
    assert_eq!(cells(&target), vec![vec![7_i64.into(), Value::Null]]);
    let ambiguous = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::I64),
            ColumnSpec::new("other", DataType::I64),
        ])
        .unwrap(),
    );
    let mut target = Table::new(
        Schema::new([ColumnSpec::new("id", DataType::I64).with_alias("other")]).unwrap(),
    );
    assert_eq!(
        target.append_named(&ambiguous),
        Err(TableError::AmbiguousColumnMapping { column: 0 })
    );
}

#[test]
fn conversion_failure_is_atomic_and_success_does_not_rerun_converter() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&calls);
    let converter = ColumnConverter::new("parse_i64", move |value| {
        count.fetch_add(1, Ordering::Relaxed);
        value
            .as_str()
            .map_err(|e| e.to_string())?
            .parse::<i64>()
            .map(Value::I64)
            .map_err(|e| e.to_string().into())
    });
    let mut target = Table::new(
        Schema::new([ColumnSpec::new("id", DataType::I64).with_converter(converter)]).unwrap(),
    );
    target.push_row([99_i64.into()]).unwrap();
    let mut source = Table::new(Schema::new([ColumnSpec::new("text", DataType::String)]).unwrap());
    source.push_row(["12".into()]).unwrap();
    source.push_row(["bad".into()]).unwrap();
    let before = cells(&target);
    assert!(matches!(
        target.append(&source),
        Err(TableError::ConversionFailed { column: 0, .. })
    ));
    assert_eq!(cells(&target), before);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    source.set_cell(1, 0, "13".into()).unwrap();
    assert_eq!(target.append(&source).unwrap(), 1..3);
    assert_eq!(calls.load(Ordering::Relaxed), 4);
    assert_eq!(
        cells(&target),
        vec![
            vec![99_i64.into()],
            vec![12_i64.into()],
            vec![13_i64.into()]
        ]
    );
}

#[test]
fn mapping_nullability_and_extras_errors_leave_destination_unchanged() {
    let schema = Schema::new([ColumnSpec::new("id", DataType::I64)]).unwrap();
    let mut target = Table::new(schema.clone());
    target.push_row([9_i64.into()]).unwrap();
    let mut source = Table::new(
        Schema::new([ColumnSpec::new("id", DataType::I64).nullable(true)])
            .unwrap()
            .with_unknown_fields(UnknownFields::Store),
    );
    source.push_row([7_i64.into()]).unwrap();
    source.push_row([Value::Null]).unwrap();
    assert_eq!(
        target.append(&source),
        Err(TableError::NullNotAllowed { column: 0 })
    );
    assert_eq!(
        target.append_mapped(&source, &[]),
        Err(TableError::NullNotAllowed { column: 0 })
    );
    assert_eq!(
        target.append_mapped(
            &source,
            &[ColumnMapping::new(0, 0), ColumnMapping::new(0, 0)]
        ),
        Err(TableError::DuplicateColumnMapping { column: 0 })
    );
    assert!(matches!(
        target.append_mapped(&source, &[ColumnMapping::new(1, 0)]),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    assert!(matches!(
        target.append_mapped(&source, &[ColumnMapping::new(0, 1)]),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    source.pop_row();
    source.set_named(0, "note", "keep me").unwrap();
    assert!(matches!(
        target.append(&source),
        Err(TableError::ColumnNotFound(_))
    ));
    let mut collision = Table::new(
        Schema::new([ColumnSpec::new("note", DataType::I64)])
            .unwrap()
            .with_unknown_fields(UnknownFields::Store),
    );
    assert!(matches!(
        collision.append(&source),
        Err(TableError::ExtraFieldConflictsWithColumn(_))
    ));
    assert!(collision.is_empty());
    assert_eq!(cells(&target), vec![vec![9_i64.into()]]);
    let empty_wrong = Table::new(Schema::new([ColumnSpec::new("text", DataType::String)]).unwrap());
    assert!(matches!(
        target.append(&empty_wrong),
        Err(TableError::TypeMismatch { .. })
    ));
    assert!(matches!(
        target.append(&Table::new(Schema::new([]).unwrap())),
        Err(TableError::RowWidth { .. })
    ));
    assert_eq!(target.append(&Table::new(schema)).unwrap(), 1..1);
}

#[test]
fn zero_column_append_keeps_physical_rows_extras_and_deletion_flags() {
    let schema = Schema::new([])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store);
    let mut source = Table::new(schema.clone());
    source.push_row_with_extras([], [("extra", 3_i64)]).unwrap();
    source.push_row([]).unwrap();
    source.tombstone_row(0).unwrap();
    let mut target = Table::new(schema);
    target.push_row([]).unwrap();
    target.append(&source).unwrap();
    assert_eq!(target.row_count(), 3);
    assert_eq!(target.tombstoned_rows().collect::<Vec<_>>(), [1]);
    assert_eq!(target.cell_named(1, "extra"), Ok(ValueRef::I64(3)));
    let compact = target.compact();
    assert_eq!(
        (0..3)
            .map(|row| compact.new_position(row))
            .collect::<Vec<_>>(),
        [Some(0), None, Some(1)]
    );
}

proptest! {
    #[test]
    fn mapped_append_matches_a_row_model(
        old in prop::collection::vec((any::<i64>(), any::<bool>()), 0..30),
        added in prop::collection::vec((prop::option::of(any::<i64>()), any::<bool>()), 0..30)
    ) {
        let schema = Schema::new([ColumnSpec::new("id", DataType::I64).nullable(true)]).unwrap();
        let mut target = Table::new(schema.clone());
        for &(id, deleted) in &old { let row = target.push_row([id.into()]).unwrap(); if deleted { target.tombstone_row(row).unwrap(); } }
        let mut source = Table::new(schema);
        for &(id, deleted) in &added { let row = source.push_row([id.map_or(Value::Null, Value::I64)]).unwrap(); if deleted { source.tombstone_row(row).unwrap(); } }
        prop_assert_eq!(target.append_mapped(&source, &[ColumnMapping::new(0,0)]).unwrap(), old.len()..old.len()+added.len());
        let expected: Vec<_> = old.iter().map(|&(v,d)| (Some(v),d)).chain(added.iter().copied()).collect();
        for (row, (value, deleted)) in expected.iter().enumerate() {
            prop_assert_eq!(target.cell(row,0).unwrap().to_owned(), value.map_or(Value::Null, Value::I64));
            prop_assert_eq!(target.is_tombstoned(row).unwrap(), *deleted);
        }
    }
}

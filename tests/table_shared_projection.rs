//! Shared mutable projection ownership, locking, validation and view contracts.

use std::sync::{Arc, Barrier, RwLock, TryLockError};

use gd::{
    ColumnConverter, ColumnSpec, DataType, Schema, SharedProjectionError, SharedTableProjection,
    Table, TableError, UnknownFields, Value, ValueRef, selection_to_csv, selection_to_json,
    shared_selection_to_json,
};

const ADA: &str = "Ada Lovelace customer record with heap storage";

fn source() -> Table {
    let mut table = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::I64),
            ColumnSpec::new("name", DataType::String).with_alias("display"),
            ColumnSpec::fixed_string("fixed", 16).nullable(true),
            ColumnSpec::new("qty", DataType::U64),
            ColumnSpec::new("cost", DataType::U64),
            ColumnSpec::new("blob", DataType::Bytes).nullable(true),
        ])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store),
    );
    table
        .push_row_with_extras(
            [
                1_i64.into(),
                ADA.into(),
                "Åsa".into(),
                10_u64.into(),
                100_u64.into(),
                vec![1_u8, 2].into(),
            ],
            [("extra", "row extra")],
        )
        .unwrap();
    table
        .push_row([
            2_i64.into(),
            "Grace".into(),
            Value::Null,
            20_u64.into(),
            200_u64.into(),
            Value::Null,
        ])
        .unwrap();
    table.set_property("owner", "source");
    table
}

#[test]
fn overlapping_projections_share_writes_payloads_and_source_lifetime() {
    let source = Arc::new(RwLock::new(source()));
    let weak = Arc::downgrade(&source);
    let all = SharedTableProjection::from_shared(Arc::clone(&source)).unwrap();
    assert!(Arc::ptr_eq(&source, &all.source_arc()));
    let a = all.project(&[0, 1, 3]).unwrap();
    let b = all.project(&[1, 3, 4]).unwrap();
    let clone = a.clone();
    assert!(std::ptr::eq(a.schema(), clone.schema()));
    assert_eq!(a.columns().as_ptr(), clone.columns().as_ptr());

    let name_ptr = a
        .read()
        .unwrap()
        .cell(0, 1)
        .unwrap()
        .as_str()
        .unwrap()
        .as_ptr();
    assert_eq!(
        name_ptr,
        b.read()
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .as_str()
            .unwrap()
            .as_ptr()
    );
    a.set_cell(0, 2, 99_u64).unwrap();
    assert_eq!(
        source.read().unwrap().cell(0, 3).unwrap(),
        ValueRef::U64(99)
    );
    assert_eq!(b.read().unwrap().cell(0, 1).unwrap(), ValueRef::U64(99));
    b.set_named(0, "display", "changed shared name").unwrap();
    assert_eq!(
        a.read().unwrap().row(0).unwrap().get_named("name"),
        Some(ValueRef::String("changed shared name"))
    );

    drop(source);
    drop(all);
    drop(a);
    drop(clone);
    assert!(weak.upgrade().is_some());
    assert_eq!(b.read().unwrap().cell(0, 1).unwrap(), ValueRef::U64(99));
    let empty = b.project(&[]).unwrap();
    drop(b);
    assert_eq!(empty.read().unwrap().row_count(), 2);
    drop(empty);
    assert!(weak.upgrade().is_none());
}

#[test]
fn guarded_access_borrows_current_cells_and_preserves_projection_metadata() {
    let all = source().into_shared_mut();
    let projection = all
        .project(&[5, 2, 1])
        .unwrap()
        .project(&[2, 0, 1])
        .unwrap();
    assert_eq!(projection.columns(), &[1, 5, 2]);
    assert_eq!(projection.column_count(), 3);
    let read = projection.read().unwrap();
    assert_eq!(read.columns(), &[1, 5, 2]);
    assert_eq!(read.column_count(), 3);
    assert_eq!(read.row_count(), 2);
    assert!(!read.is_empty());
    assert!(std::ptr::eq(read.schema(), projection.schema()));
    assert_eq!(read.column(0).unwrap().alias(), Some("display"));
    assert!(read.column(3).is_none());
    assert!(read.row(usize::MAX).is_none());
    let row = read.row(0).unwrap();
    assert_eq!(row.get_named("display"), Some(ValueRef::String(ADA)));
    assert_eq!(row.get_named("id"), None);
    assert_eq!(row.get_named("extra"), Some(ValueRef::String("row extra")));
    assert_eq!(read.rows().len(), 2);
    assert_eq!(read.rows().next_back().unwrap().position(), 1);
    assert_eq!(read.cell(1, 1).unwrap(), ValueRef::Null);
    assert_eq!(read.cell(1, 2).unwrap(), ValueRef::Null);
    assert_eq!(
        read.table().property("owner"),
        Some(ValueRef::String("source"))
    );
    for column in [1, 2] {
        let projected = row.get(column).unwrap();
        let original = read.table().cell(0, read.columns()[column]).unwrap();
        match (projected, original) {
            (ValueRef::String(a), ValueRef::String(b)) => assert_eq!(a.as_ptr(), b.as_ptr()),
            (ValueRef::Bytes(a), ValueRef::Bytes(b)) => assert_eq!(a.as_ptr(), b.as_ptr()),
            _ => panic!("expected borrowed storage"),
        }
    }
}

#[test]
fn write_guards_validate_values_and_names_without_exposing_omitted_columns() {
    let projection = source().into_shared_mut().project(&[1, 2, 3, 5]).unwrap();
    let mut write = projection.write().unwrap();
    assert_eq!(write.row_count(), 2);
    write.set_cell(0, 1, "λ").unwrap();
    write.set_cell(0, 3, vec![9_u8]).unwrap();
    write.set_named(0, "extra", "updated").unwrap();
    assert_eq!(
        write.row(0).unwrap().get_named("extra"),
        Some(ValueRef::String("updated"))
    );
    assert_eq!(write.cell(0, 1).unwrap(), ValueRef::String("λ"));
    assert_eq!(write.cell(0, 3).unwrap(), ValueRef::Bytes(&[9]));
    assert_eq!(write.table().cell(0, 2).unwrap(), ValueRef::String("λ"));
    assert!(matches!(
        write.set_named(0, "id", 8_i64),
        Err(TableError::ColumnNotFound(_))
    ));
    assert!(matches!(
        write.set_named(0, "cost", 999_u64),
        Err(TableError::ColumnNotFound(_))
    ));
    assert!(matches!(
        write.set_cell(0, 2, "wrong type"),
        Err(TableError::TypeMismatch { column: 3, .. })
    ));
    assert!(matches!(
        write.set_cell(0, 2, Value::Null),
        Err(TableError::NullNotAllowed { column: 3 })
    ));
    assert!(matches!(
        write.set_cell(0, 1, "this string exceeds the slot"),
        Err(TableError::StringTooLong { column: 2, .. })
    ));
    assert_eq!(write.cell(0, 2).unwrap(), ValueRef::U64(10));
    assert_eq!(write.cell(0, 1).unwrap(), ValueRef::String("λ"));
    assert_eq!(write.table().cell(0, 4).unwrap(), ValueRef::U64(100));
    // A later error does not roll back earlier successful writes or poison the lock.
    drop(write);
    projection.set_cell(0, 2, 42_u64).unwrap();
    assert_eq!(
        projection.read().unwrap().cell(0, 2).unwrap(),
        ValueRef::U64(42)
    );
}

#[test]
fn invalid_positions_and_empty_projections_return_checked_errors() {
    let projection = source().into_shared_mut().project(&[3]).unwrap();
    assert_eq!(
        projection.project(&[1]).unwrap_err(),
        TableError::ColumnOutOfBounds {
            column: 1,
            column_count: 1
        }
    );
    assert!(matches!(
        projection.project(&[0, 0]),
        Err(TableError::DuplicateColumnName(_))
    ));
    assert_eq!(
        projection.set_cell(0, usize::MAX, 1_u64),
        Err(SharedProjectionError::Table(
            TableError::ColumnOutOfBounds {
                column: usize::MAX,
                column_count: 1
            }
        ))
    );
    assert_eq!(
        projection.set_cell(usize::MAX, 0, 1_u64),
        Err(SharedProjectionError::Table(TableError::RowOutOfBounds {
            row: usize::MAX,
            row_count: 2
        }))
    );
    let read = projection.read().unwrap();
    assert!(matches!(
        read.cell(0, 1),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    assert!(matches!(
        read.cell(2, 0),
        Err(TableError::RowOutOfBounds { .. })
    ));
    drop(read);
    let empty = projection.project(&[]).unwrap();
    assert_eq!(empty.read().unwrap().row_count(), 2);
    assert_eq!(empty.read().unwrap().column_count(), 0);
    assert_eq!(empty.read().unwrap().row(0).unwrap().get_named("qty"), None);
    assert!(matches!(
        empty.set_cell(0, 0, 1_u64),
        Err(SharedProjectionError::Table(
            TableError::ColumnOutOfBounds { .. }
        ))
    ));
    empty.set_named(0, "extra", "allowed").unwrap();
    assert_eq!(
        empty.read().unwrap().row(0).unwrap().get_named("extra"),
        Some(ValueRef::String("allowed"))
    );

    let no_columns = Table::new(Schema::new([]).unwrap()).into_shared_mut();
    assert!(no_columns.read().unwrap().is_empty());
    no_columns
        .source_arc()
        .write()
        .unwrap()
        .push_row([])
        .unwrap();
    assert_eq!(no_columns.read().unwrap().row_count(), 1);
}

#[test]
fn live_projections_follow_appends_removals_and_compaction_between_guards() {
    let projection = source().into_shared_mut().project(&[0, 3]).unwrap();
    let source = projection.source_arc();
    source
        .write()
        .unwrap()
        .push_row([
            3_i64.into(),
            "Third".into(),
            Value::Null,
            30_u64.into(),
            300_u64.into(),
            Value::Null,
        ])
        .unwrap();
    assert_eq!(projection.read().unwrap().row_count(), 3);
    assert_eq!(
        projection.read().unwrap().cell(2, 0).unwrap(),
        ValueRef::I64(3)
    );
    assert!(source.write().unwrap().pop_row());
    assert_eq!(projection.read().unwrap().row_count(), 2);
    {
        let mut write = projection.write().unwrap();
        assert!(write.tombstone_row(0).unwrap());
        assert!(write.row(0).unwrap().is_tombstoned());
        assert!(write.restore_row(0).unwrap());
        write.tombstone_row(0).unwrap();
    }
    assert_eq!(projection.read().unwrap().live_rows().count(), 1);
    source.write().unwrap().compact();
    let read = projection.read().unwrap();
    assert_eq!(read.row_count(), 1);
    assert_eq!(read.row(0).unwrap().position(), 0);
    assert_eq!(read.cell(0, 0).unwrap(), ValueRef::I64(2));
    assert_eq!(read.cell(0, 1).unwrap(), ValueRef::U64(20));
}

#[test]
fn borrowed_exports_and_owning_snapshots_keep_existing_selection_contracts() {
    let projection = source().into_shared_mut().project(&[1, 3]).unwrap();
    projection.write().unwrap().tombstone_row(1).unwrap();
    let snapshot = projection.materialize().unwrap().into_shared();
    assert_eq!(
        snapshot.table().property("owner"),
        Some(ValueRef::String("source"))
    );
    assert!(snapshot.row(1).unwrap().is_tombstoned());
    assert_eq!(
        snapshot.row(0).unwrap().get_named("extra"),
        Some(ValueRef::String("row extra"))
    );
    {
        let read = projection.read().unwrap();
        let borrowed = read.as_borrowed();
        assert!(std::ptr::eq(borrowed.table(), read.table()));
        assert!(std::ptr::eq(borrowed.schema(), read.schema()));
        assert_eq!(borrowed.positions(), &[0, 1]);
        assert_eq!(
            selection_to_json(&borrowed).unwrap(),
            shared_selection_to_json(&snapshot).unwrap()
        );
        let filtered = borrowed.filter_rows(|_| true).project(&[1]).unwrap();
        assert_eq!(selection_to_csv(&filtered, true).unwrap(), "qty\n10\n");
        assert_eq!(read.materialize().unwrap().schema(), snapshot.schema());
    }
    {
        let mut write = projection.write().unwrap();
        write.set_cell(0, 1, 88_u64).unwrap();
        assert_eq!(
            write.as_borrowed().row(0).unwrap().get(1),
            Some(ValueRef::U64(88))
        );
    }
    assert_eq!(snapshot.row(0).unwrap().get(1), Some(ValueRef::U64(10)));
    assert_eq!(
        projection.read().unwrap().cell(0, 1).unwrap(),
        ValueRef::U64(88)
    );
}

#[test]
fn schema_replacement_is_rejected_until_matching_metadata_is_available() {
    let projection = source().into_shared_mut();
    let source = projection.source_arc();
    let old_schema = source.read().unwrap().schema_arc();
    let changed = Schema::new(old_schema.iter().rev().cloned())
        .unwrap()
        .with_unknown_fields(old_schema.unknown_fields());
    *source.write().unwrap() = Table::new(changed);
    assert!(matches!(
        projection.read(),
        Err(SharedProjectionError::SchemaChanged)
    ));
    assert!(matches!(
        projection.write(),
        Err(SharedProjectionError::SchemaChanged)
    ));
    assert!(matches!(
        projection.materialize(),
        Err(SharedProjectionError::SchemaChanged)
    ));
    assert_eq!(
        projection.set_cell(0, 0, 1_i64),
        Err(SharedProjectionError::SchemaChanged)
    );
    assert!(matches!(
        projection.project(&[0]).unwrap().read(),
        Err(SharedProjectionError::SchemaChanged)
    ));
    let rebuilt = SharedTableProjection::from_shared(Arc::clone(&source)).unwrap();
    assert_eq!(rebuilt.read().unwrap().column_count(), 6);
    assert_eq!(rebuilt.read().unwrap().column(0).unwrap().name(), "blob");

    // Equal schemas need not have the same Arc identity.
    let equal_schema = Schema::new(old_schema.iter().cloned())
        .unwrap()
        .with_unknown_fields(old_schema.unknown_fields());
    *source.write().unwrap() = Table::new(equal_schema);
    assert_eq!(projection.read().unwrap().row_count(), 0);
    assert_eq!(projection.read().unwrap().column_count(), 6);
    assert!(matches!(
        rebuilt.read(),
        Err(SharedProjectionError::SchemaChanged)
    ));
}

#[test]
fn readers_share_the_lock_and_writers_hold_exclusive_access() {
    let projection = source().into_shared_mut();
    let source = projection.source_arc();
    let read = projection.read().unwrap();
    assert!(matches!(source.try_write(), Err(TryLockError::WouldBlock)));
    let another_source = Arc::clone(&source);
    assert!(
        std::thread::spawn(move || another_source.try_read().is_ok())
            .join()
            .unwrap()
    );
    drop(read);
    let mut write = projection.write().unwrap();
    assert!(matches!(source.try_read(), Err(TryLockError::WouldBlock)));
    assert!(matches!(source.try_write(), Err(TryLockError::WouldBlock)));
    write.set_cell(0, 3, 123_u64).unwrap();
    assert_eq!(write.cell(0, 3).unwrap(), ValueRef::U64(123));
    drop(write);
    assert_eq!(
        projection.read().unwrap().cell(0, 3).unwrap(),
        ValueRef::U64(123)
    );
}

#[test]
fn concurrent_read_modify_write_batches_have_no_lost_updates_or_partial_reads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SharedTableProjection>();
    let mut table = Table::new(
        Schema::new([
            ColumnSpec::new("left", DataType::I64),
            ColumnSpec::new("right", DataType::I64),
        ])
        .unwrap(),
    );
    table.push_row([0_i64.into(), 0_i64.into()]).unwrap();
    let projection = table.into_shared_mut();
    let barrier = Arc::new(Barrier::new(5));
    let mut workers = Vec::new();
    for _ in 0..4 {
        let projection = projection.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            for _ in 0..100 {
                let mut write = projection.write().unwrap();
                let next = write.cell(0, 0).unwrap().to_i64().unwrap() + 1;
                write.set_cell(0, 0, next).unwrap();
                write.set_cell(0, 1, next).unwrap();
            }
        }));
    }
    let observer = projection.clone();
    let reader = std::thread::spawn(move || {
        barrier.wait();
        for _ in 0..400 {
            let read = observer.read().unwrap();
            assert_eq!(read.cell(0, 0).unwrap(), read.cell(0, 1).unwrap());
        }
    });
    for worker in workers {
        worker.join().unwrap();
    }
    reader.join().unwrap();
    let read = projection.read().unwrap();
    assert_eq!(read.cell(0, 0).unwrap(), ValueRef::I64(400));
    assert_eq!(read.cell(0, 1).unwrap(), ValueRef::I64(400));
}

#[test]
fn write_panics_poison_the_lock_and_read_panics_do_not() {
    let projection = source().into_shared_mut();
    let reader = projection.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = reader.read().unwrap();
            panic!("read panic");
        })
        .join()
        .is_err()
    );
    projection.set_cell(0, 3, 77_u64).unwrap();
    let writer = projection.clone();
    assert!(
        std::thread::spawn(move || {
            let mut guard = writer.write().unwrap();
            guard.set_cell(0, 3, 88_u64).unwrap();
            panic!("write panic");
        })
        .join()
        .is_err()
    );
    assert!(matches!(
        projection.read(),
        Err(SharedProjectionError::LockPoisoned)
    ));
    assert!(matches!(
        projection.write(),
        Err(SharedProjectionError::LockPoisoned)
    ));
    assert!(matches!(
        projection.set_cell(0, 3, 99_u64),
        Err(SharedProjectionError::LockPoisoned)
    ));
    assert!(matches!(
        projection.materialize(),
        Err(SharedProjectionError::LockPoisoned)
    ));
    assert!(matches!(
        SharedTableProjection::from_shared(projection.source_arc()),
        Err(SharedProjectionError::LockPoisoned)
    ));
}

#[test]
fn converters_apply_to_writes_but_not_materialization() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let converter = ColumnConverter::new("text-to-id", move |_| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Value::I64(42))
    });
    let mut table = Table::new(
        Schema::new([ColumnSpec::new("id", DataType::I64).with_converter(converter)]).unwrap(),
    );
    table.push_row([1_i64.into()]).unwrap();
    let projection = table.into_shared_mut();
    projection.set_cell(0, 0, "forty two").unwrap();
    assert_eq!(
        projection.materialize().unwrap().cell(0, 0).unwrap(),
        ValueRef::I64(42)
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

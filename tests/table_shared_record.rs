//! Shared record lifetime, ordering, copy-on-write, and parallel access contracts.

use gd::{SharedRecordTable, TableError};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
struct Record {
    id: usize,
    text: String,
}

fn fixture() -> SharedRecordTable<Record> {
    let mut table = SharedRecordTable::with_capacity(10);
    for id in 0..10 {
        table.push(Record {
            id,
            text: format!("row {id}"),
        });
    }
    table
}

#[test]
fn copies_share_records_and_survive_source_drop() {
    let source = fixture();
    let target = source.copy_rows(&[9, 1, 9]).unwrap();
    assert!(Arc::ptr_eq(&source.as_slice()[9], &target.as_slice()[0]));
    assert!(Arc::ptr_eq(&target.as_slice()[0], &target.as_slice()[2]));
    assert_eq!(Arc::strong_count(&source.as_slice()[9]), 3);
    drop(source);
    assert_eq!(target.get(0).unwrap().text, "row 9");
    assert_eq!(target.get(1).unwrap().id, 1);
    assert!(target.get(3).is_none());
}

#[test]
fn edits_copy_only_the_shared_record() {
    let source = fixture();
    let mut target = source.copy_rows(&[2, 3]).unwrap();
    target.get_mut(0).unwrap().text.push_str(" edited");
    assert_eq!(source.get(2).unwrap().text, "row 2");
    assert_eq!(target.get(0).unwrap().text, "row 2 edited");
    assert!(!Arc::ptr_eq(&source.as_slice()[2], &target.as_slice()[0]));
    assert!(Arc::ptr_eq(&source.as_slice()[3], &target.as_slice()[1]));
    let pointer = Arc::as_ptr(&target.as_slice()[0]);
    target.get_mut(0).unwrap().id = 20;
    assert_eq!(pointer, Arc::as_ptr(&target.as_slice()[0]));
    assert!(target.get_mut(2).is_none());
}

#[test]
fn filtering_is_ordered_and_invalid_copy_is_atomic() {
    let source = fixture();
    let target = source.filter(|record| record.id % 3 == 0);
    assert_eq!(
        target.as_slice().iter().map(|r| r.id).collect::<Vec<_>>(),
        [0, 3, 6, 9]
    );
    assert_eq!(
        source.copy_rows(&[0, 10, 11]).unwrap_err(),
        TableError::RowOutOfBounds {
            row: 10,
            row_count: 10
        }
    );
    assert_eq!(Arc::strong_count(&source.as_slice()[1]), 1);
    assert_eq!(source.copy_rows(&[]).unwrap().row_count(), 0);
    assert_eq!(source.filter(|_| false).row_count(), 0);
    assert_eq!(SharedRecordTable::<Record>::default().row_count(), 0);
}

#[test]
fn copying_does_not_require_a_cloneable_record() {
    struct NonClone(usize);
    let mut source = SharedRecordTable::new();
    source.push_shared(Arc::new(NonClone(7)));
    let target = source.clone();
    assert_eq!(target.copy_rows(&[0]).unwrap().get(0).unwrap().0, 7);
}

#[cfg(feature = "rayon")]
#[test]
fn parallel_filter_and_gather_preserve_order_and_shared_ownership() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .unwrap();
    let source = fixture();
    let filtered = pool.install(|| source.par_filter(|record| record.id % 2 == 0));
    let copied = pool.install(|| source.par_copy_rows(&[9, 0, 9]).unwrap());
    assert_eq!(
        filtered.as_slice().iter().map(|r| r.id).collect::<Vec<_>>(),
        [0, 2, 4, 6, 8]
    );
    assert!(Arc::ptr_eq(&source.as_slice()[9], &copied.as_slice()[0]));
    assert_eq!(
        pool.install(|| source.par_copy_rows(&[10])).unwrap_err(),
        TableError::RowOutOfBounds {
            row: 10,
            row_count: 10
        }
    );
    drop(source);
    assert_eq!(copied.get(2).unwrap().text, "row 9");
    assert_eq!(filtered.row_count(), 5);
    assert_eq!(
        pool.install(|| SharedRecordTable::<Record>::new().par_filter(|_| true))
            .row_count(),
        0
    );
}

#[cfg(feature = "rayon")]
#[test]
fn chunked_filter_and_parallel_drop_preserve_lifetimes() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .unwrap();
    for chunk in [1, 7, 4096, 20_000] {
        let mut source = SharedRecordTable::new();
        for id in 0..10_001 {
            source.push(Record {
                id,
                text: format!("row {id}"),
            });
        }
        let target = pool.install(|| source.par_filter_chunked(chunk, |r| r.id % 3 == 0));
        assert_eq!(
            target.as_slice().iter().map(|r| r.id).collect::<Vec<_>>(),
            (0..10_001).step_by(3).collect::<Vec<_>>()
        );
        for record in target.as_slice() {
            assert!(Arc::ptr_eq(record, &source.as_slice()[record.id]));
            assert_eq!(Arc::strong_count(record), 2);
        }
        pool.install(|| target.par_drop());
        assert!(source.as_slice().iter().all(|r| Arc::strong_count(r) == 1));
        let target = pool.install(|| source.par_filter_chunked(chunk, |_| true));
        let weak: Vec<_> = source.as_slice().iter().map(Arc::downgrade).collect();
        drop(source);
        assert_eq!(target.get(10_000).unwrap().text, "row 10000");
        pool.install(|| target.par_drop());
        assert!(weak.iter().all(|r| r.upgrade().is_none()));
    }
    pool.install(|| SharedRecordTable::<Record>::new().par_drop());
    let record = Arc::new(Record {
        id: 7,
        text: "shared".into(),
    });
    let table = SharedRecordTable::from(vec![Arc::clone(&record); 10_001]);
    pool.install(|| table.par_drop());
    assert_eq!(Arc::strong_count(&record), 1);
}

#[cfg(feature = "rayon")]
#[test]
fn chunked_predicate_panic_releases_partial_results() {
    let source = fixture();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        source.par_filter_chunked(2, |r| {
            assert_ne!(r.id, 5);
            true
        })
    }));
    assert!(result.is_err());
    assert!(source.as_slice().iter().all(|r| Arc::strong_count(r) == 1));
}

#[cfg(feature = "rayon")]
#[test]
#[should_panic(expected = "parallel filter chunk size must be non-zero")]
fn chunked_filter_rejects_zero_grain() {
    let _ = fixture().par_filter_chunked(0, |_| true);
}

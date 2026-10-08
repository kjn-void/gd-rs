//! Experimental sharing pipelines; generated drivers supply the same fixtures.

#![allow(dead_code)] // Each strategy is compiled into its own comparison binary.

use gd::SharedRecordTable;
use rayon::{ThreadPool, prelude::*};
use std::{rc::Rc, sync::Arc};

pub fn two_pass<S: Send + Sync>(
    source: &SharedRecordTable<S>,
    predicate: impl Fn(&S) -> bool + Send + Sync,
) -> SharedRecordTable<S> {
    let records = source.as_slice();
    let rows: Vec<_> = records
        .par_iter()
        .enumerate()
        .filter_map(|(row, record)| predicate(record).then_some(row))
        .collect();
    SharedRecordTable::from(
        rows.par_iter()
            .map(|&row| Arc::clone(&records[row]))
            .collect::<Vec<_>>(),
    )
}

pub fn fused<S: Send + Sync>(
    source: &SharedRecordTable<S>,
    predicate: impl Fn(&S) -> bool + Send + Sync,
) -> SharedRecordTable<S> {
    source.par_filter(predicate)
}

pub fn fold_reduce<S: Send + Sync>(
    source: &SharedRecordTable<S>,
    predicate: impl Fn(&S) -> bool + Send + Sync,
) -> SharedRecordTable<S> {
    SharedRecordTable::from(
        source
            .as_slice()
            .par_iter()
            .fold(Vec::new, |mut records, record| {
                if predicate(record) {
                    records.push(Arc::clone(record));
                }
                records
            })
            .reduce(Vec::new, |mut left, right| {
                left.extend(right);
                left
            }),
    )
}

pub fn chunk_concat<S: Send + Sync>(
    source: &SharedRecordTable<S>,
    predicate: impl Fn(&S) -> bool + Send + Sync,
) -> SharedRecordTable<S> {
    let chunk = source
        .row_count()
        .div_ceil(rayon::current_num_threads())
        .max(1);
    let parts: Vec<Vec<_>> = source
        .as_slice()
        .par_chunks(chunk)
        .map(|part| {
            part.iter()
                .filter_map(|record| predicate(record).then(|| Arc::clone(record)))
                .collect()
        })
        .collect();
    let mut records = Vec::with_capacity(parts.iter().map(Vec::len).sum());
    for mut part in parts {
        records.append(&mut part);
    }
    SharedRecordTable::from(records)
}

pub fn caller_rc<S: Sync>(
    source: &[Rc<S>],
    predicate: impl Fn(&S) -> bool + Sync,
    pool: &ThreadPool,
) -> Vec<Rc<S>> {
    // Rc handles never enter Rayon. Borrowing payloads is safe when S: Sync.
    // The source cannot be dropped while these references are in use.
    let views: Vec<&S> = source.iter().map(Rc::as_ref).collect();
    let rows: Vec<_> = pool.install(|| {
        views
            .par_iter()
            .enumerate()
            .filter_map(|(row, record)| predicate(record).then_some(row))
            .collect()
    });
    // All reference-count changes, including target destruction, stay here.
    rows.iter().map(|&row| Rc::clone(&source[row])).collect()
}

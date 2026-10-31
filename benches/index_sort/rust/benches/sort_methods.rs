//! Rust counterpart of `../cpp/sort_methods_benchmark.cpp`.
//!
//! A 10,000-row table (or `INDEX_SORT_ROWS`) with every column set is sorted on each of its seven
//! columns with the port's key sort. Each iteration clears the target, appends the
//! source rows, and sorts, like the C++ cases. Separate gd-rs cases create an
//! order view, or create the view and materialize a new table in that order.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, SamplingMode, criterion_group, criterion_main};
use gd::{NullOrder, SortDirection};
use index_sort_bench::reused::Workspace;
use index_sort_bench::{
    C1, C2, C3, C4, C5, C6, C7, Table, benchmark_row_count, gd_order, generate_rows, key_sort,
};

fn sort_methods(criterion: &mut Criterion) {
    let rows = benchmark_row_count();
    let source = Table::from_source(&generate_rows(rows));
    let mut target = Table::with_capacity(rows);
    let mut group = criterion.benchmark_group("sort_methods");
    if rows > 100_000 {
        // An iteration takes seconds: one per sample, ten samples.
        group.sampling_mode(SamplingMode::Flat).sample_size(10);
    }

    let columns: [fn(&mut Table); 7] = [
        key_sort::<C1>,
        key_sort::<C2>,
        key_sort::<C3>,
        key_sort::<C4>,
        key_sort::<C5>,
        key_sort::<C6>,
        key_sort::<C7>,
    ];
    let reused: [fn(&mut Workspace, &mut Table, bool); 7] = [
        Workspace::sort::<C1>,
        Workspace::sort::<C2>,
        Workspace::sort::<C3>,
        Workspace::sort::<C4>,
        Workspace::sort::<C5>,
        Workspace::sort::<C6>,
        Workspace::sort::<C7>,
    ];
    for (column, sort) in columns.into_iter().enumerate() {
        for (variant, mode) in [("key", 0), ("reuse_index", 1), ("reuse_key", 2)] {
            let mut workspace = Workspace::new(rows);
            group.bench_function(BenchmarkId::new(variant, column), |bencher| {
                bencher.iter(|| {
                    target.append_from(&source);
                    if mode == 0 {
                        sort(&mut target);
                    } else {
                        reused[column](&mut workspace, &mut target, mode == 2);
                    }
                    black_box(&target);
                    black_box(target.len())
                });
            });
        }
    }

    group.finish();
}

fn gd_orders(criterion: &mut Criterion) {
    let rows = benchmark_row_count();
    let source = gd_order::from_source(&generate_rows(rows));
    let mut group = criterion.benchmark_group("gd_rs");
    if rows > 100_000 {
        group.sampling_mode(SamplingMode::Flat).sample_size(10);
    }
    for column in 0..7 {
        group.bench_function(BenchmarkId::new("order_view", column), |bencher| {
            bencher.iter(|| {
                black_box(
                    source
                        .row_order(column, SortDirection::Ascending, NullOrder::Last)
                        .unwrap(),
                )
            });
        });
        group.bench_function(BenchmarkId::new("view_materialize", column), |bencher| {
            bencher.iter(|| {
                let order = source
                    .row_order(column, SortDirection::Ascending, NullOrder::Last)
                    .unwrap();
                black_box(source.copy_rows(order.positions()).unwrap());
            });
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = sort_methods, gd_orders
}
criterion_main!(benches);

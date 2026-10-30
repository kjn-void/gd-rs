//! Rust counterpart of `../cpp/sort_methods_benchmark.cpp`.
//!
//! A 10,000-row table with every column set is sorted on each of its seven
//! columns with the port's key sort. Each iteration clears the target, appends the
//! source rows, and sorts, like the C++ cases.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use index_sort_bench::{C1, C2, C3, C4, C5, C6, C7, ROW_COUNT, Table, generate_rows, key_sort};

fn sort_methods(criterion: &mut Criterion) {
    let source = Table::from_source(&generate_rows(ROW_COUNT));
    let mut target = Table::with_capacity(ROW_COUNT);
    let mut group = criterion.benchmark_group("sort_methods");

    let columns: [fn(&mut Table); 7] = [
        key_sort::<C1>,
        key_sort::<C2>,
        key_sort::<C3>,
        key_sort::<C4>,
        key_sort::<C5>,
        key_sort::<C6>,
        key_sort::<C7>,
    ];
    for (column, sort) in columns.into_iter().enumerate() {
        group.bench_function(BenchmarkId::new("key", column), |bencher| {
            bencher.iter(|| {
                target.append_from(&source);
                sort(&mut target);
                black_box(target.len())
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
    targets = sort_methods
}
criterion_main!(benches);

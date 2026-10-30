//! Rust counterpart of `../cpp/sort_methods_benchmark.cpp`.
//!
//! A 10,000-row table with every column set is sorted on each of its seven
//! columns three ways: physical, index, and key. Each iteration clears the
//! target, appends the source rows, and sorts, like the C++ cases.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use index_sort_bench::{
    C1, C2, C3, C4, C5, C6, C7, SORT_METHODS_ROW_COUNT, Table, generate_all_columns, index_sort,
    key_sort, physical_sort,
};

type Sort = fn(&mut Table);

/// Physical, index, and key sort for one column.
macro_rules! methods {
    ($column:expr) => {
        [
            ("physical", physical_sort::<$column> as Sort),
            ("index", index_sort::<$column>),
            ("key", key_sort::<$column>),
        ]
    };
}

fn sort_methods(criterion: &mut Criterion) {
    let source = Table::from_source(&generate_all_columns(SORT_METHODS_ROW_COUNT));
    let mut target = Table::with_capacity(SORT_METHODS_ROW_COUNT);
    let mut group = criterion.benchmark_group("sort_methods");

    let columns = [
        methods!(C1),
        methods!(C2),
        methods!(C3),
        methods!(C4),
        methods!(C5),
        methods!(C6),
        methods!(C7),
    ];
    // Upstream order: for each column, physical, index, then key.
    for (column, methods) in columns.iter().enumerate() {
        for &(method, sort) in methods {
            group.bench_function(BenchmarkId::new(method, column), |bencher| {
                bencher.iter(|| {
                    target.append_from(&source);
                    sort(&mut target);
                    black_box(target.len())
                });
            });
        }
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

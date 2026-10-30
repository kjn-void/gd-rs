//! Rust counterpart of `../cpp/index_sort_benchmark.cpp`.
//!
//! Each iteration clears the target, appends the source rows, and sorts on one
//! column, like the C++ cases. Groups are named by row count.
//!
//! The 1,000- and 10^6-row groups run all six cases. The 10,000-row group compares
//! the two ways of applying the index sort's permutation: swapping rows in place
//! (cases 04 to 06) and building a new table (cases 07 to 09).

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use index_sort_bench::{
    C1, C2, C7, ROW_COUNT, Table, generate_rows, index_gather_sort, index_sort, physical_sort,
};

const TEN_THOUSAND_ROWS: usize = 10_000;
const MILLION_ROWS: usize = 1_000_000;

type Case = (&'static str, fn(&mut Table));

const UPSTREAM_CASES: &[Case] = &[
    ("00_append_only", |_| {}),
    ("01_physical_uint64", physical_sort::<C1>),
    ("02_physical_string8", physical_sort::<C2>),
    ("03_physical_string50", physical_sort::<C7>),
    ("04_index_uint64", index_sort::<C1>),
    ("05_index_string8", index_sort::<C2>),
    ("06_index_string50", index_sort::<C7>),
];

const SWAP_VERSUS_GATHER_CASES: &[Case] = &[
    ("04_index_uint64", index_sort::<C1>),
    ("05_index_string8", index_sort::<C2>),
    ("06_index_string50", index_sort::<C7>),
    ("07_index_gather_uint64", index_gather_sort::<C1>),
    ("08_index_gather_string8", index_gather_sort::<C2>),
    ("09_index_gather_string50", index_gather_sort::<C7>),
];

fn row_count(criterion: &mut Criterion, count: usize, cases: &[Case]) {
    let source = Table::from_source(&generate_rows(count));
    let mut target = Table::with_capacity(count);
    let mut group = criterion.benchmark_group(count.to_string());
    if count == MILLION_ROWS {
        group.sample_size(10);
    }

    for &(name, sort) in cases {
        group.bench_function(BenchmarkId::from_parameter(name), |bencher| {
            bencher.iter(|| {
                target.append_from(&source);
                sort(&mut target);
                black_box(target.len())
            });
        });
    }

    group.finish();
}

fn thousand_rows(criterion: &mut Criterion) {
    row_count(criterion, ROW_COUNT, UPSTREAM_CASES);
}

fn ten_thousand_rows(criterion: &mut Criterion) {
    row_count(criterion, TEN_THOUSAND_ROWS, SWAP_VERSUS_GATHER_CASES);
}

fn million_rows(criterion: &mut Criterion) {
    row_count(criterion, MILLION_ROWS, UPSTREAM_CASES);
}

criterion_group! {
    name = thousand;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = thousand_rows, ten_thousand_rows
}
criterion_group! {
    name = million;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(15));
    targets = million_rows
}
criterion_main!(thousand, million);

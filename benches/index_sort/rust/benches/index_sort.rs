//! Rust counterpart of `../cpp/index_sort_benchmark.cpp`.
//!
//! Each iteration clears the target, appends the source rows, and sorts on one
//! column, like the C++ cases. Groups are named by row count.
//!
//! The 1,000-row group runs all six cases. The 1,000,000-row group skips the
//! selection sorts: at about n²/2 = 5·10¹¹ comparisons, one iteration would take
//! roughly 20 minutes.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use index_sort_bench::{C1, C2, C7, ROW_COUNT, Table, generate_rows, index_sort, selection_sort};

const MILLION_ROWS: usize = 1_000_000;

fn row_count(criterion: &mut Criterion, count: usize) {
    let source = Table::from_source(&generate_rows(count));
    let mut target = Table::with_capacity(count);
    let with_selection_sort = count <= ROW_COUNT;

    let mut group = criterion.benchmark_group(count.to_string());
    if !with_selection_sort {
        group.sample_size(10);
    }

    let mut case = |name: &str, sort: fn(&mut Table)| {
        group.bench_function(BenchmarkId::from_parameter(name), |bencher| {
            bencher.iter(|| {
                target.append_from(&source);
                sort(&mut target);
                black_box(target.len())
            });
        });
    };

    case("00_append_only", |_| {});
    if with_selection_sort {
        case("01_physical_uint64", selection_sort::<C1>);
        case("02_physical_string8", selection_sort::<C2>);
        case("03_physical_string50", selection_sort::<C7>);
    }
    case("04_index_uint64", index_sort::<C1>);
    case("05_index_string8", index_sort::<C2>);
    case("06_index_string50", index_sort::<C7>);

    group.finish();
}

fn thousand_rows(criterion: &mut Criterion) {
    row_count(criterion, ROW_COUNT);
}

fn million_rows(criterion: &mut Criterion) {
    row_count(criterion, MILLION_ROWS);
}

criterion_group! {
    name = thousand;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = thousand_rows
}
criterion_group! {
    name = million;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(15));
    targets = million_rows
}
criterion_main!(thousand, million);

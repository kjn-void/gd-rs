//! Sequential iterator sums versus lane-split `Column::sum_f64` / `sum_f32`.

use std::{hint::black_box, time::Duration};

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use gd::{ColumnSpec, DataType, Schema, Table, Value};

const ROWS: usize = 1_000_000;
const NULL_INTERVAL: usize = 10;

fn sample(row: usize) -> u16 {
    // A fixed pseudo-random pattern of integers 0..1000, exact in f32 and f64.
    u16::try_from((row as u64).wrapping_mul(2_654_435_761) % 1_000).unwrap()
}

fn make_table(data_type: DataType, value: fn(u16) -> Value) -> Table {
    let schema = Schema::new([
        ColumnSpec::new("required", data_type),
        ColumnSpec::new("nullable", data_type).nullable(true),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, ROWS);
    for row in 0..ROWS {
        let nullable = if row % NULL_INTERVAL == 0 {
            Value::Null
        } else {
            value(sample(row))
        };
        table.push_row([value(sample(row)), nullable]).unwrap();
    }
    table
}

#[inline(never)]
fn sequential_f64(values: &[f64]) -> f64 {
    values.iter().sum()
}

#[inline(never)]
fn sequential_nullable_f64(values: &[Option<f64>]) -> f64 {
    values.iter().flatten().sum()
}

#[inline(never)]
fn sequential_f32(values: &[f32]) -> f32 {
    values.iter().sum()
}

#[inline(never)]
fn sequential_nullable_f32(values: &[Option<f32>]) -> f32 {
    values.iter().flatten().sum()
}

fn float_sum(criterion: &mut Criterion) {
    let table = make_table(DataType::F64, |value| Value::F64(f64::from(value)));
    let required = table.column(0).unwrap();
    let nullable = table.column(1).unwrap();
    let required_values = required.as_slice::<f64>().unwrap();
    let nullable_values = nullable.as_nullable_slice::<f64>().unwrap();
    // Integer-valued inputs keep every partial sum exact, so both orders agree.
    assert_eq!(sequential_f64(required_values), required.sum_f64().unwrap());
    assert_eq!(
        sequential_nullable_f64(nullable_values),
        nullable.sum_f64().unwrap()
    );

    let mut group = criterion.benchmark_group("Table/FloatSum/1000000/F64");
    configure(&mut group);
    group.bench_function("required-iter-sum", |bencher| {
        bencher.iter(|| black_box(sequential_f64(black_box(required_values))));
    });
    group.bench_function("required-sum_f64", |bencher| {
        bencher.iter(|| black_box(black_box(required).sum_f64()));
    });
    group.bench_function("nullable-iter-flatten-sum", |bencher| {
        bencher.iter(|| black_box(sequential_nullable_f64(black_box(nullable_values))));
    });
    group.bench_function("nullable-sum_f64", |bencher| {
        bencher.iter(|| black_box(black_box(nullable).sum_f64()));
    });
    group.finish();

    let table = make_table(DataType::F32, |value| Value::F32(f32::from(value)));
    let required = table.column(0).unwrap();
    let nullable = table.column(1).unwrap();
    let required_values = required.as_slice::<f32>().unwrap();
    let nullable_values = nullable.as_nullable_slice::<f32>().unwrap();

    let mut group = criterion.benchmark_group("Table/FloatSum/1000000/F32");
    configure(&mut group);
    group.bench_function("required-iter-sum", |bencher| {
        bencher.iter(|| black_box(sequential_f32(black_box(required_values))));
    });
    group.bench_function("required-sum_f32", |bencher| {
        bencher.iter(|| black_box(black_box(required).sum_f32()));
    });
    group.bench_function("nullable-iter-flatten-sum", |bencher| {
        bencher.iter(|| black_box(sequential_nullable_f32(black_box(nullable_values))));
    });
    group.bench_function("nullable-sum_f32", |bencher| {
        bencher.iter(|| black_box(black_box(nullable).sum_f32()));
    });
    group.finish();
}

fn configure(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group
        .throughput(Throughput::Elements(ROWS as u64))
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
}

criterion_group!(benches, float_sum);
criterion_main!(benches);

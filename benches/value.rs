//! Criterion benchmarks for owned and borrowed GD values.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gd::{Value, ValueRef};

fn construct_integer(criterion: &mut Criterion) {
    criterion.bench_function("ConstructInteger", |bencher| {
        bencher.iter(|| black_box(Value::from(black_box(42_i64))));
    });
}

fn dynamic_strings(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("ValueString");
    for size in [8_usize, 64, 512, 4096, 32_768] {
        let source = "x".repeat(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("ConstructString", size),
            &source,
            |bencher, source| {
                bencher.iter(|| black_box(Value::from(black_box(source.as_str()))));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("BorrowString", size),
            &source,
            |bencher, source| {
                bencher.iter(|| black_box(ValueRef::from(black_box(source.as_str()))));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, construct_integer, dynamic_strings);
criterion_main!(benches);

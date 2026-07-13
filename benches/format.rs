//! Criterion benchmarks for table JSON and CSV serialization.

use std::{hint::black_box, sync::LazyLock};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gd::{
    Arguments, ColumnSpec, DataType, Schema, Table, Value, arguments_to_json, arguments_to_uri,
    table_to_csv, table_to_json,
};

static GROUPS: LazyLock<Vec<Value>> = LazyLock::new(|| {
    (0..16)
        .map(|group| Value::from(format!("group-{group}")))
        .collect()
});

fn table(rows: usize) -> Table {
    let schema = Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("group", DataType::String),
        ColumnSpec::new("value", DataType::I64),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, rows);
    for row in 0..rows {
        table
            .push_row([
                Value::U64(row as u64),
                GROUPS[row % GROUPS.len()].clone(),
                Value::I64(i64::try_from(row).unwrap()),
            ])
            .unwrap();
    }
    table
}

fn formatting(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Format/Table");
    for rows in [100_usize, 1_000, 10_000] {
        let table = table(rows);
        group.throughput(Throughput::Elements(rows as u64));
        group.bench_with_input(BenchmarkId::new("Json", rows), &table, |bencher, table| {
            bencher.iter(|| table_to_json(black_box(table)).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("Csv", rows), &table, |bencher, table| {
            bencher.iter(|| table_to_csv(black_box(table), true).unwrap());
        });
    }
    group.finish();
}

fn arguments() -> Arguments {
    let mut values = Arguments::new();
    values.push_named("scheme", "https");
    values.push_named("host", "example.com");
    values.push_named("port", 443_i32);
    values.push_named("path", "/api/users");
    values.push_named("query", "limit=10&offset=20");
    values.push_named("fragment", "section1");
    values.push_named("user", "admin");
    values.push_named("password", "secret123");
    values.push_named("secure", true);
    values.push_named("timeout", 5_000_i32);
    values.push_named("retry_count", 3_i32);
    values
}

fn argument_formatting(criterion: &mut Criterion) {
    let arguments = arguments();
    criterion.bench_function("Format/Arguments/UriJson", |bencher| {
        bencher.iter(|| {
            let json = arguments_to_json(black_box(&arguments)).unwrap();
            let uri = arguments_to_uri(black_box(&arguments)).unwrap();
            black_box((json, uri))
        });
    });
}

criterion_group!(benches, formatting, argument_formatting);
criterion_main!(benches);

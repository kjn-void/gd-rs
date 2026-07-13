//! Criterion benchmarks for typed table storage and column indexes.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gd::{
    ColumnSpec, DataType, IndexKeyRef, NullOrder, Schema, SortDirection, Table, Value, ValueRef,
};

fn schema() -> Schema {
    Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("group", DataType::String),
        ColumnSpec::new("value", DataType::I64),
    ])
    .unwrap()
}

fn make_table(rows: usize) -> Table {
    let mut table = Table::with_capacity(schema(), rows);
    for row in 0..rows {
        table
            .push_row([
                Value::U64(row as u64),
                Value::from(format!("group-{}", row % 16)),
                Value::I64(i64::try_from(row).unwrap()),
            ])
            .unwrap();
    }
    table
}

fn make_table_prepared(rows: usize) -> Table {
    let groups: Vec<_> = (0..16)
        .map(|group| Value::from(format!("group-{group}")))
        .collect();
    let mut table = Table::with_capacity(schema(), rows);
    for row in 0..rows {
        table
            .push_row([
                Value::U64(row as u64),
                groups[row % groups.len()].clone(),
                Value::I64(i64::try_from(row).unwrap()),
            ])
            .unwrap();
    }
    table
}

fn append_rows(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Table/AppendRows");
    for rows in [10_usize, 100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(rows as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(rows),
            &rows,
            |bencher, &rows| {
                bencher.iter(|| black_box(make_table(black_box(rows))));
            },
        );
    }
    group.finish();

    let rows = 10_000_usize;
    criterion.bench_function("Table/AppendRowsPrepared/10000", |bencher| {
        bencher.iter(|| black_box(make_table_prepared(black_box(rows))));
    });
}

fn scans(criterion: &mut Criterion) {
    let table = make_table(100_000);
    criterion.bench_function("Table/ColumnScan/100000", |bencher| {
        let column = table.column_named("value").unwrap();
        bencher.iter(|| {
            let sum: i64 = column
                .iter()
                .map(|value| match value {
                    ValueRef::I64(value) => value,
                    _ => unreachable!(),
                })
                .sum();
            black_box(sum)
        });
    });
    criterion.bench_function("Table/RowScan/100000", |bencher| {
        bencher.iter(|| {
            let sum: i64 = table
                .rows()
                .map(|row| match row.get(2).unwrap() {
                    ValueRef::I64(value) => value,
                    _ => unreachable!(),
                })
                .sum();
            black_box(sum)
        });
    });
    criterion.bench_function("Table/NamedCellScan/100000", |bencher| {
        bencher.iter(|| {
            let sum: i64 = (0..table.row_count())
                .map(|row| match table.cell_named(row, "value").unwrap() {
                    ValueRef::I64(value) => value,
                    _ => unreachable!(),
                })
                .sum();
            black_box(sum)
        });
    });
}

fn indexes(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Table/Index");
    for rows in [100_usize, 1_000, 10_000, 100_000] {
        let table = make_table(rows);
        group.bench_with_input(BenchmarkId::new("build", rows), &rows, |bencher, _| {
            bencher.iter(|| black_box(table.index(0).unwrap()));
        });
        let index = table.index(0).unwrap();
        let key = IndexKeyRef::from((rows - 1) as u64);
        group.bench_with_input(BenchmarkId::new("find-last", rows), &rows, |bencher, _| {
            bencher.iter(|| black_box(index.rows(black_box(key))));
        });
        group.bench_with_input(
            BenchmarkId::new("find-missing", rows),
            &rows,
            |bencher, _| {
                bencher.iter(|| black_box(index.rows(black_box(IndexKeyRef::from(u64::MAX)))));
            },
        );
    }
    group.finish();
}

fn row_order(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Table/RowOrder");
    for rows in [100_usize, 1_000, 5_000, 10_000, 100_000] {
        let schema = Schema::new([ColumnSpec::new("key", DataType::U64)]).unwrap();
        let mut table = Table::with_capacity(schema, rows);
        for row in 0..rows {
            let key = (row as u64).wrapping_mul(48_271) % rows as u64;
            table.push_row([Value::U64(key)]).unwrap();
        }
        group.throughput(Throughput::Elements(rows as u64));
        group.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| {
                black_box(
                    table
                        .row_order(0, SortDirection::Ascending, NullOrder::Last)
                        .unwrap(),
                )
            });
        });
    }
    group.finish();
}

criterion_group!(benches, append_rows, scans, indexes, row_order);
criterion_main!(benches);

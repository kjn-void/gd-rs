//! Criterion benchmarks for typed table storage and column indexes.

use std::{hint::black_box, time::Duration};

use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group, criterion_main,
};
use gd::{
    ColumnSpec, DataType, IndexKeyRef, NullOrder, Schema, SortDirection, Table, UnknownFields,
    Value, ValueRef,
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

fn open_schema() -> Schema {
    Schema::new([ColumnSpec::new("id", DataType::U64)])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store)
}

fn make_open_table(rows: usize) -> Table {
    let mut table = Table::with_capacity(open_schema(), rows);
    for row in 0..rows {
        let row_index = table.push_row([Value::U64(row as u64)]).unwrap();
        table
            .set_named(
                row_index,
                "custom_category",
                if row % 2 == 0 { "binary" } else { "text" },
            )
            .unwrap();
        table
            .set_named(
                row_index,
                "custom_region",
                if row % 3 == 0 { "north" } else { "south" },
            )
            .unwrap();
    }
    table
}

fn make_open_table_atomic(rows: usize) -> Table {
    let mut table = Table::with_capacity(open_schema(), rows);
    for row in 0..rows {
        table
            .push_row_with_extras(
                [Value::U64(row as u64)],
                [
                    (
                        "custom_category",
                        Value::from(if row % 2 == 0 { "binary" } else { "text" }),
                    ),
                    (
                        "custom_region",
                        Value::from(if row % 3 == 0 { "north" } else { "south" }),
                    ),
                ],
            )
            .unwrap();
    }
    table
}

fn make_wide_open_table(names: &[String], atomic: bool) -> Table {
    let mut table = Table::with_capacity(open_schema(), 1_000);
    for row in 0..1_000_usize {
        if atomic {
            table
                .push_row_with_extras(
                    [Value::U64(row as u64)],
                    names
                        .iter()
                        .enumerate()
                        .map(|(field, name)| (name.as_str(), Value::U64((row + field) as u64))),
                )
                .unwrap();
        } else {
            let row_index = table.push_row([Value::U64(row as u64)]).unwrap();
            for (field, name) in names.iter().enumerate() {
                table
                    .set_named(row_index, name, Value::U64((row + field) as u64))
                    .unwrap();
            }
        }
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

fn open_schema_fields(criterion: &mut Criterion) {
    let mut append = criterion.benchmark_group("Table/OpenSchema/AppendTwoFields");
    for rows in [100_usize, 1_000, 10_000] {
        append.throughput(Throughput::Elements(rows as u64));
        append.bench_with_input(
            BenchmarkId::from_parameter(rows),
            &rows,
            |bencher, &rows| {
                bencher.iter(|| black_box(make_open_table(black_box(rows))));
            },
        );
    }
    append.finish();

    let mut atomic_append = criterion.benchmark_group("Table/OpenSchema/AppendTwoFieldsAtomic");
    for rows in [100_usize, 1_000, 10_000] {
        atomic_append.throughput(Throughput::Elements(rows as u64));
        atomic_append.bench_with_input(
            BenchmarkId::from_parameter(rows),
            &rows,
            |bencher, &rows| {
                bencher.iter(|| black_box(make_open_table_atomic(black_box(rows))));
            },
        );
    }
    atomic_append.finish();

    let mut lookup = criterion.benchmark_group("Table/OpenSchema/LookupTwoFields");
    for rows in [100_usize, 1_000, 10_000, 100_000] {
        let table = make_open_table(rows);
        lookup.throughput(Throughput::Elements(rows as u64));
        lookup.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| {
                let total_length: usize = (0..table.row_count())
                    .map(|row| {
                        let category = match table.cell_named(row, "custom_category").unwrap() {
                            ValueRef::String(value) => value.len(),
                            _ => unreachable!(),
                        };
                        let region = match table.cell_named(row, "custom_region").unwrap() {
                            ValueRef::String(value) => value.len(),
                            _ => unreachable!(),
                        };
                        category + region
                    })
                    .sum();
                black_box(total_length)
            });
        });
    }
    lookup.finish();
}

fn wide_open_schema(criterion: &mut Criterion) {
    let names: Vec<_> = (0..1_000)
        .map(|field| format!("field_{field:04}"))
        .collect();
    let mut group = criterion.benchmark_group("Table/OpenSchemaWide/1000x1000");
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    group.bench_function("BuildLate", |bencher| {
        bencher.iter(|| black_box(make_wide_open_table(black_box(&names), false)));
    });
    group.bench_function("BuildAtomic", |bencher| {
        bencher.iter(|| black_box(make_wide_open_table(black_box(&names), true)));
    });

    let table = make_wide_open_table(&names, true);
    group.bench_function("LookupAll", |bencher| {
        bencher.iter(|| {
            let mut sum = 0_u64;
            for row in 0..table.row_count() {
                for name in &names {
                    match table.cell_named(row, name).unwrap() {
                        ValueRef::U64(value) => sum = sum.wrapping_add(value),
                        _ => unreachable!(),
                    }
                }
            }
            black_box(sum)
        });
    });
    group.finish();
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

criterion_group!(
    benches,
    append_rows,
    scans,
    open_schema_fields,
    wide_open_schema,
    indexes,
    row_order
);
criterion_main!(benches);

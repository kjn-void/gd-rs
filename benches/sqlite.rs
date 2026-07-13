//! Criterion benchmarks for materializing `SQLite` results as typed tables.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gd::{Arguments, ColumnSpec, DataType, Schema, SqliteDatabase};

fn fixture(rows: usize) -> SqliteDatabase {
    let database = SqliteDatabase::open_in_memory().unwrap();
    database
        .execute_batch(
            "CREATE TABLE item(id INTEGER NOT NULL, group_name TEXT NOT NULL, value INTEGER NOT NULL);
             BEGIN",
        )
        .unwrap();
    {
        let mut statement = database
            .connection()
            .prepare("INSERT INTO item VALUES (?1, ?2, ?3)")
            .unwrap();
        for row in 0..rows {
            let id = i64::try_from(row).unwrap();
            statement
                .execute((id, format!("group-{}", row % 16), -id))
                .unwrap();
        }
    }
    database.execute_batch("COMMIT").unwrap();
    database
}

fn schema() -> Schema {
    Schema::new([
        ColumnSpec::new("id", DataType::I64),
        ColumnSpec::new("group_name", DataType::String),
        ColumnSpec::new("value", DataType::I64),
    ])
    .unwrap()
}

fn query(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("SQLite/QueryTable");
    for rows in [100_usize, 1_000, 10_000] {
        let database = fixture(rows);
        group.throughput(Throughput::Elements(rows as u64));
        group.bench_with_input(BenchmarkId::new("inferred", rows), &rows, |bencher, _| {
            bencher.iter(|| {
                database
                    .query_table(
                        black_box("SELECT id, group_name, value FROM item"),
                        black_box(&Arguments::new()),
                    )
                    .unwrap()
            });
        });
        group.bench_with_input(BenchmarkId::new("schema", rows), &rows, |bencher, _| {
            bencher.iter(|| {
                database
                    .query_table_with_schema(
                        black_box("SELECT id, group_name, value FROM item"),
                        black_box(&Arguments::new()),
                        black_box(schema()),
                    )
                    .unwrap()
            });
        });
    }
    group.finish();
}

criterion_group!(benches, query);
criterion_main!(benches);

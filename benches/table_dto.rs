//! Criterion port of GD's "[benchmark] table dto" test.
//!
//! The C++ counterpart is `benches/cpp-reference/table_dto_benchmark.cpp`. Both build
//! 10,000 rows of three nullable `u64` inputs and a nullable `SUM` column, then write
//! each row's sum and return the grand total, which is checked on every iteration.
//! GD's `(uint64_t)` cast and raw struct casts have no safe gd-rs equivalent; the
//! typed-column variants are the corresponding safe bulk forms.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};

const ROWS: u64 = 10_000;

/// A benchmark case: name, whether the schema is nullable, and the row-sum body.
type Case = (&'static str, bool, fn(&mut Table) -> u64);

fn table(nullable: bool) -> (Table, u64) {
    let schema = Schema::new(
        ["C1", "C2", "C3", "SUM"]
            .map(|name| ColumnSpec::new(name, DataType::U64).nullable(nullable)),
    )
    .expect("benchmark schema is valid");
    let mut table =
        Table::with_capacity(schema, usize::try_from(ROWS).expect("row count fits usize"));
    let sum = if nullable { Value::Null } else { Value::U64(0) };
    for u in 0..ROWS {
        table
            .push_row([
                Value::U64(u),
                Value::U64(u * 2),
                Value::U64(u * 3),
                sum.clone(),
            ])
            .expect("benchmark row is valid");
    }
    let checksum = table
        .rows()
        .flat_map(|row| (0..3).map(move |column| row.get(column)))
        .map(|value| match value {
            Some(ValueRef::U64(value)) => value,
            other => panic!("unexpected input cell {other:?}"),
        })
        .sum();
    (table, checksum)
}

/// Mirrors `Sum01`: row iteration with a null check, writing through the row view.
fn sum01_row_iterator(table: &mut Table) -> u64 {
    let mut total = 0;
    table.rows_mut().for_each(|mut row| {
        let mut sum = 0;
        for column in 0..3 {
            if let Some(ValueRef::U64(value)) = row.get(column) {
                sum += value;
            }
        }
        row.set(3, Value::U64(sum)).expect("SUM is a u64 column");
        total += sum;
    });
    total
}

/// Mirrors `Sum02`: indexed reads with a null check and indexed writes.
fn sum02_indexed_null_check(table: &mut Table) -> u64 {
    let mut total = 0;
    for row in 0..table.row_count() {
        let mut sum = 0;
        for column in 0..3 {
            if let Ok(ValueRef::U64(value)) = table.cell(row, column) {
                sum += value;
            }
        }
        table
            .set_cell(row, 3, Value::U64(sum))
            .expect("SUM is a u64 column");
        total += sum;
    }
    total
}

/// Mirrors `Sum03`: indexed reads that assume a non-null value.
fn sum03_indexed_no_null_check(table: &mut Table) -> u64 {
    let mut total = 0;
    for row in 0..table.row_count() {
        let mut sum = 0;
        for column in 0..3 {
            let Ok(ValueRef::U64(value)) = table.cell(row, column) else {
                panic!("input cell is not a non-null u64");
            };
            sum += value;
        }
        table
            .set_cell(row, 3, Value::U64(sum))
            .expect("SUM is a u64 column");
        total += sum;
    }
    total
}

/// Borrows a nullable `u64` column's dense values and validity bytes.
fn split(column: gd::Column<'_>) -> (&[u64], &[bool]) {
    let values = column
        .as_nullable_slice::<u64>()
        .expect("input is a nullable u64 column");
    (values.values(), values.validity())
}

/// Safe counterpart of `Sum04`/`Sum05` for the nullable schema used by GD.
fn sum04_typed_nullable_columns(table: &mut Table) -> u64 {
    let ([c1, c2, c3], [sum]) = table
        .columns_io([0, 1, 2], [3])
        .expect("distinct benchmark columns");
    let ((c1, v1), (c2, v2), (c3, v3)) = (split(c1), split(c2), split(c3));
    let (sum, sum_valid) = sum
        .as_nullable_mut_slice::<u64>()
        .expect("SUM is a nullable u64 column")
        .into_parts();
    let inputs = c1
        .iter()
        .zip(v1)
        .zip(c2.iter().zip(v2))
        .zip(c3.iter().zip(v3));
    let mut total = 0;
    for ((out, out_valid), (((&a, &a_valid), (&b, &b_valid)), (&c, &c_valid))) in
        sum.iter_mut().zip(sum_valid.iter_mut()).zip(inputs)
    {
        let row_sum = a * u64::from(a_valid) + b * u64::from(b_valid) + c * u64::from(c_valid);
        *out = row_sum;
        *out_valid = true;
        total += row_sum;
    }
    total
}

/// Safe counterpart of `Sum05`, which reads raw values without consulting nulls.
fn sum05_typed_required_columns(table: &mut Table) -> u64 {
    let ([c1, c2, c3], [sum]) = table
        .columns_io([0, 1, 2], [3])
        .expect("distinct benchmark columns");
    let c1 = c1.as_slice::<u64>().expect("C1 is required u64");
    let c2 = c2.as_slice::<u64>().expect("C2 is required u64");
    let c3 = c3.as_slice::<u64>().expect("C3 is required u64");
    let sum = sum.as_mut_slice::<u64>().expect("SUM is required u64");
    let mut total = 0;
    for (((out, a), b), c) in sum.iter_mut().zip(c1).zip(c2).zip(c3) {
        let row_sum = a + b + c;
        *out = row_sum;
        total += row_sum;
    }
    total
}

fn table_dto(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("TableDto");
    group.throughput(Throughput::Elements(ROWS));
    let cases: [Case; 5] = [
        ("Sum01_RowIterator", true, sum01_row_iterator),
        ("Sum02_IndexedNullCheck", true, sum02_indexed_null_check),
        (
            "Sum03_IndexedNoNullCheck",
            true,
            sum03_indexed_no_null_check,
        ),
        (
            "Sum04_TypedNullableColumns",
            true,
            sum04_typed_nullable_columns,
        ),
        (
            "Sum05_TypedRequiredColumns",
            false,
            sum05_typed_required_columns,
        ),
    ];
    for (name, nullable, case) in cases {
        let (mut table, checksum) = table(nullable);
        group.bench_function(name, |bencher| {
            bencher.iter(|| assert_eq!(black_box(case(black_box(&mut table))), checksum));
        });
    }
    group.finish();
}

criterion_group!(benches, table_dto);
criterion_main!(benches);

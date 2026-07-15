//! Criterion benchmarks for typed table storage and column indexes.

use std::{cmp::Ordering, hint::black_box, mem::size_of, time::Duration};

use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput, criterion_group,
    criterion_main, measurement::WallTime,
};
use gd::{
    ColumnSpec, DataType, IndexKeyRef, NullOrder, Schema, SortDirection, Table, UnknownFields,
    Value, ValueRef,
};

const MIXED_NUMERIC_ROWS: usize = 10_000_000;

#[derive(Clone, Copy, Debug, PartialEq)]
struct NumericSummary {
    average: f64,
    minimum: f64,
    maximum: f64,
    median: f64,
}

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

fn mixed_numeric_schema() -> Schema {
    Schema::new([
        ColumnSpec::new("u8_value", DataType::U8),
        ColumnSpec::new("f64_value", DataType::F64),
        ColumnSpec::new("u16_value", DataType::U16),
        ColumnSpec::new("u64_value", DataType::U64),
        ColumnSpec::new("f32_value", DataType::F32),
        ColumnSpec::new("i32_value", DataType::I32),
    ])
    .unwrap()
}

#[allow(clippy::cast_precision_loss)] // Fixture values stay within exact f32/f64 integer ranges.
fn make_mixed_numeric_table(rows: usize) -> Table {
    let mut table = Table::with_capacity(mixed_numeric_schema(), rows);
    for row in 0..rows {
        let value = (u64::try_from(row).unwrap() * 48_271) % u64::try_from(rows).unwrap();
        let centered = i64::try_from(value).unwrap() - 5_000_000;
        table
            .push_row([
                Value::U8(u8::try_from(value % 251).unwrap()),
                Value::F64(value as f64 * 0.5 - 2_500_000.0),
                Value::U16(u16::try_from(value % 65_521).unwrap()),
                Value::U64(value * 1_000),
                Value::F32(centered as f32),
                Value::I32(i32::try_from(centered).unwrap()),
            ])
            .unwrap();
    }
    table
}

#[allow(clippy::cast_precision_loss)] // Validated fixture totals fit the documented f64 results.
fn summarize_integer<T>(
    values: impl Iterator<Item = T>,
    convert: impl Fn(T) -> i128,
) -> NumericSummary
where
    T: Copy + Ord,
{
    let mut collected = Vec::with_capacity(MIXED_NUMERIC_ROWS);
    let mut sum = 0_i128;
    let mut minimum = None;
    let mut maximum = None;
    for value in values {
        sum += convert(value);
        minimum = Some(minimum.map_or(value, |current| std::cmp::min(current, value)));
        maximum = Some(maximum.map_or(value, |current| std::cmp::max(current, value)));
        collected.push(value);
    }

    let count = collected.len();
    let middle = count / 2;
    let (lower, upper_middle, _) = collected.select_nth_unstable(middle);
    let lower_middle = *lower.iter().max().unwrap();
    let upper_middle = *upper_middle;
    NumericSummary {
        average: sum as f64 / count as f64,
        minimum: convert(minimum.unwrap()) as f64,
        maximum: convert(maximum.unwrap()) as f64,
        median: (convert(lower_middle) + convert(upper_middle)) as f64 / 2.0,
    }
}

#[allow(clippy::cast_precision_loss)] // The benchmark row count is exactly representable as f64.
fn summarize_float<T>(
    values: impl Iterator<Item = T>,
    convert: impl Fn(T) -> f64,
    compare: impl Fn(&T, &T) -> Ordering + Copy,
) -> NumericSummary
where
    T: Copy,
{
    let mut collected = Vec::with_capacity(MIXED_NUMERIC_ROWS);
    let mut sum = 0.0_f64;
    let mut minimum = None;
    let mut maximum = None;
    for value in values {
        sum += convert(value);
        minimum = Some(minimum.map_or(value, |current| {
            if compare(&value, &current).is_lt() {
                value
            } else {
                current
            }
        }));
        maximum = Some(maximum.map_or(value, |current| {
            if compare(&value, &current).is_gt() {
                value
            } else {
                current
            }
        }));
        collected.push(value);
    }

    let count = collected.len();
    let middle = count / 2;
    let (lower, upper_middle, _) = collected.select_nth_unstable_by(middle, compare);
    let lower_middle = *lower
        .iter()
        .max_by(|left, right| compare(left, right))
        .unwrap();
    let upper_middle = *upper_middle;
    NumericSummary {
        average: sum / count as f64,
        minimum: convert(minimum.unwrap()),
        maximum: convert(maximum.unwrap()),
        median: f64::midpoint(convert(lower_middle), convert(upper_middle)),
    }
}

fn summarize_mixed_numeric_table(table: &Table) -> [NumericSummary; 6] {
    [
        summarize_integer(
            table.column(0).unwrap().iter().map(|value| match value {
                ValueRef::U8(value) => value,
                _ => unreachable!(),
            }),
            i128::from,
        ),
        summarize_float(
            table.column(1).unwrap().iter().map(|value| match value {
                ValueRef::F64(value) => value,
                _ => unreachable!(),
            }),
            |value| value,
            f64::total_cmp,
        ),
        summarize_integer(
            table.column(2).unwrap().iter().map(|value| match value {
                ValueRef::U16(value) => value,
                _ => unreachable!(),
            }),
            i128::from,
        ),
        summarize_integer(
            table.column(3).unwrap().iter().map(|value| match value {
                ValueRef::U64(value) => value,
                _ => unreachable!(),
            }),
            i128::from,
        ),
        summarize_float(
            table.column(4).unwrap().iter().map(|value| match value {
                ValueRef::F32(value) => value,
                _ => unreachable!(),
            }),
            f64::from,
            f32::total_cmp,
        ),
        summarize_integer(
            table.column(5).unwrap().iter().map(|value| match value {
                ValueRef::I32(value) => value,
                _ => unreachable!(),
            }),
            i128::from,
        ),
    ]
}

fn summarize_mixed_numeric_table_typed(table: &Table) -> [NumericSummary; 6] {
    [
        summarize_integer(
            table
                .column(0)
                .unwrap()
                .as_slice::<u8>()
                .unwrap()
                .iter()
                .copied(),
            i128::from,
        ),
        summarize_float(
            table
                .column(1)
                .unwrap()
                .as_slice::<f64>()
                .unwrap()
                .iter()
                .copied(),
            |value| value,
            f64::total_cmp,
        ),
        summarize_integer(
            table
                .column(2)
                .unwrap()
                .as_slice::<u16>()
                .unwrap()
                .iter()
                .copied(),
            i128::from,
        ),
        summarize_integer(
            table
                .column(3)
                .unwrap()
                .as_slice::<u64>()
                .unwrap()
                .iter()
                .copied(),
            i128::from,
        ),
        summarize_float(
            table
                .column(4)
                .unwrap()
                .as_slice::<f32>()
                .unwrap()
                .iter()
                .copied(),
            f64::from,
            f32::total_cmp,
        ),
        summarize_integer(
            table
                .column(5)
                .unwrap()
                .as_slice::<i32>()
                .unwrap()
                .iter()
                .copied(),
            i128::from,
        ),
    ]
}

#[allow(clippy::cast_precision_loss)] // The benchmark fixture stays exactly representable.
fn average_unsigned<T>(values: impl ExactSizeIterator<Item = T>) -> f64
where
    u64: From<T>,
{
    let count = values.len();
    let sum = values.map(u64::from).sum::<u64>();
    sum as f64 / count as f64
}

#[allow(clippy::cast_precision_loss)] // The benchmark fixture stays exactly representable.
fn average_signed<T>(values: impl ExactSizeIterator<Item = T>) -> f64
where
    i64: From<T>,
{
    let count = values.len();
    let sum = values.map(i64::from).sum::<i64>();
    sum as f64 / count as f64
}

#[allow(clippy::cast_precision_loss)] // The row count is exactly representable as f64.
fn average_float<T>(values: impl ExactSizeIterator<Item = T>, convert: impl Fn(T) -> f64) -> f64 {
    let count = values.len();
    let sum = values.map(convert).sum::<f64>();
    sum / count as f64
}

fn median<T>(
    values: impl Iterator<Item = T>,
    compare: impl Fn(&T, &T) -> Ordering + Copy,
    convert: impl Fn(T) -> f64,
) -> f64
where
    T: Copy,
{
    let mut values: Vec<_> = values.collect();
    let middle = values.len() / 2;
    let (lower, upper_middle, _) = values.select_nth_unstable_by(middle, compare);
    let lower_middle = *lower
        .iter()
        .max_by(|left, right| compare(left, right))
        .unwrap();
    f64::midpoint(convert(lower_middle), convert(*upper_middle))
}

fn maximum_float<T>(values: impl Iterator<Item = T>) -> T
where
    T: Copy + PartialOrd,
{
    values
        .reduce(|maximum, value| if value > maximum { value } else { maximum })
        .unwrap()
}

#[allow(clippy::trivially_copy_pass_by_ref)] // Slice selection requires a reference comparator.
fn finite_f32_cmp(left: &f32, right: &f32) -> Ordering {
    left.partial_cmp(right).unwrap()
}

#[allow(clippy::trivially_copy_pass_by_ref)] // Slice selection requires a reference comparator.
fn finite_f64_cmp(left: &f64, right: &f64) -> Ordering {
    left.partial_cmp(right).unwrap()
}

fn benchmark_column_paths<Result>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    field: &str,
    table: &Table,
    value_ref: impl Fn(&Table) -> Result,
    typed_slice: impl Fn(&Table) -> Result,
) {
    group.bench_function(BenchmarkId::new("ValueRef", field), |bencher| {
        bencher.iter(|| black_box(value_ref(black_box(table))));
    });
    group.bench_function(BenchmarkId::new("TypedSlice", field), |bencher| {
        bencher.iter(|| black_box(typed_slice(black_box(table))));
    });
}

fn configure_bulk_group(group: &mut BenchmarkGroup<'_, WallTime>) {
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2));
}

macro_rules! value_ref_values {
    ($table:expr, $column:expr, $variant:ident) => {
        $table
            .column($column)
            .unwrap()
            .iter()
            .map(|value| match value {
                ValueRef::$variant(value) => value,
                _ => unreachable!(),
            })
    };
}

macro_rules! typed_values {
    ($table:expr, $column:expr, $type:ty) => {
        $table
            .column($column)
            .unwrap()
            .as_slice::<$type>()
            .unwrap()
            .iter()
            .copied()
    };
}

macro_rules! benchmark_average_unsigned {
    ($group:expr, $table:expr, $field:literal, $column:expr, $type:ty, $variant:ident) => {
        benchmark_column_paths(
            $group,
            $field,
            $table,
            |table| average_unsigned(value_ref_values!(table, $column, $variant)),
            |table| average_unsigned(typed_values!(table, $column, $type)),
        );
    };
}

macro_rules! benchmark_average_signed {
    ($group:expr, $table:expr, $field:literal, $column:expr, $type:ty, $variant:ident) => {
        benchmark_column_paths(
            $group,
            $field,
            $table,
            |table| average_signed(value_ref_values!(table, $column, $variant)),
            |table| average_signed(typed_values!(table, $column, $type)),
        );
    };
}

macro_rules! benchmark_average_float {
    ($group:expr, $table:expr, $field:literal, $column:expr, $type:ty, $variant:ident, $convert:expr) => {
        benchmark_column_paths(
            $group,
            $field,
            $table,
            |table| average_float(value_ref_values!(table, $column, $variant), $convert),
            |table| average_float(typed_values!(table, $column, $type), $convert),
        );
    };
}

macro_rules! benchmark_ordered_extreme {
    ($group:expr, $table:expr, $field:literal, $column:expr, $type:ty, $variant:ident, $method:ident) => {
        benchmark_column_paths(
            $group,
            $field,
            $table,
            |table| {
                value_ref_values!(table, $column, $variant)
                    .$method()
                    .unwrap()
            },
            |table| typed_values!(table, $column, $type).$method().unwrap(),
        );
    };
}

macro_rules! benchmark_float_extreme {
    ($group:expr, $table:expr, $field:literal, $column:expr, $type:ty, $variant:ident, $operation:path) => {
        benchmark_column_paths(
            $group,
            $field,
            $table,
            |table| $operation(value_ref_values!(table, $column, $variant)),
            |table| $operation(typed_values!(table, $column, $type)),
        );
    };
}

macro_rules! benchmark_median {
    ($group:expr, $table:expr, $field:literal, $column:expr, $type:ty, $variant:ident, $compare:path, $convert:expr) => {
        benchmark_column_paths(
            $group,
            $field,
            $table,
            |table| {
                median(
                    value_ref_values!(table, $column, $variant),
                    $compare,
                    $convert,
                )
            },
            |table| median(typed_values!(table, $column, $type), $compare, $convert),
        );
    };
}

fn verify_mixed_numeric_summary(summaries: [NumericSummary; 6]) {
    let expected = [
        NumericSummary {
            average: 124.999_272,
            minimum: 0.0,
            maximum: 250.0,
            median: 125.0,
        },
        NumericSummary {
            average: -0.25,
            minimum: -2_500_000.0,
            maximum: 2_499_999.5,
            median: -0.25,
        },
        NumericSummary {
            average: 32_709.575_594_8,
            minimum: 0.0,
            maximum: 65_520.0,
            median: 32_679.0,
        },
        NumericSummary {
            average: 4_999_999_500.0,
            minimum: 0.0,
            maximum: 9_999_999_000.0,
            median: 4_999_999_500.0,
        },
        NumericSummary {
            average: -0.5,
            minimum: -5_000_000.0,
            maximum: 4_999_999.0,
            median: -0.5,
        },
        NumericSummary {
            average: -0.5,
            minimum: -5_000_000.0,
            maximum: 4_999_999.0,
            median: -0.5,
        },
    ];
    for (actual, expected) in summaries.into_iter().zip(expected) {
        assert!((actual.average - expected.average).abs() <= 0.000_001);
        assert!((actual.minimum - expected.minimum).abs() <= 0.000_001);
        assert!((actual.maximum - expected.maximum).abs() <= 0.000_001);
        assert!((actual.median - expected.median).abs() <= 0.000_001);
    }
}

fn mixed_numeric_table_bytes(rows: usize) -> usize {
    rows * (size_of::<u8>()
        + size_of::<f64>()
        + size_of::<u16>()
        + size_of::<u64>()
        + size_of::<f32>()
        + size_of::<i32>())
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

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
// The fixture's i64 median is exactly representable; cases stay together for auditability.
fn mixed_numeric_statistics(criterion: &mut Criterion) {
    assert_eq!(mixed_numeric_table_bytes(MIXED_NUMERIC_ROWS), 270_000_000);

    {
        let mut group = criterion.benchmark_group("Table/MixedNumeric/10000000/Build");
        group
            .sampling_mode(SamplingMode::Flat)
            .sample_size(10)
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(5));
        group.bench_function("Table", |bencher| {
            bencher.iter(|| black_box(make_mixed_numeric_table(black_box(MIXED_NUMERIC_ROWS))));
        });
        group.finish();
    }

    let table = make_mixed_numeric_table(MIXED_NUMERIC_ROWS);
    verify_mixed_numeric_summary(summarize_mixed_numeric_table(&table));
    verify_mixed_numeric_summary(summarize_mixed_numeric_table_typed(&table));

    {
        let mut group = criterion.benchmark_group("Table/MixedNumeric/10000000/Average");
        configure_bulk_group(&mut group);
        benchmark_average_unsigned!(&mut group, &table, "u8", 0, u8, U8);
        benchmark_average_float!(&mut group, &table, "f64", 1, f64, F64, |value| value);
        benchmark_average_unsigned!(&mut group, &table, "u16", 2, u16, U16);
        benchmark_average_unsigned!(&mut group, &table, "u64", 3, u64, U64);
        benchmark_average_float!(&mut group, &table, "f32", 4, f32, F32, f64::from);
        benchmark_average_signed!(&mut group, &table, "i32", 5, i32, I32);
        group.finish();
    }

    {
        let mut group = criterion.benchmark_group("Table/MixedNumeric/10000000/Maximum");
        configure_bulk_group(&mut group);
        benchmark_ordered_extreme!(&mut group, &table, "u8", 0, u8, U8, max);
        benchmark_float_extreme!(&mut group, &table, "f64", 1, f64, F64, maximum_float);
        benchmark_ordered_extreme!(&mut group, &table, "u16", 2, u16, U16, max);
        benchmark_ordered_extreme!(&mut group, &table, "u64", 3, u64, U64, max);
        benchmark_float_extreme!(&mut group, &table, "f32", 4, f32, F32, maximum_float);
        benchmark_ordered_extreme!(&mut group, &table, "i32", 5, i32, I32, max);
        group.finish();
    }

    {
        let mut group = criterion.benchmark_group("Table/MixedNumeric/10000000/Median");
        configure_bulk_group(&mut group);
        benchmark_median!(&mut group, &table, "u8", 0, u8, U8, u8::cmp, f64::from);
        benchmark_median!(
            &mut group,
            &table,
            "f64",
            1,
            f64,
            F64,
            finite_f64_cmp,
            |value| value
        );
        benchmark_median!(&mut group, &table, "u16", 2, u16, U16, u16::cmp, f64::from);
        benchmark_median!(
            &mut group,
            &table,
            "u64",
            3,
            u64,
            U64,
            u64::cmp,
            |value| value as f64
        );
        benchmark_median!(
            &mut group,
            &table,
            "f32",
            4,
            f32,
            F32,
            finite_f32_cmp,
            f64::from
        );
        benchmark_median!(&mut group, &table, "i32", 5, i32, I32, i32::cmp, f64::from);
        group.finish();
    }
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
    mixed_numeric_statistics,
    indexes,
    row_order
);
criterion_main!(benches);

//! Criterion benchmarks for copying contiguous and scattered table rows.

use std::{env, hint::black_box, time::Duration};

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use gd::{ColumnSpec, DataType, Schema, Table, Value};
use rayon::ThreadPoolBuilder;

const DEFAULT_SOURCE_ROWS: usize = 1_000_000;
const DEFAULT_COPY_PERCENT: usize = 25;
const LOGICAL_ROW_BYTES: u64 = 1 + 8 + 4 + 2;

#[derive(Clone, Copy)]
struct CopyConfig {
    source_rows: usize,
    copied_rows: usize,
    copy_percent: usize,
}

impl CopyConfig {
    fn from_environment() -> Result<Self, String> {
        let source_rows = setting("GD_TABLE_COPY_SOURCE_ROWS", DEFAULT_SOURCE_ROWS)?;
        let copy_percent = setting("GD_TABLE_COPY_PERCENT", DEFAULT_COPY_PERCENT)?;
        if source_rows == 0 {
            return Err("GD_TABLE_COPY_SOURCE_ROWS must be greater than zero".into());
        }
        if !(1..=100).contains(&copy_percent) {
            return Err("GD_TABLE_COPY_PERCENT must be in 1..=100".into());
        }
        let copied_rows = source_rows
            .checked_mul(copy_percent)
            .ok_or("table-copy row calculation overflowed")?
            / 100;
        if copied_rows == 0 {
            return Err("the configured percentage must select at least one row".into());
        }
        Ok(Self {
            source_rows,
            copied_rows,
            copy_percent,
        })
    }
}

fn setting<T>(name: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    env::var(name).map_or(Ok(default), |value| {
        value
            .parse()
            .map_err(|error| format!("invalid {name}={value}: {error}"))
    })
}

fn copy_schema() -> Schema {
    Schema::new([
        ColumnSpec::new("u8_value", DataType::U8),
        ColumnSpec::new("u64_value", DataType::U64),
        ColumnSpec::new("f32_value", DataType::F32),
        ColumnSpec::new("u16_value", DataType::U16),
    ])
    .unwrap()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn make_source(rows: usize) -> Table {
    let mut table = Table::with_capacity(copy_schema(), rows);
    for row in 0..rows {
        table
            .push_row([
                Value::U8(row as u8),
                Value::U64((row as u64).wrapping_mul(48_271)),
                Value::F32((row % 1_000_003) as f32 * 0.5),
                Value::U16(row as u16),
            ])
            .unwrap();
    }
    table
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn random_rows(source_rows: usize, copied_rows: usize) -> Vec<usize> {
    let mut rows: Vec<_> = (0..source_rows).collect();
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for upper in (1..rows.len()).rev() {
        let selected =
            usize::try_from(next_random(&mut state) % u64::try_from(upper + 1).unwrap()).unwrap();
        rows.swap(upper, selected);
    }
    rows.truncate(copied_rows);
    rows.sort_unstable();
    rows
}

fn copy_row_views(source: &Table, rows: impl ExactSizeIterator<Item = usize>) -> Table {
    let mut target = Table::with_capacity(source.schema_arc(), rows.len());
    for source_row in rows {
        let values = source
            .row(source_row)
            .unwrap()
            .iter()
            .map(gd::ValueRef::to_owned)
            .collect();
        target.push_row_vec(values).unwrap();
    }
    target
}

#[allow(clippy::similar_names)]
fn copy_typed_rows(source: &Table, rows: impl ExactSizeIterator<Item = usize>) -> Table {
    let source_u8 = source.column(0).unwrap().as_slice::<u8>().unwrap();
    let source_u64 = source.column(1).unwrap().as_slice::<u64>().unwrap();
    let source_f32 = source.column(2).unwrap().as_slice::<f32>().unwrap();
    let source_u16 = source.column(3).unwrap().as_slice::<u16>().unwrap();
    let mut target = Table::with_capacity(source.schema_arc(), rows.len());
    for source_row in rows {
        target
            .push_row([
                Value::U8(source_u8[source_row]),
                Value::U64(source_u64[source_row]),
                Value::F32(source_f32[source_row]),
                Value::U16(source_u16[source_row]),
            ])
            .unwrap();
    }
    target
}

fn assert_selected_rows(source: &Table, target: &Table, rows: &[usize]) {
    assert_eq!(target.row_count(), rows.len());
    for (target_row, &source_row) in rows.iter().enumerate() {
        for column in 0..4 {
            assert_eq!(
                target.cell(target_row, column).unwrap(),
                source.cell(source_row, column).unwrap()
            );
        }
    }
}

fn validate_copies(
    source: &Table,
    range_start: usize,
    range_end: usize,
    range_rows: &[usize],
    selected_rows: &[usize],
) {
    assert_selected_rows(
        source,
        &copy_row_views(source, range_start..range_end),
        range_rows,
    );
    assert_selected_rows(
        source,
        &copy_typed_rows(source, range_start..range_end),
        range_rows,
    );
    assert_selected_rows(
        source,
        &source.copy_range(range_start..range_end).unwrap(),
        range_rows,
    );
    assert_selected_rows(
        source,
        &source.par_copy_range(range_start..range_end).unwrap(),
        range_rows,
    );
    assert_selected_rows(
        source,
        &copy_row_views(source, selected_rows.iter().copied()),
        selected_rows,
    );
    assert_selected_rows(
        source,
        &copy_typed_rows(source, selected_rows.iter().copied()),
        selected_rows,
    );
    assert_selected_rows(
        source,
        &source.copy_rows(selected_rows).unwrap(),
        selected_rows,
    );
    assert_selected_rows(
        source,
        &source.par_copy_rows(selected_rows).unwrap(),
        selected_rows,
    );
}

fn benchmark_table_copy(criterion: &mut Criterion) {
    let config = CopyConfig::from_environment().unwrap_or_else(|error| panic!("{error}"));
    let source = make_source(config.source_rows);
    let range_start = (config.source_rows - config.copied_rows) / 2;
    let range_end = range_start + config.copied_rows;
    let range_rows: Vec<_> = (range_start..range_end).collect();
    let selected_rows = random_rows(config.source_rows, config.copied_rows);
    let half_workers = (source.column_count() / 2).max(1);
    let parallel_pool = ThreadPoolBuilder::new()
        .num_threads(half_workers)
        .build()
        .unwrap();

    validate_copies(&source, range_start, range_end, &range_rows, &selected_rows);

    let group_name = format!(
        "TableCopy/source={}/selected={}({}pct)",
        config.source_rows, config.copied_rows, config.copy_percent
    );
    let mut group = criterion.benchmark_group(group_name);
    group
        .throughput(Throughput::Bytes(
            config.copied_rows as u64 * LOGICAL_ROW_BYTES,
        ))
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(3));

    group.bench_function("Range/RowView", |bencher| {
        bencher.iter(|| {
            black_box(copy_row_views(
                black_box(&source),
                black_box(range_start)..black_box(range_end),
            ))
        });
    });
    group.bench_function("Range/TypedRows", |bencher| {
        bencher.iter(|| {
            black_box(copy_typed_rows(
                black_box(&source),
                black_box(range_start)..black_box(range_end),
            ))
        });
    });
    group.bench_function("Range/Columns", |bencher| {
        bencher.iter(|| {
            black_box(
                black_box(&source)
                    .copy_range(black_box(range_start)..black_box(range_end))
                    .unwrap(),
            )
        });
    });
    group.bench_function("Range/ParallelColumns/HalfWorkers", |bencher| {
        bencher.iter(|| {
            parallel_pool.install(|| {
                black_box(
                    black_box(&source)
                        .par_copy_range(black_box(range_start)..black_box(range_end))
                        .unwrap(),
                )
            })
        });
    });
    group.bench_function("Random/RowView", |bencher| {
        bencher.iter(|| {
            black_box(copy_row_views(
                black_box(&source),
                black_box(&selected_rows).iter().copied(),
            ))
        });
    });
    group.bench_function("Random/TypedRows", |bencher| {
        bencher.iter(|| {
            black_box(copy_typed_rows(
                black_box(&source),
                black_box(&selected_rows).iter().copied(),
            ))
        });
    });
    group.bench_function("Random/Columns", |bencher| {
        bencher.iter(|| {
            black_box(
                black_box(&source)
                    .copy_rows(black_box(&selected_rows))
                    .unwrap(),
            )
        });
    });
    group.bench_function("Random/ParallelColumns/HalfWorkers", |bencher| {
        bencher.iter(|| {
            parallel_pool.install(|| {
                black_box(
                    black_box(&source)
                        .par_copy_rows(black_box(&selected_rows))
                        .unwrap(),
                )
            })
        });
    });
    group.finish();
}

criterion_group!(benches, benchmark_table_copy);
criterion_main!(benches);

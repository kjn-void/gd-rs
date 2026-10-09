//! Filter mixed records into one owned-value or shared-record target table.

use compact_str::CompactString;
use gd::{ColumnSpec, DataType, Schema, SharedRecordTable, Table, Value, ValueRef};
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::{
    hint::black_box,
    sync::Arc,
    time::{Duration, Instant},
};

fn text(row: usize, length: usize, label: &str) -> String {
    let mut value = format!("{:08}-{label}-", row % 100_000_000);
    value.push_str(&"x".repeat(length - value.len()));
    value
}

fn selector(row: usize) -> u64 {
    ((row % 100 * 37 + row / 100 * 17) % 100) as u64
}

fn fixture(rows: usize, length: usize, fixed: bool) -> Table {
    let string = |name| {
        if fixed {
            ColumnSpec::fixed_string(name, length)
        } else {
            ColumnSpec::new(name, DataType::String)
        }
    };
    let schema = Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("selector", DataType::U64),
        ColumnSpec::new("amount", DataType::U64),
        string("name"),
        string("message"),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, rows);
    for row in 0..rows {
        table
            .push_row([
                Value::U64(row as u64),
                Value::U64(selector(row)),
                Value::U64(row as u64 * 13 + 7),
                Value::from(text(row, length, "name")),
                Value::from(text(row, length, "text")),
            ])
            .unwrap();
    }
    table
}

fn selected(scores: &[u64], begin: usize, end: usize, percent: u64) -> Vec<usize> {
    let mut rows = Vec::with_capacity(end - begin);
    for (index, &score) in scores.iter().enumerate().take(end).skip(begin) {
        if score < percent {
            rows.push(index);
        }
    }
    rows
}

fn filter_copy(source: &Table, percent: u64, workers: usize, pool: &ThreadPool) -> Table {
    let scores = source.column(1).unwrap().as_slice::<u64>().unwrap();
    if workers == 1 {
        return source
            .copy_rows(&selected(scores, 0, scores.len(), percent))
            .unwrap();
    }
    pool.install(|| {
        let parts: Vec<_> = (0..workers)
            .into_par_iter()
            .map(|worker| {
                selected(
                    scores,
                    scores.len() * worker / workers,
                    scores.len() * (worker + 1) / workers,
                    percent,
                )
            })
            .collect();
        let count = parts.iter().map(Vec::len).sum();
        let mut rows = Vec::with_capacity(count);
        for part in parts {
            rows.extend(part);
        }
        // Native column-parallel gather constructs exactly one Table. There are
        // five column tasks, including two string tasks; never worker tables.
        source.par_copy_rows(&rows).unwrap()
    })
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
    }
    hash
}

fn digest(table: &Table) -> (usize, u64) {
    let mut hash = 14_695_981_039_346_656_037;
    for row in table.rows() {
        for cell in row.iter() {
            hash = match cell {
                ValueRef::U64(value) => hash_bytes(hash, &value.to_le_bytes()),
                ValueRef::String(value) => hash_bytes(
                    hash_bytes(hash, &(value.len() as u64).to_le_bytes()),
                    value.as_bytes(),
                ),
                _ => panic!("unexpected fixture type"),
            };
        }
    }
    (table.row_count(), hash)
}

#[derive(Clone)]
struct Record {
    id: u64,
    selector: u64,
    amount: u64,
    name: CompactString,
    message: CompactString,
}

fn shared_fixture(rows: usize, length: usize) -> SharedRecordTable<Record> {
    let mut table = SharedRecordTable::with_capacity(rows);
    for row in 0..rows {
        table.push(Record {
            id: row as u64,
            selector: selector(row),
            amount: row as u64 * 13 + 7,
            name: text(row, length, "name").into(),
            message: text(row, length, "text").into(),
        });
    }
    table
}

fn shared_filter(
    source: &SharedRecordTable<Record>,
    percent: u64,
    workers: usize,
    pool: &ThreadPool,
) -> SharedRecordTable<Record> {
    if workers == 1 {
        source.filter(|record| record.selector < percent)
    } else {
        pool.install(|| source.par_filter(|record| record.selector < percent))
    }
}

fn shared_digest(table: &SharedRecordTable<Record>) -> (usize, u64) {
    let mut hash = 14_695_981_039_346_656_037;
    for record in table.as_slice() {
        for number in [record.id, record.selector, record.amount] {
            hash = hash_bytes(hash, &number.to_le_bytes());
        }
        for value in [&record.name, &record.message] {
            hash = hash_bytes(
                hash_bytes(hash, &(value.len() as u64).to_le_bytes()),
                value.as_bytes(),
            );
        }
    }
    (table.row_count(), hash)
}

fn run_shared(args: &[String]) {
    let rows: usize = args[2].parse().unwrap();
    let length: usize = args[3].parse().unwrap();
    let workers: usize = args[4].parse().unwrap();
    let percent: u64 = args[5].parse().unwrap();
    let samples: usize = args[6].parse().unwrap();
    let sample_ms: u64 = args[7].parse().unwrap();
    let chunks_per_worker = match args[1].as_str() {
        "arc" => 0,
        "arc_chunks" => 1,
        "arc_chunks4" => 4,
        _ => panic!("invalid shared strategy"),
    };
    assert!(
        rows <= 100_000_000
            && (16..=4096).contains(&length)
            && (1..=64).contains(&workers)
            && percent <= 100
            && samples > 0
            && sample_ms > 0
    );
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let start = Instant::now();
    let source = shared_fixture(rows, length);
    let build_ns = start.elapsed().as_nanos();
    let filter = || {
        if chunks_per_worker == 0 || workers == 1 {
            shared_filter(black_box(&source), percent, workers, &pool)
        } else {
            let grain = rows.div_ceil(workers * chunks_per_worker).max(1);
            pool.install(|| source.par_filter_chunked(grain, |r| r.selector < percent))
        }
    };
    let target = filter();
    let verification = shared_digest(&target);
    if args[8] == "verify" {
        for (result, original) in target.as_slice().iter().zip(
            source
                .as_slice()
                .iter()
                .filter(|record| record.selector < percent),
        ) {
            assert!(Arc::ptr_eq(result, original));
        }
        drop(source);
        assert_eq!(shared_digest(&target), verification);
        println!(
            "{}",
            serde_json::json!({"count": verification.0, "digest": verification.1.to_string(),
            "independent_target": false, "shared_records": true, "source_drop_checked": true})
        );
        return;
    }
    assert_eq!(args[8], "time");
    drop(target);
    let operation = || {
        let target = black_box(filter());
        if chunks_per_worker > 0 && workers > 1 {
            pool.install(|| target.par_drop());
        } else {
            drop(target);
        }
    };
    let mut iterations = 1u32;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        if start.elapsed() >= Duration::from_millis(sample_ms) {
            break;
        }
        iterations *= 2;
    }
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        times.push(start.elapsed().as_secs_f64() * 1e9 / f64::from(iterations));
    }
    println!(
        "{}",
        serde_json::json!({"samples_ns": times, "iterations": iterations,
        "build_ns": build_ns.to_string(), "count": verification.0, "digest": verification.1.to_string()})
    );
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        9,
        "compact|fixed|arc|arc_chunks|arc_chunks4 rows length workers percent samples sample_ms verify|time"
    );
    if matches!(args[1].as_str(), "arc" | "arc_chunks" | "arc_chunks4") {
        run_shared(&args);
        return;
    }
    let fixed = match args[1].as_str() {
        "compact" => false,
        "fixed" => true,
        _ => panic!("invalid layout"),
    };
    let rows: usize = args[2].parse().unwrap();
    let length: usize = args[3].parse().unwrap();
    let workers: usize = args[4].parse().unwrap();
    let percent: u64 = args[5].parse().unwrap();
    let samples: usize = args[6].parse().unwrap();
    let sample_ms: u64 = args[7].parse().unwrap();
    assert!(
        rows <= 100_000_000
            && (16..=4096).contains(&length)
            && (1..=64).contains(&workers)
            && percent <= 100
            && samples > 0
            && sample_ms > 0
    );
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let start = Instant::now();
    let source = fixture(rows, length, fixed);
    let build_ns = start.elapsed().as_nanos();
    let target = filter_copy(&source, percent, workers, &pool);
    let verification = digest(&target);
    if args[8] == "verify" {
        // Dropping the entire source proves that both destination string columns
        // own their storage, even when the source's backing allocation is gone.
        drop(source);
        assert_eq!(digest(&target), verification);
        println!(
            "{}",
            serde_json::json!({"count": verification.0,
            "digest": verification.1.to_string(), "independent_target": true})
        );
        return;
    }
    assert_eq!(args[8], "time");
    drop(target);
    let operation = || {
        black_box(filter_copy(black_box(&source), percent, workers, &pool));
    };
    let mut iterations = 1u32;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        if start.elapsed() >= Duration::from_millis(sample_ms) {
            break;
        }
        iterations *= 2;
    }
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        times.push(start.elapsed().as_secs_f64() * 1e9 / f64::from(iterations));
    }
    println!(
        "{}",
        serde_json::json!({"samples_ns": times, "iterations": iterations,
        "build_ns": build_ns.to_string(), "count": verification.0,
        "digest": verification.1.to_string()})
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_target_has_the_same_values_but_shared_payloads() {
        let pool = ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        for rows in [0, 1, 31, 1001] {
            for length in [16, 128] {
                for percent in [0, 10, 50, 90, 100] {
                    let source = shared_fixture(rows, length);
                    let values = fixture(rows, length, false);
                    let expected = digest(&filter_copy(&values, percent, 1, &pool));
                    for workers in [1, 8] {
                        let target = shared_filter(&source, percent, workers, &pool);
                        assert_eq!(shared_digest(&target), expected);
                        for (record, original) in target
                            .as_slice()
                            .iter()
                            .zip(source.as_slice().iter().filter(|r| r.selector < percent))
                        {
                            assert!(Arc::ptr_eq(record, original));
                        }
                    }
                    let target = shared_filter(&source, percent, 8, &pool);
                    drop(source);
                    assert_eq!(shared_digest(&target), expected);
                }
            }
        }
    }

    #[test]
    fn one_owned_target_preserves_every_cell_and_order() {
        let pool = ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        for rows in [0, 1, 31, 1001] {
            for length in [16, 128] {
                for fixed in [false, true] {
                    for percent in [0, 10, 50, 90, 100] {
                        let source = fixture(rows, length, fixed);
                        let expected = digest(
                            &source
                                .copy_rows(
                                    &(0..rows)
                                        .filter(|&row| selector(row) < percent)
                                        .collect::<Vec<_>>(),
                                )
                                .unwrap(),
                        );
                        let serial = filter_copy(&source, percent, 1, &pool);
                        let parallel = filter_copy(&source, percent, 8, &pool);
                        drop(source);
                        assert_eq!(digest(&serial), expected);
                        assert_eq!(digest(&parallel), expected);
                    }
                }
            }
        }
    }
}

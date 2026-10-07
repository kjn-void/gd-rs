//! Matched text filtering, rewriting, and materialization benchmark application.

use compact_str::CompactString;
use gd::{ColumnSpec, DataType, FixedStringsMut, Schema, Table, Value, ValueRef};
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

fn message(row: usize, length: usize) -> String {
    let mut text = format!(
        "{:08}-{}-",
        row % 100_000_000,
        if row % 3 == 0 { "error" } else { "event" }
    );
    text.push_str(&"x".repeat(length - text.len()));
    text
}

fn schema(fixed: bool, length: usize) -> Schema {
    let string = |name, bytes| {
        if fixed {
            ColumnSpec::fixed_string(name, bytes)
        } else {
            ColumnSpec::new(name, DataType::String)
        }
    };
    Schema::new([
        ColumnSpec::new("id", DataType::U64),
        string("region", 8),
        string("message", length),
        string("output", length + 3),
        ColumnSpec::new("score", DataType::U64),
    ])
    .unwrap()
}

fn fixture(rows: usize, length: usize, fixed: bool) -> Table {
    let mut table = Table::with_capacity(schema(fixed, length), rows);
    for row in 0..rows {
        let text = message(row, length);
        table
            .push_row([
                Value::U64(row as u64),
                Value::from(if row % 4 == 0 { "north" } else { "south" }),
                Value::from(text.as_str()),
                Value::from(text.as_str()),
                Value::U64((row % 100) as u64),
            ])
            .unwrap();
    }
    table
}

fn selected(table: &Table, begin: usize, end: usize, fixed: bool) -> Vec<usize> {
    let scores = table.column(4).unwrap().as_slice::<u64>().unwrap();
    let mut result = Vec::new();
    if fixed {
        let regions = table.column(1).unwrap().fixed_strings().unwrap();
        let messages = table.column(2).unwrap().fixed_strings().unwrap();
        for (row, &score) in scores.iter().enumerate().take(end).skip(begin) {
            if regions.get(row).unwrap().unwrap() == "north"
                && score >= 20
                && messages.get(row).unwrap().unwrap().contains("error")
            {
                result.push(row);
            }
        }
    } else {
        let regions = table
            .column(1)
            .unwrap()
            .as_slice::<CompactString>()
            .unwrap();
        let messages = table
            .column(2)
            .unwrap()
            .as_slice::<CompactString>()
            .unwrap();
        for row in begin..end {
            if regions[row] == "north" && scores[row] >= 20 && messages[row].contains("error") {
                result.push(row);
            }
        }
    }
    result
}

fn transform_fixed(input: gd::FixedStrings<'_>, mut output: FixedStringsMut<'_>, start: usize) {
    // One reusable scratch buffer per worker, never one allocation per cell.
    let mut scratch = String::with_capacity(output.capacity());
    for (i, mut cell) in output.iter_mut().enumerate() {
        scratch.clear();
        scratch.push_str(input.get(start + i).unwrap().unwrap());
        scratch.make_ascii_uppercase();
        scratch.push_str("|ok");
        cell.set(Some(&scratch)).unwrap();
    }
}

fn transform(table: &mut Table, fixed: bool, workers: usize, pool: &ThreadPool) {
    let (input, output) = table.column_pair_mut(2, 3).unwrap();
    let count = input.len();
    if fixed {
        let input = input.fixed_strings().unwrap();
        let output = output.fixed_strings_mut().unwrap();
        if workers == 1 {
            transform_fixed(input, output, 0);
        } else {
            let mut parts = Vec::with_capacity(workers);
            let mut remainder = output;
            for worker in 0..workers {
                let begin = count * worker / workers;
                let end = count * (worker + 1) / workers;
                let (part, rest) = remainder.split_at(end - begin);
                parts.push((begin, part));
                remainder = rest;
            }
            pool.install(|| {
                parts
                    .into_par_iter()
                    .for_each(|(begin, part)| transform_fixed(input, part, begin));
            });
        }
    } else {
        let input = input.as_slice::<CompactString>().unwrap();
        let output = output.as_mut_slice::<CompactString>().unwrap();
        let rewrite = |(input, output): (&CompactString, &mut CompactString)| {
            output.clear();
            output.push_str(input);
            output.make_ascii_uppercase();
            output.push_str("|ok");
        };
        if workers == 1 {
            input.iter().zip(output.iter_mut()).for_each(rewrite);
        } else {
            // Exactly eight static row tasks, matching the C++ pool.
            let mut parts = Vec::with_capacity(workers);
            let mut remainder = output;
            for worker in 0..workers {
                let begin = count * worker / workers;
                let end = count * (worker + 1) / workers;
                let (part, rest) = remainder.split_at_mut(end - begin);
                parts.push((&input[begin..end], part));
                remainder = rest;
            }
            pool.install(|| {
                parts.into_par_iter().for_each(|(input, output)| {
                    input.iter().zip(output.iter_mut()).for_each(rewrite);
                });
            });
        }
    }
}

fn map_workers<T: Send>(
    workers: usize,
    pool: &ThreadPool,
    operation: impl Fn(usize) -> T + Send + Sync,
) -> Vec<T> {
    if workers == 1 {
        vec![operation(0)]
    } else {
        pool.install(|| (0..workers).into_par_iter().map(operation).collect())
    }
}

fn filter(table: &Table, fixed: bool, workers: usize, pool: &ThreadPool) -> Vec<Vec<usize>> {
    map_workers(workers, pool, |worker| {
        selected(
            table,
            table.row_count() * worker / workers,
            table.row_count() * (worker + 1) / workers,
            fixed,
        )
    })
}

fn pipeline(table: &Table, fixed: bool, workers: usize, pool: &ThreadPool) -> Vec<Table> {
    map_workers(workers, pool, |worker| {
        let rows = selected(
            table,
            table.row_count() * worker / workers,
            table.row_count() * (worker + 1) / workers,
            fixed,
        );
        let mut result = table.copy_rows(&rows).unwrap();
        transform(&mut result, fixed, 1, pool);
        result
    })
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
    }
    hash
}

fn digest(tables: &[Table]) -> (usize, u64) {
    let mut hash = 14_695_981_039_346_656_037;
    let mut count = 0;
    for table in tables {
        for row in table.rows() {
            count += 1;
            for cell in row.iter() {
                hash = match cell {
                    ValueRef::U64(n) => hash_bytes(hash, &n.to_le_bytes()),
                    ValueRef::String(text) => hash_bytes(
                        hash_bytes(hash, &(text.len() as u64).to_le_bytes()),
                        text.as_bytes(),
                    ),
                    _ => panic!("fixture type"),
                };
            }
        }
    }
    (count, hash)
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        9,
        "mode rows length workers operation samples sample_ms verify"
    );
    let fixed = match args[1].as_str() {
        "compact" => false,
        "fixed" => true,
        _ => panic!("invalid representation"),
    };
    let rows: usize = args[2].parse().unwrap();
    let length: usize = args[3].parse().unwrap();
    let workers: usize = args[4].parse().unwrap();
    let samples: usize = args[6].parse().unwrap();
    let sample_ms: u64 = args[7].parse().unwrap();
    assert!(
        rows > 0
            && (16..=4096).contains(&length)
            && (1..=64).contains(&workers)
            && samples > 0
            && sample_ms > 0
    );
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let build_start = Instant::now();
    let mut table = fixture(rows, length, fixed);
    let build_ns = build_start.elapsed().as_nanos();
    // Full row/cell verification happens outside timed samples in every process.
    let verification = match args[5].as_str() {
        "filter" => {
            let ids: Vec<_> = filter(&table, fixed, workers, &pool)
                .into_iter()
                .flatten()
                .collect();
            let hash = ids.iter().fold(14_695_981_039_346_656_037, |hash, &id| {
                hash_bytes(hash, &(id as u64).to_le_bytes())
            });
            (ids.len(), hash)
        }
        "transform" => {
            transform(&mut table, fixed, workers, &pool);
            digest(std::slice::from_ref(&table))
        }
        "pipeline" => digest(&pipeline(&table, fixed, workers, &pool)),
        _ => panic!("invalid operation"),
    };
    if args[8] == "verify" {
        println!(
            "{}",
            serde_json::json!({"count": verification.0, "digest": verification.1.to_string()})
        );
        return;
    }
    let mut operation = || match args[5].as_str() {
        "filter" => {
            black_box(filter(black_box(&table), fixed, workers, &pool));
        }
        "transform" => transform(black_box(&mut table), fixed, workers, &pool),
        "pipeline" => {
            black_box(pipeline(black_box(&table), fixed, workers, &pool));
        }
        _ => unreachable!(),
    };
    // Calibrate then warm up at least one full batch; batches include output destruction.
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
    let mut ns = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        ns.push(start.elapsed().as_secs_f64() * 1e9 / f64::from(iterations));
    }
    println!(
        "{}",
        serde_json::json!({"samples_ns": ns, "iterations": iterations, "build_ns": build_ns.to_string(), "count": verification.0, "digest": verification.1.to_string()})
    );
}

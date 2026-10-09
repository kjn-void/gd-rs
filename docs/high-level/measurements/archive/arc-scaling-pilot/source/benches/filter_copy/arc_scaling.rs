//! Arc construction/cleanup and CPU-scaling diagnostics; one ordered target.
use compact_str::CompactString;
use gd::SharedRecordTable;
use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use serde_json::{Value, json};
use std::{
    hint::black_box,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Record {
    id: u64,
    selector: u64,
    amount: u64,
    name: CompactString,
    message: CompactString,
}

fn fixture(rows: usize, length: usize) -> SharedRecordTable<Record> {
    let mut table = SharedRecordTable::with_capacity(rows);
    for row in 0..rows {
        let text = |label| {
            let mut text = format!("{:08}-{label}-", row % 100_000_000);
            text.extend(std::iter::repeat_n('x', length - text.len()));
            CompactString::from(text)
        };
        table.push(Record {
            id: row as u64,
            selector: ((row % 100 * 37 + row / 100 * 17) % 100) as u64,
            amount: row as u64 * 13 + 7,
            name: text("name"),
            message: text("text"),
        });
    }
    table
}

enum Target {
    Current(SharedRecordTable<Record>),
    Handles(Vec<Arc<Record>>),
}
impl Target {
    fn as_slice(&self) -> &[Arc<Record>] {
        match self {
            Self::Current(table) => table.as_slice(),
            Self::Handles(records) => records,
        }
    }
}

fn select(part: &[Arc<Record>], percent: u64) -> Vec<Arc<Record>> {
    let mut records = Vec::with_capacity(part.len());
    for record in part {
        if record.selector < percent {
            records.push(Arc::clone(record));
        }
    }
    records
}

fn construct(
    source: &SharedRecordTable<Record>,
    percent: u64,
    workers: usize,
    strategy: &str,
    pool: &ThreadPool,
) -> Target {
    if strategy == "current" {
        return Target::Current(if workers == 1 {
            source.filter(|record| record.selector < percent)
        } else {
            pool.install(|| source.par_filter(|record| record.selector < percent))
        });
    }
    let source = source.as_slice();
    if workers == 1 {
        return Target::Handles(select(source, percent));
    }
    Target::Handles(pool.install(|| {
        match strategy {
            "fused-par-drop" => source
                .par_iter()
                .filter_map(|record| (record.selector < percent).then(|| Arc::clone(record)))
                .collect(),
            "chunks" | "chunks-par-drop" => {
                let size = source.len().div_ceil(workers).max(1);
                let parts: Vec<_> = source
                    .par_chunks(size)
                    .map(|part| select(part, percent))
                    .collect();
                let mut target = Vec::with_capacity(parts.iter().map(Vec::len).sum());
                for mut part in parts {
                    target.append(&mut part);
                }
                target
            }
            "indexed-par-drop" => {
                let rows: Vec<_> = source
                    .par_iter()
                    .enumerate()
                    .filter_map(|(row, record)| (record.selector < percent).then_some(row))
                    .collect();
                rows.par_iter()
                    .map(|&row| Arc::clone(&source[row]))
                    .collect()
            }
            _ => panic!("unknown strategy {strategy}"),
        }
    }))
}

fn destroy(target: Target, workers: usize, strategy: &str, pool: &ThreadPool) {
    if workers > 1 && strategy.ends_with("par-drop") {
        let Target::Handles(records) = target else {
            unreachable!()
        };
        pool.install(|| records.into_par_iter().with_min_len(4096).for_each(drop));
    } else {
        drop(target);
    }
}

fn digest(records: &[Arc<Record>]) -> (usize, u64) {
    fn bytes(mut hash: u64, bytes: &[u8]) -> u64 {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
        }
        hash
    }
    let mut hash = 14_695_981_039_346_656_037;
    for record in records {
        for value in [record.id, record.selector, record.amount] {
            hash = bytes(hash, &value.to_le_bytes());
        }
        for value in [&record.name, &record.message] {
            hash = bytes(
                bytes(hash, &(value.len() as u64).to_le_bytes()),
                value.as_bytes(),
            );
        }
    }
    (records.len(), hash)
}

fn cpu_time() -> Duration {
    let mut stamp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: stamp is initialized and writable for the whole call. The OS writes
    // a timespec to this correctly aligned pointer; success is checked before use.
    // Rust's standard library has no process-wide CPU clock. No unsafe table or
    // Arc operations are used; this call is outside the timed wall-clock interval.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut stamp) };
    assert_eq!(result, 0);
    Duration::new(
        u64::try_from(stamp.tv_sec).unwrap(),
        u32::try_from(stamp.tv_nsec).unwrap(),
    )
}

fn batch(
    source: &SharedRecordTable<Record>,
    percent: u64,
    workers: usize,
    strategy: &str,
    pool: &ThreadPool,
    iterations: u32,
    phases: bool,
) -> Value {
    let cpu = cpu_time();
    let start = Instant::now();
    let mut construction = Duration::ZERO;
    let mut cleanup = Duration::ZERO;
    for _ in 0..iterations {
        if phases {
            let begin = Instant::now();
            let target = black_box(construct(
                black_box(source),
                percent,
                workers,
                strategy,
                pool,
            ));
            construction += begin.elapsed();
            let begin = Instant::now();
            destroy(target, workers, strategy, pool);
            cleanup += begin.elapsed();
        } else {
            let target = black_box(construct(
                black_box(source),
                percent,
                workers,
                strategy,
                pool,
            ));
            destroy(target, workers, strategy, pool);
        }
    }
    let wall = start.elapsed();
    let cpu = cpu_time() - cpu;
    json!({"wall_ns": wall.as_secs_f64()*1e9/f64::from(iterations),
           "cpu_ns": cpu.as_secs_f64()*1e9/f64::from(iterations),
           "cpu_cores": cpu.as_secs_f64()/wall.as_secs_f64(),
           "construction_ns": construction.as_secs_f64()*1e9/f64::from(iterations),
           "cleanup_ns": cleanup.as_secs_f64()*1e9/f64::from(iterations)})
}

fn verify(source: &SharedRecordTable<Record>, target: &Target, percent: u64) -> (usize, u64) {
    let expected: Vec<_> = source
        .as_slice()
        .iter()
        .filter(|r| r.selector < percent)
        .collect();
    assert_eq!(target.as_slice().len(), expected.len());
    for (actual, expected) in target.as_slice().iter().zip(expected) {
        assert!(Arc::ptr_eq(actual, expected));
        assert_eq!(Arc::strong_count(actual), 2);
    }
    digest(target.as_slice())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        11,
        "rows length workers percent rounds samples sample_ms offset strategies verify|time"
    );
    let rows: usize = args[1].parse().unwrap();
    let length: usize = args[2].parse().unwrap();
    let workers: usize = args[3].parse().unwrap();
    let percent: u64 = args[4].parse().unwrap();
    let rounds: usize = args[5].parse().unwrap();
    let samples: usize = args[6].parse().unwrap();
    let sample_ms: u64 = args[7].parse().unwrap();
    let offset: usize = args[8].parse().unwrap();
    let strategies: Vec<_> = args[9].split(',').collect();
    assert!(rows <= 100_000_000 && [16, 128].contains(&length) && (1..=64).contains(&workers));
    assert!(percent <= 100 && rounds > 0 && samples > 0 && sample_ms > 0);
    assert!(strategies.iter().all(|s| {
        [
            "current",
            "fused-par-drop",
            "chunks",
            "chunks-par-drop",
            "indexed-par-drop",
        ]
        .contains(s)
    }));
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let source = fixture(rows, length);
    let mut verification = Vec::new();
    for strategy in &strategies {
        let target = construct(&source, percent, workers, strategy, &pool);
        let (count, hash) = verify(&source, &target, percent);
        destroy(target, workers, strategy, &pool);
        assert!(source.as_slice().iter().all(|r| Arc::strong_count(r) == 1));
        verification.push(json!({"strategy": strategy, "count":count,"digest":hash.to_string(),"refcounts_restored":true}));
    }
    if args[10] == "verify" {
        // Each strategy gets an independent source-drop test, including final
        // payload destruction in parallel (outside performance measurements).
        drop(source);
        for strategy in &strategies {
            let source = fixture(rows, length);
            let target = construct(&source, percent, workers, strategy, &pool);
            let before = digest(target.as_slice());
            drop(source);
            assert_eq!(digest(target.as_slice()), before);
            assert!(target.as_slice().iter().all(|r| Arc::strong_count(r) == 1));
            destroy(target, workers, strategy, &pool);
        }
        println!(
            "{}",
            json!({"verification":verification,"source_drop_checked":true})
        );
        return;
    }
    assert_eq!(args[10], "time");
    let mut measurements = Vec::new();
    for round in 0..rounds {
        for position in 0..strategies.len() {
            let strategy = strategies[(position + offset + round) % strategies.len()];
            let mut iterations = 1u32;
            loop {
                let result = batch(
                    &source, percent, workers, strategy, &pool, iterations, false,
                );
                if result["wall_ns"].as_f64().unwrap() * f64::from(iterations)
                    >= sample_ms as f64 * 1e6
                {
                    break;
                }
                iterations *= 2;
            }
            let batches: Vec<_> = (0..samples)
                .map(|_| {
                    batch(
                        &source, percent, workers, strategy, &pool, iterations, false,
                    )
                })
                .collect();
            measurements.push(json!({"strategy":strategy,"round":round+1,"iterations":iterations,"batches":batches}));
        }
    }
    // Diagnostic stage timing is retained separately from the primary samples.
    let mut phases = Vec::new();
    for strategy in &strategies {
        let batches: Vec<_> = (0..7)
            .map(|_| batch(&source, percent, workers, strategy, &pool, 1, true))
            .collect();
        phases.push(json!({"strategy":strategy,"batches":batches}));
    }
    assert!(source.as_slice().iter().all(|r| Arc::strong_count(r) == 1));
    println!(
        "{}",
        json!({"verification":verification,"measurements":measurements,"phases":phases,
                         "record_bytes":std::mem::size_of::<Record>(),"refcounts_restored":true})
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_pipelines_preserve_order_ownership_and_reference_counts() {
        for workers in [1, 8] {
            let pool = ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            for rows in [0, 1, 31, 1001] {
                for length in [16, 128] {
                    for percent in [0, 10, 50, 90, 100] {
                        let source = fixture(rows, length);
                        for strategy in [
                            "current",
                            "fused-par-drop",
                            "chunks",
                            "chunks-par-drop",
                            "indexed-par-drop",
                        ] {
                            let target = construct(&source, percent, workers, strategy, &pool);
                            verify(&source, &target, percent);
                            destroy(target, workers, strategy, &pool);
                            assert!(source.as_slice().iter().all(|r| Arc::strong_count(r) == 1));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn cpu_clock_advances() {
        let start = cpu_time();
        for i in 0..100_000 {
            black_box(i);
        }
        assert!(cpu_time() > start);
    }
}

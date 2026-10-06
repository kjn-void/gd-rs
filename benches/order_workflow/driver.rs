//! Standalone benchmark driver, deliberately independent of Criterion binary size.
//! Setup and SQL verification are outside timing. Isolated stages measure import,
//! preparation, or all variants; complete measures the full pipeline.
mod workload;

use gd::{SqliteDatabase, Table, Value, ValueRef};
use std::{env, hint::black_box, time::Instant};
use workload::{AUDIT_NAMES, Parameters, VARIANT_COLUMNS};

// Read the eight variant requests once, before any stage timer.
fn parameters(db: &SqliteDatabase) -> Vec<Parameters> {
    let mut statement = db
        .connection()
        .prepare(
            "SELECT region,from_day,to_day,status,minimum,discount_bp FROM parameters ORDER BY id",
        )
        .unwrap();

    statement
        .query_map([], |row| {
            Ok([
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ])
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// Correctness oracle, outside timing: compare cells, NULLs, row order, and row
// count against fixture.py's independent SQL views.
fn verify_table(db: &SqliteDatabase, table: &Table, sql: &str) {
    let mut statement = db.connection().prepare(sql).unwrap();

    assert_eq!(statement.column_count(), table.column_count());

    let mut expected = statement.query([]).unwrap();

    for row in table.rows() {
        let reference = expected.next().unwrap().expect("extra output row");

        for (column, actual) in row.iter().enumerate() {
            use rusqlite::types::ValueRef as Sql;

            let expected = match reference.get_ref(column).unwrap() {
                Sql::Null => ValueRef::Null,
                Sql::Integer(v) => ValueRef::I64(v),
                Sql::Text(v) => ValueRef::String(std::str::from_utf8(v).unwrap()),
                _ => panic!("unexpected oracle type"),
            };

            assert_eq!(
                actual,
                expected,
                "{sql}: row {}, column {column}",
                row.position()
            );
        }
    }

    assert!(
        expected.next().unwrap().is_none(),
        "missing output rows: {sql}"
    );
}

// Deterministic fingerprints let the runner compare Rust and both C++ layouts.
// These verification digests are never computed inside a measured stage.
fn digest(table: &Table) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    let mut add = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    };

    add(&u64::try_from(table.row_count()).unwrap().to_le_bytes());
    add(&u64::try_from(table.column_count()).unwrap().to_le_bytes());

    for row in table.rows() {
        for value in row.iter() {
            match value {
                ValueRef::Null => add(&[0]),
                ValueRef::I64(v) => {
                    add(&[1]);
                    add(&v.to_le_bytes());
                }
                ValueRef::String(v) => {
                    add(&[2]);
                    add(&u64::try_from(v.len()).unwrap().to_le_bytes());
                    add(v.as_bytes());
                }
                _ => panic!("unexpected digest type"),
            }
        }
    }

    hash
}

fn verify(db: &SqliteDatabase, p: &[Parameters], pool: &rayon::ThreadPool, sorted: bool) {
    let input = workload::load(db);
    let prepared = workload::prepare(&input, sorted);

    // Results must own their strings after the input tables have been destroyed.
    drop(input);

    let names = AUDIT_NAMES.join(",");

    verify_table(
        db,
        &prepared.audit,
        &format!("SELECT {names} FROM expected_audit ORDER BY source_pos"),
    );
    verify_table(
        db,
        &prepared.clean,
        &format!("SELECT {names} FROM expected_clean ORDER BY source_pos"),
    );

    let mut outputs = workload::variants(&prepared.clean, p, pool);
    let clean_hash = digest(&prepared.clean);
    let hashes: Vec<_> = outputs.iter().map(digest).collect();
    let counts: Vec<_> = outputs.iter().map(Table::row_count).collect();
    let names = VARIANT_COLUMNS.map(|i| AUDIT_NAMES[i]).join(",");

    for (i, output) in outputs.iter().enumerate() {
        verify_table(
            db,
            output,
            &format!(
                "SELECT {names} FROM expected_variants WHERE parameter_id={i} ORDER BY source_pos"
            ),
        );
    }

    // Ownership check: changing one variant must leave clean and its peers
    // unchanged. Destroying prepared must not invalidate any output's strings.
    if !outputs[0].is_empty() {
        outputs[0]
            .set_cell(0, 1, Value::from("changed independently"))
            .unwrap();

        assert_eq!(digest(&prepared.clean), clean_hash);

        for (other, &hash) in outputs[1..].iter().zip(&hashes[1..]) {
            assert_eq!(digest(other), hash);
        }
    }

    drop(prepared);

    for (other, &hash) in outputs[1..].iter().zip(&hashes[1..]) {
        assert_eq!(digest(other), hash);
    }

    println!(
        "{}",
        serde_json::json!({"implementation":"rust", "verified":true,
        "workers":pool.current_num_threads(), "sorted":sorted, "counts":counts, "digests":hashes,
        "sqlite":rusqlite::version()})
    );
}

fn main() {
    let args: Vec<_> = env::args().collect();

    assert!(
        args.len() == 6,
        "usage: order_workflow DATABASE WORKERS verify|import|prepare|variants|complete SAMPLES native|sorted"
    );

    let workers: usize = args[2].parse().unwrap();
    let samples: usize = args[4].parse().unwrap();

    assert!(workers > 0 && workers <= 256 && samples > 0);
    assert!(matches!(args[5].as_str(), "native" | "sorted"));

    let sorted = args[5] == "sorted";

    // Connection setup, parameter validation, and worker-pool construction
    // happen once and are excluded from every stage measurement.
    let db = SqliteDatabase::open(&args[1]).unwrap();
    db.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-65536;")
        .unwrap();

    let p = parameters(&db);

    assert_eq!(p.len(), 8);

    for &[region, from_day, to_day, status, minimum, discount_bp] in &p {
        assert!(
            region >= -1
                && from_day <= to_day
                && status >= -1
                && minimum >= 0
                && (0..=10_000).contains(&discount_bp),
            "invalid variant parameters"
        );
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();

    let stage = args[3].as_str();

    if stage == "verify" {
        verify(&db, &p, &pool, sorted);
        return;
    }

    // Read report metadata outside timing as well.
    let rows: i64 = db
        .connection()
        .query_row("SELECT count(*) FROM lines", [], |row| row.get(0))
        .unwrap();

    // Isolated "prepare" reuses imported inputs; isolated "variants" also
    // reuses audit/clean. "complete" recreates all tables in every iteration.
    let input = matches!(stage, "prepare" | "variants").then(|| workload::load(&db));
    let prepared =
        (stage == "variants").then(|| workload::prepare(input.as_ref().unwrap(), sorted));

    let mut seconds = Vec::with_capacity(samples);

    // Iteration zero is a discarded warmup with the same work and lifetimes
    // as recorded samples. Variants waits for the whole batch before returning.
    for iteration in 0..=samples {
        let start = Instant::now();

        match stage {
            "import" => {
                // Import and destroy all three input tables.
                drop(black_box(workload::load(&db)));
            }
            "prepare" => {
                // Join, validate, materialize audit/clean, then destroy them.
                drop(black_box(workload::prepare(
                    input.as_ref().unwrap(),
                    sorted,
                )));
            }
            "variants" => {
                // Produce all eight parameterized subsets and destroy outputs.
                drop(black_box(workload::variants(
                    &prepared.as_ref().unwrap().clean,
                    &p,
                    &pool,
                )));
            }
            "complete" => {
                // Import -> prepare -> eight variants. Drop every output,
                // intermediate, and input before the sample timer stops.
                let input = workload::load(&db);
                let prepared = workload::prepare(&input, sorted);

                drop(black_box(workload::variants(&prepared.clean, &p, &pool)));
                drop(black_box(prepared));
                drop(black_box(input));
            }
            _ => panic!("unknown stage: {stage}"),
        }

        // Branch-local tables have been destroyed: allocation, copying, and
        // destruction are timed. JSON reporting below remains outside timing.
        let elapsed = start.elapsed().as_secs_f64();

        if iteration != 0 {
            seconds.push(elapsed);
        }
    }

    println!(
        "{}",
        serde_json::json!({"implementation":"rust", "rows":rows, "workers":workers,
        "stage":stage, "index":args[5], "seconds":seconds, "sqlite":rusqlite::version()})
    );
}

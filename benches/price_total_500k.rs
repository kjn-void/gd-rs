//! Cross-platform price calculation benchmark and `perf stat` workload.

use std::{env, hint::black_box, process::ExitCode, time::Instant};

use gd::{ColumnSpec, DataType, Schema, Table, Value};

const DEFAULT_ROWS: usize = 500_000;
const MINIMUM_ROWS: usize = 2_000;
const WARMUP_LOGICAL_ROWS: usize = 8_000_000;
const TIMING_LOGICAL_ROWS: usize = 256_000_000;
const PERF_LOGICAL_ROWS: usize = 2_048_000_000;
const TIMING_SAMPLES: u32 = 9;

#[derive(Clone, Copy)]
enum Mode {
    Check,
    Timing,
    Perf,
}

struct Config {
    mode: Mode,
    rows: usize,
}

#[inline(never)]
fn calculate(prices: &[f64], taxes: &[f64], quantities: &[u32], totals: &mut [f64]) {
    for (((total, &price), &tax), &quantity) in
        totals.iter_mut().zip(prices).zip(taxes).zip(quantities)
    {
        *total = f64::from(quantity) * price + tax;
    }
}

fn make_table(rows: usize) -> Table {
    let schema = Schema::new([
        ColumnSpec::new("price", DataType::F64),
        ColumnSpec::new("tax", DataType::F64),
        ColumnSpec::new("qty", DataType::U32),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, rows);
    for row in 0..rows {
        let price = 1.0 + f64::from(u32::try_from(row % 10_000).unwrap()) * 0.01;
        let tax = f64::from(u32::try_from(row % 26).unwrap());
        let quantity = u32::try_from(row % 100 + 1).unwrap();
        table
            .push_row([Value::F64(price), Value::F64(tax), Value::U32(quantity)])
            .unwrap();
    }
    table
}

fn parse_config() -> Result<Config, String> {
    let mut args = env::args().skip(1);
    let mode = match args.next().as_deref() {
        None => Mode::Check,
        Some("timing") => Mode::Timing,
        Some("perf") => Mode::Perf,
        _ => return Err("usage: price_total_500k {timing|perf} [ROWS]".to_owned()),
    };
    let rows = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| "ROWS must be an unsigned integer".to_owned())?
        .unwrap_or(DEFAULT_ROWS);
    if args.next().is_some() || rows < MINIMUM_ROWS {
        return Err(format!(
            "usage: price_total_500k {{timing|perf}} [ROWS >= {MINIMUM_ROWS}]"
        ));
    }
    Ok(Config { mode, rows })
}

fn iterations_for(logical_rows: usize, rows: usize) -> u32 {
    u32::try_from(logical_rows.div_ceil(rows)).expect("iteration count fits in u32")
}

fn run_iterations(
    iterations: u32,
    prices: &[f64],
    taxes: &[f64],
    quantities: &[u32],
    totals: &mut [f64],
) {
    for _ in 0..iterations {
        calculate(
            black_box(prices),
            black_box(taxes),
            black_box(quantities),
            black_box(&mut *totals),
        );
        black_box(&*totals);
    }
}

fn main() -> ExitCode {
    let config = match parse_config() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    let table = make_table(config.rows);
    let prices = table
        .column_named("price")
        .unwrap()
        .as_slice::<f64>()
        .unwrap();
    let taxes = table
        .column_named("tax")
        .unwrap()
        .as_slice::<f64>()
        .unwrap();
    let quantities = table
        .column_named("qty")
        .unwrap()
        .as_slice::<u32>()
        .unwrap();
    let mut totals = vec![0.0; config.rows];

    assert_eq!(prices.len(), config.rows);
    assert_eq!(taxes.len(), config.rows);
    assert_eq!(quantities.len(), config.rows);
    let warmups = iterations_for(WARMUP_LOGICAL_ROWS, config.rows);
    match config.mode {
        Mode::Check => run_iterations(1, prices, taxes, quantities, &mut totals),
        Mode::Timing => {
            let iterations = iterations_for(TIMING_LOGICAL_ROWS, config.rows);
            run_iterations(warmups, prices, taxes, quantities, &mut totals);
            for _ in 0..TIMING_SAMPLES {
                let start = Instant::now();
                run_iterations(iterations, prices, taxes, quantities, &mut totals);
                println!(
                    "{:.6}",
                    start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(iterations)
                );
            }
        }
        Mode::Perf => {
            run_iterations(warmups, prices, taxes, quantities, &mut totals);
            run_iterations(
                iterations_for(PERF_LOGICAL_ROWS, config.rows),
                prices,
                taxes,
                quantities,
                &mut totals,
            );
        }
    }

    let checksum_row = 9_999.min(config.rows - 1);
    let checksum =
        totals[0] + totals[1] + totals[25] + totals[checksum_row] + totals[config.rows - 1];
    eprintln!("mode=rust-soa rows={} checksum={checksum:.17}", config.rows);
    ExitCode::SUCCESS
}

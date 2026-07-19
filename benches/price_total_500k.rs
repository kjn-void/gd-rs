//! Cross-platform 500,000-row price calculation benchmark and `perf stat` workload.

use std::{env, hint::black_box, process::ExitCode, time::Instant};

use gd::{ColumnSpec, DataType, Schema, Table, Value};

const ROWS: usize = 500_000;
const WARMUPS: u32 = 16;
const TIMING_SAMPLES: u32 = 9;
const TIMING_ITERATIONS: u32 = 512;
const PERF_ITERATIONS: u32 = 4_096;

#[derive(Clone, Copy)]
enum Mode {
    Check,
    Timing,
    Perf,
}

#[inline(never)]
fn calculate(prices: &[f64], taxes: &[f64], quantities: &[u32], totals: &mut [f64]) {
    for (((total, &price), &tax), &quantity) in
        totals.iter_mut().zip(prices).zip(taxes).zip(quantities)
    {
        *total = f64::from(quantity) * price + tax;
    }
}

fn make_table() -> Table {
    let schema = Schema::new([
        ColumnSpec::new("price", DataType::F64),
        ColumnSpec::new("tax", DataType::F64),
        ColumnSpec::new("qty", DataType::U32),
    ])
    .unwrap();
    let mut table = Table::with_capacity(schema, ROWS);
    for row in 0..ROWS {
        let price = 1.0 + f64::from(u32::try_from(row % 10_000).unwrap()) * 0.01;
        let tax = f64::from(u32::try_from(row % 26).unwrap());
        let quantity = u32::try_from(row % 100 + 1).unwrap();
        table
            .push_row([Value::F64(price), Value::F64(tax), Value::U32(quantity)])
            .unwrap();
    }
    table
}

fn parse_mode() -> Result<Mode, String> {
    match env::args().nth(1).as_deref() {
        None => Ok(Mode::Check),
        Some("timing") => Ok(Mode::Timing),
        Some("perf") => Ok(Mode::Perf),
        _ => Err("usage: price_total_500k {timing|perf}".to_owned()),
    }
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
    let mode = match parse_mode() {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    let table = make_table();
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
    let mut totals = vec![0.0; ROWS];

    assert_eq!(prices.len(), ROWS);
    assert_eq!(taxes.len(), ROWS);
    assert_eq!(quantities.len(), ROWS);
    match mode {
        Mode::Check => run_iterations(1, prices, taxes, quantities, &mut totals),
        Mode::Timing => {
            run_iterations(WARMUPS, prices, taxes, quantities, &mut totals);
            for _ in 0..TIMING_SAMPLES {
                let start = Instant::now();
                run_iterations(TIMING_ITERATIONS, prices, taxes, quantities, &mut totals);
                println!(
                    "{:.6}",
                    start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(TIMING_ITERATIONS)
                );
            }
        }
        Mode::Perf => {
            run_iterations(WARMUPS, prices, taxes, quantities, &mut totals);
            run_iterations(PERF_ITERATIONS, prices, taxes, quantities, &mut totals);
        }
    }

    let checksum = totals[0] + totals[1] + totals[25] + totals[9_999] + totals[ROWS - 1];
    eprintln!("mode=rust-soa checksum={checksum:.17}");
    ExitCode::SUCCESS
}

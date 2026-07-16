//! One-pass Rayon benchmark for a very large two-column table.

use std::{env, process::ExitCode, time::Instant};

use gd::{ColumnSpec, DataType, Schema, Table, Value};
use rayon::prelude::*;

const DEFAULT_ROWS: usize = 100_000_000;
const DEFAULT_MAX_ARG: u32 = 23;

#[derive(Clone, Copy)]
enum Operation {
    Fibonacci,
    Square,
}

impl Operation {
    fn from_environment() -> Result<Self, String> {
        match env::var("GD_PAR_OPERATION").as_deref() {
            Err(env::VarError::NotPresent) | Ok("fibonacci") => Ok(Self::Fibonacci),
            Ok("square") => Ok(Self::Square),
            Ok(value) => Err(format!(
                "invalid GD_PAR_OPERATION={value}: expected fibonacci or square"
            )),
            Err(error) => Err(format!("invalid GD_PAR_OPERATION: {error}")),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Fibonacci => "fibonacci",
            Self::Square => "square",
        }
    }
}

#[inline(never)]
fn recursive_fibonacci(value: u32) -> u32 {
    if value < 2 {
        value
    } else {
        recursive_fibonacci(value - 1) + recursive_fibonacci(value - 2)
    }
}

fn expected_fibonacci(value: u32) -> u32 {
    let (mut previous, mut current) = (0_u32, 1_u32);
    for _ in 0..value {
        (previous, current) = (current, previous + current);
    }
    previous
}

fn transform(args: &[u32], results: &mut [u32], operation: Operation) {
    match operation {
        Operation::Fibonacci => args
            .par_iter()
            .zip(results.par_iter_mut())
            .for_each(|(&arg, result)| *result = recursive_fibonacci(arg)),
        Operation::Square => args
            .par_iter()
            .zip(results.par_iter_mut())
            .for_each(|(&arg, result)| *result = arg * arg),
    }
}

fn expected_result(arg: u32, operation: Operation) -> u32 {
    match operation {
        Operation::Fibonacci => expected_fibonacci(arg),
        Operation::Square => arg * arg,
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

#[allow(clippy::cast_precision_loss)] // The default 100M row count is exactly representable.
fn run() -> Result<(), String> {
    let cargo_test_mode =
        cfg!(debug_assertions) || env::args_os().any(|argument| argument == "--test");
    let rows = setting(
        "GD_PAR_ROWS",
        if cargo_test_mode { 400 } else { DEFAULT_ROWS },
    )?;
    let max_arg = setting(
        "GD_PAR_MAX_ARG",
        if cargo_test_mode { 20 } else { DEFAULT_MAX_ARG },
    )?;
    let operation = Operation::from_environment()?;
    let warmups = setting("GD_PAR_WARMUPS", 0_usize)?;
    let repetitions = setting("GD_PAR_REPETITIONS", 1_usize)?;
    if rows == 0 {
        return Err("GD_PAR_ROWS must be greater than zero".into());
    }
    if !(1..=23).contains(&max_arg) {
        return Err("GD_PAR_MAX_ARG must be in 1..=23".into());
    }
    if repetitions == 0 {
        return Err("GD_PAR_REPETITIONS must be greater than zero".into());
    }

    let schema = Schema::new([
        ColumnSpec::new("arg", DataType::U32),
        ColumnSpec::new("result", DataType::U32),
    ])
    .map_err(|error| error.to_string())?;

    let build_started = Instant::now();
    let mut table = Table::with_capacity(schema, rows);
    for row in 0..rows {
        let arg = u32::try_from(row % max_arg as usize).unwrap() + 1;
        table
            .push_row([Value::U32(arg), Value::U32(0)])
            .map_err(|error| error.to_string())?;
    }
    let build_seconds = build_started.elapsed().as_secs_f64();

    let transform_seconds = {
        let (args, results) = table
            .column_pair_mut(0, 1)
            .ok_or("arg and result columns must be distinct")?;
        let args = args.as_slice::<u32>().map_err(|error| error.to_string())?;
        let results = results
            .as_mut_slice::<u32>()
            .map_err(|error| error.to_string())?;
        for _ in 0..warmups {
            transform(args, results, operation);
        }
        let transform_started = Instant::now();
        for _ in 0..repetitions {
            transform(args, results, operation);
        }
        transform_started.elapsed().as_secs_f64()
    };

    let args = table
        .column_named("arg")
        .unwrap()
        .as_slice::<u32>()
        .map_err(|error| error.to_string())?;
    let results = table
        .column_named("result")
        .unwrap()
        .as_slice::<u32>()
        .map_err(|error| error.to_string())?;
    if !args
        .par_iter()
        .zip(results.par_iter())
        .all(|(&arg, &result)| result == expected_result(arg, operation))
    {
        return Err("result validation failed".into());
    }

    println!("implementation=Rust/Rayon");
    println!("rows={rows}");
    println!("arg_range=1..={max_arg}");
    println!("operation={}", operation.name());
    println!("threads={}", rayon::current_num_threads());
    println!("row_storage_bytes={}", rows * size_of::<[u32; 2]>());
    println!("build_seconds={build_seconds:.6}");
    println!("warmups={warmups}");
    println!("repetitions={repetitions}");
    println!("transform_seconds={transform_seconds:.6}");
    println!(
        "transform_seconds_per_pass={:.9}",
        transform_seconds / repetitions as f64
    );
    println!(
        "transform_rows_per_second={:.3}",
        rows as f64 * repetitions as f64 / transform_seconds
    );
    println!("validation=ok");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

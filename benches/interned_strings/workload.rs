use gd::{ColumnSpec, DataType, Schema, Table, Value};
use std::{hint::black_box, time::Instant};

fn strings(count: usize, length: usize, prefix: char) -> Vec<String> {
    (0..count)
        .map(|index| {
            let mut text = format!("{prefix}{index:08}");
            text.extend(std::iter::repeat_n('x', length - text.len()));
            text
        })
        .collect()
}

fn insert(schema: &Schema, rows: usize, input: &[String]) -> Table {
    let mut table = Table::with_capacity(schema.clone(), rows);
    for row in 0..rows {
        table
            .push_row([Value::from(input[row % input.len()].as_str())])
            .unwrap();
    }
    table
}

fn mutate(table: &mut Table, input: &[String], rotate: bool) {
    for row in 0..table.row_count() {
        let index = if rotate { (row + 1) % input.len() } else { row };
        table
            .set_cell(row, 0, Value::from(input[index].as_str()))
            .unwrap();
    }
}

fn digest(table: &Table) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for row in table.rows() {
        let value = row.get(0).unwrap();
        for &byte in value.as_str().unwrap().as_bytes().iter().chain([&0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

pub fn run(live_bytes: fn() -> usize) {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        8,
        "implementation mode operation rows unique length samples"
    );
    let interned = match args[1].as_str() {
        "interned" => true,
        "default" => false,
        _ => panic!("unknown implementation"),
    };
    let mode = args[2].as_str();
    let operation = args[3].as_str();
    let rows: usize = args[4].parse().unwrap();
    let unique: usize = args[5].parse().unwrap();
    let length: usize = args[6].parse().unwrap();
    let samples: usize = args[7].parse().unwrap();
    assert!(rows > 0 && unique > 0 && unique <= rows && length >= 9 && samples > 0);
    assert!(matches!(mode, "timing" | "memory" | "verify"));
    assert!(matches!(
        operation,
        "insert" | "copy" | "rotate" | "replace"
    ));
    let schema = Schema::new([if interned {
        ColumnSpec::interned_string("text")
    } else {
        ColumnSpec::new("text", DataType::String)
    }])
    .unwrap();
    let input = strings(unique, length, 'a');
    let replacement = strings(rows, length, 'b');
    let baseline = live_bytes();
    let source = insert(&schema, rows, &input);
    let source_bytes = live_bytes().saturating_sub(baseline);
    let expected_source = digest(&source);
    // Verify that a copy remains usable after its original has been destroyed.
    let lifetime_copy = {
        let temporary = source.clone();
        temporary.clone()
    };
    assert_eq!(digest(&lifetime_copy), expected_source);
    drop(lifetime_copy);
    let mut times = Vec::with_capacity(samples);
    let mut last_digest = 0;
    let mut result_bytes = 0;
    for _ in 0..samples {
        let result_baseline = live_bytes();
        // Mutation setup is excluded. Insert/copy include allocation and schema
        // setup; all operations exclude destruction and full-content hashing.
        let mut result = if matches!(operation, "rotate" | "replace") {
            insert(&schema, rows, &input)
        } else {
            Table::new(schema.clone())
        };
        let start = Instant::now();
        match operation {
            "insert" => result = insert(black_box(&schema), rows, black_box(&input)),
            "copy" => result = black_box(&source).clone(),
            "rotate" => mutate(black_box(&mut result), black_box(&input), true),
            "replace" => mutate(black_box(&mut result), black_box(&replacement), false),
            _ => unreachable!(),
        }
        let nanos = start.elapsed().as_nanos();
        black_box(&result);
        let after_result = live_bytes();
        // Report all additional retained storage while the source is alive.
        result_bytes = after_result.saturating_sub(result_baseline);
        last_digest = digest(&result);
        if mode == "verify" {
            result
                .set_cell(0, 0, Value::from(replacement[0].as_str()))
                .unwrap();
        }
        assert_eq!(digest(&source), expected_source, "operation changed source");
        times.push(nanos);
    }
    println!(
        "{{\"digest\":{last_digest},\"source_digest\":{expected_source},\"source_bytes\":{source_bytes},\"result_bytes\":{result_bytes},\"nanos\":{times:?}}}"
    );
}

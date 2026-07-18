//! Ten-million-row price calculation over the table's typed `SoA` columns.

use std::{hint::black_box, time::Duration};

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use gd::{ColumnSpec, DataType, Schema, Table, Value};

const ROWS: usize = 10_000_000;

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
        let qty = u32::try_from(row % 100 + 1).unwrap();
        table
            .push_row([Value::F64(price), Value::F64(tax), Value::U32(qty)])
            .unwrap();
    }
    table
}

#[inline(never)]
fn calculate_total_costs(prices: &[f64], taxes: &[f64], quantities: &[u32], totals: &mut [f64]) {
    assert_eq!(prices.len(), taxes.len());
    assert_eq!(prices.len(), quantities.len());
    assert_eq!(prices.len(), totals.len());
    for (((total, &price), &tax), &quantity) in
        totals.iter_mut().zip(prices).zip(taxes).zip(quantities)
    {
        *total = f64::from(quantity) * price * (1.0 + tax / 100.0);
    }
}

fn price_total(criterion: &mut Criterion) {
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

    calculate_total_costs(prices, taxes, quantities, &mut totals);
    for row in [0, 1, 25, 9_999, ROWS - 1] {
        let expected = f64::from(quantities[row]) * prices[row] * (1.0 + taxes[row] / 100.0);
        assert_eq!(totals[row].to_bits(), expected.to_bits());
    }

    let mut group = criterion.benchmark_group("Table/PriceTotal/10000000");
    group
        .throughput(Throughput::Elements(ROWS as u64))
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));
    group.bench_function("TypedSoA", |bencher| {
        bencher.iter(|| {
            calculate_total_costs(
                black_box(prices),
                black_box(taxes),
                black_box(quantities),
                black_box(&mut totals),
            );
            black_box(&totals);
        });
    });
    group.finish();
}

criterion_group!(benches, price_total);
criterion_main!(benches);

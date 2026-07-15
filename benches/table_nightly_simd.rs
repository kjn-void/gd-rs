//! Nightly-only explicit SIMD benchmarks for the mixed-numeric table fixture.

#![cfg_attr(nightly_simd, feature(portable_simd))]

#[cfg(not(nightly_simd))]
fn main() {}

#[cfg(nightly_simd)]
mod nightly {
    use std::{
        hint::black_box,
        simd::{
            Simd,
            cmp::SimdOrd,
            num::{SimdFloat, SimdInt, SimdUint},
        },
        time::Duration,
    };

    use criterion::{Criterion, SamplingMode};
    use gd::{ColumnSpec, DataType, Schema, Table, Value};

    const ROWS: usize = 10_000_000;

    fn schema() -> Schema {
        Schema::new([
            ColumnSpec::new("u8_value", DataType::U8),
            ColumnSpec::new("f64_value", DataType::F64),
            ColumnSpec::new("u16_value", DataType::U16),
            ColumnSpec::new("u64_value", DataType::U64),
            ColumnSpec::new("f32_value", DataType::F32),
            ColumnSpec::new("i32_value", DataType::I32),
        ])
        .unwrap()
    }

    #[allow(clippy::cast_precision_loss)]
    fn make_table() -> Table {
        let mut table = Table::with_capacity(schema(), ROWS);
        for row in 0..ROWS {
            let value = (u64::try_from(row).unwrap() * 48_271) % u64::try_from(ROWS).unwrap();
            let centered = i64::try_from(value).unwrap() - 5_000_000;
            table
                .push_row([
                    Value::U8(u8::try_from(value % 251).unwrap()),
                    Value::F64(value as f64 * 0.5 - 2_500_000.0),
                    Value::U16(u16::try_from(value % 65_521).unwrap()),
                    Value::U64(value * 1_000),
                    Value::F32(centered as f32),
                    Value::I32(i32::try_from(centered).unwrap()),
                ])
                .unwrap();
        }
        table
    }

    macro_rules! integer_maximum {
        ($name:ident, $type:ty, $lanes:literal) => {
            fn $name(values: &[$type]) -> $type {
                let mut chunks = values.chunks_exact($lanes * 4);
                let mut maximum0 = Simd::<$type, $lanes>::splat(<$type>::MIN);
                let mut maximum1 = maximum0;
                let mut maximum2 = maximum0;
                let mut maximum3 = maximum0;
                for chunk in &mut chunks {
                    maximum0 = maximum0.simd_max(Simd::from_slice(&chunk[..$lanes]));
                    maximum1 = maximum1.simd_max(Simd::from_slice(&chunk[$lanes..$lanes * 2]));
                    maximum2 = maximum2.simd_max(Simd::from_slice(&chunk[$lanes * 2..$lanes * 3]));
                    maximum3 = maximum3.simd_max(Simd::from_slice(&chunk[$lanes * 3..$lanes * 4]));
                }
                let maximum = maximum0
                    .simd_max(maximum1)
                    .simd_max(maximum2)
                    .simd_max(maximum3);
                let mut remainder = chunks.remainder().chunks_exact($lanes);
                let maximum = remainder
                    .by_ref()
                    .fold(maximum, |current, chunk| {
                        current.simd_max(Simd::from_slice(chunk))
                    })
                    .reduce_max();
                remainder
                    .remainder()
                    .iter()
                    .copied()
                    .fold(maximum, <$type>::max)
            }
        };
    }

    macro_rules! float_maximum {
        ($name:ident, $type:ty, $lanes:literal) => {
            fn $name(values: &[$type]) -> $type {
                let mut chunks = values.chunks_exact($lanes * 4);
                let mut maximum0 = Simd::<$type, $lanes>::splat(<$type>::NEG_INFINITY);
                let mut maximum1 = maximum0;
                let mut maximum2 = maximum0;
                let mut maximum3 = maximum0;
                for chunk in &mut chunks {
                    maximum0 = maximum0.simd_max(Simd::from_slice(&chunk[..$lanes]));
                    maximum1 = maximum1.simd_max(Simd::from_slice(&chunk[$lanes..$lanes * 2]));
                    maximum2 = maximum2.simd_max(Simd::from_slice(&chunk[$lanes * 2..$lanes * 3]));
                    maximum3 = maximum3.simd_max(Simd::from_slice(&chunk[$lanes * 3..$lanes * 4]));
                }
                let maximum = maximum0
                    .simd_max(maximum1)
                    .simd_max(maximum2)
                    .simd_max(maximum3);
                let mut remainder = chunks.remainder().chunks_exact($lanes);
                let maximum = remainder
                    .by_ref()
                    .fold(maximum, |current, chunk| {
                        current.simd_max(Simd::from_slice(chunk))
                    })
                    .reduce_max();
                remainder
                    .remainder()
                    .iter()
                    .copied()
                    .fold(
                        maximum,
                        |left, right| {
                            if right > left { right } else { left }
                        },
                    )
            }
        };
    }

    integer_maximum!(maximum_u8, u8, 16);
    float_maximum!(maximum_f64, f64, 2);
    integer_maximum!(maximum_u16, u16, 8);
    integer_maximum!(maximum_u64, u64, 2);
    float_maximum!(maximum_f32, f32, 4);
    integer_maximum!(maximum_i32, i32, 4);

    pub(super) fn mixed_numeric_maximum(criterion: &mut Criterion) {
        let table = make_table();
        let u8_values = table.column(0).unwrap().as_slice::<u8>().unwrap();
        let f64_values = table.column(1).unwrap().as_slice::<f64>().unwrap();
        let u16_values = table.column(2).unwrap().as_slice::<u16>().unwrap();
        let u64_values = table.column(3).unwrap().as_slice::<u64>().unwrap();
        let f32_values = table.column(4).unwrap().as_slice::<f32>().unwrap();
        let i32_values = table.column(5).unwrap().as_slice::<i32>().unwrap();

        assert_eq!(maximum_u8(u8_values), 250);
        assert_eq!(maximum_f64(f64_values), 2_499_999.5);
        assert_eq!(maximum_u16(u16_values), 65_520);
        assert_eq!(maximum_u64(u64_values), 9_999_999_000);
        assert_eq!(maximum_f32(f32_values), 4_999_999.0);
        assert_eq!(maximum_i32(i32_values), 4_999_999);

        let mut group = criterion.benchmark_group("Table/MixedNumeric/10000000/Maximum/StdSimd");
        group
            .sampling_mode(SamplingMode::Flat)
            .sample_size(10)
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(5));
        group.bench_function("u8", |bencher| {
            bencher.iter(|| black_box(maximum_u8(black_box(u8_values))));
        });
        group.bench_function("f64", |bencher| {
            bencher.iter(|| black_box(maximum_f64(black_box(f64_values))));
        });
        group.bench_function("u16", |bencher| {
            bencher.iter(|| black_box(maximum_u16(black_box(u16_values))));
        });
        group.bench_function("u64", |bencher| {
            bencher.iter(|| black_box(maximum_u64(black_box(u64_values))));
        });
        group.bench_function("f32", |bencher| {
            bencher.iter(|| black_box(maximum_f32(black_box(f32_values))));
        });
        group.bench_function("i32", |bencher| {
            bencher.iter(|| black_box(maximum_i32(black_box(i32_values))));
        });
        group.finish();
    }
}

#[cfg(nightly_simd)]
use criterion::{criterion_group, criterion_main};

#[cfg(nightly_simd)]
criterion_group!(benches, nightly::mixed_numeric_maximum);

#[cfg(nightly_simd)]
criterion_main!(benches);

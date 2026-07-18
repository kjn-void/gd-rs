//! Required and nullable `u32` maximum reductions on stable and nightly Rust.

#![cfg_attr(nightly_simd, feature(portable_simd))]

use std::{hint::black_box, mem::size_of, time::Duration};

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};

const ROWS: usize = 10_000_000;
const NULL_INTERVAL: usize = 16;

#[inline(never)]
fn maximum_required_stable(values: &[u32]) -> Option<u32> {
    values.iter().copied().max()
}

#[inline(never)]
fn maximum_nullable_stable(values: &[Option<u32>]) -> Option<u32> {
    values.iter().flatten().copied().max()
}

#[cfg(nightly_simd)]
mod explicit_simd {
    use std::simd::{Simd, cmp::SimdOrd, num::SimdUint};

    const LANES: usize = 4;
    const UNROLL: usize = 4;
    const CHUNK: usize = LANES * UNROLL;

    #[inline(never)]
    pub(super) fn maximum_required(values: &[u32]) -> Option<u32> {
        let (&first, values) = values.split_first()?;
        let mut chunks = values.chunks_exact(CHUNK);
        let mut maximum0 = Simd::<u32, LANES>::splat(first);
        let mut maximum1 = maximum0;
        let mut maximum2 = maximum0;
        let mut maximum3 = maximum0;
        for chunk in &mut chunks {
            maximum0 = maximum0.simd_max(Simd::from_slice(&chunk[..LANES]));
            maximum1 = maximum1.simd_max(Simd::from_slice(&chunk[LANES..LANES * 2]));
            maximum2 = maximum2.simd_max(Simd::from_slice(&chunk[LANES * 2..LANES * 3]));
            maximum3 = maximum3.simd_max(Simd::from_slice(&chunk[LANES * 3..CHUNK]));
        }
        let maximum = maximum0
            .simd_max(maximum1)
            .simd_max(maximum2)
            .simd_max(maximum3)
            .reduce_max();
        Some(chunks.remainder().iter().copied().fold(maximum, u32::max))
    }

    #[inline(never)]
    pub(super) fn maximum_nullable(values: &[Option<u32>]) -> Option<u32> {
        let (first_index, first) = values
            .iter()
            .enumerate()
            .find_map(|(index, value)| value.map(|value| (index, value)))?;
        let values = &values[first_index + 1..];
        let mut chunks = values.chunks_exact(CHUNK);
        let mut maximum0 = Simd::<u32, LANES>::splat(first);
        let mut maximum1 = maximum0;
        let mut maximum2 = maximum0;
        let mut maximum3 = maximum0;
        for chunk in &mut chunks {
            maximum0 = maximum0.simd_max(load_nullable(&chunk[..LANES]));
            maximum1 = maximum1.simd_max(load_nullable(&chunk[LANES..LANES * 2]));
            maximum2 = maximum2.simd_max(load_nullable(&chunk[LANES * 2..LANES * 3]));
            maximum3 = maximum3.simd_max(load_nullable(&chunk[LANES * 3..CHUNK]));
        }
        let maximum = maximum0
            .simd_max(maximum1)
            .simd_max(maximum2)
            .simd_max(maximum3)
            .reduce_max();
        Some(
            chunks
                .remainder()
                .iter()
                .filter_map(|value| *value)
                .fold(maximum, u32::max),
        )
    }

    fn load_nullable(values: &[Option<u32>]) -> Simd<u32, LANES> {
        Simd::from_array(std::array::from_fn(|lane| values[lane].unwrap_or(0)))
    }
}

fn nullable_maximum(criterion: &mut Criterion) {
    assert_eq!(size_of::<u32>(), 4);
    assert_eq!(size_of::<Option<u32>>(), 8);

    let required: Vec<u32> = (0..u32::try_from(ROWS).unwrap()).collect();
    let nullable: Vec<Option<u32>> = required
        .iter()
        .enumerate()
        .map(|(row, &value)| (row % NULL_INTERVAL != 0).then_some(value))
        .collect();
    let expected = Some(u32::try_from(ROWS - 1).unwrap());
    assert_eq!(maximum_required_stable(&required), expected);
    assert_eq!(maximum_nullable_stable(&nullable), expected);

    let mut stable = criterion.benchmark_group("Table/NullableU32/10000000/Maximum/Stable");
    configure(&mut stable);
    stable.bench_function("required-Vec-u32", |bencher| {
        bencher.iter(|| black_box(maximum_required_stable(black_box(&required))));
    });
    stable.bench_function("nullable-Vec-Option-u32", |bencher| {
        bencher.iter(|| black_box(maximum_nullable_stable(black_box(&nullable))));
    });
    stable.finish();

    #[cfg(nightly_simd)]
    {
        assert_eq!(explicit_simd::maximum_required(&required), expected);
        assert_eq!(explicit_simd::maximum_nullable(&nullable), expected);

        let mut simd = criterion.benchmark_group("Table/NullableU32/10000000/Maximum/StdSimd");
        configure(&mut simd);
        simd.bench_function("required-Vec-u32", |bencher| {
            bencher.iter(|| black_box(explicit_simd::maximum_required(black_box(&required))));
        });
        simd.bench_function("nullable-Vec-Option-u32", |bencher| {
            bencher.iter(|| black_box(explicit_simd::maximum_nullable(black_box(&nullable))));
        });
        simd.finish();
    }
}

fn configure(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group
        .throughput(Throughput::Elements(ROWS as u64))
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(5));
}

criterion_group!(benches, nullable_maximum);
criterion_main!(benches);

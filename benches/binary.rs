//! Criterion benchmarks for hex, endian cursors, and byte search.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gd::{BinaryReader, BinaryWriter, Endian, decode_hex, encode_hex, find_bytes};

fn hex(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Binary/Hex");
    for size in [16_usize, 256, 4096, 65_536] {
        let bytes = vec![0xab; size];
        let encoded = encode_hex(&bytes);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("encode", size), &size, |bencher, _| {
            bencher.iter(|| black_box(encode_hex(black_box(&bytes))));
        });
        group.bench_with_input(BenchmarkId::new("decode", size), &size, |bencher, _| {
            bencher.iter(|| black_box(decode_hex(black_box(&encoded)).unwrap()));
        });
    }
    group.finish();
}

fn endian(criterion: &mut Criterion) {
    let values: Vec<u64> = (0..4096).collect();
    let mut bytes = vec![0_u8; values.len() * 8];
    criterion.bench_function("Binary/WriteU64BE/4096", |bencher| {
        bencher.iter(|| {
            let mut writer = BinaryWriter::new(black_box(&mut bytes));
            for value in &values {
                writer.write_u64(black_box(*value), Endian::Big).unwrap();
            }
            black_box(writer.position())
        });
    });
    {
        let mut writer = BinaryWriter::new(&mut bytes);
        for value in &values {
            writer.write_u64(*value, Endian::Big).unwrap();
        }
    }
    criterion.bench_function("Binary/ReadU64BE/4096", |bencher| {
        bencher.iter(|| {
            let mut reader = BinaryReader::new(black_box(&bytes));
            let mut sum = 0_u64;
            while !reader.is_eof() {
                sum = sum.wrapping_add(reader.read_u64(Endian::Big).unwrap());
            }
            black_box(sum)
        });
    });
}

fn search(criterion: &mut Criterion) {
    let mut bytes = vec![b'a'; 65_536];
    bytes.extend_from_slice(b"needle");
    criterion.bench_function("Binary/FindLast/65542", |bencher| {
        bencher.iter(|| black_box(find_bytes(black_box(&bytes), black_box(b"needle"), 0)));
    });
}

criterion_group!(benches, hex, endian, search);
criterion_main!(benches);

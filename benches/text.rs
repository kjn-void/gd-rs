//! Criterion benchmarks for JSON, URI, and XML text conversion.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use gd::{decode_percent_component, encode_json_string, encode_percent_component, escape_xml};

fn fixture(target_bytes: usize) -> String {
    let unit = "alpha & <beta> café 😀 / path? value=42\n";
    let text = unit.repeat(target_bytes.div_ceil(unit.len()));
    let mut end = target_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn text_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Text");
    for size in [64, 4_096, 65_536] {
        let text = fixture(size);
        let percent = encode_percent_component(&text);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("JsonEncode", size),
            &text,
            |bencher, text| {
                bencher.iter(|| encode_json_string(black_box(text)));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("UriEncode", size),
            &text,
            |bencher, text| {
                bencher.iter(|| encode_percent_component(black_box(text)));
            },
        );
        group.bench_with_input(
            BenchmarkId::new("UriDecode", size),
            &percent,
            |bencher, text| {
                bencher.iter(|| decode_percent_component(black_box(text)).unwrap());
            },
        );
        group.bench_with_input(
            BenchmarkId::new("XmlEscape", size),
            &text,
            |bencher, text| {
                bencher.iter(|| escape_xml(black_box(text)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, text_benchmarks);
criterion_main!(benches);

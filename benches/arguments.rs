//! Criterion benchmarks for ordered arguments and `ahash` name indexing.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gd::{Arguments, ValueRef};

fn make_arguments(count: usize) -> Arguments {
    let mut values = Arguments::with_capacity(count);
    for position in 0..count {
        values.push_named(format!("key-{position}"), position as u64);
    }
    values
}

fn make_uri_arguments() -> Arguments {
    let mut values = Arguments::with_capacity(11);
    values.push_named("scheme", "https");
    values.push_named("host", "example.com");
    values.push_named("port", 443_i32);
    values.push_named("path", "/api/users");
    values.push_named("query", "limit=10&offset=20");
    values.push_named("fragment", "section1");
    values.push_named("user", "admin");
    values.push_named("password", "secret123");
    values.push_named("secure", true);
    values.push_named("timeout", 5000_i32);
    values.push_named("retry_count", 3_i32);
    values
}

fn checksum(value: ValueRef<'_>) -> u64 {
    match value {
        ValueRef::String(value) => value.len() as u64,
        ValueRef::Bool(value) => u64::from(value),
        _ => u64::try_from(value.to_i64().unwrap()).unwrap(),
    }
}

fn scaling(criterion: &mut Criterion) {
    let mut append = criterion.benchmark_group("Arguments/AppendNamed");
    for count in [1_usize, 4, 16, 64, 256, 1024, 4096] {
        append.bench_with_input(
            BenchmarkId::from_parameter(count),
            &count,
            |bencher, &count| {
                bencher.iter(|| black_box(make_arguments(black_box(count))));
            },
        );
    }
    append.finish();

    let mut lookup = criterion.benchmark_group("Arguments/LookupNamed");
    for count in [1_usize, 4, 16, 64, 256, 1024, 4096] {
        let values = make_arguments(count);
        let key = format!("key-{}", count - 1);
        lookup.bench_with_input(
            BenchmarkId::new("linear-last", count),
            &count,
            |bencher, _| {
                bencher.iter(|| black_box(values.get_named(black_box(&key))));
            },
        );
        lookup.bench_with_input(
            BenchmarkId::new("linear-missing", count),
            &count,
            |bencher, _| {
                bencher.iter(|| black_box(values.get_named(black_box("missing"))));
            },
        );
        let index = values.index();
        lookup.bench_with_input(
            BenchmarkId::new("ahash-last", count),
            &count,
            |bencher, _| {
                bencher.iter(|| black_box(index.get_named(black_box(&key))));
            },
        );
        lookup.bench_with_input(
            BenchmarkId::new("ahash-missing", count),
            &count,
            |bencher, _| {
                bencher.iter(|| black_box(index.get_named(black_box("missing"))));
            },
        );
    }
    lookup.finish();
}

fn uri(criterion: &mut Criterion) {
    let values = make_uri_arguments();
    criterion.bench_function("Arguments/ReadUriByName", |bencher| {
        bencher.iter(|| {
            let mut total = 0;
            for name in [
                "scheme",
                "host",
                "port",
                "path",
                "query",
                "fragment",
                "user",
                "password",
                "secure",
                "timeout",
                "retry_count",
            ] {
                total += checksum(values.get_named(black_box(name)).unwrap().value_ref());
            }
            black_box(total)
        });
    });

    criterion.bench_function("Arguments/ReadUriByPosition", |bencher| {
        bencher.iter(|| {
            let total = values
                .iter()
                .map(|argument| checksum(argument.value_ref()))
                .sum::<u64>();
            black_box(total)
        });
    });

    let index = values.index();
    criterion.bench_function("Arguments/ReadUriByAHash", |bencher| {
        bencher.iter(|| {
            let mut total = 0;
            for name in [
                "scheme",
                "host",
                "port",
                "path",
                "query",
                "fragment",
                "user",
                "password",
                "secure",
                "timeout",
                "retry_count",
            ] {
                total += checksum(index.get_named(black_box(name)).unwrap().value_ref());
            }
            black_box(total)
        });
    });

    criterion.bench_function("Arguments/BuildUriAHashIndex", |bencher| {
        bencher.iter(|| black_box(values.index()));
    });
}

criterion_group!(benches, scaling, uri);
criterion_main!(benches);

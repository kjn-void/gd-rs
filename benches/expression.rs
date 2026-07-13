//! Criterion benchmarks for expression compilation and evaluation.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gd::{ExpressionContext, ExpressionEngine};

const FORMULAS: &[(&str, &str)] = &[
    ("short", "x + y * 2"),
    ("function", "abs(x - y) + max(x, y)"),
    ("logical", "x > y && x < 100"),
];

fn compile(criterion: &mut Criterion) {
    let engine = ExpressionEngine::new();
    let mut group = criterion.benchmark_group("Expression/Compile");
    for &(name, formula) in FORMULAS {
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            formula,
            |bencher, formula| {
                bencher.iter(|| engine.compile_expression(black_box(formula)).unwrap());
            },
        );
    }
    group.finish();
}

fn evaluate(criterion: &mut Criterion) {
    let engine = ExpressionEngine::new();
    let mut group = criterion.benchmark_group("Expression/Evaluate");
    for &(name, formula) in FORMULAS {
        let program = engine.compile_expression(formula).unwrap();
        let mut context = ExpressionContext::with_capacity(2);
        context.set("x", 10_i64).unwrap().set("y", 20_i64).unwrap();
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &program,
            |bencher, program| {
                bencher.iter(|| {
                    engine
                        .evaluate(black_box(program), black_box(&mut context))
                        .unwrap()
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, compile, evaluate);
criterion_main!(benches);

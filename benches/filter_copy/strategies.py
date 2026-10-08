#!/usr/bin/env python3
"""Compare sharing pipelines with the whole-record driver's fixtures and timing."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import statistics

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'target/filter-copy/sharing-strategies'
spec = importlib.util.spec_from_file_location('filter_copy', ROOT / 'benches/filter_copy/compare.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
STRATEGIES = ['baseline', 'fused', 'fold', 'chunks', 'rc']

# Rc owns the same fields, constructed without an intermediate Arc fixture.
RC_HELPERS = '''
fn rc_fixture(rows: usize, length: usize) -> Vec<std::rc::Rc<Record>> {
    (0..rows).map(|row| std::rc::Rc::new(Record {
        id: row as u64, selector: selector(row), amount: row as u64 * 13 + 7,
        name: text(row, length, "name").into(), message: text(row, length, "text").into(),
    })).collect()
}
fn rc_filter(source: &[std::rc::Rc<Record>], percent: u64, workers: usize, pool: &ThreadPool) -> Vec<std::rc::Rc<Record>> {
    if workers == 1 {
        return source.iter().filter(|record| record.selector < percent)
            .map(std::rc::Rc::clone).collect();
    }
    shared_strategies::caller_rc(source, |record| record.selector < percent, pool)
}
fn rc_digest(table: &[std::rc::Rc<Record>]) -> (usize, u64) {
    let mut hash = 14_695_981_039_346_656_037;
    for record in table {
        for number in [record.id, record.selector, record.amount] {
            hash = hash_bytes(hash, &number.to_le_bytes());
        }
        for value in [&record.name, &record.message] {
            hash = hash_bytes(hash_bytes(hash, &(value.len() as u64).to_le_bytes()), value.as_bytes());
        }
    }
    (table.len(), hash)
}
'''


def build():
    package = OUTPUT / 'source'
    (package / 'src/bin').mkdir(parents=True, exist_ok=True)
    package.joinpath('Cargo.toml').write_text('''[package]
name = "filter-copy-sharing-strategies"
version = "0.0.0"
edition = "2024"

[dependencies]
gd = { package = "gd-rs", path = ''' + json.dumps(str(ROOT)) + ''', default-features = false, features = ["rayon"] }
compact_str = "0.9.1"
rayon = "1.12.0"
serde_json = "1.0.150"

[profile.release]
codegen-units = 1
lto = "thin"
''')
    shutil.copyfile(ROOT / 'Cargo.lock', package / 'Cargo.lock')
    original = (ROOT / 'benches/filter_copy/driver.rs').read_text()
    module = '#[path = ' + json.dumps(str(ROOT / 'benches/filter_copy/shared_strategies.rs')) + ']\nmod shared_strategies;\n'
    marker = 'pool.install(|| source.par_filter(|record| record.selector < percent))'
    if original.count(marker) != 1:
        raise RuntimeError('driver layout changed; review generated strategy drivers')
    for name, method in [('baseline', 'two_pass'), ('fused', 'fused'), ('fold', 'fold_reduce'), ('chunks', 'chunk_concat')]:
        source = original.replace(marker, f'pool.install(|| shared_strategies::{method}(source, |record| record.selector < percent))')
        package.joinpath(f'src/bin/{name}.rs').write_text(source + '\n' + module)
    start, end = original.index('fn run_shared('), original.index('\nfn main()')
    body = original[start:end].replace('shared_fixture(', 'rc_fixture(').replace('shared_filter(', 'rc_filter(').replace('shared_digest(', 'rc_digest(').replace('Arc::ptr_eq(', 'std::rc::Rc::ptr_eq(')
    package.joinpath('src/bin/rc.rs').write_text(original[:start] + RC_HELPERS + body + original[end:] + '\n' + module)
    env = os.environ.copy()
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    env['RUSTFLAGS'] = '-C target-cpu=native'
    with (OUTPUT / 'build.log').open('w') as log:
        driver.helper.run(['cargo', 'build', '--offline', '--release', '--bins', '--manifest-path', package / 'Cargo.toml',
                           '--target-dir', OUTPUT / 'build'], env=env, stdout=log, stderr=driver.helper.subprocess.STDOUT)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--allow-contended', action='store_true')
    parser.add_argument('--output', type=Path, default=OUTPUT / 'results.json')
    parser.add_argument('--oracle', type=Path, default=ROOT / 'target/filter-copy/oracle.json')
    args = parser.parse_args()
    if not args.skip_build:
        print(f'Building isolated strategy drivers; {OUTPUT / "build.log"}', flush=True)
        build()
    binaries = {name: OUTPUT / 'build/release' / name for name in STRATEGIES}
    source_paths = sorted([*ROOT.glob('src/**/*.rs'), ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
                           ROOT / 'benches/filter_copy/driver.rs', ROOT / 'benches/filter_copy/compare.py',
                           ROOT / 'benches/filter_copy/shared_strategies.rs', Path(__file__), ROOT / 'benches/text_workflow/compare.py'])
    sources = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths}
    hashes = {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in binaries.items()}
    expected = json.loads(args.oracle.read_text())
    if expected['rows'] != 1_000_000:
        raise RuntimeError('expected million-row oracle')
    report = {'metadata': {'utc': datetime.now(timezone.utc).isoformat(), 'host': platform.node(),
                          'rustc': driver.helper.text(['rustc', '-Vv']), 'rows': 1_000_000, 'workers': 8,
                          'text_bytes': [16, 128], 'selectivity': [10, 50, 90], 'rounds': 5, 'samples': 7, 'sample_ms': 50,
                          'process_niceness': os.nice(0), 'affinity': 'OS scheduling, persistent eight-thread Rayon pools',
                          'rust_flags': 'release -O3; thin LTO; codegen-units=1; target-cpu=native; no-default-features; rayon; root lockfile; offline build',
                          'source_sha256': sources, 'binary_sha256': hashes,
                          'oracle_sha256': hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
                          'contended_diagnostics': args.allow_contended, 'measurement_complete': False,
                          'ownership': 'all strategies share Record payloads; Rc handles stay on the caller; temporary buffers, concatenation and caller cleanup are timed; source construction and verification are excluded'},
              'verification': [], 'edge_verification': [], 'measurements': [], 'load_samples': []}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for rows in [0, 1, 31, 1001, 1_000_000]:
        for length in [16, 128]:
            percentages = [10, 50, 90] if rows == 1_000_000 else [0, 10, 50, 90, 100]
            oracle = expected['lengths'][str(length)] if rows == 1_000_000 else driver.oracle(rows, length, percentages)
            for percent in percentages:
                for name in STRATEGIES:
                    result = driver.helper.invoke(binaries[name], 'arc', rows, length, 8, percent, 1, 1, True)
                    if not driver.ownership_passes(result, 'arc') or any(result[k] != v for k, v in oracle[str(percent)].items()):
                        raise RuntimeError('strategy ownership/oracle mismatch')
                    report['verification' if rows == 1_000_000 else 'edge_verification'].append(
                        {'strategy': name, 'rows': rows, 'text_bytes': length, 'selectivity': percent, **result})
    print('Strategy boundary, ordering and source-drop checks passed', flush=True)
    for round_index in range(5):
        driver.helper.load_guard(report, args.allow_contended)
        order = STRATEGIES[round_index:] + STRATEGIES[:round_index]
        for length in [16, 128]:
            for percent in [10, 50, 90]:
                for name in order:
                    result = driver.helper.invoke(binaries[name], 'arc', 1_000_000, length, 8, percent, 7, 50, False)
                    if any(result[k] != v for k, v in expected['lengths'][str(length)][str(percent)].items()):
                        raise RuntimeError('timed strategy digest mismatch')
                    report['measurements'].append({'strategy': name, 'text_bytes': length,
                                                   'selectivity': percent, 'round': round_index + 1, **result})
        driver.helper.load_guard(report, args.allow_contended)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(f'Strategy round {round_index + 1}/5 complete', flush=True)
    summary = []
    for length in [16, 128]:
        for percent in [10, 50, 90]:
            for name in STRATEGIES:
                records = [r for r in report['measurements'] if (r['text_bytes'], r['selectivity'], r['strategy']) == (length, percent, name)]
                medians = [statistics.median(r['samples_ns']) for r in records]
                summary.append({'text_bytes': length, 'selectivity': percent, 'strategy': name,
                                'median_ns': statistics.median(medians), 'min_round_median_ns': min(medians), 'max_round_median_ns': max(medians)})
    report['summary'] = summary
    report['metadata']['source_and_binaries_unchanged'] = all(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest for name, digest in sources.items()) \
        and all(hashlib.sha256(binaries[name].read_bytes()).hexdigest() == digest for name, digest in hashes.items())
    report['metadata']['measurement_complete'] = True
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    if not report['metadata']['source_and_binaries_unchanged']:
        raise RuntimeError('strategy sources or executables changed')
    print(f'Wrote {args.output}', flush=True)


if __name__ == '__main__':
    main()

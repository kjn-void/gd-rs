#!/usr/bin/env python3
"""Verify and time whole-record filtering into a single destination."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import statistics
import sys

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'target/filter-copy'
spec = importlib.util.spec_from_file_location('text_helpers', ROOT / 'benches/text_workflow/compare.py')
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
IMPLEMENTATIONS = ['gd', 'std', 'compact', 'fixed', 'arc']


def ownership_passes(result, implementation):
    if implementation.startswith('arc'):
        return not result['independent_target'] and result.get('shared_records') and result.get('source_drop_checked')
    return result['independent_target']


def build():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with (OUTPUT / 'build.log').open('w') as log:
        helper.run(['cmake', '-S', ROOT / 'benches/cpp-reference', '-B', OUTPUT / 'cpp',
                    '-DCMAKE_BUILD_TYPE=Release', '-DGD_ENABLE_SANITIZERS=OFF',
                    '-DCMAKE_CXX_FLAGS=-march=native', '-DCMAKE_C_FLAGS=-march=native',
                    '-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON'], stdout=log, stderr=helper.subprocess.STDOUT)
        helper.run(['cmake', '--build', OUTPUT / 'cpp', '--target', 'gd_filter_copy', '-j', '4'],
                   stdout=log, stderr=helper.subprocess.STDOUT)
        env = os.environ.copy()
        env.pop('CARGO_ENCODED_RUSTFLAGS', None)
        env['RUSTFLAGS'] = '-C target-cpu=native'
        helper.run(['cargo', 'build', '--release', '--locked', '--example', 'filter_copy',
                    '--no-default-features', '--features', 'rayon', '--manifest-path', ROOT / 'Cargo.toml'],
                   env=env, stdout=log, stderr=helper.subprocess.STDOUT)


def oracle(rows, length, percentages):
    hashes = {p: 14_695_981_039_346_656_037 for p in percentages}
    counts = {p: 0 for p in percentages}
    for row in range(rows):
        score = (row % 100 * 37 + row // 100 * 17) % 100
        record = bytearray(row.to_bytes(8, 'little'))
        record += score.to_bytes(8, 'little') + (row * 13 + 7).to_bytes(8, 'little')
        for label in ['name', 'text']:
            value = (f'{row % 100_000_000:08}-{label}-').encode().ljust(length, b'x')
            record += len(value).to_bytes(8, 'little') + value
        for percent in percentages:
            if score < percent:
                counts[percent] += 1
                hashes[percent] = helper.hash_bytes(hashes[percent], record)
    return {str(p): {'count': counts[p], 'digest': str(hashes[p])} for p in percentages}


def summaries(report):
    output = []
    for key in sorted({(r['text_bytes'], r['workers'], r['selectivity']) for r in report['measurements']}):
        for implementation in report['metadata'].get('implementations', IMPLEMENTATIONS):
            records = [r for r in report['measurements'] if
                       (r['text_bytes'], r['workers'], r['selectivity'], r['implementation']) == (*key, implementation)]
            medians = [statistics.median(r['samples_ns']) for r in records]
            output.append({'rows': report['metadata']['rows'], 'text_bytes': key[0],
                           'workers': key[1], 'selectivity': key[2], 'implementation': implementation,
                           'median_ns': statistics.median(medians), 'min_round_median_ns': min(medians),
                           'max_round_median_ns': max(medians),
                           'peak_rss_bytes': max(r.get('peak_rss_bytes', 0) for r in records)})
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rows', type=int, default=1_000_000)
    parser.add_argument('--text-bytes', type=int, nargs='+', default=[16, 128])
    parser.add_argument('--workers', type=int, nargs='+', default=[1, 8])
    parser.add_argument('--selectivity', type=int, nargs='+', default=[10, 50, 90])
    parser.add_argument('--samples', type=int, default=7)
    parser.add_argument('--rounds', type=int)
    parser.add_argument('--arc-parallel', choices=['arc_chunks', 'arc_chunks4'])
    parser.add_argument('--case', type=int, nargs=3, action='append', metavar=('BYTES', 'WORKERS', 'PERCENT'))
    parser.add_argument('--sample-ms', type=int, default=50)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--build-only', action='store_true')
    parser.add_argument('--prepare-oracle', action='store_true')
    parser.add_argument('--allow-contended', action='store_true')
    parser.add_argument('--oracle', type=Path, default=OUTPUT / 'oracle.json')
    parser.add_argument('--output', type=Path, default=OUTPUT / 'results.json')
    args = parser.parse_args()
    implementations = [*IMPLEMENTATIONS] + ([args.arc_parallel] if args.arc_parallel else [])
    if args.rounds is None:
        args.rounds = len(implementations)
    if not 0 <= args.rows <= 100_000_000 or min(args.text_bytes) < 16 or max(args.text_bytes) > 4096 \
            or min(args.workers) < 1 or max(args.workers) > (os.cpu_count() or 1) \
            or min(args.selectivity) < 0 or max(args.selectivity) > 100 \
            or args.rounds < len(implementations) or args.rounds % len(implementations) or args.samples < 1 or args.sample_ms < 1:
        parser.error('invalid configuration; rounds must be a positive multiple of the number of implementations')
    if args.prepare_oracle:
        expected = {'rows': args.rows, 'lengths': {str(n): oracle(args.rows, n, args.selectivity) for n in args.text_bytes}}
        args.oracle.parent.mkdir(parents=True, exist_ok=True)
        args.oracle.write_text(json.dumps(expected, indent=2) + '\n')
        print(f'Wrote independent oracle {args.oracle}', flush=True)
        return
    if not args.skip_build:
        print(f'Building optimized applications; {OUTPUT / "build.log"}', flush=True)
        build()
    if args.build_only:
        return
    expected = json.loads(args.oracle.read_text())
    if expected['rows'] != args.rows:
        raise RuntimeError('oracle row count differs')
    binaries = {'gd': OUTPUT / 'cpp/gd_filter_copy', 'std': OUTPUT / 'cpp/gd_filter_copy',
                'compact': ROOT / 'target/release/examples/filter_copy',
                'fixed': ROOT / 'target/release/examples/filter_copy',
                'arc': ROOT / 'target/release/examples/filter_copy'}
    if args.arc_parallel:
        binaries[args.arc_parallel] = ROOT / 'target/release/examples/filter_copy'
    source_paths = sorted([*ROOT.glob('src/**/*.rs'), ROOT / 'benches/filter_copy/driver.rs',
                           ROOT / 'benches/filter_copy/compare.py', ROOT / 'benches/text_workflow/compare.py',
                           ROOT / 'benches/cpp-reference/filter_copy.cpp', ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
                           ROOT / 'benches/cpp-reference/CMakeLists.txt', ROOT / 'benches/cpp-reference/cmake/GdCore.cmake'])
    gd = ROOT / 'external/gd'
    report = {'metadata': {
        'utc': datetime.now(timezone.utc).isoformat(), 'host': platform.node(), 'platform': platform.platform(),
        'cpu': helper.text(['sysctl', '-n', 'machdep.cpu.brand_string']) if sys.platform == 'darwin' else helper.text(['lscpu']),
        'logical_cpus': os.cpu_count(), 'rustc': helper.text(['rustc', '-Vv']), 'cxx': helper.text(['c++', '--version']),
        'process_niceness': os.nice(0),
        'gd_revision': helper.text(['git', '-C', gd, 'rev-parse', 'HEAD']),
        'gd_rs_revision': helper.text(['git', '-C', ROOT, 'rev-parse', 'HEAD']),
        'gd_source_sha256': helper.fingerprint(gd / 'source'),
        'source_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths},
        'binary_sha256': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in binaries.items()},
        'oracle_sha256': hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
        'rows': args.rows, 'text_bytes': args.text_bytes, 'workers': args.workers, 'selectivity': args.selectivity,
        'samples': args.samples, 'rounds': args.rounds, 'sample_ms': args.sample_ms,
        'implementations': implementations, 'arc_parallel': args.arc_parallel, 'selected_cases': args.case,
        'rust_flags': 'release -O3; codegen-units=1; lto=thin; target-cpu=native; no-default-features; rayon; locked',
        'cpp_flags': 'Release -O3 -DNDEBUG -march=native; IPO ON; sanitizers OFF',
        'affinity': 'OS scheduling, no affinity; persistent exact-size pools; one benchmark process at a time',
        'parallel_strategy': 'C++: two joined dispatches, filter static row ranges then copy disjoint destination ranges; Rust native: filter ranges then column-parallel gather over five columns; Arc: SharedRecordTable::par_filter fuses row filtering and Arc cloning in Rayon local buffers and concatenates them in source order into one destination vector; optional arc_chunks/arc_chunks4 use par_filter_chunked with one/four chunks per worker and joined par_drop cleanup',
        'schema': 'five fields: id u64, selector u64, amount u64, name string, message string; both strings exactly text_bytes ASCII bytes; Arc stores the fields in Record structs in one Arc<Record> column',
        'ownership': {'gd': 'independent values', 'std': 'independent values', 'compact': 'independent values',
                      'fixed': 'independent values', 'arc': 'shared immutable Record payloads; target owns Arc handles; edits use copy-on-write'},
        'predicate': 'selector < percentage; selector = (row%100*37 + row//100*17)%100; exact density per 100 rows',
        'timing_contract': 'source construction, pool startup, oracle, full digest outside samples; fresh target allocation, filtering, temporary indices where used, prefix/gather, synchronization, copying and destruction inside; one ordered target; no worker output tables; Arc shares records and times reference increments/decrements without copying string payloads or freeing records because source remains alive',
        'contended_diagnostics': args.allow_contended, 'invocation': sys.argv, 'measurement_complete': False,
    }, 'load_samples': [], 'verification': [], 'edge_verification': [], 'measurements': []}
    if args.arc_parallel:
        report['metadata']['ownership'][args.arc_parallel] = 'shared immutable records; same Arc ownership as arc; chunked filtering and joined parallel handle release'
    if sys.platform == 'darwin':
        levels = int(helper.text(['sysctl', '-n', 'hw.nperflevels']))
        keys = ['hw.physicalcpu', 'hw.logicalcpu', 'hw.nperflevels']
        keys += [f'hw.perflevel{i}.{field}' for i in range(levels) for field in ['name', 'physicalcpu']]
        report['metadata']['cpu_topology'] = helper.text(['sysctl', *keys])
        report['metadata']['ram_bytes'] = int(helper.text(['sysctl', '-n', 'hw.memsize']))
        report['metadata']['power_settings'] = helper.text(['pmset', '-g', 'custom'])
        report['metadata']['thermal_before'] = helper.text(['pmset', '-g', 'therm'])
    else:
        report['metadata']['memory'] = helper.text(['free', '-b'])
        model = Path('/proc/device-tree/model')
        if model.exists():
            report['metadata']['board'] = model.read_bytes().rstrip(b'\0').decode()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        for rows in [0, 1, 31, 1001]:
            for length in args.text_bytes:
                edge = oracle(rows, length, [0, 10, 50, 90, 100])
                for workers in args.workers:
                    for percent in [0, 10, 50, 90, 100]:
                        for implementation in implementations:
                            result = helper.invoke(binaries[implementation], implementation, rows, length, workers, percent, 1, 1, True)
                            if any(result[k] != v for k, v in edge[str(percent)].items()) or not ownership_passes(result, implementation):
                                raise RuntimeError('edge/ownership verification failed')
                            report['edge_verification'].append({'implementation': implementation, 'rows': rows,
                                'text_bytes': length, 'workers': workers, 'selectivity': percent, **result})
        for length in args.text_bytes:
            print(f'Checking {args.rows:,} records; two {length}-byte strings', flush=True)
            for workers in args.workers:
                for percent in args.selectivity:
                    if args.case and [length, workers, percent] not in args.case:
                        continue
                    oracle_result = expected['lengths'][str(length)][str(percent)]
                    for implementation in implementations:
                        result = helper.invoke(binaries[implementation], implementation, args.rows, length, workers, percent, 1, 1, True)
                        if any(result[k] != v for k, v in oracle_result.items()) or not ownership_passes(result, implementation):
                            raise RuntimeError('full/ownership verification failed')
                        report['verification'].append({'implementation': implementation, 'rows': args.rows,
                            'text_bytes': length, 'workers': workers, 'selectivity': percent, **result})
            for round_index in range(args.rounds):
                helper.load_guard(report, args.allow_contended)
                order = implementations[round_index % len(implementations):] + implementations[:round_index % len(implementations)]
                for workers in args.workers:
                    for percent in args.selectivity:
                        if args.case and [length, workers, percent] not in args.case:
                            continue
                        for implementation in order:
                            result = helper.invoke(binaries[implementation], implementation, args.rows, length, workers, percent, args.samples, args.sample_ms, False)
                            if any(result[k] != v for k, v in expected['lengths'][str(length)][str(percent)].items()):
                                raise RuntimeError('timed process digest differs from oracle')
                            report['measurements'].append({'implementation': implementation, 'rows': args.rows,
                                'text_bytes': length, 'workers': workers, 'selectivity': percent,
                                'round': round_index + 1, **result})
                helper.load_guard(report, args.allow_contended)
                args.output.write_text(json.dumps(report, indent=2) + '\n')
                print(f'  {length} bytes: round {round_index + 1}/{args.rounds} complete', flush=True)
        report['summary'] = summaries(report)
        report['metadata']['measurement_complete'] = True
    finally:
        report['metadata']['gd_source_unchanged'] = helper.fingerprint(gd / 'source') == report['metadata']['gd_source_sha256']
        report['metadata']['source_unchanged'] = all(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest for name, digest in report['metadata']['source_sha256'].items())
        report['metadata']['binaries_unchanged'] = all(hashlib.sha256(binaries[name].read_bytes()).hexdigest() == digest for name, digest in report['metadata']['binary_sha256'].items())
        if sys.platform == 'darwin':
            report['metadata']['thermal_after'] = helper.text(['pmset', '-g', 'therm'])
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    if not all(report['metadata'][k] for k in ['gd_source_unchanged', 'source_unchanged', 'binaries_unchanged']):
        raise RuntimeError('source or executable changed during measurements')
    print(f'Wrote {args.output}', flush=True)


if __name__ == '__main__':
    main()

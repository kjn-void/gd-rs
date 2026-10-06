#!/usr/bin/env python3
"""Build, verify, and rotate Rust, GD DTO, and GD SIMD order-workflow applications."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys

import fixture

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'target/order-workflow'


def command(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, text=True, **kwargs)


def text(args):
    return command(args, capture_output=True).stdout.strip()


def source_fingerprint(gd):
    # Fingerprint the reference source and any optional upstream build descriptions.
    files = [*sorted((gd / 'source').rglob('*')), gd / 'CMakeLists.txt', gd / 'CMakePresets.json']
    digest = hashlib.sha256()
    for file in files:
        if file.is_file():
            digest.update(str(file.relative_to(gd)).encode())
            digest.update(file.read_bytes())
    return digest.hexdigest()


def build(gd):
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with (OUTPUT / 'compare-build.log').open('w') as log:
        command(['cmake', '-S', ROOT / 'benches/cpp-reference', '-B', OUTPUT / 'cpp',
                 '-DCMAKE_BUILD_TYPE=Release', f'-DGD_SOURCE_DIR={gd}',
                 '-DGD_ENABLE_SANITIZERS=OFF', '-DCMAKE_CXX_FLAGS=-march=native',
                 '-DCMAKE_C_FLAGS=-march=native', '-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON'],
                stdout=log, stderr=subprocess.STDOUT)
        command(['cmake', '--build', OUTPUT / 'cpp', '--target', 'gd_order_workflow',
                 'gd_order_workflow_simd', '-j', '4'],
                stdout=log, stderr=subprocess.STDOUT)
        env = os.environ.copy()
        env.pop('CARGO_ENCODED_RUSTFLAGS', None)
        env.update(RUSTFLAGS='-C target-cpu=native', CFLAGS='-march=native')
        command(['cargo', 'build', '--release', '--example', 'order_workflow', '--features', 'rayon',
                 '--locked', '--manifest-path', ROOT / 'Cargo.toml'], env=env, stdout=log, stderr=subprocess.STDOUT)


def invoke(binary, db, workers, stage, samples, index):
    timing = ['/usr/bin/time', '-l'] if sys.platform == 'darwin' else ['/usr/bin/time', '-v']
    args = [binary, db, str(workers), stage, str(samples), index]
    completed = command([*timing, *args], capture_output=True)
    result = json.loads(completed.stdout)
    if sys.platform == 'darwin':
        match = re.search(r'(\d+)\s+maximum resident set size', completed.stderr)
        multiplier = 1
    else:
        match = re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)', completed.stderr)
        multiplier = 1024
    if not match:
        raise RuntimeError(f'Could not read peak RSS: {completed.stderr}')
    result['peak_rss_bytes'] = int(match[1]) * multiplier
    result['command'] = [str(a) for a in args]
    return result


def sizes(binaries):
    result = {}
    files = {
        'rust_application': ['benches/order_workflow/workload.rs'],
        'rust_selection_library': ['src/table/selection.rs'],
        'rust_driver': ['benches/order_workflow/driver.rs'],
        'cpp_application_and_adapters': ['benches/cpp-reference/order_workflow/workload.hpp'],
        'cpp_simd_adapter': ['benches/cpp-reference/order_workflow/simd_table.hpp'],
        'cpp_driver_and_pool': ['benches/cpp-reference/order_workflow/driver.cpp',
                                'benches/cpp-reference/order_workflow/pool.hpp'],
        'shared_fixture_and_runner': ['benches/order_workflow/fixture.py', 'benches/order_workflow/compare.py'],
        'rust_api_tests': ['tests/table_selection.rs'],
    }
    for key, paths in files.items():
        content = '\n'.join((ROOT / path).read_text() for path in paths)
        result[key] = {'files': paths, 'lines': len(content.splitlines()),
                       'nonblank_lines': sum(bool(line.strip()) for line in content.splitlines()),
                       'bytes': sum((ROOT / path).stat().st_size for path in paths)}
    for language, binary in binaries.items():
        stripped = OUTPUT / (language + '-stripped')
        shutil.copy2(binary, stripped)
        command(['strip', stripped], capture_output=True)
        linked = text(['otool', '-L', stripped]) if sys.platform == 'darwin' else text(['ldd', stripped])
        result[language + '_executable'] = {'unstripped_bytes': binary.stat().st_size,
                                            'stripped_bytes': stripped.stat().st_size, 'dynamic_libraries': linked}
    return result


def check_rejections(binaries, hand, folder):
    """Exercise realistic extensions beyond the generated numeric/parameter bounds."""
    cases = [
        ('gross_overflow', 'UPDATE lines SET quantity=9223372036854775807,unit_price=2 WHERE id=8', 'prepare'),
        ('discount_overflow', 'UPDATE lines SET quantity=1,unit_price=922337203685478 WHERE id=8', 'variants'),
        ('invalid_discount', 'UPDATE parameters SET discount_bp=10001 WHERE id=0', 'variants'),
    ]
    results = []
    for name, sql, stage in cases:
        path = folder / (name + '.sqlite')
        shutil.copy2(hand, path)
        with fixture.sqlite3.connect(path) as db:
            db.execute(sql)
        for language, binary in binaries.items():
            completed = subprocess.run([str(binary), str(path), '2', stage, '1', 'native'],
                                       text=True, capture_output=True)
            if completed.returncode == 0:
                raise RuntimeError(f'{language} accepted {name}')
            results.append({'implementation': language, 'case': name,
                            'returncode': completed.returncode, 'stderr': completed.stderr})
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gd', type=Path, default=ROOT / 'external/gd')
    parser.add_argument('--rows', type=int, nargs='+', default=[10000, 100000, 1000000])
    parser.add_argument('--workers', type=int, nargs='+', default=[n for n in [1, 2, 4, 8] if n <= (os.cpu_count() or 1)])
    parser.add_argument('--samples', type=int, default=5)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--output', type=Path, default=OUTPUT / 'results.json')
    args = parser.parse_args()
    if min(args.rows) < 20 or max(args.rows) > 100_000_000 or min(args.workers) < 1 or max(args.workers) > (os.cpu_count() or 1):
        parser.error('invalid row count or workers exceeding logical CPUs')
    if args.samples < 1 or args.rounds < 2:
        parser.error('at least one sample and two process rounds required (three recommended)')
    gd = args.gd.resolve()
    before = source_fingerprint(gd)
    if not args.skip_build:
        print('Building optimized standalone applications...', flush=True)
        build(gd)
    binaries = {'rust': ROOT / 'target/release/examples/order_workflow',
                'cpp': OUTPUT / 'cpp/gd_order_workflow',
                'cpp_simd': OUTPUT / 'cpp/gd_order_workflow_simd'}
    report = {'metadata': {
        'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
        'logical_cpus': os.cpu_count(), 'cpu': text(['sysctl', '-n', 'machdep.cpu.brand_string']) if sys.platform == 'darwin' else platform.processor(),
        'cpu_topology': text(['sysctl', 'hw.physicalcpu', 'hw.logicalcpu', 'hw.perflevel0.physicalcpu', 'hw.perflevel1.physicalcpu']) if sys.platform == 'darwin' else text(['lscpu']),
        'rustc': text(['rustc', '-Vv']), 'cxx': text(['c++', '--version']),
        'gd_revision': text(['git', '-C', gd, 'rev-parse', 'HEAD']),
        'gd_rs_revision': text(['git', '-C', ROOT, 'rev-parse', 'HEAD']),
        'gd_source_sha256': before, 'seed': fixture.SEED,
        'rust_flags': 'release: opt-level=3, codegen-units=1, lto=thin; target-cpu=native; features sqlite,rayon; CFLAGS=-march=native',
        'cpp_flags': 'Release: -O3 -DNDEBUG -march=native; CMAKE_INTERPROCEDURAL_OPTIMIZATION=ON; sanitizers OFF',
        'affinity': 'OS scheduling, no affinity; persistent pools; one process at a time',
        'invocation': sys.argv, 'fixture_sqlite': fixture.sqlite3.sqlite_version,
        'samples': args.samples, 'rounds': args.rounds,
        'implementations': {'rust': 'gd::Table', 'cpp': 'gd::table::table_column_buffer',
                            'cpp_simd': 'gd::table::simd::table_8_8 with benchmark adapter'},
        'process_order': 'rotate starting implementation each round; reverse every three rounds',
        'simd_adapter': 'unmodified gd_table_simd.cpp; syntax-placeholder-only generated header; '
                        'packed null-bitmap column; packed cell/reference getters; owned pointer/schema; '
                        'geometric import reservation; application projected gather and packed filtering',
    }, 'verification': [], 'measurements': []}
    sqlite_version = None
    OUTPUT.mkdir(parents=True, exist_ok=True)
    # Regenerate under a unique directory: never silently reuse a stale fixture.
    import tempfile
    with tempfile.TemporaryDirectory(prefix='fixtures-', dir=OUTPUT) as temporary:
        folder = Path(temporary)
        hand = folder / 'hand.sqlite'
        fixture.generate(hand, small=True)
        report['rejection_checks'] = check_rejections(binaries, hand, folder)
        datasets = [(11, hand)]
        for rows in args.rows:
            db = folder / f'{rows}.sqlite'
            fixture.generate(db, rows)
            datasets.append((rows, db))
        for rows, db in datasets:
            baseline = None
            for workers in args.workers:
                for language, binary in binaries.items():
                    result = invoke(binary, db, workers, 'verify', 1, 'native')
                    if sqlite_version is None:
                        sqlite_version = result['sqlite']
                    if result['sqlite'] != sqlite_version:
                        raise RuntimeError(f'SQLite versions differ: {sqlite_version} vs {result["sqlite"]}')
                    report['metadata']['sqlite_version'] = sqlite_version
                    key = (result['counts'], result['digests'])
                    if baseline is None:
                        baseline = key
                    if key != baseline:
                        raise RuntimeError(f'Cross-language/worker mismatch at {rows}, {language}, {workers}')
                    result['rows'] = rows
                    report['verification'].append(result)
            sorted_check = invoke(binaries['rust'], db, 1, 'verify', 1, 'sorted')
            if (sorted_check['counts'], sorted_check['digests']) != baseline:
                raise RuntimeError('Sorted-index diagnostic mismatch')
            sorted_check['rows'] = rows
            report['verification'].append(sorted_check)
            print(f'Verified every cell: {rows} lines, three implementations, all worker counts and both index algorithms', flush=True)
            if db == hand:
                continue
            cases = [('import', 1, 'native'), ('prepare', 1, 'native'), ('prepare', 1, 'sorted')]
            cases += [(stage, workers, 'native') for stage in ['variants', 'complete'] for workers in args.workers]
            for stage, workers, index in cases:
                for round_id in range(args.rounds):
                    order = list(binaries.items())
                    if (round_id // len(order)) % 2:
                        order.reverse()
                    offset = round_id % len(order)
                    order = order[offset:] + order[:offset]
                    for language, binary in order:
                        result = invoke(binary, db, workers, stage, args.samples, index)
                        result['round'] = round_id
                        report['measurements'].append(result)
                        median = statistics.median(result['seconds']) * 1000
                        print(f'{rows:>7} {stage:8} w={workers} {index:6} {language:4} {median:9.3f} ms', flush=True)
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(report, indent=2) + '\n')
    report['sizes'] = sizes(binaries)
    source_paths = [*sorted((ROOT / 'benches/order_workflow').glob('*.rs')),
                    *sorted((ROOT / 'benches/order_workflow').glob('*.py')),
                    *sorted((ROOT / 'benches/cpp-reference/order_workflow').glob('*.cpp')),
                    *sorted((ROOT / 'benches/cpp-reference/order_workflow').glob('*.hpp')),
                    ROOT / 'src/table/selection.rs', ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
                    ROOT / 'benches/cpp-reference/CMakeLists.txt',
                    ROOT / 'benches/cpp-reference/cmake/GdCore.cmake']
    report['metadata']['source_sha256'] = {
        str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths}
    report['metadata']['gd_source_unchanged'] = source_fingerprint(gd) == before
    if not report['metadata']['gd_source_unchanged']:
        raise RuntimeError('GD source changed during comparison')
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Results: {args.output}', flush=True)


if __name__ == '__main__':
    main()

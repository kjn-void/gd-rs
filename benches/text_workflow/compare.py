#!/usr/bin/env python3
"""Build, verify against an independent oracle, and compare four text table layouts."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'target/text-workflow'
IMPLEMENTATIONS = ['gd', 'std', 'compact', 'fixed']
OPERATIONS = ['filter', 'transform', 'pipeline']


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, text=True, **kwargs)


def text(args):
    return run(args, capture_output=True).stdout.strip()


def fingerprint(folder):
    digest = hashlib.sha256()
    for path in sorted(folder.rglob('*')):
        if path.is_file():
            digest.update(str(path.relative_to(folder)).encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()


def build():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with (OUTPUT / 'build.log').open('w') as log:
        run(['cmake', '-S', ROOT / 'benches/cpp-reference', '-B', OUTPUT / 'cpp',
             '-DCMAKE_BUILD_TYPE=Release', '-DGD_ENABLE_SANITIZERS=OFF',
             '-DCMAKE_CXX_FLAGS=-march=native', '-DCMAKE_C_FLAGS=-march=native',
             '-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON'], stdout=log, stderr=subprocess.STDOUT)
        run(['cmake', '--build', OUTPUT / 'cpp', '--target', 'gd_text_workflow', '-j', '4'], stdout=log, stderr=subprocess.STDOUT)
        env = os.environ.copy()
        env.pop('CARGO_ENCODED_RUSTFLAGS', None)
        env['RUSTFLAGS'] = '-C target-cpu=native'
        run(['cargo', 'build', '--release', '--locked', '--example', 'text_workflow',
             '--no-default-features', '--features', 'rayon', '--manifest-path', ROOT / 'Cargo.toml'],
            env=env, stdout=log, stderr=subprocess.STDOUT)


def cpu_snapshot():
    # Process CPU time deltas avoid treating ps's lifetime %CPU as current load.
    result = {}
    for line in text(['ps', '-axo', 'pid=,time=,comm=']).splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) != 3:
            continue
        pid, elapsed, name = fields
        pieces = elapsed.split(':')
        try:
            seconds = sum(float(value) * (60 ** i) for i, value in enumerate(reversed(pieces)))
            result[int(pid)] = (seconds, name)
        except ValueError:
            pass
    return result


def load_sample():
    start = time.monotonic()
    before = cpu_snapshot()
    time.sleep(2)
    after = cpu_snapshot()
    elapsed = time.monotonic() - start
    processes = []
    for pid, (cpu, name) in after.items():
        if pid in before:
            percent = max(0, cpu - before[pid][0]) / elapsed * 100
            if percent > 5:
                processes.append({'pid': pid, 'cpu_percent': round(percent, 1), 'command': name})
    processes.sort(key=lambda p: p['cpu_percent'], reverse=True)
    return {'utc': datetime.now(timezone.utc).isoformat(), 'processes': processes,
            'aggregate_cpu_percent': round(sum(p['cpu_percent'] for p in processes), 1)}


def load_guard(report, allow_contended):
    sample = load_sample()
    report['load_samples'].append(sample)
    # Confirm elevated activity so a brief macOS maintenance burst does not
    # masquerade as sustained competing work. Retain both boundary samples.
    if sample['aggregate_cpu_percent'] > 100 and not allow_contended:
        confirmation = load_sample()
        report['load_samples'].append(confirmation)
        if confirmation['aggregate_cpu_percent'] > 100:
            raise RuntimeError('Host has sustained unrelated CPU load: ' + json.dumps(confirmation) +
                               '. Pause the jobs and rerun, or explicitly request contended diagnostics with --allow-contended.')


def wait_for_idle(report, allow_contended, wait):
    while True:
        begin = len(report['load_samples'])
        try:
            load_guard(report, allow_contended)
            return report['load_samples'][begin:]
        except RuntimeError:
            if not wait:
                raise
            print('  Background work active; waiting 30 seconds for a quiet period...', flush=True)
            time.sleep(30)


def invoke(binary, implementation, rows, length, workers, operation, samples, sample_ms, verify):
    args = [binary, implementation, rows, length, workers, operation, samples, sample_ms, 'verify' if verify else 'time']
    timing = ['/usr/bin/time', '-l'] if sys.platform == 'darwin' else ['/usr/bin/time', '-v']
    completed = run([*timing, *args], capture_output=True)
    result = json.loads(completed.stdout)
    pattern = r'(\d+)\s+maximum resident set size' if sys.platform == 'darwin' else r'Maximum resident set size \(kbytes\):\s*(\d+)'
    match = re.search(pattern, completed.stderr)
    if match:
        result['peak_rss_bytes'] = int(match[1]) * (1 if sys.platform == 'darwin' else 1024)
    result['command'] = [str(a) for a in args]
    return result


def hash_bytes(hash_value, data):
    for byte in data:
        hash_value = ((hash_value ^ byte) * 1_099_511_628_211) & ((1 << 64) - 1)
    return hash_value


def oracle(rows, length):
    hashes = {operation: 14_695_981_039_346_656_037 for operation in OPERATIONS}
    matches = 0
    for row in range(rows):
        region = b'north' if row % 4 == 0 else b'south'
        message = (f'{row % 100_000_000:08}-' + ('error' if row % 3 == 0 else 'event') + '-').encode().ljust(length, b'x')
        output = message.upper() + b'|ok'
        keep = region == b'north' and row % 100 >= 20 and b'error' in message
        if keep:
            matches += 1
            hashes['filter'] = hash_bytes(hashes['filter'], row.to_bytes(8, 'little'))
        encoded = bytearray(row.to_bytes(8, 'little'))
        for string in [region, message, output]:
            encoded += len(string).to_bytes(8, 'little') + string
        encoded += (row % 100).to_bytes(8, 'little')
        hashes['transform'] = hash_bytes(hashes['transform'], encoded)
        if keep:
            hashes['pipeline'] = hash_bytes(hashes['pipeline'], encoded)
    return {operation: {'count': rows if operation == 'transform' else matches,
                        'digest': str(hashes[operation])} for operation in OPERATIONS}


def summary(report):
    keys = sorted({(r['rows'], r['text_bytes'], r['workers'], r['operation']) for r in report['measurements']})
    output = []
    for rows, length, workers, operation in keys:
        for implementation in IMPLEMENTATIONS:
            measurements = [r for r in report['measurements'] if
                            (r['rows'], r['text_bytes'], r['workers'], r['operation'], r['implementation']) ==
                            (rows, length, workers, operation, implementation)]
            medians = [statistics.median(r['samples_ns']) for r in measurements]
            output.append({'rows': rows, 'text_bytes': length, 'workers': workers,
                           'operation': operation, 'implementation': implementation,
                           'median_ns': statistics.median(medians),
                           'min_round_median_ns': min(medians), 'max_round_median_ns': max(medians),
                           'peak_rss_bytes': max(r.get('peak_rss_bytes', 0) for r in measurements)})
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rows', type=int, nargs='+', default=[32, 100, 1000, 10000, 100000, 1000000])
    parser.add_argument('--text-bytes', type=int, nargs='+', default=[16, 128])
    parser.add_argument('--workers', type=int, nargs='+', default=[1, 8])
    parser.add_argument('--samples', type=int, default=5)
    parser.add_argument('--rounds', type=int, default=4)
    parser.add_argument('--sample-ms', type=int, default=25)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--allow-contended', action='store_true', help='explicitly measure diagnostics under background load')
    parser.add_argument('--wait-for-idle', action='store_true', help='wait for quiet periods and discard rounds interrupted by background work')
    parser.add_argument('--output', type=Path, default=OUTPUT / 'results.json')
    args = parser.parse_args()
    if min(args.rows) < 1 or max(args.rows) > 100_000_000 or min(args.text_bytes) < 16 or max(args.text_bytes) > 4096 or min(args.workers) < 1 or max(args.workers) > (os.cpu_count() or 1) or args.samples < 1 or args.rounds < 4 or args.rounds % 4 or args.sample_ms < 1:
        parser.error('invalid size, worker count, samples, or rounds (use multiples of four)')
    gd = ROOT / 'external/gd'
    before = fingerprint(gd / 'source')
    if not args.skip_build:
        print('Building optimized applications; log in target/text-workflow/build.log', flush=True)
        build()
    binaries = {'gd': OUTPUT / 'cpp/gd_text_workflow',
                'std': OUTPUT / 'cpp/gd_text_workflow',
                'compact': ROOT / 'target/release/examples/text_workflow',
                'fixed': ROOT / 'target/release/examples/text_workflow'}
    source_paths = sorted([*ROOT.glob('src/**/*.rs'), *ROOT.glob('benches/text_workflow/*'),
                           ROOT / 'benches/cpp-reference/text_workflow.cpp', ROOT / 'Cargo.toml',
                           ROOT / 'Cargo.lock', ROOT / 'benches/cpp-reference/CMakeLists.txt',
                           ROOT / 'benches/cpp-reference/cmake/GdCore.cmake'])
    report = {'metadata': {
        'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
        'cpu': text(['sysctl', '-n', 'machdep.cpu.brand_string']) if sys.platform == 'darwin' else platform.processor(),
        'logical_cpus': os.cpu_count(), 'rustc': text(['rustc', '-Vv']), 'cxx': text(['c++', '--version']),
        'gd_revision': text(['git', '-C', gd, 'rev-parse', 'HEAD']),
        'gd_rs_revision': text(['git', '-C', ROOT, 'rev-parse', 'HEAD']),
        'gd_source_sha256': before,
        'source_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths if p.is_file()},
        'rust_flags': 'release -O3; codegen-units=1; lto=thin; target-cpu=native; no-default-features; features=rayon; --locked',
        'cpp_flags': 'Release -O3 -DNDEBUG -march=native; interprocedural optimization ON; sanitizers OFF',
        'affinity': 'OS scheduling, no affinity; persistent pools, exactly one static row task per worker; one process at a time',
        'implementations': {'gd': 'AoS gd::table::table_column_buffer, inline bounded string slots',
                            'std': 'C++ AoS std::vector<StringRow>, ordinary std::string fields',
                            'compact': 'gd::Table SoA, existing CompactString storage, typed descriptor slices',
                            'fixed': 'gd::Table SoA, fixed slot byte buffer per string column with offset/length per row'},
        'timing_contract': 'fixtures and full digest checks outside timing; transform warms output capacity; filtering and pipeline include allocation and destruction; pipeline returns ordered worker shards; no final merge',
        'samples': args.samples, 'rounds': args.rounds, 'sample_ms': args.sample_ms,
        'contended_diagnostics': args.allow_contended, 'invocation': sys.argv,
        'measurement_complete': False,
    }, 'load_samples': [], 'timing_load_samples': [], 'discarded_rounds': [], 'verification': [], 'measurements': []}
    if sys.platform == 'darwin':
        report['metadata']['cpu_topology'] = text(['sysctl', 'hw.physicalcpu', 'hw.logicalcpu', 'hw.perflevel0.physicalcpu', 'hw.perflevel1.physicalcpu'])
        report['metadata']['ram_bytes'] = int(text(['sysctl', '-n', 'hw.memsize']))
        report['metadata']['power_settings'] = text(['pmset', '-g', 'custom'])
        report['metadata']['thermal_state_before'] = text(['pmset', '-g', 'therm'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        for rows in args.rows:
            for length in args.text_bytes:
                print(f'Verifying {rows:,} rows, {length}-byte messages against independent oracle...', flush=True)
                expected = oracle(rows, length)
                for workers in args.workers:
                    for operation in OPERATIONS:
                        for implementation in IMPLEMENTATIONS:
                            check = invoke(binaries[implementation], implementation, rows, length, workers, operation, 1, 1, True)
                            if any(check[k] != v for k, v in expected[operation].items()):
                                raise RuntimeError(f'Incorrect result: {implementation}, {rows}, {length}, {workers}, {operation}: {check}; oracle {expected[operation]}')
                            report['verification'].append({'implementation': implementation, 'rows': rows, 'text_bytes': length, 'workers': workers, 'operation': operation, **check})
                for round_index in range(args.rounds):
                    order = IMPLEMENTATIONS[round_index % 4:] + IMPLEMENTATIONS[:round_index % 4]
                    if (round_index // 4) % 2:
                        order = list(reversed(order))
                    while True:
                        before_round = wait_for_idle(report, args.allow_contended, args.wait_for_idle)
                        measured = []
                        for workers in args.workers:
                            for operation in OPERATIONS:
                                for implementation in order:
                                    result = invoke(binaries[implementation], implementation, rows, length, workers, operation, args.samples, args.sample_ms, False)
                                    if any(result[k] != v for k, v in expected[operation].items()):
                                        raise RuntimeError('Timed process verification differs from oracle')
                                    measured.append({'implementation': implementation, 'rows': rows, 'text_bytes': length, 'workers': workers, 'operation': operation, 'round': round_index + 1, **result})
                        after_begin = len(report['load_samples'])
                        try:
                            load_guard(report, args.allow_contended)
                        except RuntimeError as error:
                            report['discarded_rounds'].append({'rows': rows, 'text_bytes': length, 'round': round_index + 1,
                                                              'reason': str(error), 'measurements': measured})
                            if not args.wait_for_idle:
                                raise
                            print(f'  Discarding round {round_index + 1}: background work returned.', flush=True)
                            continue
                        report['measurements'].extend(measured)
                        report['timing_load_samples'].extend(before_round)
                        report['timing_load_samples'].extend(report['load_samples'][after_begin:])
                        print(f'  Round {round_index + 1}/{args.rounds} completed', flush=True)
                        args.output.write_text(json.dumps(report, indent=2) + '\n')
                        break
        report['summary'] = summary(report)
        report['metadata']['measurement_complete'] = True
    finally:
        report['metadata']['gd_source_unchanged'] = fingerprint(gd / 'source') == before
        if sys.platform == 'darwin':
            report['metadata']['thermal_state_after'] = text(['pmset', '-g', 'therm'])
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    if not report['metadata']['gd_source_unchanged']:
        raise RuntimeError('GD source fingerprint changed')
    print(f'Wrote {args.output}', flush=True)


if __name__ == '__main__':
    main()

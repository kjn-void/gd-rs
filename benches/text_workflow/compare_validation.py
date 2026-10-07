#!/usr/bin/env python3
"""Compare identical fixed-string workloads before and after trusted UTF-8 reads."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import statistics

import compare


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checked', type=Path, required=True, help='original checked-read executable')
    parser.add_argument('--trusted', type=Path, default=compare.ROOT / 'target/release/examples/text_workflow')
    parser.add_argument('--baseline-report', type=Path,
                        default=compare.ROOT / 'docs/high-level/measurements/text-workflow-m3max.json')
    parser.add_argument('--rows', type=int, nargs='+', default=[32, 100, 1000, 10000, 100000, 1000000])
    parser.add_argument('--samples', type=int, default=7)
    parser.add_argument('--rounds', type=int, default=4)
    parser.add_argument('--sample-ms', type=int, default=50)
    parser.add_argument('--allow-contended', action='store_true', help='label measurements under competing host activity')
    parser.add_argument('--output', type=Path, default=compare.ROOT / 'target/text-validation/results.json')
    args = parser.parse_args()
    if args.samples < 1 or args.sample_ms < 1 or args.rounds < 2 or args.rounds % 2:
        parser.error('use positive samples and batch length, and an even number of rounds')
    baseline = json.loads(args.baseline_report.read_text())
    expected = {(r['rows'], r['text_bytes'], r['workers'], r['operation']):
                {key: r[key] for key in ['count', 'digest']}
                for r in baseline['verification'] if r['implementation'] == 'fixed'}
    if any((rows, length, worker, operation) not in expected for rows in args.rows
           for length in [16, 128] for worker in [1, 8] for operation in compare.OPERATIONS):
        parser.error('requested rows must have independent oracle results in the baseline report')
    driver = compare.ROOT / 'benches/text_workflow/driver.rs'
    if sha256(driver) != baseline['metadata']['source_sha256']['benches/text_workflow/driver.rs']:
        parser.error('the workload driver changed; this experiment must isolate the storage change')
    binaries = {'checked': args.checked.resolve(), 'trusted': args.trusted.resolve()}
    sources = sorted([*compare.ROOT.glob('src/**/*.rs'), driver,
                      compare.ROOT / 'Cargo.toml', compare.ROOT / 'Cargo.lock', Path(__file__).resolve()])
    report = {'metadata': {
        'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
        'rustc': compare.text(['rustc', '-Vv']),
        'cpu': compare.text(['sysctl', '-n', 'machdep.cpu.brand_string']),
        'cpu_topology': compare.text(['sysctl', 'hw.physicalcpu', 'hw.logicalcpu',
                                     'hw.perflevel0.physicalcpu', 'hw.perflevel1.physicalcpu']),
        'ram_bytes': int(compare.text(['sysctl', '-n', 'hw.memsize'])),
        'power_settings': compare.text(['pmset', '-g', 'custom']),
        'thermal_state_before': compare.text(['pmset', '-g', 'therm']),
        'rust_flags': baseline['metadata']['rust_flags'], 'affinity': baseline['metadata']['affinity'],
        'base_commit': compare.text(['git', '-C', compare.ROOT, 'rev-parse', 'HEAD']),
        'baseline_report': str(args.baseline_report), 'baseline_report_sha256': sha256(args.baseline_report),
        'baseline_source_sha256': baseline['metadata']['source_sha256'],
        'source_sha256': {str(p.relative_to(compare.ROOT)): sha256(p) for p in sources},
        'binary_sha256': {name: sha256(path) for name, path in binaries.items()},
        'samples': args.samples, 'rounds': args.rounds, 'sample_ms': args.sample_ms,
        'contended_diagnostics': args.allow_contended,
        'timing_contract': baseline['metadata']['timing_contract'],
        'measurement_complete': False,
    }, 'verification': [], 'measurements': [], 'load_samples': [],
        'timing_load_samples': [], 'discarded_rounds': []}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    try:
        for rows in args.rows:
            for length in [16, 128]:
                print(f'Verifying {rows:,} rows, {length}-byte messages...', flush=True)
                for worker in [1, 8]:
                    for operation in compare.OPERATIONS:
                        for name, binary in binaries.items():
                            check = compare.invoke(binary, 'fixed', rows, length, worker, operation, 1, 1, True)
                            if any(check[k] != v for k, v in expected[rows, length, worker, operation].items()):
                                raise RuntimeError('verification differs from independent oracle')
                            report['verification'].append({'version': name, 'rows': rows,
                                'text_bytes': length, 'workers': worker, 'operation': operation, **check})
                for round_index in range(args.rounds):
                    order = list(binaries) if round_index % 2 == 0 else list(reversed(binaries))
                    while True:
                        before = compare.wait_for_idle(report, args.allow_contended, True)
                        measured = []
                        for worker in [1, 8]:
                            for operation in compare.OPERATIONS:
                                for name in order:
                                    result = compare.invoke(binaries[name], 'fixed', rows, length, worker,
                                                            operation, args.samples, args.sample_ms, False)
                                    if any(result[k] != v for k, v in expected[rows, length, worker, operation].items()):
                                        raise RuntimeError('timed output differs from independent oracle')
                                    measured.append({'version': name, 'rows': rows, 'text_bytes': length,
                                        'workers': worker, 'operation': operation, 'round': round_index + 1, **result})
                        end = len(report['load_samples'])
                        try:
                            compare.load_guard(report, args.allow_contended)
                        except RuntimeError as error:
                            report['discarded_rounds'].append({'rows': rows, 'text_bytes': length,
                                'round': round_index + 1, 'reason': str(error), 'measurements': measured})
                            print('  Discarding round interrupted by background work.', flush=True)
                            continue
                        report['measurements'].extend(measured)
                        report['timing_load_samples'].extend(before)
                        report['timing_load_samples'].extend(report['load_samples'][end:])
                        args.output.write_text(json.dumps(report, indent=2) + '\n')
                        print(f'  Round {round_index + 1}/{args.rounds} completed', flush=True)
                        break
        report['summary'] = []
        for rows in args.rows:
            for length in [16, 128]:
                for worker in [1, 8]:
                    for operation in compare.OPERATIONS:
                        medians = {}
                        for name in binaries:
                            values = [statistics.median(r['samples_ns']) for r in report['measurements']
                                      if (r['rows'], r['text_bytes'], r['workers'], r['operation'], r['version'])
                                      == (rows, length, worker, operation, name)]
                            medians[name] = {'median_ns': statistics.median(values),
                                            'min_round_median_ns': min(values), 'max_round_median_ns': max(values)}
                        report['summary'].append({'rows': rows, 'text_bytes': length, 'workers': worker,
                            'operation': operation, **medians,
                            'speedup': medians['checked']['median_ns'] / medians['trusted']['median_ns']})
        report['metadata']['measurement_complete'] = True
    finally:
        report['metadata']['thermal_state_after'] = compare.text(['pmset', '-g', 'therm'])
        report['metadata']['source_unchanged'] = all(sha256(compare.ROOT / name) == digest
            for name, digest in report['metadata']['source_sha256'].items())
        report['metadata']['binaries_unchanged'] = all(sha256(binaries[name]) == digest
            for name, digest in report['metadata']['binary_sha256'].items())
        args.output.write_text(json.dumps(report, indent=2) + '\n')
    if not report['metadata']['source_unchanged'] or not report['metadata']['binaries_unchanged']:
        raise RuntimeError('source or executable changed during measurement')
    print(f'Wrote {args.output}', flush=True)


if __name__ == '__main__':
    main()

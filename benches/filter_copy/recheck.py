#!/usr/bin/env python3
"""Confirm noisy or close cases without replacing the primary measurements."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import statistics

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('filter_copy', ROOT / 'benches/filter_copy/compare.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('primary', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--max-cases', type=int, default=3)
    parser.add_argument('--case', action='append', nargs=3, type=int, default=[],
                        metavar=('BYTES', 'WORKERS', 'PERCENT'),
                        help='also repeat an explicitly selected surprising case')
    args = parser.parse_args()
    primary = json.loads(args.primary.read_text())
    m = primary['metadata']
    if not m['measurement_complete'] or not all(m[k] for k in ['gd_source_unchanged', 'source_unchanged', 'binaries_unchanged']):
        raise RuntimeError('primary run incomplete or changed')
    binaries = {'gd': ROOT / 'target/filter-copy/cpp/gd_filter_copy',
                'std': ROOT / 'target/filter-copy/cpp/gd_filter_copy',
                'compact': ROOT / 'target/release/examples/filter_copy',
                'fixed': ROOT / 'target/release/examples/filter_copy',
                'arc': ROOT / 'target/release/examples/filter_copy'}
    def unchanged():
        return all(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest for name, digest in m['source_sha256'].items()) \
            and all(hashlib.sha256(binaries[name].read_bytes()).hexdigest() == digest for name, digest in m['binary_sha256'].items()) \
            and driver.helper.fingerprint(ROOT / 'external/gd/source') == m['gd_source_sha256']
    if not unchanged():
        raise RuntimeError('sources/executables differ from primary')
    candidates = []
    matrix = {}
    for length in [16, 128]:
        for workers in [1, 8]:
            for percent in [10, 50, 90]:
                records = [r for r in primary['summary'] if (r['text_bytes'], r['workers'], r['selectivity']) == (length, workers, percent)]
                spread = max(r['max_round_median_ns'] / r['min_round_median_ns'] for r in records)
                fastest = sorted(r['median_ns'] for r in records)
                gap = fastest[1] / fastest[0]
                matrix[length, workers, percent] = (max(spread / 1.2, 1.1 / gap), length, workers, percent, spread, gap)
                if spread >= 1.2 or gap <= 1.1:
                    candidates.append(matrix[length, workers, percent])
    cases = sorted(candidates, reverse=True)[:args.max_cases]
    for key in map(tuple, args.case):
        if key not in matrix:
            parser.error('--case must belong to the primary measurement matrix')
        if matrix[key] not in cases:
            cases.append(matrix[key])
    report = {'metadata': {'utc': datetime.now(timezone.utc).isoformat(), 'host': platform.node(),
                          'primary_sha256': hashlib.sha256(args.primary.read_bytes()).hexdigest(),
                          'source_sha256': m['source_sha256'], 'binary_sha256': m['binary_sha256'],
                          'rounds': 5, 'samples': 7, 'sample_ms': 50, 'rows': m['rows'],
                          'contended_diagnostics': m['contended_diagnostics'], 'measurement_complete': False,
                          'selection': 'greatest relative round spread or closest winning times; spread >=20% or winner gap <=10%; explicit additional surprising cases when requested',
                          'max_auto_cases': args.max_cases, 'explicit_cases': args.case},
              'cases': [{'text_bytes': n, 'workers': w, 'selectivity': p, 'primary_max_round_ratio': s,
                         'primary_winner_ratio': g} for _, n, w, p, s, g in cases], 'measurements': [], 'load_samples': []}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    for _, length, workers, percent, _, _ in cases:
        expected = next(r for r in primary['verification'] if (r['text_bytes'], r['workers'], r['selectivity']) == (length, workers, percent))
        for round_index in range(5):
            driver.helper.load_guard(report, m['contended_diagnostics'])
            order = driver.IMPLEMENTATIONS[round_index:] + driver.IMPLEMENTATIONS[:round_index]
            for implementation in order:
                result = driver.helper.invoke(binaries[implementation], implementation, m['rows'], length, workers, percent, 7, 50, False)
                if (result['count'], result['digest']) != (expected['count'], expected['digest']):
                    raise RuntimeError('confirmation output mismatch')
                report['measurements'].append({'implementation': implementation, 'text_bytes': length,
                                              'workers': workers, 'selectivity': percent, 'round': round_index + 1, **result})
            driver.helper.load_guard(report, m['contended_diagnostics'])
            args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(f'Confirmed {length} bytes, {workers} workers, {percent}% selection', flush=True)
    report['summary'] = driver.summaries(report)
    report['metadata']['source_and_binaries_unchanged'] = unchanged()
    report['metadata']['measurement_complete'] = True
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    if not report['metadata']['source_and_binaries_unchanged']:
        raise RuntimeError('sources/executables changed during confirmation')
    print(f'Wrote {args.output}', flush=True)


if __name__ == '__main__':
    main()

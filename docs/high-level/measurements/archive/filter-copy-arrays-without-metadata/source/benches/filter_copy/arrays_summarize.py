#!/usr/bin/env python3
"""Validate and render the paired GD/fixed-array SoA experiment."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('arrays', ROOT / 'benches/filter_copy/arrays.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
KEYS = ('text_bytes', 'workers', 'selectivity', 'implementation')
SOURCES = ('Sources: [Rust fixed-array SoA](../../benches/filter_copy/fixed_arrays.rs), '
           '[unchanged GD memcpy driver](../../benches/cpp-reference/filter_copy.cpp), '
           '[paired runner and oracle](../../benches/filter_copy/arrays.py).')


def key(row):
    return tuple(row[k] for k in KEYS)


def validate(path, primary=False):
    data = json.loads(path.read_text())
    meta = data['metadata']
    for field in ['measurement_complete', 'gd_source_unchanged', 'source_unchanged', 'binaries_unchanged']:
        if not meta[field]:
            raise RuntimeError(f'{path}: failed {field}')
    if meta['rows'] != 1_000_000 or meta['process_niceness'] != 0:
        raise RuntimeError(f'{path}: unexpected row count or process priority')
    for name, digest in meta['source_sha256'].items():
        if hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != digest:
            raise RuntimeError(f'{path}: current benchmark source differs: {name}')
    expected = {key(row): (row['count'], row['digest']) for row in data['verification']}
    for row in data['verification'] + data['edge_verification']:
        if not row['independent_target']:
            raise RuntimeError('target independence check failed')
    for row in data['measurements']:
        if (row['count'], row['digest']) != expected[key(row)] or len(row['samples_ns']) != meta['samples']:
            raise RuntimeError('timing digest or sample count differs')
    summary = runner.summaries(data)
    if summary != data['summary']:
        raise RuntimeError('stored summary does not match raw samples')
    if primary and (len(summary) != 24 or len(data['measurements']) != 24 * meta['rounds']
                    or len(data['verification']) != 24 or len(data['edge_verification']) != 160):
        raise RuntimeError('primary matrix is incomplete')
    for case in expected:
        rounds = [r['round'] for r in data['measurements'] if key(r) == case]
        if sorted(rounds) != list(range(1, meta['rounds'] + 1)):
            raise RuntimeError('missing or duplicate process rounds')
        other = (*case[:3], 'arrays' if case[3] == 'gd' else 'gd')
        if expected[case] != expected[other]:
            raise RuntimeError('implementations disagree on ordered result')
    return data, {key(row): row for row in summary}


def ratio(index, length, workers, percent):
    return index[length, workers, percent, 'gd']['median_ns'] / index[length, workers, percent, 'arrays']['median_ns']


def geomean(values):
    return math.exp(statistics.mean(math.log(value) for value in values))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--confirmations', type=Path)
    parser.add_argument('--layout-probe', type=Path)
    parser.add_argument('--report', type=Path, default=ROOT / 'docs/high-level/filter-copy-arrays-m6-results.md')
    parser.add_argument('--figure', type=Path, default=ROOT / 'docs/high-level/measurements/filter-copy-arrays-m6.png')
    args = parser.parse_args()
    data, index = validate(args.input, primary=True)
    confirmations, confirmed = validate(args.confirmations) if args.confirmations else (None, {})
    if confirmations:
        for field in ['host', 'source_sha256', 'binary_sha256', 'oracle_sha256', 'samples', 'sample_ms']:
            if data['metadata'][field] != confirmations['metadata'][field]:
                raise RuntimeError(f'confirmation differs from primary: {field}')
    meta = data['metadata']
    if args.layout_probe:
        probe = json.loads(args.layout_probe.read_text())
        if (probe['host'] != meta['host'] or probe['gd_revision'] != meta['gd_revision']
                or probe['gd_source_sha256'] != meta['gd_source_sha256']
                or probe['cpp_source_sha256'] != meta['source_sha256']['benches/cpp-reference/filter_copy.cpp']
                or probe['output'] != '16 bytes per string: GD row 72 bytes\n128 bytes per string: GD row 296 bytes\n'):
            raise RuntimeError('GD layout probe differs from the measured source or expected layout')
    groups = [(length, workers) for workers in [1, 8] for length in [16, 128]]
    overall = geomean(ratio(index, length, workers, p) for length, workers in groups for p in [10, 50, 90])
    lines = ['# GD row memcpy versus fixed-array SoA on the M6', '', SOURCES, '',
             'This is a separate, freshly paired experiment. The existing five-variant, three-host results are unchanged.', '',
             f'One million source rows contain three `u64` columns and two text columns, each exactly 16 or 128 ASCII bytes. '
             'Filtering `selector < percentage` selects 10%, 50%, or 90% of the rows. Every match deep-copies all five '
             'fields into one independently owned, ordered destination with the same schema.', '',
             'GD uses the existing `table_column_buffer` implementation and copies each complete inline row with `std::memcpy`. '
             'The Rust experiment uses three `Vec<u64>` columns and two `Vec<[u8; N]>` columns, with `N = 16` or `128`. '
             'It is a **benchmark-local typed SoA prototype**, not the existing dynamic `gd::Table`: that API does not '
             'currently accept fixed byte-array cells. There are no CompactString objects, length descriptors, shared '
             'handles, or per-cell allocations in the Rust target.', '',
             'Both implementations filter static source row ranges first, retain matched indices, compute destination '
             'ranges, allocate one target, then deep-copy disjoint row ranges. Rust uses an exact eight-thread Rayon '
             'pool; GD retains its persistent C++ worker pool. One worker executes on the caller. All workers join '
             'before the target is returned. No worker output tables or extra concatenation of payloads are used.', '',
             'Rust writes values into `Vec::spare_capacity_mut()` using safely split slices, then sets column lengths '
             'after every slot has been initialized. This avoids a zero-fill pass absent from GD. Strings are raw '
             'fixed arrays with their length known from the schema. GD additionally stores string lengths, terminators '
             'and alignment: its row stride is 72/296 bytes, versus 56/280 payload bytes across the Rust columns. '
             'The byte layouts therefore have equivalent values but different metadata costs.' +
             (' The [untimed M6 layout probe](measurements/filter-copy-arrays-m6-inline-layout.json) '
              'records the exact source snippet, compiler command and row strides.' if args.layout_probe else ''), '',
             '## Fresh paired results', '', SOURCES, '',
             '**Contended diagnostics:** these runs use the authorized current background load, without CPU affinity. '
             'The geometric mean weights all 12 cases equally. Ratios are GD time divided by Rust time: above 1 means Rust is faster.', '',
             f'Overall Rust/GD speed ratio: **{overall:.3f}×**. Rust is faster in '
             f'**{sum(ratio(index, length, workers, p) > 1 for length, workers in groups for p in [10, 50, 90])} '
             'of 12 primary cases**.', '',
             '| Workers | Bytes per string | Rust speed relative to GD, geometric mean |',
             '|---:|---:|---:|']
    for length, workers in groups:
        gm = geomean(ratio(index, length, workers, p) for p in [10, 50, 90])
        lines.append(f'| {workers} | {length} | {gm:.3f}× |')
    lines += ['', '| Bytes/string | Workers | Selected | GD ms | Rust arrays ms | Rust/GD speed |',
              '|---:|---:|---:|---:|---:|---:|']
    for length, workers in groups:
        for percent in [10, 50, 90]:
            gd = index[length, workers, percent, 'gd']['median_ns'] / 1e6
            rust = index[length, workers, percent, 'arrays']['median_ns'] / 1e6
            lines.append(f'| {length} | {workers} | {percent}% | {gd:.3f} | {rust:.3f} | {gd / rust:.3f}× |')
    lines += ['', '![Fresh M6 paired timings](measurements/filter-copy-arrays-m6.png)', '',
              'These are complete filter-and-copy timings, so they include the SoA advantage of scanning a contiguous '
              'numeric selector column. They do not isolate copy-only throughput or establish that either layout '
              'wins for every operation. The experiment also compares runtime-sized GD memcpy with Rust copies '
              'whose array sizes are known at compile time; those are material implementation differences.', '',
              '## Repeats and variability', '', SOURCES, '']
    if confirmations:
        lines += ['Selected cases were repeated separately under the same settings. The primary numbers above remain intact.', '',
                  '| Bytes/string | Workers | Selected | Primary Rust/GD | Repeat Rust/GD |',
                  '|---:|---:|---:|---:|---:|']
        for length, workers, percent in sorted({k[:3] for k in confirmed}):
            lines.append(f'| {length} | {workers} | {percent}% | {ratio(index, length, workers, percent):.3f}× | '
                         f'{ratio(confirmed, length, workers, percent):.3f}× |')
        repeat_cases = sorted({k[:3] for k in confirmed})
        repeat_wins = sum(ratio(confirmed, *case) > 1 for case in repeat_cases)
        lines += ['', f'Rust is faster in {repeat_wins} of {len(repeat_cases)} confirmation cases. '
                  'The advantage varies between runs, especially with eight workers; these diagnostics support '
                  'the direction of the result more strongly than an exact speedup factor.', '',
                  'Cases were selected by the largest relative round-median range in each of the four groups, '
                  'supplemented by the fastest and closest-to-parity primary results when different.']
    else:
        lines.append('No separate confirmations supplied.')
    lines += ['', 'The figure shows the median of round medians; whiskers show the minimum and maximum round median, '
              'not confidence intervals. Boundary load observations and all batch samples are retained in the raw data.', '',
              '## Measurement and reproduction', '', SOURCES, '',
              f'- Host: `{meta["host"]}`, `{meta["cpu"]}`; `{meta["platform"]}`.',
              f'- CPU topology: {meta["cpu_topology"].replace(chr(10), "; ")}.',
              f'- RAM: {meta["ram_bytes"] / 2**30:.0f} GiB; process niceness {meta["process_niceness"]}.',
              f'- Rust: `{meta["rustc"].splitlines()[0]}`. C++: `{meta["cxx"].splitlines()[0]}`.',
              f'- Rust flags: `{meta["rust_flags"]}`. C++ flags: `{meta["cpp_flags"]}`.',
              f'- GD revision: `{meta["gd_revision"]}`; gd-rs base revision: `{meta["gd_rs_revision"]}` plus the fingerprinted experiment files.',
              f'- Measurement UTC: `{meta["utc"]}`; {meta["rounds"]} alternating-order process rounds, '
              f'{meta["samples"]} batches per process, calibrated to at least {meta["sample_ms"]} ms per batch.',
              f'- Primary correctness: {len(data["verification"])} million-row, ordered full-cell digest/source-drop checks '
              f'and {len(data["edge_verification"])} empty/boundary/ownership checks; independent Python oracle.',
              '- Source, executable and GD fingerprints remained unchanged during the run.',
              '- Source construction, worker startup and verification are excluded. Target allocation, filtering, '
              'temporary indices, synchronization, copying and target destruction are included.',
              '- Separate Rust AddressSanitizer tests and the Rust 1.86 example check passed; timed binaries use no instrumentation.', '',
              '[Primary raw data](measurements/filter-copy-arrays-m6.json)' +
              (', [confirmation raw data](measurements/filter-copy-arrays-m6-confirmations.json).' if confirmations else '.'), '',
              '```sh',
              'PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arrays.py --prepare-oracle',
              'PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arrays.py --allow-contended', '',
              '# Repeat the selected cases without replacing the primary JSON:',
              'PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arrays.py --skip-build --allow-contended \\',
              '  ' + ' '.join('--case ' + ' '.join(map(str, case)) for case in sorted({k[:3] for k in confirmed})) + ' \\',
              '  --output target/filter-copy-arrays/confirmations.json', '',
              '# Render the retained primary and confirmation files:',
              'python3 benches/filter_copy/arrays_summarize.py \\',
              '  --input docs/high-level/measurements/filter-copy-arrays-m6.json \\',
              '  --confirmations docs/high-level/measurements/filter-copy-arrays-m6-confirmations.json \\',
              '  --layout-probe docs/high-level/measurements/filter-copy-arrays-m6-inline-layout.json',
              '```', '']
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    import numpy as np
    fig, axes = plt.subplots(2, 2, figsize=(12, 8), layout='constrained')
    for ax, (length, workers) in zip(axes.flat, groups):
        positions = np.arange(3)
        for implementation, label, color, offset in [('gd', 'GD whole-row memcpy', '#4878a8', -0.18),
                                                     ('arrays', 'Rust fixed-array SoA', '#e19c24', 0.18)]:
            rows = [index[length, workers, percent, implementation] for percent in [10, 50, 90]]
            times = [r['median_ns'] / 1e6 for r in rows]
            errors = [[(r['median_ns'] - r['min_round_median_ns']) / 1e6 for r in rows],
                      [(r['max_round_median_ns'] - r['median_ns']) / 1e6 for r in rows]]
            ax.bar(positions + offset, times, 0.34, label=label, color=color, yerr=errors,
                   capsize=3, error_kw={'elinewidth': 1})
        ax.set_title(f'{workers} worker{"s" if workers > 1 else ""} · {length} bytes per string')
        ax.set_xticks(positions, ['10%', '50%', '90%'])
        ax.set_xlabel('Rows selected')
        ax.set_ylabel('Milliseconds per filter + deep copy')
        ax.grid(axis='y', alpha=0.2)
        ax.set_axisbelow(True)
        ax.spines[['top', 'right']].set_visible(False)
    axes.flat[0].legend(fontsize=9)
    fig.suptitle(f'M6 · 1,000,000 rows · five columns · GD memcpy vs Rust fixed arrays\n'
                 f'Contended diagnostics · fresh paired runs · overall Rust speed {overall:.2f}× GD', fontsize=14)
    args.figure.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.figure, dpi=180)
    plt.close(fig)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text('\n'.join(lines))
    print(f'Wrote {args.report} and {args.figure}; overall ratio {overall:.3f}×')


if __name__ == '__main__':
    main()

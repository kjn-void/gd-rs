#!/usr/bin/env python3
"""Render the text-workflow JSON matrix as Markdown and optional performance plots."""
import argparse
import json
from pathlib import Path

IMPLEMENTATIONS = ['gd', 'std', 'compact', 'fixed']
LABELS = {'gd': 'GD buffer', 'std': 'C++ std::string',
          'compact': 'gd-rs CompactString', 'fixed': 'gd-rs fixed buffer'}
SOURCES = ('Sources: [Rust workloads](../../benches/text_workflow/driver.rs), '
           '[GD buffer and C++ std::string workloads](../../benches/cpp-reference/text_workflow.cpp), '
           '[runner and independent oracle](../../benches/text_workflow/compare.py).')


def render(report, raw_name, chart_name):
    meta = report['metadata']
    records = report['summary']
    lookup = {(r['rows'], r['text_bytes'], r['workers'], r['operation'], r['implementation']): r for r in records}
    sizes = sorted({r['rows'] for r in records})
    lengths = sorted({r['text_bytes'] for r in records})
    workers = sorted({r['workers'] for r in records})
    largest = max(sizes)
    rows = ['# Text filtering and transforms: GD and gd-rs', '',
            'This comparison exercises text directly in ordinary application tables: '
            'filtering, rewriting a string column, and filtering plus materializing and transforming rows. '
            'It compares four representations of the same five-column data.', '',
            '| Case | Storage and access |', '|---|---|',
            '| GD buffer | Unmodified `gd::table::table_column_buffer`, row-major (AoS), preallocated bounded inline text slots |',
            '| C++ std::string | `std::vector<StringRow>`, AoS rows with three ordinary `std::string` fields; a STL baseline rather than a GD table |',
            '| gd-rs CompactString | Existing `gd::Table` SoA string storage, checked typed descriptor slices |',
            '| gd-rs fixed buffer | `gd::Table` SoA, one fixed-slot byte buffer per string column; every row has an offset and length |', '',
            'The new gd-rs storage is a supported table layout, with UTF-8 byte limits, '
            'nullability, conversions, atomic failed writes, independent copies, append between layouts, '
            'compaction, indexes, and disjoint parallel mutable views. '
            'See the [API](../api/tables.md#fixed-capacity-string-buffers) and '
            '[storage implementation](../../src/table/fixed_string.rs).', '',
            '## Conditions and interpretation', '',
            f'Measured {meta["utc"][:10]} on {meta["cpu"]}, {meta["logical_cpus"]} logical CPUs, '
            f'{meta["platform"]}. Rust: `{meta["rustc"].splitlines()[0]}`. '
            f'C++: `{meta["cxx"].splitlines()[0]}`. GD is pinned to `{meta["gd_revision"]}`. '
            f'The gd-rs worktree was based on `{meta["gd_rs_revision"]}`; exact source hashes are in the raw results.', '',
            f'Rust flags: `{meta["rust_flags"]}`. C++ flags: `{meta["cpp_flags"]}`. '
            f'Scheduling: {meta["affinity"]}.', '']
    if meta['contended_diagnostics']:
        rows += ['**These are contended diagnostics measured with background jobs active. '
                 'They are not an unloaded performance baseline; worker scaling and close rankings '
                 'cannot be attributed solely to the table implementations.**', '']
    else:
        rows += ['Final timings come from rounds that passed checks for competing background work. '
                 'The runner sampled unrelated process CPU time before timing and after every round '
                 'and rejected sustained activity above its one-busy-core threshold, confirming elevated '
                 'snapshots with a second sample. OS scheduling and ordinary '
                 'desktop activity remain part of these host measurements.', '']
    load = [s['aggregate_cpu_percent'] for s in report.get('timing_load_samples', report['load_samples'])]
    rows += [f'Unrelated CPU snapshots ranged from {min(load):.1f}% to {max(load):.1f}% '
             '(100% is one CPU core; only processes above 5% are counted). '
             'These are boundary snapshots, not continuous profiling. '
             f'The final run checked {len(report["verification"])} implementation/size/worker/workload '
             f'combinations against the independent oracle and recorded {len(report["measurements"])} timed processes. '
             f'Each process recorded {meta["samples"]} calibrated batches; the minimum batch target was '
             f'{meta["sample_ms"]} ms. Tables report the median of {meta["rounds"]} process-round medians. '
             'Round ranges and all samples are retained in the raw JSON.', '',
             f'{len(report.get("discarded_rounds", []))} rounds were discarded because substantial '
             'background work returned. Discarded samples and waiting-period load snapshots are kept '
             'in the raw JSON and excluded from the result tables.', '',
             f'[Raw measurements](measurements/{raw_name}) contain commands, source fingerprints, '
             'compiler versions, topology, thermal state, background load, and process RSS. '
             'The GD source fingerprint was unchanged.', '',
             '## Workload and timing boundaries', '',
             f'Rows: {", ".join(f"{n:,}" for n in sizes)}. Message lengths: '
             f'{", ".join(map(str, lengths))} ASCII bytes. Workers: {", ".join(map(str, workers))}. '
             'The schema is `id: u64`, `region: text`, `message: text`, `output: text`, and `score: u64`. '
             'IDs and message prefixes vary by row. `north` occurs every fourth row, `error` every third '
             'row, and score is `row % 100`.', '',
             '- Filter: collect ordered row positions where region is `north`, score is at least 20, '
             'and message contains `error`. Approximately 6.67% of large tables match.',
             '- Transform: rewrite every output cell as ASCII uppercase message plus `|ok`. '
             'The output becomes three bytes longer. The source column is unchanged.',
             '- Pipeline: filter, copy all five columns into independently owned output tables, '
             'then transform their output strings. Each worker returns its own ordered shard; no final merge is timed.', '',
             'Pool construction, fixture generation, and full output digest checks are outside timing. '
             'One-worker operations execute directly; eight-worker operations dispatch exactly eight disjoint row tasks. '
             'Transform measures repeated writes after output capacity has warmed. Filter and pipeline '
             'include result allocation and destruction. These timing boundaries are the same for all cases.', '',
             'GD gathers whole rows through its public row-buffer API. The benchmark schema contains '
             'inline strings and no null metadata or indexed references, so byte copying owns the complete payload. '
             'The STL case copies `StringRow` values, including string ownership. gd-rs uses native '
             'column-wise `copy_rows`. GD and fixed-buffer transforms use one scratch string per worker; '
             'the ordinary C++/Rust strings rewrite their own output buffers. '
             'Search functions, case-conversion loops, runtime scheduling, bounds checks, and UTF-8 validation '
             'also differ; this experiment does not isolate layout or language alone.', '', SOURCES, '']
    if chart_name:
        rows += [f'![Text workload scaling](measurements/{chart_name})', '']
    names = {'filter': 'Filtering text', 'transform': 'Rewriting text', 'pipeline': 'Filter, copy, and transform'}
    for operation, title in names.items():
        rows += [f'## {title}', '', SOURCES, '', 'Times are microseconds per complete operation; lower is faster.', '']
        for length in lengths:
            rows += [f'### {length}-byte messages', '',
                     '| Rows | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |',
                     '|---:|---:|---:|---:|---:|---:|']
            for count in sizes:
                for worker in workers:
                    values = [lookup[count, length, worker, operation, impl]['median_ns'] / 1000 for impl in IMPLEMENTATIONS]
                    rows.append(f'| {count:,} | {worker} | ' + ' | '.join(f'{v:.3g}' for v in values) + ' |')
            rows += ['']
    if 1 in workers and 8 in workers:
        rows += ['## Scaling at the largest size', '', SOURCES, '',
                 f'{largest:,} rows. Speedup is one-worker time divided by eight-worker time. '
                 'A value below 1 means the parallel run is slower.', '',
                 '| Text bytes | Operation | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |',
                 '|---:|---|---:|---:|---:|---:|']
        for length in lengths:
            for operation in names:
                speedups = [lookup[largest, length, 1, operation, impl]['median_ns'] /
                            lookup[largest, length, 8, operation, impl]['median_ns'] for impl in IMPLEMENTATIONS]
                rows.append(f'| {length} | {operation} | ' + ' | '.join(f'{v:.2f}×' for v in speedups) + ' |')
        rows += ['']
    rows += ['## Process memory', '', SOURCES, '',
             f'Peak RSS at {largest:,} rows during transform, in MiB. This includes the source fixture, '
             'verification, worker runtime, and allocator overhead, so it is not an isolated table footprint.', '',
             '| Text bytes | Workers | GD buffer | C++ std::string | gd-rs CompactString | gd-rs fixed buffer |',
             '|---:|---:|---:|---:|---:|---:|']
    for length in lengths:
        for worker in workers:
            values = [lookup[largest, length, worker, 'transform', impl]['peak_rss_bytes'] / (1024 ** 2) for impl in IMPLEMENTATIONS]
            rows.append(f'| {length} | {worker} | ' + ' | '.join(f'{v:.1f}' for v in values) + ' |')
    rows += ['', 'On this 64-bit host, ordinary Rust/C++ string descriptors are 24 bytes and use '
             'inline storage for these short values. The new fixed-buffer descriptors use two `usize` fields '
             '(16 bytes) plus each reserved slot. Region capacity is 8 bytes, message capacity is its byte '
             'length, and output capacity is message length plus 3. Slots are reserved even for nulls; '
             'this benchmark has no nulls. Larger-than-needed capacities increase the footprint.', '',
             '## What the experiment establishes', '',
             'Text is supported in the gd-rs column layout, including filtering, longer output strings, '
             'owned materialization, and parallel writes. The fixed buffer removes per-cell string allocations '
             'but adds offsets, reserved capacity, and UTF-8 validation on borrow. It is an alternative storage '
             'contract rather than a guaranteed speed improvement. The measurements above determine which '
             'implementation is faster for each tested operation and size.', '',
             'The comparison covers fixed-length ASCII messages and one selectivity. Unicode boundary behavior '
             'is tested for correctness, but Unicode case folding, variable-length distributions, joins, '
             'random edits, database import, nullable scans, and oversized-write performance are outside this '
             'timing matrix. It does not establish a universal AoS/SoA ranking or any OLAP preprocessing cost.', '',
             'Validation: the repository CI script passed formatting, strict Clippy, all-features tests, '
             'minimal-feature tests, rustdoc, and the Rust 1.86 library check. Fixed-string tests include '
             'randomized equivalence with ordinary tables, UTF-8 byte limits, null/empty values, failed-write '
             'atomicity, copies, append between layouts, compaction, index/sort/format integration, and scoped '
             'parallel mutation. Separate C++ AddressSanitizer runs passed the oracle on 1, 32, and 1,001 rows '
             'with one and eight workers for both C++ representations and all three operations. '
             'Sanitized executables were not used for performance measurements.', '',
             'Reproduce the final matrix:', '', '```sh',
             './benches/run_text_workflow.sh --samples ' + str(meta['samples']) + ' --rounds ' + str(meta['rounds']) +
             ' --sample-ms ' + str(meta['sample_ms']) + (' --allow-contended' if meta['contended_diagnostics'] else ' --wait-for-idle'),
             '```', '',
             'The [benchmark README](../../benches/text_workflow/README.md) describes smaller runs '
             'and the diagnostic mode. The [report generator](../../benches/text_workflow/summarize.py) '
             'recreates these tables and optional charts from the raw JSON.', '']
    return '\n'.join(rows)


def plot(report, path):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    records = report['summary']
    lengths = sorted({r['text_bytes'] for r in records})
    operations = ['filter', 'transform', 'pipeline']
    workers = sorted({r['workers'] for r in records})
    colors = {'gd': '#087f8c', 'std': '#e09f3e', 'compact': '#5851a7', 'fixed': '#c44354'}
    fig, axes = plt.subplots(len(lengths) * len(workers), 3, figsize=(14, 3.1 * len(lengths) * len(workers)), squeeze=False)
    for row_index, (length, worker) in enumerate((length, worker) for length in lengths for worker in workers):
        for column_index, operation in enumerate(operations):
            ax = axes[row_index, column_index]
            for implementation in IMPLEMENTATIONS:
                data = sorted([r for r in records if (r['text_bytes'], r['workers'], r['operation'], r['implementation']) == (length, worker, operation, implementation)], key=lambda r: r['rows'])
                ax.plot([r['rows'] for r in data], [r['median_ns'] / 1000 for r in data], marker='o', markersize=4, color=colors[implementation], label=LABELS[implementation])
            ax.set_xscale('log'); ax.set_yscale('log')
            ax.grid(alpha=0.2, which='both')
            ax.set_title(f'{operation.title()} · {length} bytes · {worker} worker' + ('s' if worker != 1 else ''))
            ax.set_xlabel('Rows')
            ax.set_ylabel('Microseconds per operation')
    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(handles, labels, loc='upper center', bbox_to_anchor=(0.5, 0.965), ncol=4, frameon=False)
    fig.suptitle('Text tables: latency across row counts' + (' — contended diagnostics' if report['metadata']['contended_diagnostics'] else ''), y=0.995)
    fig.tight_layout(rect=[0, 0, 1, 0.94])
    path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(path, dpi=150, bbox_inches='tight')
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path)
    parser.add_argument('--markdown', type=Path)
    parser.add_argument('--chart', type=Path)
    args = parser.parse_args()
    report = json.loads(args.results.read_text())
    if not report['metadata'].get('measurement_complete', True):
        raise RuntimeError('The measurement matrix is incomplete; do not publish a partial run.')
    content = render(report, args.results.name, args.chart.name if args.chart else None)
    if args.markdown:
        args.markdown.write_text(content)
    else:
        print(content)
    if args.chart:
        plot(report, args.chart)


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Render paired checked/trusted UTF-8 read measurements."""
import argparse
import json
from pathlib import Path

SOURCES = ('Sources: [unchanged Rust workload](../../benches/text_workflow/driver.rs), '
           '[paired runner](../../benches/text_workflow/compare_validation.py), '
           '[fixed-string storage](../../src/table/fixed_string.rs), '
           '[original checked-read storage](https://github.com/kjn-void/gd-rs/blob/d0000f7856cdcb6bc9920dd59e04820477341609/src/table/fixed_string.rs). '
           'This isolates two Rust implementations; C++ is not remeasured here. '
           'The [C++ workloads](../../benches/cpp-reference/text_workflow.cpp) belong to the '
           '[original four-case comparison](text-workflow-results.md).')


def render(report, raw_name, chart_name):
    meta = report['metadata']
    records = report['summary']
    largest = max(r['rows'] for r in records)
    lines = ['# Fixed strings: validate at the write boundary', '',
             'Fixed-buffer reads now trust the UTF-8 validity established by string-typed writes. '
             'The public API still checks bounds, capacity, and nullability. '
             'Raw bytes and descriptors remain private; safe `str` mutation preserves UTF-8. '
             'Three small internal unchecked conversions replace repeated validity scans.', '',
             'The checked-read executable was preserved from the original implementation before '
             'rebuilding. Its workload source is identical to the new executable\'s; transform scratch '
             'copies and per-cell gathering are unchanged. This experiment measures the read change.', '',
             '## Paired measurements', '', SOURCES, '',
             f'Measured {meta["utc"][:10]} on {meta["cpu"]}, `{meta["platform"]}`, '
             f'with `{meta["rustc"].splitlines()[0]}`. Flags: `{meta["rust_flags"]}`. '
             f'Scheduling: {meta["affinity"]}.', '',
             ('**Contended diagnostics: competing host activity was allowed. Alternating order reduces '
              'ordering bias but does not remove contention; close differences and worker scaling '
              'need quiet-host confirmation.**' if meta['contended_diagnostics'] else
              'Rounds interrupted by sustained competing CPU activity were discarded and retried.'), '',
             f'Each configuration has {meta["rounds"]} rounds, alternating executable order. Each process '
             f'records {meta["samples"]} calibrated batches, with a {meta["sample_ms"]} ms minimum target. '
             'Values are medians of process medians. Fixtures, pool creation, and complete output checks '
             'are outside timing; filter and pipeline include allocation and destruction. '
             'Transform measures warmed output capacity; pipeline returns ordered worker shards.', '',
             f'All {len(report["verification"])} before/after output checks matched the independent oracle '
             f'from the original matrix, as did all {len(report["measurements"])} timed processes. '
             'CPU activity was sampled before and after each round. '
             f'{len(report["discarded_rounds"])} rounds were discarded. Boundary samples do not '
             'continuously profile competing activity.', '',
             f'[Raw paired measurements](measurements/{raw_name}) include individual samples, process '
             'ranges, load checks, compiler context, source hashes, binary hashes, and commands. '
             'No source or executable changed during measurement.', '']
    if chart_name:
        lines += [f'![Speedup from trusted fixed-string reads](measurements/{chart_name})', '']
    lines += [f'## At {largest:,} rows', '', SOURCES, '',
              'Milliseconds per operation; lower is faster. Speedup is checked-read time divided by '
              'trusted-read time.', '',
              '| Text bytes | Workers | Operation | Checked reads | Trusted reads | Speedup |',
              '|---:|---:|---|---:|---:|---:|']
    for r in records:
        if r['rows'] == largest:
            lines.append(f'| {r["text_bytes"]} | {r["workers"]} | {r["operation"]} | '
                         f'{r["checked"]["median_ns"] / 1e6:.3f} | '
                         f'{r["trusted"]["median_ns"] / 1e6:.3f} | {r["speedup"]:.2f}× |')
    lines += ['']
    for operation in ['filter', 'transform', 'pipeline']:
        lines += [f'## {operation.capitalize()}: few to many rows', '', SOURCES, '',
                  'Microseconds per operation; lower is faster.', '',
                  '| Rows | Text bytes | Workers | Checked reads | Trusted reads | Speedup |',
                  '|---:|---:|---:|---:|---:|---:|']
        for r in records:
            if r['operation'] == operation:
                lines.append(f'| {r["rows"]:,} | {r["text_bytes"]} | {r["workers"]} | '
                             f'{r["checked"]["median_ns"] / 1000:.3g} | '
                             f'{r["trusted"]["median_ns"] / 1000:.3g} | {r["speedup"]:.2f}× |')
        lines += ['']
    lines += ['## Safety and remaining costs', '',
              'Writes accept valid `&str` or owned strings; incoming byte decoding establishes UTF-8 '
              'before producing these types. Stored lengths select only valid cell prefixes. Unused '
              'slot suffixes can contain stale bytes after shorter replacements and are never borrowed '
              'as text. Copies, append, compaction, and safe mutable string views preserve this invariant. '
              'Each unchecked conversion documents it and retains safe slicing for bounds checks.', '',
              'Short text still requires a separate descriptor and slot rather than inline string '
              'storage. Transforms still copy through scratch, and gathering still copies strings cell '
              'by cell. Those costs are independent of UTF-8 validity scans.', '',
              'Reproduce:', '', '```sh',
              'python3 benches/text_workflow/compare_validation.py \\',
              '  --checked target/text-validation/checked-read' +
              (' --allow-contended' if meta['contended_diagnostics'] else ''), '```', '',
              'See the [benchmark README](../../benches/text_workflow/README.md#utf-8-read-validation-comparison) '
              'for building both executables with matching flags.', '']
    return '\n'.join(lines)


def plot(report, path):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    fig, axes = plt.subplots(1, 3, figsize=(14, 4.1), squeeze=False)
    styles = [(16, 1, '#087f8c', '-'), (16, 8, '#087f8c', '--'),
              (128, 1, '#c44354', '-'), (128, 8, '#c44354', '--')]
    for ax, operation in zip(axes[0], ['filter', 'transform', 'pipeline']):
        for length, workers, color, linestyle in styles:
            data = [r for r in report['summary'] if
                    (r['text_bytes'], r['workers'], r['operation']) == (length, workers, operation)]
            data.sort(key=lambda r: r['rows'])
            ax.plot([r['rows'] for r in data], [r['speedup'] for r in data], color=color,
                    linestyle=linestyle, marker='o', markersize=4,
                    label=f'{length} bytes · {workers} worker' + ('s' if workers != 1 else ''))
        ax.axhline(1, color='#777777', linewidth=1)
        ax.set_xscale('log')
        ax.grid(alpha=0.2)
        ax.set_title(operation.capitalize())
        ax.set_xlabel('Rows')
        ax.set_ylabel('Speedup (checked / trusted)')
        ax.set_ylim(bottom=0)
    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(handles, labels, loc='upper center', bbox_to_anchor=(0.5, 0.93), ncol=4, frameon=False)
    fig.suptitle('Fixed strings: effect of removing UTF-8 validation from reads' +
                 (' — contended diagnostics' if report['metadata']['contended_diagnostics'] else ''), y=0.995)
    fig.tight_layout(rect=[0, 0, 1, 0.83])
    path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(path, dpi=150, bbox_inches='tight')
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('results', type=Path)
    parser.add_argument('--markdown', type=Path, required=True)
    parser.add_argument('--chart', type=Path)
    args = parser.parse_args()
    report = json.loads(args.results.read_text())
    if not report['metadata']['measurement_complete']:
        raise RuntimeError('Do not publish an incomplete measurement matrix')
    args.markdown.write_text(render(report, args.results.name, args.chart.name if args.chart else None))
    if args.chart:
        plot(report, args.chart)


if __name__ == '__main__':
    main()

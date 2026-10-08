#!/usr/bin/env python3
"""Render verified whole-record benchmark reports and figures."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[2]
NAMES = {'gd': 'GD memcpy', 'std': 'C++ STL std::string', 'compact': 'gd-rs CompactString',
         'fixed': 'gd-rs fixed buffer', 'arc': 'gd-rs Arc<Record> (shared)'}
OWNED = ['gd', 'std', 'compact', 'fixed']
COLORS = ['#277da1', '#f8961e', '#43aa8b', '#9b5de5', '#ef476f']
GROUPS = [('All 12 cases', None, None), ('1 worker, 16-byte strings', 1, 16),
          ('1 worker, 128-byte strings', 1, 128), ('8 workers, 16-byte strings', 8, 16),
          ('8 workers, 128-byte strings', 8, 128)]
LINKS = ('[Rust driver](../../benches/filter_copy/driver.rs), [C++ driver](../../benches/cpp-reference/filter_copy.cpp), '
         '[runner/oracle](../../benches/filter_copy/compare.py), [shared-record API](../../src/table/shared_record.rs); '
         'the Arc case has no C++ `shared_ptr` counterpart in this test')


def geomean(values):
    values = list(values)
    return math.exp(statistics.mean(math.log(value) for value in values))


def checked(path):
    data = json.loads(path.read_text())
    metadata = data['metadata']
    if not all(metadata[k] for k in ['measurement_complete', 'gd_source_unchanged', 'source_unchanged', 'binaries_unchanged']):
        raise ValueError(f'Incomplete or changed benchmark: {path}')
    if metadata['rows'] != 1_000_000 or metadata['text_bytes'] != [16, 128] or metadata['workers'] != [1, 8] or metadata['selectivity'] != [10, 50, 90]:
        raise ValueError('unexpected matrix')
    if len(data['verification']) != 60 or len(data['edge_verification']) != 400 or len(data['measurements']) != 60 * metadata['rounds']:
        raise ValueError('missing measurements or checks')
    expected = {(r['text_bytes'], r['workers'], r['selectivity']): (r['count'], r['digest']) for r in data['verification']}
    for r in data['verification'] + data['measurements']:
        if (r['count'], r['digest']) != expected[r['text_bytes'], r['workers'], r['selectivity']]:
            raise ValueError('digest mismatch')
        if 'samples_ns' in r and (len(r['samples_ns']) != metadata['samples'] or any(v <= 0 for v in r['samples_ns'])):
            raise ValueError('invalid sample')
    for r in data['verification'] + data['edge_verification']:
        if r['implementation'] == 'arc':
            if r['independent_target'] or not r.get('shared_records') or not r.get('source_drop_checked'):
                raise ValueError('shared record lifetime check missing')
        elif not r['independent_target']:
            raise ValueError('independent target ownership check missing')
    lookup = {}
    for r in data['summary']:
        key = (r['text_bytes'], r['workers'], r['selectivity'], r['implementation'])
        rounds = [m for m in data['measurements'] if (m['text_bytes'], m['workers'], m['selectivity'], m['implementation']) == key]
        if sorted(m['round'] for m in rounds) != list(range(1, metadata['rounds'] + 1)):
            raise ValueError('duplicated or absent round')
        if not math.isclose(statistics.median(statistics.median(m['samples_ns']) for m in rounds), r['median_ns']):
            raise ValueError('summary differs from raw measurements')
        lookup[key] = r
    expected_keys = {(n, w, p, i) for n in [16, 128] for w in [1, 8]
                     for p in [10, 50, 90] for i in NAMES}
    if set(lookup) != expected_keys:
        raise ValueError('incomplete summary')
    return data, lookup


def relative(lookup, implementation, workers=None, length=None):
    return geomean(lookup[n, w, p, 'gd']['median_ns'] / lookup[n, w, p, implementation]['median_ns']
                   for n in [16, 128] for w in [1, 8] for p in [10, 50, 90]
                   if (workers is None or workers == w) and (length is None or length == n))


def figures(hosts, destination):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.ticker import FuncFormatter, LogLocator, NullLocator
    import numpy as np
    def separate_labels(fig, annotations):
        fig.canvas.draw()
        renderer = fig.canvas.get_renderer()
        placed = []
        for annotation in annotations:
            if not annotation.get_text():
                continue
            for _ in range(12):
                bounds = annotation.get_window_extent(renderer).expanded(1.12, 1.18)
                if not any(bounds.overlaps(previous) for previous in placed):
                    break
                x, y = annotation.get_position()
                annotation.set_position((x, y + 7))
            placed.append(bounds)
    plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 10})
    for slug, label, _, lookup in hosts:
        fig, axes = plt.subplots(2, 2, figsize=(14, 9))
        annotations = []
        for ax, (workers, length) in zip(axes.flat, [(1, 16), (1, 128), (8, 16), (8, 128)]):
            positions = np.arange(3)
            for index, implementation in enumerate(NAMES):
                values = [lookup[length, workers, p, implementation]['median_ns'] / 1e6 for p in [10, 50, 90]]
                bars = ax.bar(positions + (index - 2) * .16, values, width=.15, color=COLORS[index], label=NAMES[implementation])
                annotations.extend(ax.bar_label(bars, labels=[f'{v:.2f}' if v < 10 else f'{v:.1f}' for v in values], padding=3 + index % 2 * 9, fontsize=8))
            ax.set_xticks(positions, ['10%', '50%', '90%'])
            ax.set_title(f'{workers} worker{"s" if workers > 1 else ""}, two {length}-byte strings')
            ax.set_xlabel('Selected source rows')
            ax.set_ylabel('Milliseconds per operation (log scale)')
            ax.set_yscale('log')
            formatter = FuncFormatter(lambda value, _: f'{value:g}')
            ax.yaxis.set_major_locator(LogLocator(base=10, subs=(1, 2, 5)))
            ax.yaxis.set_minor_locator(NullLocator())
            ax.yaxis.set_major_formatter(formatter)
            ax.set_ylim(top=ax.get_ylim()[1] * 1.65)
            ax.grid(axis='y', which='major', alpha=.22)
            ax.set_axisbelow(True)
        handles, labels = axes[0, 0].get_legend_handles_labels()
        fig.legend(handles, labels, loc='lower center', ncol=3, bbox_to_anchor=(.5, .015))
        fig.suptitle(f'{label} — filter and copy one million complete records', fontsize=17)
        fig.text(.5, .94, '3 integers + 2 strings · first four copy payloads; Arc shares records · allocation and cleanup included · contended diagnostics', ha='center', fontsize=10)
        fig.subplots_adjust(top=.87, bottom=.15, hspace=.45, wspace=.22)
        separate_labels(fig, annotations)
        fig.savefig(destination / f'filter-copy-{slug}.png', dpi=180)
        plt.close(fig)
    fig, axes = plt.subplots(2, 3, figsize=(17, 9))
    annotations = []
    positions = np.arange(5)
    for ax, (title, workers, length) in zip(axes.flat, GROUPS):
        for index, (_, label, _, lookup) in enumerate(hosts):
            values = [relative(lookup, implementation, workers, length) for implementation in NAMES]
            bars = ax.bar(positions + (index - 1) * .25, values, width=.24,
                          color=['#277da1', '#f8961e', '#43aa8b'][index], label=label)
            labels = [f'{v:.2f}×' if column or index == 1 else '' for column, v in enumerate(values)]
            annotations.extend(ax.bar_label(bars, labels=labels, padding=3 + index % 2 * 12, fontsize=8))
        ax.axhline(1, color='#666666', linewidth=1, linestyle='--')
        ax.set_title(title)
        ax.set_xticks(positions, ['GD', 'C++\nSTL', 'Rust\ncompact', 'Rust\nfixed', 'Rust Arc\n(shared)'])
        ax.set_ylabel('Geometric mean speed relative to GD')
        ax.set_ylim(0, ax.get_ylim()[1] * 1.17)
        ax.grid(axis='y', alpha=.2)
        ax.set_axisbelow(True)
    axes.flat[5].axis('off')
    handles, labels = axes[0, 0].get_legend_handles_labels()
    axes.flat[5].legend(handles, labels, loc='center', fontsize=13, frameon=False)
    axes.flat[5].text(.5, .13, 'Above 1× = faster than GD on the same host\nEach case receives equal weight.\nArc shares payloads; first four copy them.\nCurrent-load diagnostic measurements.', ha='center', transform=axes.flat[5].transAxes)
    fig.suptitle('Whole-record filter and copy — three hosts, five representations', fontsize=18)
    fig.subplots_adjust(top=.90, bottom=.07, hspace=.37, wspace=.25)
    separate_labels(fig, annotations)
    fig.savefig(destination / 'filter-copy-comparison.png', dpi=180)
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, nargs=3, required=True, metavar='JSON')
    parser.add_argument('--confirmations', type=Path, nargs=3)
    parser.add_argument('--string-layout', type=Path, nargs=3)
    parser.add_argument('--sharing-strategies', type=Path)
    parser.add_argument('--source-snapshot', help='full Git commit containing the measured sources')
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/high-level/filter-copy-results.md')
    args = parser.parse_args()
    hosts = []
    for path, (slug, label) in zip(args.input, [('m3max', 'Apple M3 Max'), ('m6', 'Apple M6'), ('rk3588', 'RK3588 / ARM Linux')]):
        data, lookup = checked(path)
        hosts.append((slug, label, data, lookup))
    for _, _, data, _ in hosts[1:]:
        for key in ['source_sha256', 'gd_source_sha256', 'oracle_sha256']:
            if data['metadata'][key] != hosts[0][2]['metadata'][key]:
                raise ValueError(f'Host sources/oracles differ: {key}')
    snapshot_note = None
    if args.source_snapshot:
        if not re.fullmatch(r'[0-9a-f]{40}', args.source_snapshot):
            raise ValueError('source snapshot must be a full Git commit')
        for name, digest in hosts[0][2]['metadata']['source_sha256'].items():
            source = subprocess.check_output(['git', 'show', f'{args.source_snapshot}:{name}'], cwd=ROOT)
            if hashlib.sha256(source).hexdigest() != digest:
                raise ValueError(f'measured source differs from snapshot: {name}')
        snapshot_note = ('Timed sources are preserved in '
                         f'[commit `{args.source_snapshot[:7]}`](https://github.com/kjn-void/gd-rs/tree/{args.source_snapshot}). '
                         'The published source fingerprints match that commit. Build that snapshot '
                         'to reproduce the measured sources byte for byte.')
    destination = args.output.parent / 'measurements'
    destination.mkdir(parents=True, exist_ok=True)
    for (slug, _, _, _), source in zip(hosts, args.input):
        output = destination / f'filter-copy-{slug}.json'
        if output.resolve() != source.resolve():
            output.write_bytes(source.read_bytes())
    confirmations = []
    if args.confirmations:
        for (slug, label, primary, _), source, primary_path in zip(hosts, args.confirmations, args.input):
            data = json.loads(source.read_text())
            if not data['metadata']['measurement_complete'] or not data['metadata']['source_and_binaries_unchanged']:
                raise ValueError('incomplete confirmations')
            if data['metadata']['source_sha256'] != primary['metadata']['source_sha256']:
                raise ValueError('confirmation sources differ')
            if data['metadata']['primary_sha256'] != hashlib.sha256(primary_path.read_bytes()).hexdigest():
                raise ValueError('confirmation primary fingerprint differs')
            (destination / f'filter-copy-{slug}-confirmations.json').write_bytes(source.read_bytes())
            confirmations.append((slug, label, primary, data))
    if args.string_layout:
        for (slug, _, _, _), source in zip(hosts, args.string_layout):
            (destination / f'filter-copy-{slug}-string-layout.txt').write_bytes(source.read_bytes())
    figures(hosts, destination)
    rankings = [sorted(OWNED, key=lambda i: relative(lookup, i), reverse=True) for _, _, _, lookup in hosts]
    if all(r[:2] == ['gd', 'fixed'] for r in rankings):
        conclusion = '**Among the four independent-copy variants, GD’s whole-row memcpy path has the best overall geometric mean on all three hosts; gd-rs fixed buffers rank second overall on all three.**'
    else:
        conclusion = '**Independent-copy leaders:** ' + ', '.join(f'{label}: {NAMES[ranking[0]]}' for (_, label, _, _), ranking in zip(hosts, rankings)) + '.'
    wins = {i: 0 for i in NAMES}
    for _, _, _, lookup in hosts:
        for n in [16, 128]:
            for w in [1, 8]:
                for p in [10, 50, 90]:
                    wins[min(NAMES, key=lambda i: lookup[n, w, p, i]['median_ns'])] += 1
    conclusion += ' With the shared Arc variant included, primary first-place counts across 36 host/case combinations are: ' + ', '.join(f'{NAMES[i]} {wins[i]}' for i in NAMES) + '.'
    lines = ['# Whole-record filtering into one target', '',
             conclusion, '',
             f'Measured on {hosts[0][2]["metadata"]["utc"].split("T")[0]}. Each source has **1,000,000 records**, with **three `u64` fields and two string fields**. Both strings are exactly 16 or 128 ASCII bytes. A numeric predicate selects exactly 10%, 50%, or 90% of the source. Every implementation returns one ordered target containing all five fields. Runs use one or eight workers.', '',
             'GD copies each matched complete inline row with `std::memcpy`. The STL case uses standard `std::vector` and ordinary `std::string` copy operations. gd-rs is Yoshman’s SoA implementation, tested with native `CompactString` storage and its fixed-buffer string support. These first four variants create independent record and string storage.', '',
             '**The fifth variant uses the new `SharedRecordTable<S>` API, backed by `Vec<Arc<S>>`.** Its one column holds handles to structs containing the same five fields; the strings use `CompactString`. Filtering clones only Arc handles and shares record payloads. The target survives source destruction, and `get_mut` uses copy-on-write when `S: Clone`, but payload cloning on mutation is outside this test. This changes ownership and storage layout, so its speed is not a measure of deep-copy performance. The original dynamic `Table` remains unchanged.', '',
             f'Sources: {LINKS}; [method and commands](../../benches/filter_copy/README.md).', '',
             *([snapshot_note, ''] if snapshot_note else []),
             '**These are diagnostics under each machine’s current background load.** All runs explicitly enabled contended-diagnostic mode; observed load is recorded per host below. Target allocation, filtering, row indices, synchronization, copying, and destruction are timed. Source generation, pool startup, and full verification are outside timing. Arc cleanup decrements handles while the source remains alive, so it does not free record or string payloads. Native optimized builds use LTO without sanitizers. Five rotated process rounds contain seven calibrated batches each; each reported time is the median of the five round medians.', '',
             '## Relative performance', '', f'Sources: {LINKS}. Each ratio is `GD time / variant time` on the same host; **above 1× means faster than GD**. The all-case geometric mean weights all 12 combinations equally. Compiler, standard-library, core topology, and load differences prevent attributing cross-host changes solely to the CPU.', '',
             '| Host | GD memcpy | C++ STL std::string | gd-rs CompactString | gd-rs fixed buffer | gd-rs Arc (shared) |',
             '|---|---:|---:|---:|---:|---:|']
    for _, label, _, lookup in hosts:
        lines.append(f'| {label} | ' + ' | '.join(f'{relative(lookup, i):.3f}×' for i in NAMES) + ' |')
    lines += ['', '![Relative performance on three hosts](measurements/filter-copy-comparison.png)', '',
              '### Four worker/string groups', '', f'Sources: {LINKS}. Each group gives equal weight to the three selection percentages.', '',
              '| Host | Group | GD memcpy | C++ STL | Rust compact | Rust fixed | Rust Arc (shared) |', '|---|---|---:|---:|---:|---:|---:|']
    for _, label, _, lookup in hosts:
        for title, workers, length in GROUPS[1:]:
            lines.append(f'| {label} | {title} | ' + ' | '.join(f'{relative(lookup, i, workers, length):.3f}×' for i in NAMES) + ' |')
    lines += ['', '### Scaling from one to eight workers', '', f'Sources: {LINKS}. Geometric mean of `1-worker time / 8-worker time` over both string sizes and all three percentages. Cleanup is included.', '',
              '| Host | GD memcpy | C++ STL | Rust compact | Rust fixed | Rust Arc (shared) |', '|---|---:|---:|---:|---:|---:|']
    for _, label, _, lookup in hosts:
        lines.append(f'| {label} | ' + ' | '.join(f'{geomean(lookup[n,1,p,i]["median_ns"] / lookup[n,8,p,i]["median_ns"] for n in [16,128] for p in [10,50,90]):.3f}×' for i in NAMES) + ' |')
    for slug, label, data, lookup in hosts:
        m = data['metadata']
        lines += ['', f'## {label}: absolute times', '', f'Sources: {LINKS}; [raw samples and host metadata](measurements/filter-copy-{slug}.json).', '',
                  f'![{label} operation times](measurements/filter-copy-{slug}.png)', '',
                  '| String bytes each | Workers | Selected rows | GD memcpy ms | C++ STL ms | Rust compact ms | Rust fixed ms | Rust Arc (shared) ms |',
                  '|---:|---:|---:|---:|---:|---:|---:|---:|']
        for n in [16,128]:
            for w in [1,8]:
                for p in [10,50,90]:
                    lines.append(f'| {n} | {w} | {p}% ({p*10000:,}) | ' + ' | '.join(f'{lookup[n,w,p,i]["median_ns"]/1e6:.3f}' for i in NAMES) + ' |')
        load = [r['aggregate_cpu_percent'] for r in data['load_samples']]
        lines += ['', f'- Host: `{m["host"]}`; {m["logical_cpus"]} logical CPUs.',
                  f'- Rust: `{m["rustc"].splitlines()[0]}`; C++: `{m["cxx"].splitlines()[0]}`.',
                  f'- Process niceness: {m["process_niceness"]}; OS scheduling without affinity.',
                  f'- Boundary observations of unrelated process CPU use: {min(load):.1f}–{max(load):.1f}% (100% is one core; processes below 5% are excluded).',
                  f'- {len(data["verification"])} million-row source-destruction checks, {len(data["edge_verification"])} edge checks, and {len(data["measurements"])} timed processes. Every ordered digest passed.',
                  '- Source, GD, and executable fingerprints remained unchanged during the run.']
        if 'cpu_topology' in m:
            lines += ['', '```text', m['cpu_topology'], '```']
        if 'board' in m:
            lines += ['', f'Board: {m["board"]}; four Cortex-A76 cores and four Cortex-A55 cores.']
    if confirmations:
        lines += ['', '## Confirmation runs', '', f'Sources: {LINKS}; [confirmation selector and runner](../../benches/filter_copy/recheck.py). Up to three noisy or close cases per host were repeated in five rotated process rounds. An additional case can be selected explicitly to check surprising Arc scaling; each JSON records the selection. **The original measurements remain the basis of every graph and geometric mean.**', '',
                  '| Host | Strings | Workers | Selection | Primary winner | Confirmation winner | Largest variant change |',
                  '|---|---:|---:|---:|---|---|---:|']
        links = []
        for slug, label, primary, data in confirmations:
            for case in data['cases']:
                n, w, p = case['text_bytes'], case['workers'], case['selectivity']
                select = lambda records: {r['implementation']: r['median_ns'] for r in records if (r['text_bytes'], r['workers'], r['selectivity']) == (n, w, p)}
                before, after = select(primary['summary']), select(data['summary'])
                change = max(abs(after[i] / before[i] - 1) for i in NAMES) * 100
                lines.append(f'| {label} | {n} | {w} | {p}% | {NAMES[min(before, key=before.get)]} | {NAMES[min(after, key=after.get)]} | {change:.1f}% |')
            links.append(f'[{label} confirmation samples](measurements/filter-copy-{slug}-confirmations.json): all {len(data["measurements"])} repeated process digests passed.')
        lines += ['', *links, '']
    if args.sharing_strategies:
        sharing = json.loads(args.sharing_strategies.read_text())
        m = sharing['metadata']
        if not m['measurement_complete'] or not m['source_and_binaries_unchanged']:
            raise ValueError('incomplete sharing experiment')
        if m['workers'] != 8 or m['rows'] != 1_000_000 or m['rounds'] != 5 \
                or len(sharing['verification']) != 30 or len(sharing['edge_verification']) != 200 or len(sharing['measurements']) != 150:
            raise ValueError('unexpected sharing experiment matrix')
        for name, digest in m['source_sha256'].items():
            if name in hosts[0][2]['metadata']['source_sha256'] and hosts[0][2]['metadata']['source_sha256'][name] != digest:
                raise ValueError('sharing experiment source differs from primary')
        names = {'baseline': 'Arc two passes', 'fused': 'Arc fused', 'fold': 'Arc fold/reduce',
                 'chunks': 'Arc chunk/concat', 'rc': 'Rc caller construction'}
        lookup = {(r['text_bytes'], r['selectivity'], r['strategy']): r['median_ns'] for r in sharing['summary']}
        if set(lookup) != {(n, p, i) for n in [16, 128] for p in [10, 50, 90] for i in names}:
            raise ValueError('incomplete sharing summary')
        for key, value in lookup.items():
            rounds = [r for r in sharing['measurements'] if (r['text_bytes'], r['selectivity'], r['strategy']) == key]
            if sorted(r['round'] for r in rounds) != [1, 2, 3, 4, 5] or not math.isclose(statistics.median(statistics.median(r['samples_ns']) for r in rounds), value):
                raise ValueError('sharing summary differs from samples')
        expected_sharing = {(r['text_bytes'], r['selectivity']): (r['count'], r['digest'])
                            for r in hosts[0][2]['verification']
                            if r['workers'] == 8 and r['implementation'] == 'gd'}
        if m['oracle_sha256'] != hosts[0][2]['metadata']['oracle_sha256']:
            raise ValueError('sharing oracle fingerprint differs')
        for r in sharing['verification'] + sharing['measurements']:
            if (r['count'], r['digest']) != expected_sharing[r['text_bytes'], r['selectivity']]:
                raise ValueError('sharing digest differs from primary oracle')
        for r in sharing['verification'] + sharing['edge_verification']:
            if r['independent_target'] or not r.get('shared_records') or not r.get('source_drop_checked'):
                raise ValueError('sharing lifetime check missing')
        (destination / 'filter-copy-sharing-strategies-m3max.json').write_bytes(args.sharing_strategies.read_bytes())
        lines += ['', '## Sharing pipelines: M3 Max, eight workers', '',
                  'Sources: [Rust strategy implementations](../../benches/filter_copy/shared_strategies.rs), [Rust fixtures and timing](../../benches/filter_copy/driver.rs), [generation and measurement runner](../../benches/filter_copy/strategies.py). The [C++ driver](../../benches/cpp-reference/filter_copy.cpp) has no matching Arc/Rc pipeline cases.', '',
                  'This separate M3 Max experiment tests the same million records and all three selection rates, with five rotated rounds and seven batches per process. All pipelines share record payloads. Temporary buffers, concatenation and caller-thread cleanup are timed; source generation and pool startup are excluded. It has 30 full source-drop checks, 200 edge checks and 150 timed processes. All ordered digests passed; [raw samples and fingerprints](measurements/filter-copy-sharing-strategies-m3max.json) are retained. These diagnostic samples do not replace the primary three-host tables.', '',
                  'The two-pass Arc path filters positions and then gathers handles. Fused Arc uses the public SharedRecordTable API to filter and clone together. Explicit fold/reduce merges local vectors through a reduction tree; chunk/concat produces one local vector per source chunk before concatenating. The Rc prototype uses standard `Vec<Rc<Record>>`: the caller first borrows record payloads as `&Record`, Rayon filters those references into positions, and the caller constructs the destination by cloning Rc handles. Rc handles never enter a worker or worker result. It is an experimental container, not another gd-rs public table type.', '',
                  '**Speed relative to fused Arc** (`fused time / strategy time`; above 1× is faster). Each case receives equal weight.', '',
                  '| Group | Arc two passes | Arc fused | Arc fold/reduce | Arc chunk/concat | Rc caller construction |',
                  '|---|---:|---:|---:|---:|---:|']
        for title, lengths in [('All six cases', [16, 128]), ('16-byte strings', [16]), ('128-byte strings', [128])]:
            lines.append(f'| {title} | ' + ' | '.join(f'{geomean(lookup[n,p,"fused"] / lookup[n,p,i] for n in lengths for p in [10,50,90]):.3f}×' for i in names) + ' |')
        lines += ['', '| Strings | Selected | Arc two passes ms | Arc fused ms | Arc fold/reduce ms | Arc chunk/concat ms | Rc caller ms |',
                  '|---:|---:|---:|---:|---:|---:|---:|']
        for n in [16, 128]:
            for p in [10, 50, 90]:
                lines.append(f'| {n} | {p}% | ' + ' | '.join(f'{lookup[n,p,i]/1e6:.3f}' for i in names) + ' |')
        lines += ['', 'Rc suffices for caller-owned handles with parallel read-only payload access when the record is Sync. Direct Rayon traversal of `Vec<Rc<S>>`, or transferring worker-local Rc vectors, is rejected because Rc is neither Send nor Sync. Arc supports the fused parallel handle-copy pipeline and permits tables to cross threads when the record is Send + Sync. Avoiding atomic counts with Rc trades them for reference/index buffers and serial destination construction; the complete workflow determines the result.', '']
    lines += ['', '## Interpretation and limits', '', f'Sources: {LINKS}.', '',
              'The independent-copy cases directly exercise GD’s whole-row memcpy advantage: the destination owns the complete record, including both strings. The Arc case instead materializes a vector of shared record handles. All return one ordered target with readable complete records. The benchmark does not measure transforming text, string search predicates, fetching one record into an external struct, NULLs, variable-length distributions, reference-backed GD strings, or database I/O.', '',
              'The eight-worker strategies have different granularity. C++ dispatches eight filter ranges, computes prefix offsets, then dispatches eight disjoint target ranges. The STL destination’s empty string objects are constructed before parallel assignment. Rust’s dynamic Table filters row ranges with Rayon and then calls native column-parallel gather: five columns create five copy tasks, with most string work in two tasks. SharedRecordTable fuses row filtering and Arc cloning in Rayon local buffers, then concatenates those buffers into one target in source order. This concatenation is timed and moves handles without incrementing their reference counts again. One-worker Arc filtering reserves the source count as an upper bound and appends matched handles. Destruction is performed by the calling thread for all variants. These are material implementation costs, with no shared per-row mutex.', '',
              'Arc avoids copying string bytes and allocating target string payloads, but still dereferences separately allocated source records and performs atomic reference-count increments and decrements. Source construction includes one Arc allocation per record and is excluded from the timings. Copy-on-write edits and final payload destruction are also excluded. A C++ `std::vector<std::shared_ptr<Record>>` could use the same sharing approach; it was not included, so these measurements cannot attribute the benefit of sharing to Rust alone.', '',
              'The [untimed representation probe](../../benches/cpp-reference/filter_copy_string_layout.cpp) confirms that the 16-byte payload lives inside each macOS `std::string` object (libc++); Linux’s libstdc++ stores both 16-byte and 128-byte payloads outside the object. Both macOS toolchains use libc++ 220106, while Linux reports libstdc++ 20260321. This changes allocation and cleanup costs. `CompactString` has its own inline representation. Fixed-buffer results include each library’s actual gather implementation and descriptor work, not only a raw byte-copy loop.', '',
              'Verification checks every output value and order against an independent Python oracle. Separate processes destroy the source and recheck both target string columns. Empty, one-row and uneven-range fixtures plus 0% and 100% selection also pass. Local C++ AddressSanitizer checks separately exercise [168 ownership and boundary cases](measurements/filter-copy-asan.json), using the [safety runner](../../benches/filter_copy/check_safety.py); no sanitizer numbers enter the performance tables. Apple ASan memory checks ran with LeakSanitizer disabled because that platform does not support it. The full repository CI script, including Clippy, documentation, both feature configurations and Rust 1.86, passed.', '',
              'These measurements establish behavior for this schema, predicate, ownership requirement and scheduling. They do not establish that AoS or SoA is universally faster. High round-to-round variation should be interpreted with the retained samples and confirmation measurements.', '']
    if args.string_layout:
        lines += ['Untimed probe output: ' + ', '.join(f'[{label}](measurements/filter-copy-{slug}-string-layout.txt)' for slug, label, _, _ in hosts) + '.', '']
    args.output.write_text('\n'.join(lines))
    print(f'Wrote {args.output}')


if __name__ == '__main__':
    main()

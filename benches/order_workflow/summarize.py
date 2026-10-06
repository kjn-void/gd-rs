#!/usr/bin/env python3
"""Render recorded measurements without inventing or averaging speedups."""
import collections
import json
from pathlib import Path
import statistics
import sys


def main(path):
    data = json.loads(Path(path).read_text())
    groups = collections.defaultdict(list)
    memory = collections.defaultdict(list)
    for m in data['measurements']:
        key = (m['rows'], m['stage'], m['workers'], m['index'], m['implementation'])
        groups[key].extend(m['seconds'])
        memory[key].append(m['peak_rss_bytes'])
    simd = any(k[4] == 'cpp_simd' for k in groups)
    rounds, samples = data['metadata']['rounds'], data['metadata']['samples']
    print('# Order workflow: recorded measurements\n')
    print('Sources: [Rust application](../../benches/order_workflow/workload.rs), '
          '[C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), '
          '[GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), '
          '[measurement runner](../../benches/order_workflow/compare.py). '
          'See the [analysis and limitations](order-workflow.md).\n')
    print(f'Times are median milliseconds across {rounds} process rounds, {samples} samples '
          'each, after one warmup per process. Ranges show minimum–maximum samples; they are '
          'not confidence intervals. Ratios divide elapsed times. Peak RSS is the '
          'largest whole-process high-water mark across rounds, including setup.\n')
    if simd:
        print('GD DTO is the existing `table_column_buffer` application. GD SIMD uses '
              '`simd::table_8_8`, the unmodified `gd_table_simd.cpp`, and the counted adapter. '
              'The native-index rows compare all three implementations; sorted changes only '
              'the Rust join index. See [adapter details and limits](order-workflow.md#gd-simd-variant).\n')
    for rows in sorted({k[0] for k in groups}):
        print(f'## {rows:,} order lines\n')
        print('Sources: [Rust](../../benches/order_workflow/workload.rs), '
              '[C++](../../benches/cpp-reference/order_workflow/workload.hpp), '
              '[GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).\n')
        if simd:
            print('| Stage | Workers | Index | Rust ms (range) | GD DTO ms (range) | GD SIMD ms (range) | DTO/Rust | SIMD/Rust | SIMD/DTO |')
            print('|---|---:|---|---:|---:|---:|---:|---:|---:|')
        else:
            print('| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |')
            print('|---|---:|---|---:|---:|---:|---:|---:|')
        for key in dict.fromkeys(k[:4] for k in groups if k[0] == rows):
            _, stage, workers, index = key
            r, c = groups[(*key, 'rust')], groups[(*key, 'cpp')]
            def timing(values):
                return f'{statistics.median(values)*1000:.3f} ({min(values)*1000:.3f}–{max(values)*1000:.3f})'
            if simd:
                s = groups[(*key, 'cpp_simd')]
                print(f'| {stage} | {workers} | {index} | {timing(r)} | {timing(c)} | {timing(s)} | '
                      f'{statistics.median(c)/statistics.median(r):.2f}× | '
                      f'{statistics.median(s)/statistics.median(r):.2f}× | '
                      f'{statistics.median(s)/statistics.median(c):.2f}× |')
            else:
                print(f'| {stage} | {workers} | {index} | {timing(r)} | {timing(c)} | '
                      f'{statistics.median(c)/statistics.median(r):.2f}× | '
                      f'{max(memory[(*key,"rust")])/2**20:.1f} | {max(memory[(*key,"cpp")])/2**20:.1f} |')
        print()
        if simd:
            print('| Stage | Workers | Index | Rust peak MiB | GD DTO peak MiB | GD SIMD peak MiB |')
            print('|---|---:|---|---:|---:|---:|')
            for key in dict.fromkeys(k[:4] for k in groups if k[0] == rows):
                _, stage, workers, index = key
                peaks = [max(memory[(*key, lang)])/2**20 for lang in ['rust', 'cpp', 'cpp_simd']]
                print(f'| {stage} | {workers} | {index} | {peaks[0]:.1f} | {peaks[1]:.1f} | {peaks[2]:.1f} |')
            print()
    print('## Source and executable sizes\n')
    print('Sources: [Rust application](../../benches/order_workflow/workload.rs), '
          '[Rust selection APIs](../../src/table/selection.rs), '
          '[C++ application and adapters](../../benches/cpp-reference/order_workflow/workload.hpp), '
          '[GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), '
          '[size measurement code](../../benches/order_workflow/compare.py).\n')
    print('| Component | Physical lines | Nonblank lines | Bytes |\n|---|---:|---:|---:|')
    for key, size in data['sizes'].items():
        if 'lines' in size:
            print(f'| {key} | {size["lines"]} | {size["nonblank_lines"]} | {size["bytes"]:,} |')
    print('\nLines include comments; formatting differs between languages. The Rust selection '
          'module is counted separately; other library and dependency source is excluded.\n')
    print('| Standalone program | Unstripped bytes | Stripped bytes |\n|---|---:|---:|')
    for lang in (['rust', 'cpp', 'cpp_simd'] if simd else ['rust', 'cpp']):
        size = data['sizes'][lang + '_executable']
        print(f'| {lang} | {size["unstripped_bytes"]:,} | {size["stripped_bytes"]:,} |')
    print('\nExecutables include the application, timing/correctness driver, retained library code, '
          'and SQLite. Rust also uses Rayon and serde_json; C++ uses the counted pool and a small '
          'JSON emitter. Both use system dynamic libraries, listed in the raw JSON. '
          'These are program footprints, not intrinsic table-library sizes.\n')
    print('## Verification and environment\n')
    print('Sources: [fixture and SQL oracle](../../benches/order_workflow/fixture.py), '
          '[Rust verifier](../../benches/order_workflow/driver.rs), '
          '[C++ verifier](../../benches/cpp-reference/order_workflow/driver.cpp).\n')
    print(f'{len(data["verification"])} verification invocations completed: the hand fixture and '
          'each measured size, every implementation at every worker count, plus the Rust sorted-index '
          'diagnostic. Every cell was compared with independent SQL; all output counts and '
          'digests matched across languages, index algorithms, and worker counts.\n')
    for rows in sorted({v['rows'] for v in data['verification']}):
        v = next(v for v in data['verification'] if v['rows'] == rows)
        print(f'- {rows:,} input lines → variant counts `{v["counts"]}`.')
    print('\n```text')
    for key, value in data['metadata'].items():
        print(f'{key}: {value}')
    print('```')


if __name__ == '__main__':
    main(sys.argv[1])

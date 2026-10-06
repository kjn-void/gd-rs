#!/usr/bin/env python3
"""Separate C++ sanitizer diagnostics; never records performance measurements."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

import fixture

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'target/order-workflow'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gd', type=Path, default=ROOT / 'external/gd')
    parser.add_argument('--implementation', choices=['cpp', 'cpp_simd'], default='cpp')
    args = parser.parse_args()
    target = 'gd_order_workflow_simd' if args.implementation == 'cpp_simd' else 'gd_order_workflow'
    OUT.mkdir(parents=True, exist_ok=True)
    records = []
    with tempfile.TemporaryDirectory(prefix='safety-', dir=OUT) as temp:
        db = Path(temp) / 'input.sqlite'
        fixture.generate(db, rows=10000)
        for name, sanitizer in [('asan', 'address,undefined'), ('tsan', 'thread')]:
            build = OUT / name
            with (OUT / f'{name}-build.log').open('w') as log:
                subprocess.run(['cmake', '-S', str(ROOT / 'benches/cpp-reference'), '-B', str(build),
                                '-DCMAKE_BUILD_TYPE=Debug', f'-DGD_SOURCE_DIR={args.gd.resolve()}',
                                '-DGD_ENABLE_SANITIZERS=ON', f'-DGD_SANITIZERS={sanitizer}'],
                               check=True, stdout=log, stderr=subprocess.STDOUT)
                subprocess.run(['cmake', '--build', str(build), '--target', target,
                                'gd_order_workflow_probes', '-j', '4'],
                               check=True, stdout=log, stderr=subprocess.STDOUT)
            for recover in ([False, True] if name == 'asan' else [False]):
                env = os.environ.copy()
                env['ASAN_OPTIONS'] = 'detect_leaks=0' if sys.platform == 'darwin' else 'detect_leaks=1'
                env['UBSAN_OPTIONS'] = f'halt_on_error={0 if recover else 1}:print_stacktrace=1'
                env['TSAN_OPTIONS'] = 'halt_on_error=1'
                command = [str(build / target), str(db), '8', 'verify', '1', 'native']
                completed = subprocess.run(command, text=True, capture_output=True, env=env)
                label = ('cpp_simd-' if args.implementation == 'cpp_simd' else '') + name + ('-recover' if recover else '-failfast')
                (OUT / f'{label}.log').write_text(completed.stderr + completed.stdout)
                findings = re.findall(r'^.*(?:runtime error:|SUMMARY:|ERROR:|WARNING: ThreadSanitizer|FATAL:).*$',
                                      completed.stderr, flags=re.MULTILINE)
                result = {'name': label, 'sanitizers': sanitizer, 'returncode': completed.returncode,
                          'verification_completed': '"verified":true' in completed.stdout,
                          'findings': findings, 'stderr': completed.stderr,
                          'command': command, 'leak_detection': sys.platform != 'darwin'}
                records.append(result)
                print(json.dumps({k: v for k, v in result.items() if k not in ['stderr', 'command']}), flush=True)
            if name == 'asan':
                for probe in ['index-miss', 'name-view']:
                    env['UBSAN_OPTIONS'] = 'halt_on_error=1:print_stacktrace=1'
                    command = [str(build / 'gd_order_workflow_probes'), probe]
                    completed = subprocess.run(command, text=True, capture_output=True, env=env)
                    result = {'name': probe, 'sanitizers': sanitizer,
                              'returncode': completed.returncode, 'stdout': completed.stdout,
                              'stderr': completed.stderr, 'command': command,
                              'verification_completed': False, 'leak_detection': sys.platform != 'darwin',
                              'findings': re.findall(r'^.*(?:runtime error:|SUMMARY:|ERROR:).*$',
                                                    completed.stderr, flags=re.MULTILINE)}
                    records.append(result)
                    print(json.dumps({k: v for k, v in result.items() if k not in ['stderr', 'command']}), flush=True)
    filename = 'cpp_simd-safety.json' if args.implementation == 'cpp_simd' else 'safety.json'
    (OUT / filename).write_text(json.dumps(records, indent=2) + '\n')


if __name__ == '__main__':
    main()

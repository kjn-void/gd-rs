#!/usr/bin/env python3
"""Check the C++ destinations under ASan, outside performance measurements."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('filter_copy', ROOT / 'benches/filter_copy/compare.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/filter-copy/asan/gd_filter_copy')
    parser.add_argument('--oracle', type=Path, default=ROOT / 'target/filter-copy/oracle.json')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/filter-copy/asan.json')
    args = parser.parse_args()
    full = json.loads(args.oracle.read_text())
    # Apple ASan does not support LeakSanitizer. Its memory checks still exercise
    # accesses after freeing the source and every disjoint destination range.
    options = 'detect_leaks=0' if sys.platform == 'darwin' else 'detect_leaks=1'
    records = []
    for rows in [0, 1, 31, 1001, 1_000_000]:
        for length in [16, 128]:
            expected = driver.oracle(rows, length, [0, 10, 50, 90, 100]) if rows < 1_000_000 else full['lengths'][str(length)]
            for workers in [1, 8]:
                for percent in ([0, 10, 50, 90, 100] if rows < 1_000_000 else [90]):
                    for implementation in ['gd', 'std']:
                        command = [str(args.binary), implementation, str(rows), str(length), str(workers), str(percent), '1', '1', 'verify']
                        result = json.loads(subprocess.check_output(command, text=True, env={**os.environ, 'ASAN_OPTIONS': options}))
                        if not result['independent_target'] or any(result[k] != v for k, v in expected[str(percent)].items()):
                            raise RuntimeError(f'Incorrect destination: {command}')
                        records.append({'rows': rows, 'text_bytes': length, 'workers': workers,
                                        'selectivity': percent, 'implementation': implementation, **result})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({'platform': platform.platform(), 'sanitizer': 'AddressSanitizer',
                                      'options': options, 'checks': records}, indent=2) + '\n')
    print(f'ASan ownership/oracle checks passed: {len(records)}', flush=True)


if __name__ == '__main__':
    main()

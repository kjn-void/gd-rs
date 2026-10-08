#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
exec env PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/compare.py "$@"

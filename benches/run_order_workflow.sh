#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
exec python3 "$root/benches/order_workflow/compare.py" "$@"

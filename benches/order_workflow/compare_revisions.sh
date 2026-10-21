#!/usr/bin/env bash
# Compare the Rust order workflow between two gd-rs source trees on one host.
#
# Usage: compare_revisions.sh BASE_TREE CHANGED_TREE [WORKDIR]
#
# Both trees are built with the comparison runner's flags, verified against the SQL
# oracle on a hand-checked fixture and a one-million-line fixture, then timed in two
# rounds that alternate base and changed builds. Each line reports the median of five
# samples and the whole-process peak RSS. Run on an otherwise idle host; on macOS the
# script holds an idle-sleep assertion for its whole duration.
set -euo pipefail
base=$(cd "$1" && pwd)
changed=$(cd "$2" && pwd)
work=$(mkdir -p "${3:-target/order-workflow/revisions}" && cd "${3:-target/order-workflow/revisions}" && pwd)
if [ "$(uname)" = Darwin ] && [ -z "${GD_CAFFEINATED:-}" ]; then
    GD_CAFFEINATED=1 exec caffeinate -i "$0" "$@"
fi
export RUSTFLAGS='-C target-cpu=native' CFLAGS='-march=native'
for tree in "$base" "$changed"; do
    (cd "$tree" && cargo build --release --example order_workflow --features rayon -q)
done
fixture="$base/benches/order_workflow/fixture.py"
db="$work/fixture-1m.sqlite"
small="$work/fixture-hand.sqlite"
[ -f "$db" ] || python3 "$fixture" "$db" --rows 1000000
[ -f "$small" ] || python3 "$fixture" "$small" --small
for tree in "$base" "$changed"; do
    for workers in 1 8; do
        "$tree/target/release/examples/order_workflow" "$small" "$workers" verify 1 native > /dev/null
        "$tree/target/release/examples/order_workflow" "$db" "$workers" verify 1 native > /dev/null
    done
done
echo "verified both trees"
median='import json, statistics, sys
for line in sys.stdin:
    if line.startswith("{"):
        print("%.2f" % (statistics.median(json.loads(line)["seconds"]) * 1e3))
        break'
for round in 1 2; do
    for tree in "$base" "$changed"; do
        label=$([ "$tree" = "$base" ] && echo base || echo changed)
        for case in "prepare 1" "variants 1" "variants 8" "complete 1" "complete 8"; do
            set -- $case
            if [ "$(uname)" = Darwin ]; then
                out=$( { /usr/bin/time -l "$tree/target/release/examples/order_workflow" "$db" "$2" "$1" 5 native; } 2>&1 )
                rss=$(echo "$out" | awk '/maximum resident set size/ {printf "%.1f", $1 / 1048576}')
            else
                out=$( { /usr/bin/time -v "$tree/target/release/examples/order_workflow" "$db" "$2" "$1" 5 native; } 2>&1 )
                rss=$(echo "$out" | awk -F': ' '/Maximum resident set size/ {printf "%.1f", $2 / 1024}')
            fi
            printf '%-7s round=%s %-9s w=%s median_ms=%s peak_rss_mib=%s\n' \
                "$label" "$round" "$1" "$2" "$(echo "$out" | python3 -c "$median")" "$rss"
        done
    done
done

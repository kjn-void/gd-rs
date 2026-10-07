# Text table comparison

Run from the repository root:

```sh
./benches/run_text_workflow.sh
```

The default matrix compares GD AoS bounded inline text, C++ AoS rows containing
ordinary `std::string` fields, gd-rs existing `CompactString` columns, and gd-rs
fixed-capacity column buffers. The STL case uses `std::vector<StringRow>` and
does not wrap `std::string` in a GD table. It uses 32,
100, 1,000, 10,000, 100,000, and 1,000,000 rows, 16- and 128-byte messages,
and one and eight workers. An independent Python oracle checks the complete
ordered output digest, including every string and numeric cell, before timings.

Each table has `id`, `region`, `message`, `output`, and `score`. Region is `north`
every fourth row, the message contains `error` every third row, and score is
`row % 100`. Filtering requires `region == north`, `score >= 20`, and a message
containing `error`, selecting approximately 6.67% of large tables. Transform
rewrites output as uppercase ASCII message plus `|ok`. Pipeline filters, copies
all five columns into independently owned output tables, and transforms them.
Worker shards retain row order and are returned separately; no final merge is
timed. Empty and uneven shards are supported.

The Rust and C++ programs use persistent pools and exactly one disjoint row task
per requested worker. One-worker operations execute directly. Pool construction,
fixtures, and full digest verification are outside timing. Transform measures
repeated writes after output capacity has warmed; filtering and pipeline include
output allocation and destruction. Each process calibrates a batch to at least
25 ms and records five batches. Four independent process rounds rotate
GD/std/compact/fixed through every execution position. Summary values are medians
of the round medians, with round ranges also retained.

```sh
./benches/run_text_workflow.sh --rows 32 1000 --text-bytes 16 128 --workers 1 8
./benches/run_text_workflow.sh --skip-build --rounds 8 --samples 7
./benches/run_text_workflow.sh --wait-for-idle --samples 7 --rounds 4 --sample-ms 50
```

The runner rejects sustained unrelated CPU load above one busy core, confirming
an elevated two-second snapshot with a second sample and retaining both. This
allows brief OS maintenance bursts while detecting ongoing competing jobs.
`--wait-for-idle` waits in 30-second intervals and retries any round whose ending
load check detects substantial competing work. It discards that whole round,
preserves completed quiet rounds, and keeps the discarded samples separately in
the raw JSON. Only accepted rounds contribute to the published summary.
To explicitly measure a
contended diagnostic rather than an unloaded performance baseline:

```sh
./benches/run_text_workflow.sh --allow-contended \
  --output target/text-workflow/contended-results.json
```

Metadata records process CPU time deltas around each round, compiler versions,
native architecture flags, enabled features, CPU topology, scheduling policy,
source hashes, commands, peak process RSS, and a before/after GD source
fingerprint. RSS includes the fixture, verification, runtime, and temporary output;
it is not an isolated table-storage measurement. Fixture build time is also
recorded as a diagnostic and includes language-specific data generation.
`--skip-build` assumes the binaries already use the documented flags. Generated
binaries, logs, and raw output stay under `target/text-workflow`.

GD uses its unmodified `table_column_buffer` with inline bounded string slots,
public variant views and cell writes, `memcpy` numeric reads, and whole-row gather
through its public row-buffer API. This gather is valid for the benchmark's
inline-text, no-null, no-reference schema. gd-rs uses checked typed descriptor
slices or fixed-buffer views and native column-wise `copy_rows`. The four
representations differ in search, case-conversion code, bounds/UTF-8 checks,
allocation, and runtime scheduling as well as physical layout. These measurements
do not isolate language cost or prove a universal AoS/SoA ranking.

Sources: [Rust](driver.rs), [GD](../cpp-reference/text_workflow.cpp),
[runner and oracle](compare.py). The [report](../../docs/high-level/text-workflow-results.md)
records the measured matrix and context.

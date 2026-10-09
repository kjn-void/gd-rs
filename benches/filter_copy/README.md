# Whole-record filter and copy

Sources: [Rust driver](driver.rs), [C++ driver](../cpp-reference/filter_copy.cpp),
[runner and independent oracle](compare.py).

The [M6 comparison](../../docs/high-level/examples/filter-copy-results.md) uses
GD memcpy = 1× in its graphs. See
[Arc scaling and parallel cleanup](#arc-core-scaling-and-parallel-cleanup-on-m6)
for measurement and report-generation commands.

The reports are collected in [high-level/examples](../../docs/high-level/examples/README.md).
The [five-representation illustration](../../docs/high-level/examples/images/m6-comparison-memory-layout.png)
shows AoS row copies, SoA column gathering, fixed text buffers, Arc sharing and
small-string optimization. Regenerate its PNG and editable SVG with
`python3 benches/filter_copy/comparison_layout.py` (Matplotlib required).

## Whole-record protocol

One source contains 1,000,000 rows with exactly five non-null columns: three `u64`
values (`id`, `selector`, `amount`) and two strings (`name`, `message`). Both strings
contain either 16 or 128 ASCII bytes. Every block of 100 rows contains each selector
value once, in a permuted order that changes between blocks. Filtering on
`selector < percentage` selects exactly 10%, 50%, or 90% of the million rows.
The predicate is numeric: this tests copying complete records that contain text.

Each operation creates **one target**, preserves source order, and returns all five
fields. The first four variants copy records into independently owned storage;
the fifth shares records through Arc handles. There are no per-worker target tables
and no untimed merge. Fixtures and persistent worker pools are created outside
timing. Target allocation, filtering, temporary row indices, destination offsets,
copying, synchronization, and target destruction are all timed.

The five representations are:

- GD `table_column_buffer`, with two inline bounded strings. A fresh target copies
  the schema and reserves exactly the matched row count. Workers copy each matched
  **complete row with `std::memcpy`**. This schema has no referenced strings,
  NULL bitmap, or row-status sidecars. Destination metadata is set before dispatch;
  workers write disjoint byte ranges and join before the result is returned.
- C++ standard STL containers: `std::vector<StringRow>` containing three integers
  and two ordinary `std::string` fields. One worker reserves and copy-constructs
  selected rows. Eight workers assign disjoint elements of one sized vector; this
  includes constructing its empty strings before dispatch. Normal string copying
  gives the destination its own payload; raw copying of string objects would not.
- gd-rs native strings (`CompactString`): `Table::copy_rows` with one worker and
  `Table::par_copy_rows` in an eight-thread Rayon pool.
- gd-rs fixed string buffers: the same Table APIs, with a contiguous fixed-slot
  byte buffer and offset/length descriptors for each string column.
- gd-rs shared records: [SharedRecordTable<S>](../../src/table/shared_record.rs)
  stores one column of `Arc<Record>` handles. Each record contains the same three
  integers and two `CompactString` fields. `filter` or Rayon-backed `par_filter_chunked`
  clones matched handles into one target; record and string payloads are shared.
  Source destruction leaves the target valid. `get_mut` uses copy-on-write when
  `S: Clone`; mutation and its payload cloning are outside this benchmark.
  There is **no C++ `shared_ptr` counterpart** in this test.

For C++, parallel filtering and copying are separate joined dispatches over static
row ranges. Counts determine disjoint target ranges; a row mutex is unnecessary.
Rust filters static ranges with Rayon, combines their indices, and uses native
column-parallel gather. With this schema the gather has five column tasks, including
two expensive string tasks. An eight-thread pool does not imply eight simultaneous
string-copy tasks. Arc filtering fuses predicate evaluation and handle cloning into
Rayon local buffers, then concatenates the buffers into one vector in source order.
Each Arc source chunk has a preallocated local vector sized to its source length.
One-worker Arc filtering reserves the source row count as a capacity upper bound.
`par_drop` releases target handles on Rayon workers and joins before returning;
one-worker cleanup is serial. SharedRecordTable is a public typed table.

Arc changes ownership and storage layout. It avoids target payload allocations
and string copying, but adds record pointer dereferences and atomic reference
counts. Its timed destruction decrements target handles while the source stays
alive, so record and string payloads are not freed. Source construction, including
one Arc allocation per record, is excluded. The numbers do not compare deep-copy
costs or prove that sharing is specific to Rust; C++ can use the same approach.

Every process verifies the full ordered target digest outside timing. Independent
Python-generated digests check all cells. Separate verification processes destroy
the source and then recheck the target. The runner also checks empty sources,
one-row sources, uneven ranges, and 0%/100% selection. Rust has an ownership test;
C++ ownership checks can also run under AddressSanitizer, separately from timing.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/compare.py --prepare-oracle
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/compare.py --arc-parallel arc_chunks --rounds 6
```

The M6 comparison uses six rotated process rounds, seven batches per
process, and calibration to at least 50 ms per batch. Results use the median of
the six round medians (each is the median of its seven batches). Pools are persistent, OS scheduling applies, and no CPU
affinity is imposed. Executables are native Release builds with LTO and no
sanitizers. The JSON records source, executable, oracle and GD fingerprints,
compiler versions, host topology, full samples, peak process RSS, and boundary
background-load observations. Peak RSS includes the source and process overhead;
it is not the target's allocation size. `--allow-contended` explicitly labels a run
as diagnostic measurement under the current background load.

Reports and figures are generated with `arc_scaling_summarize.py`. Speed ratios
use the GD time divided by the variant time on the same host. Their geometric
means weight each selection percentage, string size, and worker count equally.

```sh
# Repeat the noisiest or closest three cases; keep the primary results intact.
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/recheck.py \
  target/filter-copy/results.json --output target/filter-copy/confirmations.json

# Optionally add a surprising case, e.g. short-string Arc scaling on the M3 Max.
# Add --case 16 8 10 to that command; this supplements the automatic selection.

# Untimed STL string representation probe, using the measurement compiler.
c++ -O3 -march=native -std=c++20 \
  benches/cpp-reference/filter_copy_string_layout.cpp \
  -o target/filter-copy/string-layout
target/filter-copy/string-layout

# Correctness instrumentation is a separate executable and output directory.
cmake -S benches/cpp-reference -B target/filter-copy/asan \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  -DGD_ENABLE_SANITIZERS=ON -DGD_SANITIZERS=address
cmake --build target/filter-copy/asan --target gd_filter_copy -j 4
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/check_safety.py

```

The optional M3 Max sharing experiment is reproducible with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/strategies.py --allow-contended \
  --output target/filter-copy/sharing-strategies.json
```

Its [Rust implementations](shared_strategies.rs) compare two-pass Arc, fused Arc,
explicit fold/reduce, chunk/concat, and caller-only Rc construction. The generator
reuses `driver.rs` for fixtures, calibration, complete digests and source-drop
checks. Temporary generated crates and executables live under `target`; the root
manifest is unchanged. It uses the root lockfile and cached dependencies in an
offline native Release/LTO build. Every pipeline shares payloads, has one final
ordered target and includes caller cleanup. There is no matched C++ pipeline.
Five rotated rounds produce 150 timed processes, with 30 full ownership checks
and 200 edge checks. These are a separate local experiment.

The report renderer requires Matplotlib and NumPy. On `172.30.30.2`, CMake 4.2.3
and its missing shared libraries were extracted from Ubuntu packages into
`target/filter-copy/tools/prefix`, without changing system packages. Add its
`usr/bin` directory to `PATH` and its `usr/lib/aarch64-linux-gnu` directory to
`LD_LIBRARY_PATH` when building there. Rust is in `/home/kjn/.cargo/bin`.

## Fixed-array SoA versus GD memcpy

Sources: [Rust typed five-column table](fixed_arrays.rs),
[unchanged C++ GD driver](../cpp-reference/filter_copy.cpp),
[paired runner](arrays.py), [report renderer](arrays_summarize.py).

This separate M6 experiment uses the same million rows, two 16/128-byte strings,
10/50/90% selection and 1/8 workers. Rust stores the strings as `Vec<TextCell<N>>`
columns alongside three `Vec<u64>` columns. The destination has the same five
columns and independently owns every copied byte. This is a benchmark-local typed
SoA prototype: the existing dynamic `gd::Table` cannot store fixed byte-array cells.
Each `#[repr(C)]` text cell contains a `u32` metadata word and `[u8; N]`.
The upper 8 bits hold a tag; the lower 24 bits hold the valid length. Lengths are
validated once on construction, then whole cells are copied without validation.
Both copied tags and lengths are checked outside timing. Variable valid lengths
are also tested separately. Text cell strides are 20/132 bytes; total row storage
is 64/288 bytes, versus GD's 72/296 bytes.

Both variants filter static row ranges into temporary index lists, allocate one
destination, and deep-copy disjoint destination ranges in a second joined phase.
Rust uses Rayon and writes all five fields for each selected row; GD uses its
existing full-row `memcpy` path. No worker tables, shared handles or per-cell string
allocations are used. Safe mutable slice splitting partitions the Rust destination;
the documented `set_len` block exposes values only after every slot is initialized.
Spare capacity avoids zeroing the destination before copying. Boundary/ownership
tests also run separately with AddressSanitizer. Both implementations copy a
32-bit metadata word per text cell. GD additionally carries terminators, spare
capacity and alignment; its existing per-cell word stores the valid length.

The paired runner uses four alternating-order process rounds and seven calibrated
50-ms batches per process. Each case is weighted equally. Both variants are
measured together, separately from the Arc comparison.
The [M6 paired report](../../docs/high-level/examples/filter-copy-arrays-m6-results.md)
contains all 12 absolute timings, four group speed ratios, raw data and variability.
The [earlier metadata-free cohort and its matching sources](../../docs/high-level/measurements/archive/filter-copy-arrays-without-metadata/README.md)
are preserved separately and excluded from the current means.

The [memory-layout illustration](../../docs/high-level/examples/images/gd-rust-memory-layout.png)
shows source, filter, copy and destination for GD and gd-rs with constant size fields,
including the text metadata. Its editable SVG is beside the PNG; regenerate both with
`python3 benches/filter_copy/memory_layout.py` (Matplotlib required).

```sh
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arrays.py --prepare-oracle
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arrays.py --allow-contended

# Example separate confirmation; the primary JSON is retained unchanged.
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arrays.py --skip-build \
  --allow-contended --case 16 8 50 --case 128 1 90 \
  --output target/filter-copy-arrays/confirmations.json

python3 benches/filter_copy/arrays_summarize.py \
  --input docs/high-level/measurements/filter-copy-arrays-m6.json \
  --confirmations docs/high-level/measurements/filter-copy-arrays-m6-confirmations.json \
  --layout-probe docs/high-level/measurements/filter-copy-arrays-m6-inline-layout.json
```

## Arc core scaling and parallel cleanup on M6

Sources: [scaling harness](arc_scaling.rs), [scaling runner](arc_scaling.py),
[public shared-record API](../../src/table/shared_record.rs),
[Rust comparison driver](driver.rs), [C++ comparison driver](../cpp-reference/filter_copy.cpp).
The C++ driver has no shared-pointer counterpart.

The [M6 report](../../docs/high-level/examples/arc-scaling-m6-results.md) measures
1, 2, 4, 6, 8 and 12 Rayon workers with the same million-row fixture.
`par_filter_chunked` filters and clones handles into reserved local vectors with
one source chunk per worker, then concatenates those buffers into one ordered table
without cloning handles again. `par_drop` releases handles on the current Rayon
pool and joins before returning. Both are safe, explicit public APIs. At one worker
the benchmark uses serial filtering and cleanup. Arc shares record payloads, and
the source remains alive throughout each timed operation.

The core sweep measures four rounds, with
seven primary batches calibrated to at least 50 ms. It records process CPU time
alongside elapsed time to measure concurrent CPU use, plus seven separate phase
diagnostics for construction and destruction. No phase sample enters the primary
means. Each source chunk contains approximately 125,000 rows at eight workers.

The GD comparison uses `--arc-parallel arc_chunks` and six rotated process rounds.
Every reported implementation passes million-row and boundary/ownership checks.
The noisiest case in each text-size/worker group is repeated, along with the
128-byte, eight-worker, 90%-selection case. The report keeps primary timings
and repeats separate to show variability. Results are current-load diagnostics.
The core sweep additionally checks reference counts before and after target cleanup.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/arc_scaling.py \
  --strategies chunks-par-drop \
  --workers 1 2 4 6 8 12 --rounds 4 --samples 7 --sample-ms 50 \
  --verify-full --allow-contended --output target/arc-scaling/cores.json

PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/compare.py \
  --arc-parallel arc_chunks --rounds 6 --allow-contended \
  --output target/arc-scaling/comparison.json
# Repeat selected cases with --skip-build --case BYTES WORKERS PERCENT.
# Exact selected cases and commands are retained in the confirmation JSON.

python3 benches/filter_copy/arc_scaling_summarize.py \
  --cores docs/high-level/measurements/arc-scaling-m6.json \
  --comparison docs/high-level/measurements/filter-copy-arc-m6.json \
  --confirmations docs/high-level/measurements/filter-copy-arc-m6-confirmations.json \
  --regression docs/high-level/measurements/filter-copy-arc-m6-regression.json \
  --selection docs/high-level/measurements/arc-scaling-m6-selection.json
```

The renderer shows five representations, including gd-rs Arc with chunked filtering
and parallel cleanup. Graphs use GD memcpy as the 1× baseline
(higher is faster). The worker graph uses one and eight workers, where both GD and
Arc were measured; the full Arc-only core sweep appears in a table.

The CPU-clock harness has a separate manifest to keep its `libc` diagnostic
dependency out of the library. Check it in addition to normal repository CI:

```sh
cargo test --locked --offline --manifest-path benches/filter_copy/arc_scaling/Cargo.toml
cargo clippy --locked --offline --manifest-path benches/filter_copy/arc_scaling/Cargo.toml \
  --all-targets -- -D warnings
cargo +1.86.0 check --locked --offline --manifest-path benches/filter_copy/arc_scaling/Cargo.toml
```

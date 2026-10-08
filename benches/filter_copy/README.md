# Whole-record filter and copy

Sources: [Rust driver](driver.rs), [C++ driver](../cpp-reference/filter_copy.cpp),
[runner and independent oracle](compare.py).

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
- gd-rs shared records: the new [SharedRecordTable<S>](../../src/table/shared_record.rs)
  stores one column of `Arc<Record>` handles. Each record contains the same three
  integers and two `CompactString` fields. `filter` or Rayon-backed `par_filter`
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
One-worker Arc filtering reserves the source row count as a capacity upper bound.
The existing dynamic Table is unchanged; SharedRecordTable is a separate public
typed table added for this workflow.

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
PYTHONDONTWRITEBYTECODE=1 python3 benches/filter_copy/compare.py
```

The default measurement uses five rotated process rounds, seven batches per
process, and calibration to at least 50 ms per batch. Results use the median of
the five round medians (each is the median of its seven batches). Pools are persistent, OS scheduling applies, and no CPU
affinity is imposed. Executables are native Release builds with LTO and no
sanitizers. The JSON records source, executable, oracle and GD fingerprints,
compiler versions, host topology, full samples, peak process RSS, and boundary
background-load observations. Peak RSS includes the source and process overhead;
it is not the target's allocation size. `--allow-contended` explicitly labels a run
as diagnostic measurement under the current background load.

Each host has 60 million-row verification cases, 400 small boundary/ownership
checks, and 300 timed process runs. All five representations were measured again
together when adding Arc. The earlier four-variant cohort is retained under
`docs/high-level/measurements/archive/filter-copy-four-variant`; it has different
source fingerprints and is excluded from the current figures and tables. Archived
results describe the earlier source snapshot; use the current five-variant results
for comparisons involving SharedRecordTable.

Host-specific reports and figures are generated with `summarize.py`. Speed ratios
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

python3 benches/filter_copy/summarize.py \
  --input target/filter-copy/m3max.json target/filter-copy/m6.json \
          target/filter-copy/rk3588.json \
  --confirmations target/filter-copy/m3max-confirmations.json \
                  target/filter-copy/m6-confirmations.json \
                  target/filter-copy/rk3588-confirmations.json \
  --string-layout target/filter-copy/string-layout-m3max.txt \
                  target/filter-copy/string-layout-m6.txt \
                  target/filter-copy/string-layout-rk3588.txt \
  --sharing-strategies target/filter-copy/sharing-strategies.json
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
and 200 edge checks. These are a separate local experiment, excluded from the
primary three-host geometric means.

The report renderer requires Matplotlib and NumPy. On `172.30.30.2`, CMake 4.2.3
and its missing shared libraries were extracted from Ubuntu packages into
`target/filter-copy/tools/prefix`, without changing system packages. Add its
`usr/bin` directory to `PATH` and its `usr/lib/aarch64-linux-gnu` directory to
`LD_LIBRARY_PATH` when building there. Rust is in `/home/kjn/.cargo/bin`.

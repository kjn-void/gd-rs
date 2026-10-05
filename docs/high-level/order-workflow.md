# Three-table order workflow: Rust and C++ GD

This benchmark turns the GD author's examples—mixing three tables, taking subsets,
creating parameterized variants, and validating database data—into one reproducible
application. It compares the implementations in this repository against the
**unmodified** C++ GD submodule at `external/gd`, pinned to
`cb11cff90d05260a88d59a30c9da421cd4e19c34`. The C++ applications and
[CMake build recipe](../../benches/cpp-reference/cmake/GdCore.cmake) are maintained
in `gd-rs`; build output and generated forwarding headers stay under `target`.

The implementation sources are the [Rust application](../../benches/order_workflow/workload.rs)
and [C++ application](../../benches/cpp-reference/order_workflow/workload.hpp).
The [full measurement tables](order-workflow-results.md) include sample ranges,
peak memory, source sizes, executable sizes, and environment details.

## Coverage of the GD author's examples

**Both implementations exercise the requested joins, subsets, parameterized
variants, database-data validation, and marking or excluding invalid rows.** The
following maps the author's wording to the concrete operations being measured:

| Author's example | What both applications do | Timed stage |
|---|---|---|
| “mixa data mellan tabellerna, kanske joina ihop tre stycken” | Perform two left joins across order lines, orders, and customers, then combine fields from all three in one 11-column audit table. | `prepare` |
| “plocka ut tårtbitar” | Select subsets of rows and project six columns into separate, independently owned result tables. | `variants` |
| “skapa upp varianter baserat på olika inparametrar” | Produce eight result tables using different region, date-range, status, minimum-amount, and discount parameters. | `variants` |
| “data som är inläst från databas och valideras” | Import three tables from SQLite, then check missing order/customer references, absent or empty customer names, inactive customers, null or nonpositive quantities, null or negative prices, and missing order dates. | `import` + `prepare` |
| “värden som inte är korrekta skall ... markeras” | Retain every order line in the audit table and accumulate error flags identifying the failed checks. | `prepare` |
| “värden som inte är korrekta skall plockas bort” | Copy only rows with no validation errors into a separate clean table; invalid rows are excluded from that result and remain available in the audit table. | `prepare` |

Here, **a variant is a parameterized result table** (*resultatvariant*). The
`variants` time covers the entire batch of eight tables, including filtering,
copying selected columns, calculating discounted amounts, and destroying outputs.
The same eight outputs are produced at every worker count. Import, joining,
validation, and clean-table construction precede this stage and run sequentially;
`complete` measures the whole pipeline.

**The benchmark checks the results of these operations cell by cell.** The
[Rust verifier](../../benches/order_workflow/driver.rs) and
[C++ verifier](../../benches/cpp-reference/order_workflow/driver.cpp) compare the
audit table, clean table, and all eight variants against an
[independent SQL oracle](../../benches/order_workflow/fixture.py), including nulls,
row counts, and row order. This checks both which invalid rows are flagged/excluded
and which values each parameterized subset contains.

## Table schemas and benchmark flow

The diagrams follow the [SQLite fixture schema](../../benches/order_workflow/fixture.py),
[Rust application](../../benches/order_workflow/workload.rs),
[C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), and
[timing runner](../../benches/order_workflow/compare.py).

### SQLite input schema

The three business tables are `customers`, `orders`, and `lines`. The auxiliary
`parameters` table supplies the eight variants and is read outside timing. `UK`
means an actual unique index; the relationship lines show possible lookup matches,
not enforced foreign-key constraints. Missing and null references are deliberate
fixture inputs. Each order/line can match zero or one customer/order respectively.

```mermaid
erDiagram
    customers |o..o{ orders : "id = customer_id"
    orders |o..o{ lines : "id = order_id"

    customers {
        INTEGER id UK "NOT NULL"
        TEXT name "nullable; UTF-8"
        INTEGER region "NOT NULL"
        INTEGER active "NOT NULL; fixture uses 0 or 1"
    }
    orders {
        INTEGER id UK "NOT NULL"
        INTEGER customer_id "nullable; may reference a missing customer"
        INTEGER day "nullable"
        INTEGER status "NOT NULL"
    }
    lines {
        INTEGER id "NOT NULL; unique in generated data"
        INTEGER order_id "nullable; may reference a missing order"
        INTEGER quantity "nullable; includes nonpositive values"
        INTEGER unit_price "nullable; cents; includes negative values"
    }
    parameters {
        INTEGER id "fixture has eight rows, IDs 0 through 7"
        INTEGER region "-1 means any region"
        INTEGER from_day "inclusive lower bound"
        INTEGER to_day "exclusive upper bound"
        INTEGER status "-1 means any status"
        INTEGER minimum "inclusive minimum gross amount in cents"
        INTEGER discount_bp "discount in basis points, 0 through 10000"
    }
```

Every `parameters` column is nullable in the SQLite DDL, although the generated
parameter rows contain no nulls. The drivers require eight parameter rows and
validate their numeric ranges. The default timed fixtures contain 10,000, 100,000,
or 1,000,000 lines. For N lines, the other inputs contain N/5 orders and N/20
customers.
The separate eleven-line correctness fixture has its own hand-authored contents.
All three business tables are imported in SQLite `rowid` order, preserving their
shuffled source order. The native table schemas use nullable `i64` columns and a
nullable UTF-8 string column for `name`.

### Application data flow and output schemas

This is the logical flow of one **`complete` iteration**. The standalone stages
execute only their own part; the timing table below specifies what they reuse.
Each invocation opens its database connection, reads/validates the parameters,
and creates its worker pool before starting any timer.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 400, "rankSpacing": 35}}}%%
flowchart TB
    DB[("Shared SQLite fixture")]
    SETUP["Outside timers: open connection; set read-only query mode<br/>Read eight parameters in ID order; validate ranges<br/>Create persistent pool with 1, 2, 4, or 8 workers"]
    FAIL["Reject invocation<br/>Parameter error or checked arithmetic overflow"]
    DB --> SETUP
    SETUP -. "invalid parameters" .-> FAIL

    subgraph IMPORT["import — sequential"]
        READ["Read customers, orders, lines<br/>SELECT * ORDER BY rowid<br/>Allocate and populate three native tables"]
    end
    SETUP --> READ

    subgraph PREPARE["prepare — sequential"]
        J1["1. Build index on customers.id<br/>Probe with orders.customer_id<br/>Keep order-position to optional customer-position mapping"]
        J2["2. Build index on orders.id<br/>Probe with lines.order_id<br/>Keep line-position to optional order-position mapping"]
        COMBINE["3. For every line in source order<br/>Follow line → order → customer mappings<br/>Missing dimension fields become NULL"]
        VALIDATE["4. Accumulate seven validation flags<br/>Gross amount = quantity × unit_price when both are valid<br/>Otherwise gross amount is NULL"]
        AUDIT["5. Materialize AUDIT — one row per input line<br/>i64: line_id, order_id, customer_id<br/>UTF-8: name<br/>i64: region, day, status, quantity, unit_price<br/>i64: amount_cents, errors"]
        CLEAN["6. Materialize CLEAN — copy rows where errors = 0<br/>Same 11 columns as AUDIT, in the same order<br/>Independent owned values; source row order preserved"]
        J1 --> J2 --> COMBINE --> VALIDATE --> AUDIT --> CLEAN
    end
    READ --> J1
    VALIDATE -. "gross amount overflow" .-> FAIL

    subgraph VARIANTS["variants — eight tasks on the persistent worker pool"]
        DISPATCH["Share CLEAN read-only<br/>One task per parameter row; each task owns its destination"]
        FILTER["Each task scans CLEAN in source order<br/>Region matches or parameter = -1<br/>from_day ≤ day &lt; to_day<br/>Status matches or parameter = -1<br/>gross amount ≥ minimum"]
        PROJECT["Copy selected rows and six columns<br/>line_id, name, region, day, status, amount_cents"]
        DISCOUNT["Replace each copied amount with<br/>floor((gross × (10000 - discount_bp) + 5000) / 10000)<br/>Checked i64 arithmetic; round nonnegative cents half up"]
        OUTPUTS["Wait for all tasks: eight independent VARIANT tables<br/>i64: line_id; UTF-8: name<br/>i64: region, day, status, amount_cents<br/>Retain all eight until the batch completes"]
        DISPATCH --> FILTER --> PROJECT --> DISCOUNT --> OUTPUTS
    end
    CLEAN --> DISPATCH
    SETUP -. "parameters and worker pool" .-> DISPATCH
    DISCOUNT -. "multiply or add overflow" .-> FAIL
    OUTPUTS --> DESTROY["complete: destroy eight outputs, AUDIT, CLEAN,<br/>and all three imported tables before stopping the timer"]
```

The audit column order is exactly the order shown. `CLEAN` shares that schema;
each of the eight variants has the six-column projected schema shown. Native
schemas retain nullable capability even when a result contains no nulls. Audit
`errors` is always populated; `amount_cents` is gross in audit/clean and discounted
in variants. Validation flags mark rows; arithmetic overflow instead aborts the
invocation. The seven flag meanings and conditional checks are specified in
[What the application actually does](#what-the-application-actually-does).

The join arrows above describe the **actual execution order**: order-to-customer
mapping first, then line-to-order mapping. They do not represent two fully
materialized intermediate joined tables. Rust normally builds hash indexes;
C++ builds sorted indexes and checks candidate equality for missing keys. The
Rust `sorted` diagnostic substitutes a sorted index for the same operations.
Only the eight variant tasks run concurrently, with immutable input and separate
output ownership as described in
[the GD concurrency requirements](#what-the-application-must-enforce-for-concurrent-gd-use).

### Exact timing boundaries

Sources: [Rust driver](../../benches/order_workflow/driver.rs) and
[C++ driver](../../benches/cpp-reference/order_workflow/driver.cpp).

| Measured stage | Prepared once outside the timed loop | Work inside each timed iteration, including destruction |
|---|---|---|
| `import` | Open database connection, parameters, worker pool | Import and destroy all three native input tables |
| `prepare` | Common setup plus imported input tables | Build both indexes and mappings; validate and construct audit/clean; destroy indexes, mappings, audit, and clean |
| `variants` | Common setup plus imported inputs and prepared audit/clean | Scan the reusable clean table eight times, filter/project/copy, apply discounts, wait for all tasks, then destroy all eight outputs |
| `complete` | Open database connection, parameters, worker pool | Import, prepare, produce all eight variants, and destroy every input/intermediate/output table |

The input-row-count query is also outside timing. Each process executes one
warmup iteration of its selected stage, discards that duration, and then records
five samples by default. Input tables stay alive across `prepare` samples;
input and prepared tables stay alive across `variants` samples. A new worker pool
is created for each process and reused for all its iterations.

### Verification and measurement orchestration

The SQL views are a correctness oracle used outside timing; the measured joins
and filtering execute in the Rust/C++ table applications. The
[runner](../../benches/order_workflow/compare.py) runs one implementation process
at a time. The following is the default full comparison, excluding the separate
sanitizer and focused preparation-repeat runs.

```mermaid
%%{init: {"flowchart": {"wrappingWidth": 400, "rankSpacing": 35}}}%%
flowchart TB
    BUILD["Build optimized Rust and C++ programs"]
    HANDFIXTURE["Generate the shared 11-line hand fixture<br/>Include expected_audit, expected_clean, expected_variants SQL views"]
    FIXTURE["Generate shared 10k, 100k, and 1M line fixtures<br/>Include the same independent SQL oracle views"]
    REJECT["Outside timing: both programs must reject<br/>gross overflow, discount overflow, and invalid discount"]
    DATASET["Take next dataset: hand case, then increasing sizes"]
    VERIFY["Outside timing: both languages at 1, 2, 4, and 8 workers<br/>Also Rust sorted-index diagnostic at one worker<br/>Verify every output cell, NULL, row count, and order against SQL<br/>Check ownership; compare counts/digests across implementations<br/>Require matching runtime SQLite versions"]
    HAND{"11-line hand fixture?"}
    CASE["Take next stage / index / worker case<br/>import: 1 worker<br/>prepare: 1 worker, native and sorted<br/>variants and complete: 1, 2, 4, 8 workers"]
    R1["Round 1: Rust process, then C++ process<br/>Each: one discarded warmup, then five timed samples"]
    R2["Round 2: C++ process, then Rust process<br/>Each: one discarded warmup, then five timed samples"]
    MORECASE{"More cases for this dataset?"}
    MOREDATA{"More datasets?"}
    REPORT["Report timing samples and medians, peak process RSS,<br/>source/executable sizes, versions, and source fingerprints"]
    BUILD --> HANDFIXTURE --> REJECT --> FIXTURE --> DATASET --> VERIFY --> HAND
    HAND -- "yes: correctness only" --> MOREDATA
    HAND -- "no" --> CASE --> R1 --> R2 --> MORECASE
    MORECASE -- "yes" --> CASE
    MORECASE -- "no" --> MOREDATA
    MOREDATA -- "yes" --> DATASET
    MOREDATA -- "no" --> REPORT
```

## Performance summary

On an Apple M3 Max (12 performance + 4 efficiency cores), both implementations
produced the same correct outputs at 10,000, 100,000, and 1,000,000 lines. Rust was
faster in each measured stage in these builds. C++ had a smaller executable and
lower peak memory in the single-worker complete-workflow case. Neither result
establishes that one language or library is universally better.

For **1,000,000 order lines**, median time across ten recorded samples:

| Stage | Workers | Rust | C++ GD | C++ / Rust time |
|---|---:|---:|---:|---:|
| import | 1 | 149.76 ms | 210.07 ms | 1.40× |
| prepare | 1 | 181.59 ms | 310.76 ms | 1.71× |
| variants | 1 | 63.71 ms | 185.73 ms | 2.92× |
| variants | 8 | 14.97 ms | 70.74 ms | 4.73× |
| complete | 1 | 373.80 ms | 678.30 ms | 1.81× |
| complete | 8 | 327.82 ms | 578.53 ms | 1.76× |

Sources: [Rust](../../benches/order_workflow/workload.rs),
[C++](../../benches/cpp-reference/order_workflow/workload.hpp), and
[all samples](measurements/order-workflow-m3max.json).
The in-memory preparation diagnostic using sorted indexes on both sides measured
**209.95 ms Rust / 296.25 ms C++**.
The sorted-index adapter reduces Rust's advantage, but a substantial gap remains.
This diagnostic still includes each implementation's validation and materialization
costs; it does not isolate storage layout or compiler code generation.

Going from one to eight workers sped up the fixed variant batch by
**4.26× in Rust** and
**2.63× in C++**. The complete workflow improved by
only **1.14×** and **1.17×**, respectively. Only variant production is
parallel, and the eight tasks have unequal output sizes. Scaling is not monotonic
in every measured case. This measures parallel work under application-enforced
ownership and synchronization. GD does not provide a thread-safe shared mutable
table here.

Complete-workflow throughput was **2.68 million input lines/s Rust versus 1.47
million C++** with one worker, and **3.05 versus 1.73 million** with eight workers.
This normalizes by original order-line count per completed workflow; each workflow
also imports the customer/order tables and produces all eight variants. It is not
a count of individual cell operations or output rows.

Single-worker complete-process peak RSS was **823.7 MiB Rust / 670.2 MiB C++**.
At eight workers it was **837.2 / 720.2 MiB**. C++'s packed nullable storage is a
potential contributor, but RSS also includes retained tables, allocator behavior,
SQLite caches, and the driver; these numbers do not isolate storage efficiency.

The Rust application is **175 lines / 5,862 bytes**, plus **155 lines / 6,125 bytes**
of reusable library APIs. C++ application and adapters are **133 lines /
6,578 bytes**. Lines include comments and reflect different formatting; Rust's
application has fewer bytes despite more lines. Library implementation size is
reported separately from the application and harness/test sizes. Stripped standalone
programs are **2,384,272 bytes Rust / 1,325,104 bytes C++**. This includes their
respective runtimes, retained libraries, SQLite, and verification/timing code.


## Preparation timing repeat

Sources: [Rust application](../../benches/order_workflow/workload.rs),
[C++ application](../../benches/cpp-reference/order_workflow/workload.hpp),
[shared timing helper](../../benches/order_workflow/compare.py), and
[repeat samples](measurements/order-workflow-prepare-repeat.json).

The million-line preparation cases were repeated because the main run had wide
ranges, especially for Rust's sorted-index diagnostic. The repeat used the same
binaries, a freshly generated and fully verified million-line fixture, one worker,
and two alternating process rounds with five samples each after a warmup.

| Index | Rust median (range) | C++ median (range) | C++ / Rust time |
|---|---:|---:|---:|
| native | 169.31 ms (156.63–173.03) | 295.58 ms (273.17–307.70) | 1.75× |
| sorted | 189.24 ms (176.82–206.40) | 294.85 ms (279.85–319.84) | 1.56× |

The direction of the result persists; exact stage times vary between runs. These
repeat measurements supplement the main tables rather than replacing their slower
samples. A separate full-dataset repeat was excluded because unrelated .NET/Xcode
builds started during timing. The focused repeat records process-load snapshots
before and after each invocation and did not encounter that competing build load.

To reproduce the focused repeat, build and generate a million-line fixture as
described in the [runner instructions](../../benches/order_workflow/README.md), then
run both drivers in `verify` mode. For each index mode (`native`, `sorted`), invoke
`DATABASE 1 prepare 5 INDEX` in Rust, C++, C++, Rust order. Each driver supplies its
own warmup. The [Rust driver](../../benches/order_workflow/driver.rs) and
[C++ driver](../../benches/cpp-reference/order_workflow/driver.cpp) are the same
executables used for the main comparison.

## What the application actually does

The [fixture generator and independent SQL oracle](../../benches/order_workflow/fixture.py)
create one SQLite file used by both programs. At the largest size it contains
50,000 customers, 200,000 orders, and 1,000,000 order lines. Smaller fixtures keep
those proportions. Insertion order is shuffled with a fixed seed; even-numbered
keys and deliberately missing odd-numbered keys exercise genuine failed lookups,
including misses between existing keys. Repeated foreign keys give many lines per
order and orders per customer; dimension IDs are unique.

| Stage | Required observable result |
|---|---|
| Import | Three native tables containing all source values, including UTF-8 names and nulls |
| Prepare | Left join lines → orders → customers; materialize an 11-column audit table with error flags; materialize a separate clean table |
| Variants | Produce eight independently owned, six-column tables using region, half-open day ranges, status, inclusive minimum amounts, and discounts |
| Complete | Import, prepare, produce all eight outputs, and destroy intermediate/output tables |

The audit columns are `line_id`, `order_id`, `customer_id`, `name`, `region`, `day`,
`status`, `quantity`, `unit_price`, `amount_cents`, and `errors`. Joins retain every
line, including unmatched rows. Their missing dimension fields are null. The
original order ID remains visible even when the order cannot be found.

Validation accumulates a bitmask: missing order (1), missing customer for an
existing order (2), absent/empty name for an existing customer (4), inactive
customer (8), null/nonpositive quantity (16), null/negative price (32), and missing
day for an existing order (64). Parent absence does not manufacture additional
child-field errors. Clean rows have zero flags and are physically copied into a
new table; they are not just hidden behind deletion markers.

Amounts use signed 64-bit integer cents. A gross amount exists only when quantity
and price are valid. Variant amounts use `(gross * (10000 - discount_bp) + 5000) /
10000`, rounding nonnegative amounts to the nearest cent, with ties rounded up.
Both applications check multiplication/rounding overflow and reject invalid
parameter ranges. Overflow aborts the benchmark invocation rather than silently
wrapping or generating a partially valid output; a production importer would need
an explicit recovery/error-reporting policy. Generated ordinary inputs stay well
inside the arithmetic limits.

Each variant selects six columns: line ID, name, region, day, status, and amount.
All output strings are owned by their result tables. Variant zero selects every
clean row; other variants select different fractions. The same fixed batch of eight
variants is executed at 1, 2, 4, and 8 workers. Work is not multiplied by worker count.

## Correctness and the meaning of validation

Both [Rust verification](../../benches/order_workflow/driver.rs) and
[C++ verification](../../benches/cpp-reference/order_workflow/driver.cpp) compare
**every output cell**, null, row count, and row position against independently
written SQL. This is done before timing every dataset, at every worker count.
Canonical digests then confirm that all eight outputs also match across languages,
worker counts, and the Rust sorted-index diagnostic. A checksum alone is not the
oracle.

An eleven-line fixture has hand-calculated error masks and discounted totals. It
covers missing keys, nulls, combined errors, zero amounts, rounding, empty outputs,
an inclusive minimum amount, the inclusive lower day boundary, and the exclusive
upper day boundary. Verifiers destroy imported tables before inspecting prepared
results, mutate one variant while checking the source and siblings, and destroy
the prepared tables before inspecting remaining outputs. This exercises ownership
rather than just identical arithmetic.

The [public Rust API tests](../../tests/table_selection.rs) additionally cover
many-to-many duplicate expansion, empty joins, nulls, deleted rows, metadata,
UTF-8, duplicate/reordered selections, bounds/type failures, and independence after
mutation. A property test compares indexed joins with a simple nested-loop oracle
on randomized nullable keys. Many-to-many fanout is tested for correctness but is
not part of the performance dataset, which has unique dimension keys.

The runner also checks that both applications reject gross overflow, discounted
intermediate overflow, and an out-of-range discount. These are extension boundary
checks, separate from the timed ordinary data.

## How the implementations differ

Rust uses `gd::Table` with typed column vectors. The reusable
[selection module](../../src/table/selection.rs) provides `select_rows`, `filter_rows`,
`project`, combined `select`, and `left_join_rows`. Predicate filtering skips deleted
rows; explicit row/column selection preserves deletion flags and metadata.
Projection/gathering copies selected columns directly instead of rebuilding every
row through dynamic values. The native join index is a hash index. Rayon supplies
the persistent worker pool.

C++ uses **`gd::table::table_column_buffer` (also named `gd::table::dto::table`)**,
its SQLite cursor, sorted integer indexes, and native `harvest` for copying selected
rows/columns. This is GD table storage throughout; it is not a substitute vector-of-
structs implementation. Variable-length names use `rstring`. The
`eTableFlagDuplicateStrings` option disables repeated deduplication searches and
allows independent copied strings. Nullable fields use GD's null bitmap. The
[small persistent C++ pool](../../benches/cpp-reference/order_workflow/pool.hpp)
assigns one variant per task and is counted separately as harness code.

The C++ index needs an application-side equality check after `find`: the pinned
[index implementation](../../external/gd/source/gd_table_index.cpp) returns success
for any non-end `lower_bound` candidate, even when the requested key is absent.
Removing the check would associate invalid orders/customers with the next larger
key. That workaround is visible in `Join`, counted in application size, and
included in timing. GD's existing `join_s` scans for the first matching row; the
benchmark instead uses its sorted index to avoid quadratic joining at scale.

This is **not** a benchmark of GD's separate `gd::table::table` member-table class
or its shared-schema lifecycle. The chosen DTO class directly supports projected
harvesting and disabling string deduplication. Conclusions about other GD table
classes would require another implementation. Likewise, short names that fit
Rust's compact strings do not represent all long-string/blob workloads.

## What went well, and what did not

Both implementations successfully express the requested workflow and produce
identical results. GD's runtime schemas, nullable cells, cursor adapter, and
`harvest` are useful building blocks. Its packed rows use less space per nullable
integer than Rust's `Option<i64>` columns. The C++ application uses GD's published
APIs. Concurrent readers with separately owned outputs produced correct results in
both implementations under the application protocol described below.

Rust provides predicate filtering, combined projection/gathering, and indexed
join selection, with documented contracts and tests. The size comparison reports
the selection module separately from the application. The application can express
selection and reuse an index without manual lifetime management. Nullable column
storage and materialized intermediates have memory costs. The tested pipeline
uses eager materialization; it has no query optimizer, streaming execution, packed
validity bitmaps, or lazy results.

GD required care around index semantics, explicit null handling, reference-string
storage options, and independent ownership across workers. Both applications still
use positional column numbers. A reordered schema can therefore introduce a
business-logic bug even if the program compiles. Both implement validation rules
in application code; neither library knows what an inactive customer or invalid
price means.

## What the application must enforce for concurrent GD use

The GD table used here is **not an internally synchronized, thread-safe shared
container**. The measured result is that the application can organize concurrent
reads of a fully prepared table and writes to separate tables. It does not show
that GD makes concurrent access safe automatically. The locks, task ownership,
publication, and completion barriers come from the
[application's worker pool](../../benches/cpp-reference/order_workflow/pool.hpp).

The application must maintain all of these conditions:

1. **Finish construction before publishing inputs.** Import, schema construction,
   joins, validation, and clean-table construction finish on the caller thread.
   The pool publishes the input and parameter pointers under its mutex; workers
   acquire that mutex before using them. Passing a raw pointer alone would not
   establish the required ordering.
2. **Freeze the entire input for the whole batch.** No thread may edit cells,
   null flags, schemas, names, row counts, storage, or referenced values, or append,
   remove, reserve, compact, move, or destroy that table while workers read it.
   A `const Table&` in one function does not stop another alias from doing so.
3. **Give each output exactly one writer.** Each task constructs its own table,
   schema, and string storage, using `harvest` to copy values from the source.
   An atomic task counter assigns each preallocated result slot to one worker.
   The result vector is not resized, and workers never append to a shared GD table.
   Different row numbers alone are insufficient permission to mutate one table:
   operations can touch shared allocation, counts, null metadata, and string storage.
4. **Keep every borrowed view's owner alive and unchanged.** Cell `variant_view`s
   and string views borrow source storage. They are consumed while that storage is
   alive; the selected values are copied into each destination. The application
   retains the clean table, parameters, and result slots until every worker has
   finished, including when a worker reports an exception.
5. **Synchronize completion before inspecting or reusing results.** Workers report
   completion through the pool mutex/condition variable. `Run` waits for all tasks
   before returning results or rethrowing an exception. Only one caller may use a
   pool instance at a time; overlapping `Run` calls or destruction during a call
   violate this harness's contract.
6. **Keep database work outside the worker batch.** The GD SQLite connection and
   cursor are used by the caller thread. SQLite's own threading mode does not make
   GD tables, cursors, borrowed cells, or application lifetimes thread-safe.

These requirements follow from the actual operations. GD's
[`harvest`](../../external/gd/source/gd_table_column-buffer.cpp) reads the source
and calls unsynchronized `row_add` on the destination; its table buffers, row
counts, and column vectors are ordinary mutable state. The
[string/binary reference counter](../../external/gd/source/gd_table.h) is a plain
`int` incremented/decremented without atomics. The separate member-table class
also has a [plain integer schema reference counter](../../external/gd/source/gd_table_column.h).
Consequently, distinct wrapper objects are not by themselves evidence that their
shared backing storage or reference-count operations can be used concurrently.
That class's `set_locked()` sets a sentinel that disables reference counting; it
is **not a mutex**, does not synchronize schema access, and leaves deletion timing
to the caller. Its shared-schema lifecycle is not exercised by this benchmark.

For a shared mutable GD table, the application would need external synchronization
covering every conflicting read/write, shared backing object, and borrowed view's
use, or a design that transfers exclusive ownership. Locking only the call that
returns a view and then using the view after unlocking is insufficient if another
thread can invalidate it. `eTableFlagDuplicateStrings` is a storage policy, not a
thread-safety switch. This benchmark uses independent destinations and the
specific read/copy paths above; it does not establish that every `const` GD method
is safe to call concurrently.

ThreadSanitizer reported no race for this protocol on the tested run. That is
limited evidence about the **application's protocol**, not a thread-safety
certification for GD. In safe Rust, the shared borrows used by Rayon and the owning
result types enforce several of these restrictions at compile time; changing to
shared mutation still requires an explicit synchronization design.
These concurrency conditions do not resolve the GD undefined behavior reported below.

## Safety and extension risks

Sources: [sanitizer runner](../../benches/order_workflow/check_safety.py),
[focused C++ probes](../../benches/cpp-reference/order_workflow/probes.cpp),
[application code](../../benches/cpp-reference/order_workflow/workload.hpp), and
[recorded diagnostics](measurements/order-workflow-safety.json).

The following findings were reproduced against the unchanged pinned GD source:

| Check | Observed result | Interpretation |
|---|---|---|
| Ordinary 10,000-line pipeline, ASan + UBSan, fail-fast | Aborted at an unaligned `uint16_t` store in `gd_table.cpp:63` | The pipeline is not free of detected undefined behavior |
| Same pipeline, UBSan recovery enabled | Full SQL verification completed; four alignment UB sites were reported; no ASan address error was reported | Correct observed results do not remove the UB |
| Eight-worker pipeline, ThreadSanitizer | Completed full verification with no reported data race | Supports this particular immutable-input/owned-output pattern, not arbitrary GD concurrency |
| Raw GD index probe | Searching for 20 in keys 10 and 30 reported a hit on row 1 | Confirms the missing-key workaround is necessary |
| Column name supplied as a valid two-byte `string_view` without a trailing NUL | ASan aborted on a heap-buffer-overflow read in `names::add` | Adding names from slices/substrings can introduce a memory-safety bug unless the extra readable byte requirement is addressed |

The four observed alignment sites are table-name storage
([`gd_table.cpp:63`](../../external/gd/source/gd_table.cpp)), table-column name access
([`gd_table_column-buffer.h:300`](../../external/gd/source/gd_table_column-buffer.h)),
the shared name accessor ([`gd_table.h:774`](../../external/gd/source/gd_table.h)), and
database-record name storage
([`gd_database_record.cpp:46`](../../external/gd/source/gd_database_record.cpp)).
The separate column-name probe reaches a `memcpy` that copies the view's length
**plus one byte** from its source; a `string_view` does not promise that byte exists.
The ordinary application uses NUL-terminated literal column names, so that specific
buffer over-read is an extension probe rather than a measured-workload failure.

Sanitizers were run in separate debug builds, never for published timing numbers.
The GD core and C++ application were instrumented; the separate SQLite C target
was not instrumented, so these checks do not cover the entire native dependency stack.
Leak detection is unsupported by this macOS ASan runtime and was disabled; no claim
about leak freedom follows. TSan ran at eight workers on 10,000 lines, not every
size/schedule. No exploitability assessment was performed. Because UB exists on
the actual C++ pipeline, its release measurements describe observed behavior on
this machine, not a guarantee of portable behavior under all optimizations.

Both release applications rejected the two overflow cases and an invalid
discount. Rust stopped with a checked-operation panic; C++ returned a caught
exception. This is fail-fast behavior, not graceful per-row recovery. The complete
Rust repository checks passed, including formatting, Clippy, all-features and
minimal-features tests, documentation, and the Rust 1.86 MSRV check.


The Rust application and selection module contain no `unsafe` blocks. Borrowed table/index APIs prevent
mutation while references are in use, and parallel outputs are independently owned.
That is a useful protection against lifetime mistakes and data races in safe Rust,
not a security certification for the full dependency stack: SQLite is native code,
and dependencies may contain unsafe implementations. The test results do not prove
absence of vulnerabilities in either program.

Returned row positions are snapshots in **both** implementations. Rust's join
result is an owned vector of positions; it does not keep either table borrowed.
Removing or compacting rows and then reusing positions can still cause a logical
error. Selection also preserves row extras/properties: projection is not a data-
redaction boundary.

| Extension | Rust | C++ GD in this application |
|---|---|---|
| Another region/date/discount variant | Add a parameter row; existing pipeline and SQL oracle cover it | Same |
| Another predicate or output column | Compose selection/projection; bounds and runtime cell types are checked | Extend predicate loop and `harvest` column list; preserve null/type assumptions |
| Duplicate dimension keys | Library join expands all matches, but this application's order-to-customer mapping assumes unique dimension IDs and must change | Sorted lookup returns one row; adapter and application must change to handle fanout |
| Larger monetary values | Checked arithmetic fails explicitly; decide whether overflow should become a row flag | Application helpers provide equivalent checks; GD does not add them automatically |
| More workers | Rayon owns scheduling; shared borrows and owned outputs enforce aliasing restrictions | Application must enforce publication, immutable input, single-writer outputs, borrowed-data lifetimes, and completion barriers; GD supplies none of these protections |
| Sharing mutable state | Requires an explicit safe synchronization design | Unsynchronized conflicting access is a data race; an external locking/ownership protocol must cover table operations, shared backing state, and outstanding views |

For extending this particular pipeline, I would favor the Rust version: ownership,
borrowed indexes, checked table access, and reusable selection contracts remove
several ways to introduce memory/lifetime bugs. C++ remains compact and capable,
but correctness depends more on preserving adapter checks and GD-specific
preconditions. That judgment is based on the implemented operations and observed
failures, not a controlled study of developer productivity.

Neither language prevents a wrong predicate, rounding rule, or column mapping.
The independent SQL oracle and boundary fixtures are what make those extensions
reviewable. Shorter source code alone does not establish ease of maintenance.

## Measurement boundaries and reproducibility

Run from the repository root:

```sh
git submodule update --init external/gd
./benches/run_order_workflow.sh
python3 benches/order_workflow/summarize.py target/order-workflow/results.json
python3 benches/order_workflow/check_safety.py
./scripts/ci.sh
```

See [runner documentation](../../benches/order_workflow/README.md) for individual
checks and smaller runs. The [recorded raw measurements](measurements/order-workflow-m3max.json)
retain all samples, commands, versions, flags, source fingerprints, verification
counts/digests, failure checks, memory readings, and linked libraries.

Performance uses optimized assertions-off builds without sanitizers. Both target
the native CPU and use LTO. Each case has two separate process runs in alternating
language order, one warmup per process, and five recorded samples per process.
Tables, output allocations, index construction in `prepare`, and destruction are
timed. Fixture creation, correctness checks, database connection opening, parameter
loading, and thread-pool creation are excluded. Repeated `variants` runs reuse the
prepared clean table and do not rebuild joins or indexes. All eight outputs are
retained until the batch completes.

Import reads the same file through warm OS/database caches; it is not a cold-disk
I/O test. The complete workflow is database rows through materialized outputs,
not process launch latency. Peak RSS is a whole-process high-water mark including
setup and retained inputs; it is not the incremental allocation of a single stage.
No CPU affinity was set. Heterogeneous cores, scheduling, caching, allocation, and
the uneven sizes of the eight tasks affect scaling. Additional workers cannot
accelerate the serial import/prepare phases in this implementation.

Both measured programs use **SQLite 3.53.2**: Rust through bundled `rusqlite`
**0.40.2 / libsqlite3-sys 0.38.2**, C++ through the hash-pinned amalgamation in
the maintained CMake recipe. The runner records the runtime versions and rejects
a mismatch before timing. Fixture creation uses Python SQLite **3.53.4**, outside
timing, to produce the single input file consumed by both programs.

Matching the engine version removes one confounder, but SQLite compile-time
options and adapters are not identical: rusqlite enables additional extensions
and API armor, while CMake uses the amalgamation defaults. Import and complete
timings compare these actual software stacks. Prepare and variants isolate the
in-memory workload from SQLite. Compiler versions also differ. This does not
isolate a pure language effect or establish either library's optimal possible
implementation.

The workload validates the specified joins, error policies, selections, ownership,
parallel result equivalence, and their observed costs at these sizes. It does not
establish universal speed, general SQL/query-engine performance, ease of use for
all applications, concurrent mutation safety, or security against arbitrary
malicious databases.

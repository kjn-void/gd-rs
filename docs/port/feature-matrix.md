# GD and gd-rs feature matrix

Source audit of the current gd-rs worktree and its pinned
[`external/gd`](../../external/gd) submodule at
`cb11cff90d05260a88d59a30c9da421cd4e19c34`, performed on 2026-10-05.

GD is the broader toolkit: it includes extensive table-copy
operations, aggregate helpers, SQL construction, ODBC, and application utilities.
gd-rs adds capabilities and contracts that are absent from GD's corresponding
APIs: concurrent row collection, checked disjoint mutable views, lifetime-bound
indexes, duplicate-expanding left joins, and schema-driven JSON table import.
Neither is a drop-in replacement for the other.

This compares the exported Rust API with the C++ source, including facilities outside
the benchmark's build. **Native** means an implementation exists, not that every
overload or platform was tested. **Partial** means a narrower contract or a known
implementation problem. **Application** means callers must compose operations or supply
another library. **Absent** means no corresponding library API was found; it does not
mean the language cannot implement it. Where gd-rs deliberately leaves an integration to
the application, the matrix names that choice instead of treating it as a missing core
feature. These external crates are not bundled or re-exported by gd-rs. Source links
beneath each matrix identify the evidence. This is an API/source audit, not a new
performance run, exhaustive compilation of GD, or security certification.

## Values and arguments

| Capability | GD | gd-rs |
|---|---|---|
| Null, boolean, signed/unsigned 8–64-bit integers, 32/64-bit floats | Native tagged values | Native `Value` / `ValueRef` variants |
| Text, binary data, UUID values | Native | Native |
| Wide strings and distinct narrow/UTF string representations | Native `wchar_t` strings and separate type tags | Partial: one UTF-8 text type; UTF-16 input can be decoded, but is not a stored value kind |
| Opaque pointer as a dynamic value | Native `void*` variant | Absent intentionally |
| Owned values and zero-copy borrowed values | Native; caller maintains backing-storage lifetime | Native; Rust lifetimes enforce the borrow |
| General runtime conversion by destination type | Native `convert_to` / conversion tags | Partial: checked numeric helpers and explicit schema converters; no equivalent general coercion engine |
| Ordered arguments with duplicate names and positional entries | Native | Native |
| Argument lookup by name, occurrence, or position | Native | Native, including `ArgumentIndex` |
| Packed argument byte layout and raw-buffer access | Native | Absent: ordinary owned Rust values |
| Reference-counted argument storage | Native `shared::arguments` | Application: ordinary ownership/`Arc`; no packed shared-argument API |
| Compile-time prevention of argument-index invalidation by mutation | Absent | Native borrowing `ArgumentIndex<'a>` |

Sources: [GD values](../../external/gd/source/gd_variant.h),
[GD views](../../external/gd/source/gd_variant_view.h),
[GD arguments](../../external/gd/source/gd_arguments.h),
[GD shared arguments](../../external/gd/source/gd_arguments_shared.h),
[GD argument index](../../external/gd/source/gd_arguments_index.h),
[Rust value kinds](../../src/value/data_type.rs),
[Rust values](../../src/value/owned.rs),
[Rust arguments](../../src/arguments.rs).
Type names in GD's larger type enumeration are not counted as proof of complete
date/time, arbitrary-precision decimal, array, or object value support.

## Table storage, selection, and mutation

GD has multiple table classes. The rows below name DTO-specific features where
appropriate; they do not imply every GD table class has identical behavior.

| Capability | GD | gd-rs |
|---|---|---|
| Typed columns, named schema, aliases, nullable cells | Native | Native `Schema`, `ColumnSpec`, `Table` |
| Table properties and row-local extra named fields | Native; table-class/tag dependent | Native properties and `UnknownFields::Store` |
| Packed row storage, cell addresses, layout control | Native | Absent: typed column vectors, no compatible packed ABI |
| Borrow a whole numeric column as a contiguous typed slice | Application: row-strided storage; `harvest<T>` copies values | Native `Column::as_slice<T>` for required columns and `as_nullable_slice<T>` for nullable columns |
| Borrow checked, disjoint input/output columns for mutation | Application; pointer aliasing is caller responsibility | Native `columns_io`, `column_pair_mut`, typed mutable slices |
| Insert/read/update a row or cell | Native | Native, with typed errors for width/type/nullability/bounds failures |
| Allocate empty rows and fill later | Native; payload initialization depends on tags | Absent equivalent: inserted Rust rows must be valid complete rows |
| Rename existing column metadata | Native DTO `column_rename` | Application: construct a new schema/table; no rename API |
| Select row ranges, row lists, and columns | Native `harvest`, range-copy and related operations | Native owned copies and borrowed `TableSelection` views |
| Filter rows using application business rules | Native scans/callbacks plus harvest; equality `find_all` helpers | Native `select_rows`, materializing `filter_rows`, and borrowed `filter_view` |
| Append another table with column mapping or matching names | Native DTO `append`, including conversion overloads | Native `append`, `append_named`, `append_mapped`; destination converters and atomic batch validation |
| Copy values into existing rows by column/name mapping | Native DTO `plant` | Application: setters or typed column views |
| Split into multiple owned tables by row count | Native DTO `split` | Application: repeated `copy_range` |
| Deleted-row markers without immediately moving rows | Native row-state metadata; operation/tag dependent | Native tombstones, restore, live views and live-row counts |
| Arbitrary application row-state flags and free-slot reuse | Native row state and `find_first_free_row` | Partial: tombstones only; no general state-bit API or free-slot allocator |
| Remove tombstones and obtain old-to-new row positions | Application: caller tracks remapping | Native `compact` returns `RowCompaction` |
| Arbitrary row deletion with immediate physical shifting | Native `erase` | Partial: tombstone plus compact; `pop_row` removes the last row |
| Named per-column conversion callback used during insertion/import | Partial: built-in conversion tags, application validation | Native `ColumnConverter` attached to the schema |

Sources: [GD DTO API](../../external/gd/source/gd_table_column-buffer.h),
[GD DTO implementation](../../external/gd/source/gd_table_column-buffer.cpp),
[GD member table](../../external/gd/source/gd_table_table.h),
[GD argument table](../../external/gd/source/gd_table_arguments.h),
[Rust table](../../src/table.rs), [Rust schema](../../src/table/schema.rs),
[Rust views](../../src/table/views.rs), [Rust selection](../../src/table/selection.rs), [Rust borrowed views](../../src/table/selection_view.rs),
[Rust append](../../src/table/append.rs),
[Rust compaction](../../src/table/compaction.rs).
Selection returns snapshots, borrowed views, or owned copies; gd-rs does not expose a general
lazy dataframe/query pipeline. Neither schema automatically validates application
rules such as permitted regions or positive quantities: callers supply those rules.

## Indexes, joins, sorting, and aggregates

| Capability | GD | gd-rs |
|---|---|---|
| Single-column equality index | Native sorted integer/string indexes; see exact-miss caveat below | Native hash index for booleans, integer widths, strings, bytes, UUIDs |
| Two-column composite index | Native `index_composite<T1,T2>` and two-column `create_index_g`; extraction supports strings/integer conversions | Native `CompositeIndex<N>` over arbitrary positive fixed-width tuples |
| Get all duplicate matches directly from an index | Partial: index lookup returns one row; scans or direct index traversal needed for all | Native `ColumnIndex::rows` and `CompositeIndex::rows` return every matching position |
| Index explicit null/deleted row positions separately | Application | Native `null_rows` and `tombstoned_rows`; deleted rows excluded from key buckets |
| Index whose lifetime prevents table mutation/invalidation | Absent; caller preserves storage and rebuilds after relevant changes | Native borrowing `ColumnIndex<'a>` and `CompositeIndex<'a, N>` |
| Pairwise join helper | Partial: DTO `join_s` returns only the first right match for each matched left row | Native single/composite joins return every duplicate match and `None` for unmatched left rows; lazy borrowed iterators are available |
| Defined left-join null/deletion/type policy | Application around GD primitives | Native: nulls do not match, deleted rows are excluded, key types must match exactly |
| Join three tables and materialize an application result | Application, demonstrated in order workflow | Application composing pairwise joins, demonstrated in order workflow |
| Physically reorder rows by a column | Native DTO selection/bubble sort and null sorting | Application: materialize a `RowOrder`; no in-place table sort API |
| Stable borrowed sort order without moving table cells | Application | Native `RowOrder`, explicit null placement and float total order |
| Multi-column sort / SQL-style grouping pipeline | Application | Application |
| Column minimum, sum, null/non-null counts | Native aggregate implementations; not exercised by this audit | Application over column slices/iterators; no aggregate facade |
| Median, percentile, distinct values/count, substring count | Native aggregate bodies; distinctness uses string conversion, not typed key equality | Application |
| Average, variance, standard deviation | **Partial/broken source:** see caveat below | Application; no named aggregate API |

Sources: [GD indexes](../../external/gd/source/gd_table_index.h),
[GD index implementation](../../external/gd/source/gd_table_index.cpp),
[GD join/sort implementation](../../external/gd/source/gd_table_column-buffer.cpp),
[GD aggregates](../../external/gd/source/gd_table_aggregate.h),
[Rust index](../../src/table/index.rs), [Rust composite index](../../src/table/composite.rs), [Rust join](../../src/table/selection.rs),
[Rust ordering](../../src/table/ordering.rs).

GD's integer/string index `find` uses `lower_bound` without verifying equality;
an absent key can therefore report the next larger key as a match. The composite
index's `find` **does** check equality. The existing
[workflow report](../high-level/order-workflow.md) records the scalar-index probe
and the C++ application's equality check.

GD's DTO `join_s` is not a general relational inner join: it calls
`find_variant_view` once per left row and omits additional right-side duplicates.
It is also not a left outer join. The benchmark's C++ implementation supplies its
own joining logic rather than relying on this helper.

In the pinned aggregate header, the core `average` overload is declared without a
definition found in the source tree. `variance<TYPE>` dispatches to
`variance<int64_t>` or `variance<double>` in the same implementation, without a
calculation/base case. `std_deviation` calls that template without specifying its
non-deducible `TYPE`. These source defects prevent treating the three names as
working statistics support; this audit did not instantiate every other aggregate.

## Concurrency and safety contracts

| Capability / guarantee | GD | gd-rs |
|---|---|---|
| Multiple threads append complete rows to a shared collector | No synchronized table-append API found; application locking or per-thread tables | Native `ConcurrentTableBuilder::push_row*(&self)` |
| Convert concurrent collection into a dense table | Application merges worker results | Native consuming `into_table` / `append_to` |
| Parallel mutation of disjoint row ranges | Application proves disjointness, storage stability and metadata safety | Native `RowsMut::split_at`; optional Rayon row mutation |
| Parallel table copying | Application threads and ownership discipline | Native `par_copy_range` / `par_copy_rows` with `rayon` feature |
| Share immutable table data across readers | Possible under application-enforced lifetime/publication/no-mutation rules | Native shared borrowing / `Arc<Table>` under Rust's type contracts |
| Unsynchronized arbitrary mutation of an ordinary shared table | **Not supported safely** | **Not supported**; ordinary mutation requires exclusive access |
| Prevent dangling value/row/index views at compile time | No | Yes through borrowing for the safe API |
| Release-build checked table type/bounds/nullability failures | Partial; many raw access paths rely on assertions and caller preconditions | Typed errors/optional accessors on checked APIs; slice indexing still has Rust panic behavior |

Sources: [GD reference counters](../../external/gd/source/gd_table.h),
[GD shared columns](../../external/gd/source/gd_table_column.h),
[Rust concurrent collector](../../src/table/concurrent.rs),
[Rust mutable rows](../../src/table/row_mut.rs), [Rust views](../../src/table/views.rs).

**GD's table classes are not internally synchronized multithread-safe containers.**
Before sharing them, the application must safely publish fully initialized inputs,
keep their tables/schemas/string storage alive, prevent concurrent writes and
reallocation, and coordinate all operations that touch shared metadata or reference
counts—including relevant copy/destruction paths. Concurrent writers need suitable
external synchronization or genuinely independent storage. Merely assigning
different logical rows to threads does not prove the supporting storage is independent.
GD's reference counters here are ordinary integers. The member-schema `set_locked`
sets a reference-count sentinel; it is not a mutex.

Rust's concurrent builder is a staging collector, not a concurrent mutable `Table`:
it publishes complete validated rows, assigns positions by concurrent insertion
order, and transposes rows into columns when consumed. While producers run, its
count reports the completely published contiguous prefix. Returned plain position
vectors from selection/join are not lifetime guards and can become stale after
compaction or removal. Rust's stronger contracts do not establish freedom from
logic bugs, dependency bugs, resource exhaustion, or unsafe-code defects.

Existing sanitizer findings and the limited scope of the read-only threaded
benchmark are documented in the [workflow report](../high-level/order-workflow.md).
No new sanitizer or performance results are claimed by this matrix.

## Formats, database access, and expressions

| Capability | GD | gd-rs |
|---|---|---|
| Arguments to JSON / URI query parameters | Native, different treatment of unnamed/duplicate JSON fields | Native; JSON rejects ambiguous unnamed/duplicate fields; URI preserves duplicates |
| Tables to CSV / JSON | Native multiple layouts/options; source defects in some paths | Native fixed formats; skips tombstones |
| Per-cell output callbacks, selected-row output, JSON layout options | Native table I/O overloads | Partial: fixed serializers plus sorted/borrowed-selection output; richer layouts/callbacks require application formatting |
| CSV into a typed table | Native `read_g` into a prepared DTO table | Native `table_from_csv` with explicit schema |
| JSON array of objects into a typed table | Application; shallow JSON-object parser exists, but no table importer found | Native `table_from_json`, schema aliases/converters and unknown-field policy |
| Standalone shallow JSON object into arguments | Native `parse_shallow_object_g` | Absent direct arguments importer |
| Binary table/schema/buffer serialization | Native DTO `serialize` family; raw layout format | Absent; `BinaryReader`/`Writer` are primitives, not table persistence |
| Table to SQL INSERT text | Native `write_insert_g` | Application: use SQLite parameter binding or a separate SQL formatting layer |
| SQL query builders and value/template formatting | Native query/builder/value modules | Application: SQL construction is a separate layer; gd-rs supplies SQLite parameter binding |
| SQLite open/execute/query and table loading | Native database/cursor and table bridge | Native optional `sqlite` adapter, enabled by default |
| Generic database interface and ODBC driver | Native | Application selects other drivers directly; gd-rs provides SQLite only |
| Streaming database cursor API | Native C++ cursor | Partial: Rust wrapper materializes tables; native rusqlite access via `connection()` / `into_connection()` |
| Checked SQLite binding mode/count/name and integer-range policy | Driver/wrapper-specific, not the same contract | Native typed errors for mixed/missing/extra/duplicate bindings and out-of-range `u64` |
| Compile once, evaluate repeatedly with changing variables | Native custom expression compiler/runtime | Native Rhai-backed `Program` and `ExpressionContext` |
| GD token/code representation and original expression syntax | Native | Absent compatibility; Rhai syntax and AST instead |
| Register application functions | Native callback/function registry | Native through Rhai `inner_mut()` |
| Default execution-operation, nesting and collection limits | No corresponding bounded wrapper policy found | Native default 1,000,000 operations, 64 call levels, depth 64, 1 MiB strings, 1,000,000-element arrays/maps |
| Preserve every GD scalar type through expression evaluation | Runtime-specific | Partial: integers normalize to `i64`, floats to `f64`, UUID to text; excessive `u64` and non-scalar output rejected |

Sources: [GD table I/O](../../external/gd/source/gd_table_io.h), [GD JSON
parser](../../external/gd/source/parse/gd_parse_json.h), [GD SQL
builder](../../external/gd/source/gd_sql_query_builder.h), [GD
SQLite](../../external/gd/source/gd_database_sqlite.h), [GD
ODBC](../../external/gd/source/gd_database_odbc.h), [GD expression
runtime](../../external/gd/source/expression/gd_expression_runtime.h), [Rust
formatting](../../src/format.rs), [Rust import](../../src/format_import.rs), [Rust
SQLite](../../src/sqlite.rs), [Rust expressions](../../src/expression.rs). The
[compatibility document](compatibility.md) details current format defects and
intentionally different semantics. Neither Rust CSV nor JSON table output is a complete
round-trip archive of schema, properties, extras, and deletion state.

## Text, binary, and surrounding toolkit

| Capability | GD | gd-rs |
|---|---|---|
| UTF validation/conversion, JSON-string escaping, percent encoding, XML escaping | Native utility families | Native focused helpers; not the whole GD utility surface |
| Escaped splitting, character prefixes, control-character trimming | Native utility equivalents | Native helpers |
| Binary search/hex encoding/endian-aware primitive I/O | Native | Native checked `BinaryReader` / `BinaryWriter` and free functions |
| Base64 encode/decode/validation | Native `gd_translate` | Application: use a Base64 codec crate directly |
| URI parsing, pattern matching, format-string and line parsing helpers | Native `parse/` modules | Application: use URI/parser crates and standard formatting directly; percent-component decoding is not a URI parser |
| Custom strings, string collections, vectors and arenas | Native toolkit types | Standard Rust containers; application selects specialized storage when needed |
| CLI argument/options parser | Native `gd_cli_options` | Application: gd-rs assumes use of `clap` directly |
| Logging/printers/macros and file rotation | Native `gd_log_*`, `gd_file_rotate` | Application selects its logging sink/appender; optional instrumentation belongs at that boundary |
| File/path helpers and archive/repository streams | Native `gd_file`, `io/` modules | Application: use `std::fs`, `std::path`, `std::io` and archive crates directly |
| Console styling/printing and keyboard helpers | Native `console/`, `io/gd_io_keyboard` | Application selects terminal crates such as `crossterm` / `indicatif`; gd-rs provides table debug formatting |
| COM-style interfaces and command/server routing | Native `gd_com`, `com/gd_com_server` | Application: use Rust traits and `Arc` for routing/ownership; actual COM interoperability needs a separate binding |
| Standalone math/algebra/string-math utilities | Native `math/` modules | Application: use standard numeric operations or a suitable numerical crate directly |
| Public packed SIMD table API | Partial: `gd_table_simd.h` contains a literal `...` function-body placeholder | Absent corresponding API; SIMD inside dependencies is not that feature |

Sources: [GD text](../../external/gd/source/gd_utf8.h), [GD
binary](../../external/gd/source/gd_binary.h), [GD
Base64](../../external/gd/source/gd_translate.h), [GD source
tree](../../external/gd/source), [GD SIMD
header](../../external/gd/source/gd_table_simd.h), [Rust public
exports](../../src/lib.rs), [Rust text](../../src/text.rs), [Rust
binary](../../src/binary.rs). Toolkit entries establish source availability, not
portability or complete runtime validation of every subsystem. The application choices
follow the [port's scope decisions](porting-plan.md#scope); they are not features
implemented inside gd-rs.

## Practical gaps and extension effort

For a table-processing application, the most concrete remaining missing gd-rs
conveniences are mapped writes into existing rows (`plant`), aggregate functions,
multi-column ordering, direct column renaming, and richer output options. Composite
indexes, mapped append, and borrowed selections are native APIs. Remaining
table conveniences can be added without copying GD's packed storage design. ODBC,
SQL construction, CLI/logging, and archive frameworks are separate scope decisions,
not small table API omissions.

GD can implement the same application workflows, but matching gd-rs's guarantees
requires additional work: validated JSON import, complete duplicate/null-aware
left-join semantics, synchronized row publication, lifetime management, and
index-invalidation discipline. These responsibilities must remain correct when
the application is extended; a successful immutable-input benchmark does not make
them library guarantees.

The [order workflow](../high-level/order-workflow.md) demonstrates the common
application workload: loading database tables, joining three tables, marking and
excluding invalid rows, taking slices, and generating parameterized variants.
Its measurements apply to those concrete implementations. Feature breadth alone
does not establish which library is faster for a new workload.

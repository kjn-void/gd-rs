# Compatibility decisions

This file records intentional differences from the current C++ behavior, reviewed on
2026-10-05 against gd-rs `f7c7b91` and GD submodule
`cb11cff90d05260a88d59a30c9da421cd4e19c34`. Rust contracts are covered by current tests.
C++ observations are grounded in the pinned source; that tree has no unit-test
directory. Maintained workflow probes cover the paths linked in the [workflow
report](../high-level/order-workflow.md), not every C++ observation below.

| Area | C++ observation | Rust contract | Status |
|---|---|---|---|
| Dynamic values | manually tagged owned/view layouts | `Value` and `ValueRef<'a>` sum types | implemented |
| Text ownership | allocation flag and pointer conventions | `CompactString` / borrowed `&str` | implemented |
| Arguments | packed ordered buffer; duplicate/unnamed entries | ordered `Vec<Argument>`; duplicates and unnamed preserved | implemented |
| Argument index lifetime | offsets and string views can become stale after mutation | immutable borrow prevents mutation | implemented |
| Table layout | packed row-major fixed buffer plus references | typed vectors per column | implemented |
| Table input conversion | call-site `tag_convert` attempts broad runtime coercion | strict by default; named per-column converters opt in at schema boundaries | intentional API difference; implemented |
| Table properties | mutable argument-backed property bag | uniquely named insertion-ordered `Arguments` owned by each table | implemented |
| Unknown table fields | argument-backed tables redirect unknown names to per-row dynamic storage | strict by default; `UnknownFields::Store` enables lazy owned row extras | implemented |
| Row status | packed per-row state words with caller-managed use/deleted flags and optional `tag_meta` scanning | lazily allocated tombstone flags at stable physical positions; live views, indexes, ordering, and formatting exclude them, while cell access and diagnostics stay physical; `compact` removes tombstoned rows and maps old positions, and freed slots are never reused (`find_first_free_row` has no counterpart) | intentional API difference; implemented |
| Empty null-enabled row | cells marked non-null with uninitialized fixed payloads | null must be explicit; non-null columns reject null | intentionally rejected |
| Column name lengths | unaligned `uint16_t` pointer casts | normal string containers | C++ defect retained; Rust avoids it |
| Scalar table index miss | lower-bound result accepted without equality; composite lookup checks equality | exact equality required | C++ source defect; Rust exact |
| Composite indexes and joins | two-column composite lookup checks equality; DTO join returns first matching right row only | positive-width `CompositeIndex<N>`; single/composite left joins expand duplicates, exclude deleted rows, and never match nulls | implemented; not a general SQL join engine |
| Mapped table append | DTO append by positions/names, with optional coercion | positional/named/explicit mapping; destination converters, batch validation before mutation, preserved extras/tombstones | implemented; stricter contract |
| Row and column selection | harvest and copy operations | owned selections plus borrowing `TableSelection` / `SelectedRow`; direct selected JSON/CSV output | implemented |
| Concurrent collection and mutation | no general internally synchronized table API; callers manage storage, lifetimes, publication and non-atomic reference counts | synchronized complete-row builder; shared immutable reads; checked disjoint mutable partitions; ordinary table mutation stays exclusive | implemented; callbacks and locks remain application responsibilities |
| Binary cursor failure | cursor clamps but error remains unobservable | typed error; failed operation is atomic | C++ source defect; Rust implemented |
| Binary floating point | endian read numerically converts integer bits | exact IEEE-754 bit transfer | C++ source defect; Rust implemented |
| Empty hex input | rejected by validator | valid encoding of an empty byte sequence | intentional difference |
| Byte substring search | naive scalar scan | `memchr::memmem` semantics | implemented |
| UTF-8 validation | exact-end multibyte sequence rejected; pointer traversal | `std::str::from_utf8`; text APIs accept `&str` | defect rejected; implemented |
| C-string conversion wrappers | code-point count used as byte length, truncating tails | one length-safe `&str` entry point | defect rejected |
| JSON string encoding | fragment overloads disagree; astral values truncated | complete `serde_json` string literal with round trip | intentional API difference |
| URI component encoding | project-specific allow-list; `+` decodes as space | same allow-list and plus rule; strict errors | implemented |
| XML escaping | five predefined entities | same entities; borrowing fast path | implemented |
| Escaped split of empty text | no parts | one empty part, matching `str::split` | intentional difference |
| Table row sorting | destructive selection/bubble sort, O(n²) | stable borrowed permutation, O(n log n) | intentional API difference; implemented |
| Argument JSON object | unnamed omitted; duplicate members allowed | unnamed and duplicate names are errors | intentional losslessness policy |
| Argument URI | stale escape buffer corrupts fields | independent percent-encoded pairs; duplicates preserved | C++ source defect; Rust implemented |
| Table JSON | alternating columns skipped; no outer array | complete array of objects | C++ source defect; Rust implemented |
| Table CSV | comma inserted between records | `csv` crate record semantics | C++ source defect; Rust implemented |
| Table import | CSV `read_g` into a prepared table; no JSON table reader; binary dump restores raw buffers | `table_from_json` and `table_from_csv` with a caller-supplied schema, exact numeric round trips, and full row validation | intentional API difference; implemented |
| Expression representation | token vectors, postfix stack, and manually tagged values | Rhai AST plus `Value` boundary | intentional API difference; implemented |
| Formula syntax | project tokenizer with keyword aliases and postfix helpers | Rhai expression grammar and precedence | intentional language difference |
| Script syntax | custom `begin`/`end` and partial Lua translation | brace-delimited Rhai control flow | intentional language difference |
| Expression integers | one signed 64-bit alternative | GD integers checked into `i64`; integer outputs are `I64` | implemented |
| Expression functions | `void*` registry plus numeric signature flags | typed Rhai function registration | intentional safety difference |
| Malformed trailing operator | postfix compiler lacks complete operand validation; evaluation can underflow | compilation rejects it | C++ source defect; Rust implemented |
| Expression resource use | no matching bounded wrapper policy in the normal runtime | default limits on operations, call/expression depth, string size and array/map size; escape hatches can change policy | intentional safety difference |
| CLI options | custom parser, help generator, aliases, and subcommands | application uses `clap` directly | application responsibility |
| Files and paths | wrappers around streams, handles, and `std::filesystem` | application uses `std::fs`, `std::io`, and `std::path` | application responsibility |
| File rotation and logger | custom mutable rotation/logger state | application selects a maintained sink/appender | application responsibility |
| Console helpers | platform branches and direct output | application selects its terminal UI crate | application responsibility |
| COM-like routing | manual GUID queries and reference counting | application uses Rust traits and `Arc` at its boundary; actual COM interoperability requires a separate binding | application responsibility |
| Arenas and vectors | custom allocation/storage utilities | standard containers; specialized crates only after measurement | application responsibility |
| Pure SQL construction | database-adjacent optional formatting layer | application owns SQL construction; SQLite adapter binds parameters | application responsibility |
| SQLite integration | manually owned connection/cursor/record wrappers | `rusqlite` connection plus checked GD binding/table adapter | implemented |
| Other database integration | generic interfaces, ODBC, and other drivers | application selects drivers directly; crate adapter is SQLite-only | outside current core scope |

## Application integration choices

The rows above do not imply that the C++ integration behavior is safe or portable. They
record that these facilities are not public `gd-rs` APIs. An application can use its own
crates without routing them through this data-model crate. The [feature
matrix](feature-matrix.md) names those application-level choices separately from
capabilities that gd-rs implements itself.

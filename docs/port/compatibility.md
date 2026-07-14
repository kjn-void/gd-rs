# Compatibility decisions

This file records intentional differences from the characterized C++ behavior. A
difference is accepted only when tests describe both the C++ observation and the Rust
contract.

| Area | C++ observation | Rust contract | Status |
|---|---|---|---|
| Dynamic values | manually tagged owned/view layouts | `Value` and `ValueRef<'a>` sum types | implemented |
| Text ownership | allocation flag and pointer conventions | `CompactString` / borrowed `&str` | implemented |
| Arguments | packed ordered buffer; duplicate/unnamed entries | ordered `Vec<Argument>`; duplicates and unnamed preserved | implemented |
| Argument index lifetime | offsets and string views can become stale after mutation | immutable borrow prevents mutation | implemented |
| Table layout | packed row-major fixed buffer plus references | typed vectors per column | implemented |
| Unknown table fields | argument-backed tables redirect unknown names to per-row dynamic storage | strict by default; `UnknownFields::Store` enables lazy owned row extras | implemented |
| Empty null-enabled row | cells marked non-null with uninitialized fixed payloads | null must be explicit; non-null columns reject null | intentionally rejected |
| Column name lengths | unaligned `uint16_t` pointer casts | normal string containers | C++ defect retained; Rust avoids it |
| Table index miss | lower-bound result accepted without equality | exact equality required | C++ defect characterized; Rust exact |
| Binary cursor failure | cursor clamps but error remains unobservable | typed error; failed operation is atomic | C++ defect characterized; Rust implemented |
| Binary floating point | endian read numerically converts integer bits | exact IEEE-754 bit transfer | C++ defect characterized; Rust implemented |
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
| Argument URI | stale escape buffer corrupts fields | independent percent-encoded pairs; duplicates preserved | C++ defect characterized; Rust implemented |
| Table JSON | alternating columns skipped; no outer array | complete array of objects | C++ defect characterized; Rust implemented |
| Table CSV | comma inserted between records | `csv` crate record semantics | C++ defect characterized; Rust implemented |
| Expression representation | token vectors, postfix stack, and manually tagged values | Rhai AST plus `Value` boundary | intentional API difference; implemented |
| Formula syntax | project tokenizer with keyword aliases and postfix helpers | Rhai expression grammar and precedence | intentional language difference |
| Script syntax | custom `begin`/`end` and partial Lua translation | brace-delimited Rhai control flow | intentional language difference |
| Expression integers | one signed 64-bit alternative | GD integers checked into `i64`; integer outputs are `I64` | implemented |
| Expression functions | `void*` registry plus numeric signature flags | typed Rhai function registration | intentional safety difference |
| Malformed trailing operator | compilation accepts `1 +`; evaluator underflows | compilation rejects it | C++ death-tested; Rust implemented |
| Expression resource use | no operation limit in the normal runtime | bounded operations, call depth, and expression depth | intentional safety difference |
| CLI options | custom parser, help generator, aliases, and subcommands | application uses `clap` directly | intentionally omitted from core |
| Files and paths | wrappers around streams, handles, and `std::filesystem` | application uses `std::fs`, `std::io`, and `std::path` | intentionally omitted from core |
| File rotation and logger | custom mutable rotation/logger state | application selects a maintained sink/appender | intentionally omitted from core |
| Console helpers | platform branches and direct output | application selects its terminal UI crate | intentionally omitted from core |
| COM-like routing | manual GUID queries and reference counting | application uses Rust traits and `Arc` at its boundary | intentionally omitted from core |
| Arenas and vectors | custom allocation/storage utilities | standard containers; specialized crates only after measurement | intentionally omitted from core |
| Pure SQL construction | database-adjacent optional formatting layer | separate future package only if golden-output tests justify it | intentionally omitted from core |
| SQLite integration | manually owned connection/cursor/record wrappers | `rusqlite` connection plus checked GD binding/table adapter | implemented |
| Other database integration | generic interfaces, ODBC, and other drivers | excluded | final |

## Explicitly omitted integration layers

The rows above do not imply that the C++ integration behavior is safe or portable.
They record that these facilities are not public `gd-rs` APIs. An application can
choose its own crates without routing them through this data-model crate. If a later
requirement moves one into scope, it needs C++ characterization, Rust tests, public
documentation, and matched benchmarks before this decision changes.

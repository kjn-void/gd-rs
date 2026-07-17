# C++ `gd` issues

This document records findings in the C++ implementation that affect the Rust port. It is
deliberately critical: undocumented quirks must either become tested compatibility
requirements or be rejected explicitly. They must not enter the Rust implementation
by accident.

Port scope, design choices, crate selection, sequencing, and acceptance gates live in
[`porting-plan.md`](porting-plan.md).

The current audit covers the C++ sources in `../gd`. Tests and benchmarks are kept
separate from product code in the reproducible counts in
[`source-stats.md`](source-stats.md).

## Baseline and confirmed defects

The C++ product tree contains 138 C/C++ files and 62,408 non-comment source lines;
the exact scope and method are in [`source-stats.md`](source-stats.md). Source volume
does not establish correctness, and the recently added characterization suite covers
only the ported surface. Consequently, the C++ implementation is a behavioral
reference, not an automatically trusted specification.

SQLite is now in scope as a narrow adapter and its C++ implementation is included in
this audit. The following integration remains out of scope:

- generic `gd::database` interfaces;
- ODBC and drivers other than SQLite;
- COM-like connection/cursor wrappers and custom record-buffer APIs.

The pure `gd_sql_*` query and formatting helpers do not connect to a database. They
may be considered later as an optional module because golden-output tests can cover
them without a live database. They are not part of the initial port.

### SQLite connection copies can double-close the same handle

`gd::database::sqlite::database` copies both `m_psqlite3` and the owner flag. Two
copied objects can therefore finalize the same SQLite connection. Its move assignment
also calls the copy overload of `common_construct`, leaving the source owning the
same handle. Both paths can produce a double close or use-after-close.

### SQLite cursor copy and move operations leave members uninitialized

The cursor copy/move constructors call empty `common_construct` overloads. Because
the constructors do not initialize `m_uState`, `m_pstmt`, or `m_pdatabase`, destroying
or using the result reads indeterminate state and may finalize an arbitrary pointer.
The corresponding assignment operators silently leave the destination unchanged.

### SQLite bindings can outlive temporary payloads

`sqlite3_bind_text` and `sqlite3_bind_blob` are passed a null destructor, which means
SQLite applies `SQLITE_STATIC` lifetime rules. At least one text path binds a local
converted string that is destroyed before the later `sqlite3_step`. SQLite may then
read dangling storage. Bindings whose source is not guaranteed to outlive the
statement must use `SQLITE_TRANSIENT` or explicitly retained storage.

### SQLite record buffers perform potentially unaligned typed access

Cursor update and record access cast byte-buffer positions to `int16_t*`, `int32_t*`,
`int64_t*`, and `double*` and dereference them. The buffer layout does not establish
the required alignment at every offset, so these reads and writes can be undefined
behavior. The same class of defect already found in the table name arena applies
here; byte copies into aligned locals avoid the assumption.

### SQLite interface reference counts have data races

`database_i` and `cursor_i` increment and decrement ordinary `int` reference counts
without atomics or a lock. Concurrent reference acquisition/release is a data race
that can leak, delete a live interface, or delete it twice. The Rust adapter exposes
ordinary `rusqlite` ownership and does not reproduce these interface objects.

### Unnamed SQLite parameters can create an invalid string view

`cursor::get_parameter_name` directly constructs `std::string_view` from
`sqlite3_bind_parameter_name`. SQLite returns null for an unnamed positional
parameter, so this path violates the string-view constructor precondition.

### SQLite record buffers use scalar deletion for arrays

`gd::database::record::buffers` stores derived buffers as
`std::unique_ptr<uint8_t>`, but allocates them with `new uint8_t[...]`. Destruction
therefore pairs array allocation with scalar deletion. Resizing repeats the same
mismatch when `reset` releases the old allocation. AddressSanitizer can diagnose
this as an allocation/deallocation mismatch.

### Declared SQLite `BLOB` columns are classified as integers

`cursor::get_column_type_s` classifies a declaration beginning with `B` as binary
only when its third character is `N`. That handles `BINARY`, but maps `BLOB` to
`eColumnTypeCompleteInt64`. The characterization test records this result; the
matched SQLite benchmark bypasses this record layer rather than measuring corrupted
materialization.

### Null-enabled tables create non-null empty rows

The `table_column_buffer(tag_null)` constructor enables per-cell null metadata, but
`row_add()` initializes every new cell as non-null but does not initialize the
fixed-width payload bytes. Reading such a cell can expose indeterminate allocator
contents. A caller must invoke `cell_set_null` or write a value explicitly for every
cell. This is a correctness and information-disclosure defect, not behavior to retain.

### Column-name lengths perform unaligned integer access

The C++ name arena prefixes each string with a `uint16_t`, but consecutive variable
length strings do not preserve two-byte alignment. Direct pointer casts in
`names::add`, `names::get_name_s`, and `table_column_buffer::column::{name,alias}`
therefore trigger UndefinedBehaviorSanitizer. The product source remains unchanged;
tests avoid executing this undefined path in sanitizer builds.

### Binary floating-point reads destroy the bit pattern

The endian-aware C++ readers decoded a `uint32_t`/`uint64_t` and then used a numeric
`static_cast` to `float`/`double`. For example, the bytes for `1.5F` became the numeric
floating-point value of integer `0x3fc00000`, not `1.5`. The writers preserve bits
with `memcpy`, so read and write are not inverses. The characterization test records
the resulting numeric conversion without changing the reader.

### Binary reader/writer overflow was not observable

Checked stream operators clamp their cursor to `end` on overflow, while `error()`
tests whether the cursor is greater than `end`. That condition cannot become true,
so failed reads return a zero value with `error() == false`. The characterization
test records the unobservable failure; the product source remains unchanged.

## Core type and value issues

### Type identity is manually encoded and duplicated

The type system in
[`gd_types.h`](../../../gd/source/gd_types.h) combines a type number, group flags,
width flags, and reference flags in integers. `variant`, `arguments`, and table code
then partially duplicate those definitions. This creates several risks:

- a value can contain a recognized type number with an inconsistent group or size;
- different components mask different portions of the integer;
- `Unknown` acts as both an absent value and a type error;
- reference/ownership is encoded as a runtime flag rather than in the C++ type;
- pointer values are admitted into otherwise serializable data structures.

### Owned and borrowed variants rely on layout compatibility

`variant` and `variant_view` are designed to have compatible layouts and are treated
as cast-compatible. The approach depends on manual allocation flags and on callers
keeping borrowed storage alive. A mistaken ownership flag or expired source can turn
an ordinary value access into a leak, double free, or dangling read.

### The `string_view` constructor can read past its input

The `std::string_view` constructor in
[`gd_variant.h`](../../../gd/source/gd_variant.h) copies
`length + 1` bytes from `string_view::data()` before writing the terminator. A view
only guarantees `length` readable bytes, so this can read beyond the view. A view
ending at an allocation or protected-page boundary can therefore trigger an
out-of-bounds read.

### Conversion and comparison semantics are underspecified

Conversion and comparison behavior is broad but not specified rigorously. Ambiguous
cases include:

- cross-width signed and unsigned comparisons;
- integer/float conversion and overflow;
- NaN, infinity, and signed zero behavior;
- `Unknown` versus null;
- ASCII, UTF-8, JSON, XML, and wide-string distinctions;
- failed conversion behavior;
- ordering across unlike types.

## Arguments containers

There are at least four overlapping public representations:

- `gd::argument::arguments`, an owned or externally backed packed byte buffer;
- `gd::argument::shared::arguments`, a manually reference-counted packed buffer;
- `gd::args`, a vector of owned key/value objects;
- `gd::args_view`, a vector of borrowed key/value objects.

`arguments` and `shared::arguments` duplicate thousands of lines of parsing,
mutation, and conversion logic. The packed representation mixes storage, indexing,
ownership, iteration, and serialization in one abstraction.

Named lookup scans the encoded entries sequentially in
[`gd_arguments.cpp`](../../../gd/source/gd_arguments.cpp). For `n` entries:

- lookup by name is **O(n)** time and **O(1)** extra space;
- looking up `k` different names independently is **O(k n)**;
- insertion is amortized **O(1)** only when it appends into spare capacity;
- resize, insertion in the middle, and removal are **O(n)** because bytes move;
- iteration is **O(n)**, with decoding work at each entry;
- storage is compact, but names and runtime tags remain repeated per entry.

Linear lookup may be optimal for very small argument lists, so Rust should begin
with an ordered `Vec<Entry>` to preserve duplicates, unnamed values, and iteration
order. Benchmarks should determine when an auxiliary name index pays for itself.
An optional lazily built `HashMap<&str, SmallVec<usize>>` gives expected **O(1)**
lookup while retaining ordered storage, at **O(n)** additional space.

The packed C++ layout should be a separate codec, not the live Rust container.
Decoder tests must cover truncated names, invalid lengths, invalid type tags,
alignment, integer overflow, duplicate names, and hostile input.

### Data race: shared argument reference count

`shared::arguments::buffer::m_iReferenceCount` is an ordinary `int`; increment,
decrement, deletion, and copy-on-write checks are not atomic or protected by a lock.
Concurrent copying or dropping of instances sharing a buffer is a data race and can
lead to a leak, double delete, or use-after-free. See
[`gd_arguments_shared.h`](../../../gd/source/gd_arguments_shared.h).

Rust should use `Arc<[Entry]>` or another standard ownership primitive if sharing is
needed. Mutable sharing should require synchronization or copy-on-write through
`Arc::make_mut`. No custom reference counter is justified here.

### Argument serializers reuse stale escape buffers

The URI formatter in `gd_arguments_io.cpp` reuses `stringEscaped` for names and
values without clearing it. The conversion routines append, so later fields can
contain escaped text from earlier names or values. Some branches pass the same
string as both input view and append destination, which can invalidate that view
during reallocation. JSON field names are inserted without escaping, and the JSON
object silently omits unnamed arguments even though they are valid container
entries. Duplicate names are emitted as duplicate object members with no policy for
readers that collapse them.

Rust formatters need a representability contract. URI pairs can preserve duplicate
names but must reject unnamed entries. A JSON object should reject unnamed and
duplicate names rather than silently lose information. Field names and values must
go through maintained format encoders, and formatting should write each value from
immutable input into a distinct destination.

The characterization test records the stale-buffer corruption. The product source
remains unchanged.

## Tables

The table family is the largest subsystem and has three heavily duplicated
implementations: `table_column_buffer`, `table`, and `arguments::table`. They rely on
matching member offsets and casts between implementations. For example,
[`gd_table_table.cpp`](../../../gd/source/gd_table_table.cpp) asserts compatible
member offsets with `table_column_buffer`. Rust must have one table implementation
with optional capabilities rather than layout-compatible sibling classes.

### Storage layout is not columnar

Documentation repeatedly calls the DTO table columnar, but row lookup is implemented
as `data + row * row_size` in
[`gd_table_column-buffer.h`](../../../gd/source/gd_table_column-buffer.h). Values for
one row are adjacent; values for one column are separated by the entire row width.
This is a packed row store with separate storage for variable-sized references.

For `r` rows and a fixed row width `w`:

- fixed cell storage is **O(r w)**;
- row iteration is cache-friendly and **O(r w)** when all cells are visited;
- scanning one column is **O(r)** but has a stride of `w`, which can waste cache
  bandwidth as rows become wide;
- adding capacity reallocates and copies **O(r w)** bytes;
- null and row-state metadata add **O(r)** space;
- variable-sized values add their payload size plus reference bookkeeping.

The Rust design should use a schema plus typed `ColumnData` vectors and validity
bitmaps. This makes column scans contiguous and avoids a `Value` allocation per
cell. Row iteration can be exposed as a borrowing view assembled from the columns.
Criterion must compare row-oriented and column-oriented workloads before choosing
between the representations.

### Repeated linear schema lookup

Column name and alias lookup linearly scan all columns in
[`gd_table_column-buffer.cpp`](../../../gd/source/gd_table_column-buffer.cpp). With
`c` columns, name resolution is **O(c)**. A named operation performed for every cell
can therefore become **O(r c)** before doing useful cell work. Rust should build a
name/alias map when a schema is finalized, using **O(c)** extra space for expected
**O(1)** lookup.

### Quadratic sorting

The table exposes selection sort and bubble sort implementations in
[`gd_table_column-buffer.cpp`](../../../gd/source/gd_table_column-buffer.cpp). Both
take **O(r²)** comparisons and **O(1)** auxiliary space. Because swapping rows copies
or moves a complete row, the practical upper bound includes row width:
**O(r² + swaps × w)**, commonly described here as **O(r² w)** byte movement in the
worst case.

The Rust table sorts a permutation of row indexes using a stable **O(r log r)**
algorithm and retains that permutation in a lifetime-bound `RowOrder`. This needs
**O(r)** auxiliary space, avoids quadratic behavior, and provides a documented
null-order policy.
Selection and bubble sorts should only remain as named compatibility exercises in
the C++ benchmark suite, not as production Rust algorithms.

The selection-sort range assertion currently checks `uFrom + uFrom` rather than
`uFrom + uCount`. This is a concrete range-validation defect and needs a regression
test.

### Broken binary-search result validation

Both index implementations call `lower_bound` and report success whenever the
iterator is not `end`; neither verifies that the returned key equals the requested
key. A search for a missing value can therefore return the next greater value as a
match. See [`gd_table_index.cpp`](../../../gd/source/gd_table_index.cpp).

Index construction is otherwise **O(r log r)** time and **O(r)** space, with intended
**O(log r)** lookup. GoogleTest must capture the current bug as a regression test;
the Rust index must return `None` for non-equal lower bounds.

The string index stores `string_view` keys. Table mutation, reference-store growth,
or destruction can invalidate those views. Indexes also have no generation marker
or automatic invalidation after table mutation. Rust should either own index keys or
borrow the table for the complete index lifetime, and should associate every index
with a table generation.

### Data race: shared column metadata

`detail::columns::m_iReference` is an ordinary `int` modified without atomics or a
mutex in [`gd_table_column.h`](../../../gd/source/gd_table_column.h). Documentation
describes shared columns as suitable for threaded use, but concurrent copy/drop can
race exactly like the shared argument counter. Rust uses `Arc<Schema>` and makes the
schema immutable after construction.

Table contents are not safe for concurrent mutation. Public documentation must
distinguish immutable shared schema from shared mutable table data.

### Copying an internal table does not retain its shared columns

The public `table(const table&)` copy constructor delegates to
`common_construct(const table&)`. That function copies `o.m_pcolumns` into the new
table but does not call `add_reference()`. Both table destructors subsequently call
`release()` on the same manually reference-counted `detail::columns` object. The
first destruction can therefore delete the shared column metadata while the other
table still points to it; later access or destruction becomes a use-after-free or a
second release through a dangling pointer.

This appears to be an omission rather than an alternative ownership convention:
`common_construct(const table&, tag_columns)` and
`common_construct(detail::columns*)` both increment the reference count immediately
after assigning `m_pcolumns`. The corresponding ordinary copy path in
`arguments::table` has the same discrepancy. See
[`gd_table_table.cpp`](../../../gd/source/gd_table_table.cpp) and
[`gd_table_arguments.cpp`](../../../gd/source/gd_table_arguments.cpp).

Rust represents shared immutable schemas with `Arc<Schema>`. Cloning retains the
schema atomically, and safe code cannot release it while a table still owns a clone.

### Table JSON skips alternating columns and omits the outer array

The named JSON formatter increments `uColumn` in both the `for` header and the loop
body. It therefore emits columns 0, 2, 4, and so on while silently dropping every
other value. Its multi-row output is a comma-separated sequence of objects followed
by a newline, not a complete JSON value with an enclosing array. The array-oriented
formatter has the same missing outer container. Header names are also written
without a complete JSON serializer.

The characterization test records both the skipped column and missing outer array.
The product source remains unchanged.

### Table CSV inserts a comma between records

The C++ CSV formatter appends `",\n"` between rows and then also emits field commas,
creating an extra empty field at the start or end of records. Its header helper
quotes every header and does not share a single record writer with the body. The
characterization test records the extra comma; the product source remains unchanged.

## UTF-8, text, and parsing

The project implements substantial custom UTF-8 traversal, conversion, escaping,
normalization, URI handling, JSON handling, and string containers. This increases
the amount of unsafe boundary logic without a conformance suite.

Rust should use `str`, `char_indices`, and established crates. Candidate crates must
be selected per behavior rather than hidden behind a large custom text module:

- `unicode-normalization` for normalization;
- `unicode-segmentation` only when grapheme semantics are required;
- `serde_json` for JSON;
- `url` and `percent-encoding` for URI work;
- `csv` for CSV;
- `uuid` for UUID parsing and formatting;
- `base64` and `hex-simd` for binary text encodings.

Property tests and fuzzing should cover invalid UTF-8 bytes at codec boundaries,
overlong sequences, truncated escapes, malformed JSON, percent encoding, embedded
NULs, and round trips. Rust `String` APIs should not preserve C-string terminator
assumptions.

### URI decoding writes outside the vector's element range

Both string-returning `uri::convert_uri_to_uf8` overloads call `reserve(uSize)` on an
empty `std::vector<char>` and then write through `vectorText.data()`. Capacity does
not create elements: the vector's size remains zero, so those writes occur outside
the lifetime of any `char` elements even when the allocator supplied enough memory.
The code then constructs a string from those bytes. This relies on storage outside
the C++ container's valid element range and is not behavior to preserve.

Rust percent decoding should allocate a real output buffer through a maintained
crate and validate the decoded bytes as UTF-8 before returning `String`. Malformed or
truncated percent sequences must return a typed error, not an empty string that is
indistinguishable from successful decoding of empty input.

### Multi-byte splitting reads past suffixes and copies the wrong ranges

The `split(string_view, string_view, vector<string>&)` implementations compare the
full delimiter with `memcmp` without first checking that the remaining input is at
least the delimiter length. A partial delimiter at the end can therefore read past
the view. On a non-match, `stringPart += (char*)pubszPosition` appends the entire
NUL-terminated suffix rather than the current byte. Besides producing repeated,
incorrect output, this makes an otherwise linear split **O(n²)** time and output in
the common no-delimiter case. The implementation also assumes the view has an
accessible NUL terminator, which `std::string_view` does not guarantee.

Rust should expose borrowed splitting through `str::split` and collect owned parts
only when ownership is requested. Standard splitting is **O(n)** time plus the
reported output and cannot inspect bytes outside the input slice.

### Trim helpers dereference one-past-end pointers

`trim(begin, end)` initializes the reverse cursor to `end` and dereferences it before
moving backward. The range convention elsewhere treats `end` as exclusive, so that
read is outside the supplied view. Several wrappers also form `&*begin()` for empty
`string_view` values, and the core helpers assert that begin is strictly less than
end. Empty text is therefore either unsupported, undefined, or accidentally accepted
depending on the overload and allocation behind the view.

Rust trimming should be a borrowed `&str` operation with empty input explicitly
valid. The port must distinguish the C++ definition of whitespace (bytes `<= 0x20`)
from Rust's Unicode whitespace and name the narrower operation when compatibility is
needed.

### UTF-8 traversal validates too little before pointer movement

Several traversal methods choose a width solely from the lead-byte lookup table and
then advance without checking remaining length, continuation-byte form, overlong
encodings, surrogate code points, or the Unicode maximum. Assertions disappear in
release builds and some bounded overloads can advance beyond `end`. The Rust public
text API should accept `&str` when valid UTF-8 is required and use
`std::str::from_utf8` at byte boundaries. It should not expose unchecked code-point
stepping.

The validator also uses `remaining > sequence_length` instead of `>=`, so a valid
multibyte character ending exactly at the supplied boundary is rejected. This makes
validation depend on whether an unrelated trailing byte or C-string terminator was
included in the range. The C++ characterization suite records this defect; Rust's
byte-boundary contract follows `std::str::from_utf8`.

Several C-string convenience wrappers compute their byte end as `begin + strlen(...)`,
but unqualified lookup resolves to `gd::utf8::strlen`, which returns a code-point
count rather than `std::strlen`'s byte count. Any multibyte character therefore moves
the end pointer too little. URI and JSON conversions can silently truncate the tail
or process only part of a multibyte sequence. The tests demonstrate the same URI
input producing different output through the pointer and `string_view` overloads.
Rust has one `&str` entry point per operation and derives boundaries from `str::len`.

### JSON escaping can emit invalid or lossy JSON

The JSON escape lookup covers quote, backslash, and five named control escapes, but
leaves other U+0000–U+001F control characters unescaped even though JSON strings
forbid them. For non-ASCII input, the string overload always emits one `\uXXXX`
sequence. Code points above U+FFFF are truncated to their low 16 bits instead of
being represented as a UTF-16 surrogate pair, so characters such as U+1F600 do not
round trip. The raw-buffer and `std::string` overloads also disagree: the former
copies multibyte UTF-8 bytes while the latter converts them to `\uXXXX`.

Rust should have one JSON string-content contract backed by `serde_json`, including
all control characters and valid surrogate-pair handling. Decode errors must include
the parser's position and category.

## Expression engine

The expression subsystem duplicates tokenization, shunting-yard compilation, a
postfix interpreter, a second dynamic value, a function registry, and a separate
statement bytecode layer. It is more than 8,000 lines before its glue code. These
layers share invariants through numeric token fields, raw pointers, and assertions,
making malformed-source behavior depend on which entry point was used.

### Incomplete binary expressions reached an empty value stack

The tokenizer and postfix compiler accept `1 +` as a successful compilation. During
evaluation, the binary-operator path checked for one stack value but then popped two.
The second `top()` operates on an empty `std::stack`, which is undefined behavior and
can crash or read invalid storage. GoogleTest records the compile-time acceptance and
uses a death test for evaluation. The product source remains unchanged.

This is not only a syntax-quality issue: any path that lets a malformed postfix token
sequence reach the evaluator could trigger the same underflow. Stack-effect validation
belongs in compilation, and evaluation must still treat bytecode as fallible input.

### Method lookup could return the wrong function or index an empty registry

`runtime::find_method` indexed `m_vectorMethod[0]` without checking whether any method
table was registered. Its `lower_bound` path returned every non-end result without an
equality check; in release builds the assertion disappeared, so a missing name could
resolve to the next lexicographic method. Calling that method changes program meaning
and may also mismatch its expected arity. Namespace lookup compared a namespace-sized
prefix with `memcmp` before proving the requested name was that long.

GoogleTest records the empty-registry crash with a death test. The other lookup
hazards remain in the product source.

### Function signatures are erased into `void*`

Every built-in function pointer is cast to `void*`. At dispatch, numeric flags and
input/output counts select a different function-pointer typedef and `reinterpret_cast`
the stored address back. Standard C++ does not guarantee round trips between object
and function pointers, and one incorrect metadata field invokes a function through an
incompatible type, which is undefined behavior. The compiler cannot verify registry
entries against their declared arity or return shape.

Rust does not port this registry. Rhai's `register_fn` accepts a concrete Rust callable
and derives its argument and result types. Application functions therefore do not need
a parallel set of signature flags.

### Variable resolution scales with variables times references

Runtime variables are stored in a `vector<pair<string, value>>`, and every lookup scans
from the beginning. A formula executing `t` variable-reference operations over `v`
variables can spend **O(t v)** time on lookup, with **O(v)** retained variable storage.
The standard method table uses binary search, so its intended lookup is **O(log m)**
for `m` correctly sorted methods.

The Rust adapter keeps Rhai's stack-like scope because it supports shadowing and the
observed formulas use small contexts. Its worst-case named lookup is also **O(v)** and
is documented rather than hidden. Large-context workloads should compare a Rhai
variable resolver backed by `AHashMap` before adding another index.

## Logging, files, console, and platform code

The logger has an optional mutex at the logger level, but printer and file paths
contain explicit `TODO: lock this` comments. A thread-safe logger wrapper does not
make every printer implementation thread-safe. Concurrent file writes and rotation
can race.

Do not port the logger implementation. Expected failures use typed `Result` values
and produce no output. If a concrete later integration needs spans, expose optional
`tracing` instrumentation and leave subscriber selection to the application. Use an
established rolling-file appender if rotation is required. Likewise:

- use `std::fs`, `std::path`, `Read`, `Write`, and `Seek` for files and archives;
- use `clap` for CLI parsing unless characterization proves required syntax that
  cannot be expressed by it;
- use `crossterm` or `indicatif` for optional terminal behavior;
- omit custom arena/vector implementations until benchmarks prove a need;
- prefer `smallvec`, `bumpalo`, or another maintained crate when a measured need
  exists.

The POSIX console path includes an explicitly unimplemented operation. Platform APIs
must have platform-specific tests; unsupported operations should return a typed
error, not assert.

These facilities are intentionally not modules in `gd-rs`. CLI schemas belong in an
application's `clap::Command`; file and path operations use `std`; rotation belongs to
the selected logging sink; COM-like routing is replaced by application traits and
standard `Arc` ownership. Pure SQL construction remains an optional, database-adjacent
package and would require dialect-specific golden tests. This avoids adding wrapper
APIs whose only job is to rename maintained Rust facilities.

## Assertion-based validation and unchecked typed access

The non-database code contains roughly 1,887 assertion sites, 270
`reinterpret_cast` sites, and hundreds of direct memory-copy operations. Assertions
often validate public inputs such as names, row bounds, types, and parser states.
In release builds, failed assertions disappear, potentially allowing invalid indexes
or pointer arithmetic to continue.

Packed buffers also perform typed loads through cast pointers. Unless every offset is
proved aligned, such loads can be undefined on some architectures. The alignment
precondition is not expressed in the buffer types and is not validated consistently
before access.

## Issue summary and classification

Classifications are not mutually exclusive. **Undefined behavior** identifies a
direct violation of the C++ object, lifetime, alignment, bounds, or call rules.
**Memory-unsafe access** identifies an out-of-bounds, dangling, uninitialized, or
otherwise invalid memory access even when the detailed language consequence depends
on the executed path. **Validation** includes missing or incorrect argument, state,
type, bounds, and result validation.

A checked box means that the specific unchecked access, lifetime violation,
data race, or invalid type-level operation cannot be expressed through safe Rust.
It does not mean safe Rust prevents the surrounding logical error: checked indexing
may still panic, and incorrect validation or serialization can still compile.

| Area and finding | Classification | Safe Rust blocks unsafe form | Likely consequence |
|---|---|:---:|---|
| SQLite connection copies and move assignment retain the same owned handle | Ownership/lifetime; memory-unsafe access; undefined behavior | ☑ | Double close, use-after-close, or operations through a stale SQLite handle |
| SQLite cursor copy/move leaves members uninitialized | Uninitialized state; memory-unsafe access; undefined behavior | ☑ | Finalizing an arbitrary pointer or reading indeterminate state |
| SQLite text/blob bindings use temporary payloads with `SQLITE_STATIC` semantics | Ownership/lifetime; memory-unsafe access; undefined behavior | ☑ | SQLite reads dangling text or blob storage |
| SQLite record buffers dereference typed pointers at unproved alignments | Alignment; memory-unsafe access; undefined behavior | ☑ | Misaligned reads/writes, traps, or silent corruption |
| SQLite interface reference counts are ordinary shared integers | Data race; ownership/lifetime; undefined behavior | ☑ | Leak, premature deletion, double deletion, or use-after-free |
| Unnamed SQLite parameters are used to construct a `string_view` from null | Missing argument/result validation; memory-unsafe access; undefined behavior | ☑ | Null-pointer access while determining the string length |
| SQLite record arrays are owned by scalar `unique_ptr<uint8_t>` | Allocation/deallocation mismatch; memory-unsafe access; undefined behavior | ☑ | Heap corruption during destruction or resize |
| Declared SQLite `BLOB` columns are classified as integers | Invalid type validation; correctness | ☐ | Binary values receive an incompatible record layout or conversion path |
| Null-enabled table rows begin as non-null with uninitialized payloads | Uninitialized data; memory-unsafe access; information disclosure; correctness | ☑ | Indeterminate values are exposed as valid cells |
| Column-name arenas perform unaligned `uint16_t` access | Alignment; memory-unsafe access; undefined behavior | ☑ | Misaligned loads/stores and platform-dependent failures |
| Binary floating-point readers numerically convert encoded integer bits | Correctness; data corruption | ☐ | Decoded values do not preserve the serialized IEEE-754 bit pattern |
| Binary cursor overflow is clamped while `error()` remains false | Missing bounds/error validation; correctness | ☐ | Truncated input is accepted and zero values appear successfully decoded |
| Type identity is manually composed from duplicated numeric flags | Invalid-state representation; type safety; architecture | ☐ | Inconsistent tag, group, width, and ownership combinations are representable |
| Owned and borrowed variants depend on layout compatibility and runtime flags | Ownership/lifetime; type safety; memory-unsafe access; undefined behavior | ☑ | Dangling reads, leaks, or double frees after a bad flag or expired view |
| The `string_view` variant constructor copies `length + 1` source bytes | Bounds validation; memory-unsafe access; undefined behavior | ☑ | One-byte out-of-bounds read at the end of a view |
| Variant conversions and cross-type comparisons are underspecified | Specification gap; validation; correctness | ☐ | Caller-visible behavior varies across widths, special floats, and unlike types |
| Argument storage has several overlapping packed/owned/borrowed implementations | Duplication; architecture; maintainability | ☐ | Divergent invariants, codecs, ownership rules, and bug fixes |
| Named argument lookup linearly decodes the packed entries | Algorithmic complexity | ☐ | **O(k n)** work when `k` names are independently looked up among `n` entries |
| Shared argument buffers use a non-atomic reference count | Data race; ownership/lifetime; undefined behavior | ☑ | Leak, double delete, or use-after-free during concurrent copy/drop |
| Argument serializers reuse stale buffers and can alias an input view with its output | Lifetime/invalidation; memory-unsafe access; serialization correctness; undefined behavior | ☑ | Corrupted URI fields, invalid JSON names, or reads through an invalidated view |
| Table implementations duplicate layouts and cast between sibling classes | Type safety; architecture; undefined-behavior risk | ☑ | A layout change silently invalidates offset and cast assumptions |
| The documented columnar table is actually a packed row store | Documentation mismatch; space/cache efficiency | ☐ | Strided column scans and avoidable cache traffic for wide rows |
| Named table access repeatedly scans schema metadata | Algorithmic complexity | ☐ | A row/column traversal can perform **O(r c)** lookup work before cell work |
| Table row sorting uses selection/bubble-style physical swaps | Algorithmic complexity; write amplification | ☐ | **O(r²)** comparisons and commonly **O(r² w)** byte movement |
| The selection-sort range assertion checks `uFrom + uFrom` | Missing argument/range validation | ☐ | Invalid ranges can pass while valid ranges can be rejected |
| Table index lookup accepts any non-end `lower_bound` result | Missing result validation; correctness | ☐ | A missing key is reported as the next greater key |
| String indexes retain views without mutation invalidation or a generation check | Ownership/lifetime; stale reference; memory-unsafe access | ☑ | Table growth or destruction leaves dangling index keys |
| Shared table-column metadata uses a non-atomic reference count | Data race; ownership/lifetime; undefined behavior | ☑ | Premature deletion, double deletion, or use-after-free |
| Internal-table copies do not retain their shared column metadata | Ownership/lifetime; memory-unsafe access; undefined behavior | ☑ | The first copy destroyed can leave the other with a dangling schema pointer and cause use-after-free or double release |
| Table JSON skips alternating columns and omits the outer array | Serialization correctness; missing output validation | ☐ | Silent data loss and output that is not one complete JSON value |
| Table CSV inserts a comma between records | Serialization correctness | ☐ | Extra empty fields and inconsistent record widths |
| Text handling duplicates UTF traversal, escaping, and parsing primitives | Duplication; architecture; validation risk | ☐ | Inconsistent boundary rules and a broad memory-safety audit surface |
| URI decoding writes into reserved vector capacity without creating elements | Object lifetime; memory-unsafe access; undefined behavior | ☑ | Writes outside the vector's element range |
| Multi-byte splitting compares beyond suffix bounds and appends whole suffixes | Bounds validation; memory-unsafe access; undefined behavior; algorithmic complexity | ☑ | Out-of-bounds reads plus **O(n²)** time/output on ordinary input |
| Trim helpers dereference the exclusive end and mishandle empty views | Bounds validation; memory-unsafe access; undefined behavior | ☑ | One-past-end or invalid empty-range reads |
| UTF-8 traversal advances from lead bytes without complete sequence validation | Encoding validation; bounds validation; memory-unsafe access; undefined behavior | ☑ | Out-of-bounds pointer movement, truncated processing, or acceptance of invalid UTF-8 |
| UTF-8 validation rejects a multibyte character ending exactly at the supplied boundary | Invalid bounds validation; correctness | ☐ | Valid text is rejected depending on unrelated trailing storage |
| C-string wrappers use a code-point count as a byte count | Invalid length validation; data loss | ☐ | Multibyte URI/JSON input is truncated or only partly processed |
| JSON escaping leaves controls unescaped and truncates astral code points | Serialization correctness; Unicode validation; data loss | ☐ | Invalid JSON or text that cannot round trip |
| The expression subsystem duplicates parsers, tagged values, bytecode, and registries | Duplication; architecture; maintainability | ☐ | Invariants diverge between compilation and execution layers |
| An incomplete binary expression reaches an empty evaluation stack | Missing syntax/state validation; memory-unsafe access; undefined behavior | ☑ | Empty-stack access and process termination or corruption |
| Method lookup indexes an empty registry and does not verify exact matches | Missing state/result/bounds validation; memory-unsafe access; correctness; undefined behavior | ☑ | Crash, out-of-bounds access, or dispatch to the wrong function |
| Function pointers are erased to `void*` and reconstructed from numeric metadata | Type safety; invalid dispatch metadata; undefined behavior | ☑ | Calling through an incompatible function-pointer type |
| Expression variables use a linear scan for every reference | Algorithmic complexity | ☐ | **O(t v)** lookup work for `t` references over `v` variables |
| Logger printer/file/rotation state is not consistently synchronized | Data race; I/O correctness; undefined behavior | ☑ | Interleaved writes, corrupted rotation state, or races on shared objects |
| A POSIX console operation is explicitly unimplemented | Missing platform implementation; API completeness | ☐ | Platform-dependent failure or assertion instead of a reported error |
| Public-input checks rely extensively on release-disabled assertions | Missing argument/state/bounds validation; undefined-behavior exposure | ☐ | Invalid indexes or pointer arithmetic continue unchecked in release builds |
| Packed buffers use typed cast loads without a general alignment proof | Alignment; memory-unsafe access; undefined behavior | ☑ | Systemic platform-dependent misaligned access beyond the named examples |

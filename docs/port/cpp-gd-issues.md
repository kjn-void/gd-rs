# C++ `gd` issues

This document records findings in the C++ implementation that affect the Rust port. It is
deliberately critical: undocumented quirks must either become tested compatibility
requirements or be rejected explicitly. They must not enter the Rust implementation
by accident.

Current port scope, design choices and crate selection live in
[`porting-plan.md`](porting-plan.md).

Reviewed on 2026-10-05 against GD submodule `cb11cff90d05260a88d59a30c9da421cd4e19c34`
and gd-rs `f7c7b91`. The current audit covers the C++ sources in the `external/gd`
submodule. Tests and benchmarks are kept separate from product code in the reproducible
counts in [`source-stats.md`](source-stats.md).

Concurrency, UUID, logger, and move-operation findings were added on 2026-10-06 against
the same GD revision and gd-rs `70c89f2`. Their probes ran on macOS ARM64 (Apple M3 Max,
Apple Clang) and Linux x86_64 (Intel Core Ultra 5 225H, GCC 13.3).

## Baseline and confirmed defects

The C++ product tree contains 140 C/C++ files and 64,587 non-comment source lines; the
exact scope and method are in [`source-stats.md`](source-stats.md). Source volume does
not establish correctness. The pinned GD revision has no checked-in unit-test directory.
Findings below are based on the pinned source. Maintained workflow checks and sanitizer
probes are linked in the [workflow report](../high-level/order-workflow.md). That
evidence covers specific paths, not all overloads. Consequently, GD is a behavioral
reference, not an automatically trusted specification.

SQLite is in scope as a narrow adapter and its C++ implementation is included in this
audit. The following integration remains out of scope:

- generic `gd::database` interfaces;
- ODBC and drivers other than SQLite;
- COM-like connection/cursor wrappers and custom record-buffer APIs.

The pure `gd_sql_*` query and formatting helpers do not connect to a database. They are
outside gd-rs; applications own SQL construction and can use separate libraries without
routing them through the data-model crate.

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

`cursor::get_column_type_s` classifies a declaration beginning with `B` as binary only
when its third character is `N`. That handles `BINARY`, but maps `BLOB` to
`eColumnTypeCompleteInt64`. The matched SQLite benchmark bypasses this record layer
rather than measuring corrupted materialization.

### Null-enabled tables create non-null empty rows

The `table_column_buffer(tag_null)` constructor enables per-cell null metadata, but
`row_add()` initializes every new cell as non-null but does not initialize the
fixed-width payload bytes. Reading such a cell can expose indeterminate allocator
contents. A caller must invoke `cell_set_null` or write a value explicitly for every
cell. This is a correctness and information-disclosure defect, not behavior to retain.

### Column-name lengths perform unaligned integer access

The C++ name arena prefixes each string with a `uint16_t`, but consecutive variable
length strings do not preserve two-byte alignment. Direct pointer casts in `names::add`,
`names::get_name_s`, and `table_column_buffer::column::{name,alias}` can violate
alignment requirements. The maintained workflow report documents focused sanitizer
findings; no full GD test suite is present in the submodule.

### Binary floating-point reads destroy the bit pattern

The endian-aware C++ readers decode a `uint32_t`/`uint64_t` and then use a numeric
`static_cast` to `float`/`double`. For example, the bytes for `1.5F` become the numeric
floating-point value of integer `0x3fc00000`, not `1.5`. The writers preserve bits with
`memcpy`, so read and write are not inverses. The maintained binary benchmark uses a
bit-preserving adapter rather than this broken floating-point read path.

### Binary reader/writer overflow is not observable

Checked stream operators clamp their cursor to `end` on overflow, while `error()` tests
whether the cursor is greater than `end`. That condition cannot become true, so failed
reads return a zero value with `error() == false`. Rust reports a typed error and leaves
the cursor unchanged.

## Core type and value issues

### Type identity is manually encoded and duplicated

The type system in
[`gd_types.h`](../../external/gd/source/gd_types.h) combines a type number, group flags,
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
[`gd_variant.h`](../../external/gd/source/gd_variant.h) copies
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

### Random UUID generation shares one unsynchronized engine

`uuid::new_uuid_s()` and the `uuid(tag_random)` constructor draw from `r64`, which
uses a process-wide static `std::mt19937_64` and a shared distribution without a lock
([`gd_uuid.h`](../../external/gd/source/gd_uuid.h)). Concurrent generation is a data
race on the engine state. Beyond undefined behavior, two threads can read the same state
and produce the same "random" UUID. The separate `uuid_generate_g` in
[`gd_types.cpp`](../../external/gd/source/gd_types.cpp) uses a `thread_local` engine
and does not have this problem; callers must know which of the two generators they use.

gd-rs parses and formats UUIDs but does not generate them. Applications that need random
UUIDs can enable the `uuid` crate's `v4` feature, which is safe to call from several
threads.

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
[`gd_arguments.cpp`](../../external/gd/source/gd_arguments.cpp). For `n` entries:

- lookup by name is **O(n)** time and **O(1)** extra space;
- looking up `k` different names independently is **O(k n)**;
- insertion is amortized **O(1)** only when it appends into spare capacity;
- resize, insertion in the middle, and removal are **O(n)** because bytes move;
- iteration is **O(n)**, with decoding work at each entry;
- storage is compact, but names and runtime tags remain repeated per entry.

Rust uses an ordered `Vec<Argument>` to preserve duplicates, unnamed values, and
iteration order. The optional explicitly constructed `ArgumentIndex` uses an `AHashMap`
with borrowed names and `SmallVec` positions. Lookup is expected **O(1)** for
fixed-length names, plus hashing/name-comparison cost, with **O(n)** additional space.
Linear lookup can still be useful for small lists; the argument benchmarks compare index
construction and repeated reads separately.

The packed C++ layout is not the live Rust container, and no compatible argument codec
is exported. Rust's checked binary primitives do not constitute an argument
serialization format.

### Data race: shared argument reference count

`shared::arguments::buffer::m_iReferenceCount` is an ordinary `int`; increment,
decrement, deletion, and copy-on-write checks are not atomic or protected by a lock.
Concurrent copying or dropping of instances sharing a buffer is a data race and can
lead to a leak, double delete, or use-after-free. See
[`gd_arguments_shared.h`](../../external/gd/source/gd_arguments_shared.h).

Rust's ordinary `Arguments` can be wrapped in `Arc` by the application. Mutable sharing
requires synchronization or ownership-based copying; gd-rs does not export a separate
packed shared-argument container or a custom reference counter.

### Moving shared arguments deletes the static empty buffer

Every `shared::arguments` object starts with `m_pbuffer` pointing at the process-wide
static `m_buffer_s`. The move constructor calls `common_construct(arguments&&)`, which
skips the first release because the new object is still null and then unconditionally
calls `m_pbuffer->release()` again
([`gd_arguments_shared.h`](../../external/gd/source/gd_arguments_shared.h)). That
release targets the static empty buffer. With assertions enabled it trips
`assert( this != &m_buffer_s )`. Without assertions it decrements the static count from
1 to 0 and runs `delete[]` on the static object. An AddressSanitizer build without
assertions aborts at `gd_arguments_shared.h:372`, called from line 972, on a single
thread moving a one-value object:

```cpp
gd::argument::shared::arguments a;
a.append("id", 1);
gd::argument::shared::arguments b(std::move(a));   // deletes the static m_buffer_s
```

Every move construction is affected, including implicit ones: the move constructor is
`noexcept`, so a growing `std::vector<shared::arguments>` moves its elements on
reallocation. The static buffer is also process-wide mutable state, so concurrent moves
would race on it as well.

### Argument serializers reuse stale escape buffers

The URI formatter in `gd_arguments_io.cpp` reuses `stringEscaped` for names and
values without clearing it. The conversion routines append, so later fields can
contain escaped text from earlier names or values. Some branches pass the same
string as both input view and append destination, which can invalidate that view
during reallocation. JSON field names are inserted without escaping, and the JSON
object silently omits unnamed arguments even though they are valid container
entries. Duplicate names are emitted as duplicate object members with no policy for
readers that collapse them.

Rust's URI formatter preserves duplicate names and rejects unnamed entries; its
JSON-object formatter rejects unnamed and duplicate names. Field names and values use
maintained format encoders, with immutable input and a distinct destination.

## Tables

The table family is the largest subsystem and has three heavily duplicated
implementations: `table_column_buffer`, `table`, and `arguments::table`. They rely on
matching member offsets and casts between implementations. For example,
[`gd_table_table.cpp`](../../external/gd/source/gd_table_table.cpp) asserts compatible
member offsets with `table_column_buffer`. Rust must have one table implementation
with optional capabilities rather than layout-compatible sibling classes.

### Storage layout is not columnar

Documentation repeatedly calls the DTO table columnar, but row lookup is implemented
as `data + row * row_size` in
[`gd_table_column-buffer.h`](../../external/gd/source/gd_table_column-buffer.h). Values for
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

Rust uses a schema plus typed `ColumnData` vectors and validity bitmaps. Column scans
are contiguous without storing a dynamic `Value` per cell. Row iteration is a borrowing
view assembled from the columns. The maintained table benchmarks cover both row-oriented
access and typed column scans; this is not a claim that column storage is preferable for
every workload.

### Repeated linear schema lookup

Column name and alias lookup linearly scan all columns in
[`gd_table_column-buffer.cpp`](../../external/gd/source/gd_table_column-buffer.cpp).
With `c` columns, name resolution is **O(c)**. For `r` rows, one named lookup per row
costs **O(r c)**, while named access for every cell costs **O(r c²)**. Rust builds a
name/alias map when a schema is finalized, using **O(c)** extra space for expected
**O(1)** lookup, plus name hashing/comparison costs.

### Quadratic sorting

The table exposes selection sort and bubble sort implementations in
[`gd_table_column-buffer.cpp`](../../external/gd/source/gd_table_column-buffer.cpp). Both
take **O(r²)** comparisons and **O(1)** auxiliary space. Because swapping rows copies
or moves a complete row, the practical upper bound includes row width:
**O(r² + swaps × w)**, commonly described here as **O(r² w)** byte movement in the
worst case.

The Rust table sorts a permutation of row indexes using a stable **O(r log r)**
algorithm and retains that permutation in a lifetime-bound `RowOrder`. This needs
**O(r)** auxiliary space, avoids quadratic behavior, and provides a documented
null-order policy. Rust does not export selection/bubble sort or a destructive
table-sort API.

The selection-sort range assertion currently checks `uFrom + uFrom` rather than `uFrom +
uCount`. This is a source-level range-validation defect; no maintained probe currently
exercises it.

### Broken binary-search result validation

The scalar integer and string index implementations call `lower_bound` and report
success whenever the iterator is not `end`; neither verifies that the returned key
equals the requested key. A search for a missing value can therefore return the next
greater value as a match. See
[`gd_table_index.cpp`](../../external/gd/source/gd_table_index.cpp). The two-column
`index_composite<T1,T2>` in
[`gd_table_index.h`](../../external/gd/source/gd_table_index.h) does check equality; the
defect must not be generalized to that implementation.

Index construction is otherwise **O(r log r)** time and **O(r)** space, with intended
**O(log r)** lookup for fixed-size keys. The maintained [workflow
probes](../../benches/cpp-reference/order_workflow/probes.cpp) exercise the scalar miss
defect. Rust's single/composite hash indexes require exact equality, return all
duplicate positions, and exclude null/deleted keys.

The string index stores `string_view` keys. Table mutation, reference-store growth, or
destruction can invalidate those views. Indexes also have no generation marker or
automatic invalidation after table mutation. Rust's `ColumnIndex` and `CompositeIndex`
borrow the table for their complete usable lifetimes, preventing conflicting mutation.
They therefore need no generation counter. Returned plain position vectors are snapshots
and can become stale after later compaction/removal.

### Data race: shared column metadata

`detail::columns::m_iReference` is an ordinary `int` modified without atomics or a
mutex in [`gd_table_column.h`](../../external/gd/source/gd_table_column.h). Documentation
describes shared columns as suitable for threaded use, but concurrent copy/drop can
race exactly like the shared argument counter. Rust uses `Arc<Schema>` and makes the
schema immutable after construction.

The race was reproduced with tables that are each owned by one thread. Only the
`detail::columns` object is shared, attached the way GD's own documented example does
it. The owning thread keeps its reference for the entire run and never releases it:

```cpp
auto pcolumns = gd::table::table::new_columns_s();   // count 1, held by main until exit
for(int t = 0; t < threads; ++t)
    workers.emplace_back([pcolumns, iterations] {
        for(int i = 0; i < iterations; ++i) {
            gd::table::table owned;                  // thread-local table
            owned = pcolumns;                        // add_reference()
        }                                            // ~table: release(), delete at zero
    });
// after join, pcolumns->get_reference() should be 1
```

ThreadSanitizer reports a data race between `add_reference()` (`gd_table_column.h:308`)
and `release()` (`gd_table_column.h:352`) on both platforms. Release-build outcomes:

| Platform and build | Threads × iterations | Runs | Correct | Count too high (leak) | Freed-memory value | Crash |
|---|---|---:|---:|---:|---:|---:|
| ARM64, Apple Clang `-O3` | 4 × 200,000 | 40 | 13 | 10 | 10 | 7 |
| x86_64, GCC `-O3` | 4 × 200,000 | 440 | 439 | 1 | 0 | 0 |
| x86_64, GCC `-O3` | 4 × 20,000,000 | 40 | 28 | 12 | 0 | 0 |
| x86_64, GCC `-O3` | 14 × 20,000,000 | 40 | 21 | 19 | 0 | 0 |
| x86_64, GCC `-O1` | 4 × 200,000 | 400 | 11 | 79 | 23 | 287 |
| x86_64, GCC `-O1` | 4 × 20,000,000 | 40 | 0 | 0 | 0 | 40 |
| x86_64, GCC `-O1` | 14 × 20,000,000 | 40 | 0 | 0 | 0 | 40 |

Crashes were segmentation faults and allocator aborts. A "freed-memory value" is a final
count read after the columns object had already been deleted.

- **Keeping the owner's reference does not prevent early deletion.** A lost increment
  lets the count reach zero while the owner still holds the object, after which
  `release()` deletes it.
- **x86's stronger memory ordering does not make the counter atomic.** Ordering rules
  govern when other cores observe loads and stores; `++` and `--` remain separate
  load, modify, and store steps. Only `lock`-prefixed instructions on x86, or
  `LDADD`/`LDXR`+`STXR` on ARM64, make them atomic, and a plain `int` gets neither.
- **The failure mode follows code generation, not the CPU.** Apple Clang and GCC `-O1`
  emit a separate load, modify, and store for each update, so the count can reach zero
  early and crash. GCC `-O3` inlined both calls into one read, a store of `count + 1`,
  and a store of the original value; because a data race is undefined behavior, the
  compiler may assume no other writer. Other threads' updates are overwritten, and the
  count can only drift upward, which leaks. The same source on the same x86 CPU
  therefore ranges from failing once in 440 runs to crashing in every run.
- **The short x86 `-O3` run rarely fails because the threads barely overlap.** Each run
  takes about 7 ms, so most threads finish before the next one starts. Longer runs
  fail in 30–48% of cases.

Locked mode avoids this race. After `set_locked()`, `add_reference()` and `release()`
only read the `-2` sentinel, and concurrent reads do not race. The owner must then call
`delete_locked()` after every table using the object is gone. No GD code calls
`set_locked()`; the documented example and the internal `new_columns_s()` call sites
use counting mode.

GD table contents are not safe for unsynchronized conflicting access. Even tables with
independent row buffers can share reference-counted metadata. The application must
enforce safe publication, lifetimes, storage stability, and synchronization of all
shared metadata/reference-count changes; see the [concurrent-use
requirements](../high-level/order-workflow.md#what-the-application-must-enforce-for-concurrent-gd-use).
Rust provides shared immutable tables, checked disjoint mutable views and a synchronized
complete-row collector; ordinary `Table` mutation remains exclusive.

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
[`gd_table_table.cpp`](../../external/gd/source/gd_table_table.cpp) and
[`gd_table_arguments.cpp`](../../external/gd/source/gd_table_arguments.cpp).

Rust represents shared immutable schemas with `Arc<Schema>`. Cloning retains the
schema atomically, and safe code cannot release it while a table still owns a clone.

### Table JSON skips alternating columns and omits the outer array

The named JSON formatter increments `uColumn` in both the `for` header and the loop
body. It therefore emits columns 0, 2, 4, and so on while silently dropping every
other value. Its multi-row output is a comma-separated sequence of objects followed
by a newline, not a complete JSON value with an enclosing array. The array-oriented
formatter has the same missing outer container. Header names are also written
without a complete JSON serializer.

### Table CSV inserts a comma between records

The C++ CSV formatter appends `",\n"` between rows and then also emits field commas,
creating an extra empty field at the start or end of records. Its header helper quotes
every header and does not share a single record writer with the body. The extra field
separator is present in the pinned formatter. Rust uses the `csv` record writer instead.

## UTF-8, text, and parsing

The project implements substantial custom UTF-8 traversal, conversion, escaping,
normalization, URI handling, JSON handling, and string containers. This increases
the amount of unsafe boundary logic without a conformance suite.

Rust uses valid UTF-8 `str` boundaries and focused helpers backed by `serde_json`,
`percent-encoding`, `csv`, `uuid` and `hex-simd`. Other facilities remain application
choices rather than existing gd-rs APIs:

- `unicode-normalization` for normalization;
- `unicode-segmentation` only when grapheme semantics are required;
- `url` for complete URI parsing;
- a Base64 codec for Base64 text encodings.

Rust's text tests cover malformed encodings/escapes and round trips. Public text helpers
use Rust string lengths instead of C-string terminator assumptions. This is not evidence
that every GD text overload has been tested for conformance.

### Default UTF-8 strings share copy-on-write buffers through a non-atomic count

A default-constructed `gd::utf8::string`, and the `const char*`, iterator, and
initializer-list constructors, use reference-counted storage
([`gd_utf8_string.h:287`](../../external/gd/source/gd_utf8_string.h)). Copying shares
the buffer and increments `buffer::m_iReferenceCount`, an ordinary `int32_t`, and the
copy-on-write path checks the count before releasing and cloning. A copy therefore
looks like an independent value, like `std::string`, but copies handed to different
threads race on the shared count and can leak, double free, or clone from a buffer
another thread just freed. C++11 disallowed copy-on-write `std::string` for this
reason.

Source comments say to use `storage::unique` in threaded code and not to use reference
counting when a string is accessed from several threads (lines 290 and 517). That
documents the limitation, but the unsafe mode is the default and the type gives no
indication at the point where a copy crosses a thread boundary.

### UTF-8 string move assignment shares a buffer without retaining it

`string& operator=(string&& o)` assigns `o.m_pbuffer` without releasing the previous
buffer or adding a reference, then calls `m_pbuffer->set_null_buffer(o.m_pbuffer)`
([`gd_utf8_string.h:315`](../../external/gd/source/gd_utf8_string.h)).
`set_null_buffer` assigns only its by-value parameter (line 550), so `o` keeps the
buffer. Both strings then own one buffer whose count is still 1, and the destination's
previous buffer leaks. Destroying both strings is a heap use-after-free; an
AddressSanitizer build reports it on a single thread:

```cpp
gd::utf8::string a("first value");
gd::utf8::string b("second value");
b = std::move(a);   // a and b share one buffer with count 1; b's old buffer leaks
```

The move constructor has the same no-op `set_null_buffer` call but adds a reference, so
it behaves as a shared copy rather than a move. `gd_utf8_string.cpp` is not part of the
GD core library built by gd-rs's benchmark recipe; the probe compiled it separately.

### URI decoding uses reserved capacity as an untracked output buffer

Both string-returning `uri::convert_uri_to_uf8` overloads call `reserve(uSize)` on an
empty `std::vector<char>` and then write through `vectorText.data()`. Capacity does not
increase size: the vector remains logically empty while a raw pointer is used for
output, followed by constructing a string from that output. The standard [`vector::data`
contract](https://eel.is/c++draft/vector.data) guarantees the range through `size()`,
not a public writable range through `capacity()`. This is a portability/buffer-contract
concern. The size mismatch alone is not proof that every write to allocated `char`
storage is undefined behavior; that also needs an analysis of the implementation,
pointer and object lifetimes. A sized output buffer avoids the assumption. No current
maintained probe establishes a crash for this path.

Rust percent decoding uses a maintained decoder and validates UTF-8 before returning
`String`. Malformed/truncated escapes and invalid UTF-8 return typed errors; successful
empty output is distinct from failure.

### Multi-byte splitting reads past suffixes and copies the wrong ranges

The `split(string_view, string_view, vector<string>&)` implementations compare the
full delimiter with `memcmp` without first checking that the remaining input is at
least the delimiter length. A partial delimiter at the end can therefore read past
the view. On a non-match, `stringPart += (char*)pubszPosition` appends the entire
NUL-terminated suffix rather than the current byte. Besides producing repeated,
incorrect output, this makes an otherwise linear split **O(n²)** time and output in
the common no-delimiter case. The implementation also assumes the view has an
accessible NUL terminator, which `std::string_view` does not guarantee.

Applications can use borrowed `str::split` directly. gd-rs's narrower `split_escaped`
helper handles a character delimiter and escape character, returning owned parts. Both
stay within valid input; it is not a port of every GD split overload.

### Trim helpers dereference one-past-end pointers

`trim(begin, end)` initializes the reverse cursor to `end` and dereferences it before
moving backward. The range convention elsewhere treats `end` as exclusive, so that
read is outside the supplied view. Several wrappers also form `&*begin()` for empty
`string_view` values, and the core helpers assert that begin is strictly less than
end. Empty text is therefore either unsupported, undefined, or accidentally accepted
depending on the overload and allocation behind the view.

Rust's `trim_ascii_control` returns a borrowed `&str`, accepts empty input and trims the
C++ byte range `<= 0x20`. Unicode whitespace trimming remains `str::trim`.

### UTF-8 traversal validates too little before pointer movement

Several traversal methods choose a width solely from the lead-byte lookup table and then
advance without checking remaining length, continuation-byte form, overlong encodings,
surrogate code points, or the Unicode maximum. Assertions disappear in release builds
and some bounded overloads can advance beyond `end`. The Rust public text API accepts
`&str` when valid UTF-8 is required and uses `std::str::from_utf8` at byte boundaries.
No unchecked code-point stepping is exported.

The validator also uses `remaining > sequence_length` instead of `>=`, so a valid
multibyte character ending exactly at the supplied boundary is rejected. This makes
validation depend on whether an unrelated trailing byte or C-string terminator was
included in the range. Rust's byte-boundary contract follows `std::str::from_utf8`.

Several C-string convenience wrappers compute their byte end as `begin + strlen(...)`,
but unqualified lookup resolves to `gd::utf8::strlen`, which returns a code-point count
rather than `std::strlen`'s byte count. Any multibyte character therefore moves the end
pointer too little. URI and JSON conversions can silently truncate the tail or process
only part of a multibyte sequence. These paths can therefore disagree with the
length-bounded `string_view` overloads. Rust has one `&str` entry point per operation
and derives boundaries from `str::len`.

### JSON escaping can emit invalid or lossy JSON

The JSON escape lookup covers quote, backslash, and five named control escapes, but
leaves other U+0000–U+001F control characters unescaped even though JSON strings
forbid them. For non-ASCII input, the string overload always emits one `\uXXXX`
sequence. Code points above U+FFFF are truncated to their low 16 bits instead of
being represented as a UTF-16 surrogate pair, so characters such as U+1F600 do not
round trip. The raw-buffer and `std::string` overloads also disagree: the former
copies multibyte UTF-8 bytes while the latter converts them to `\uXXXX`.

Rust's `encode_json_string` produces a complete JSON string literal through
`serde_json`, with control-character escaping and astral-character round trips.
`decode_json_string` requires a complete literal and preserves the parser's error
information. This differs from GD's append-oriented fragment APIs.

## Expression engine

The expression subsystem duplicates tokenization, shunting-yard compilation, a postfix
interpreter, a second dynamic value, a function registry, and a separate statement
bytecode layer. These layers share invariants through numeric token fields, raw
pointers, and assertions, making malformed-source behavior depend on which entry point
is used.

### Incomplete binary expressions can reach an empty value stack

The tokenizer and postfix compiler accept `1 +` as a successful compilation. During
evaluation, the binary-operator path checks for one stack value but then pops two. The
second `top()` operates on an empty `std::stack`, which is undefined behavior and can
crash or read invalid storage. Rust rejects malformed source at compilation.

This is not only a syntax-quality issue: any path that lets a malformed postfix token
sequence reach the evaluator could trigger the same underflow. Stack-effect validation
belongs in compilation, and evaluation must still treat bytecode as fallible input.

### Method lookup can return the wrong function or index an empty registry

`runtime::find_method` indexes `m_vectorMethod[0]` without checking whether any method
table is registered. Its `lower_bound` path returns every non-end result without an
equality check; in release builds the assertion disappears, so a missing name could
resolve to the next lexicographic method. Calling that method changes program meaning
and may also mismatch its expected arity. Namespace lookup compares a namespace-sized
prefix with `memcmp` before proving the requested name is that long.

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
remains a current limitation; no hash-backed variable resolver is exported.

## Logging, files, console, and platform code

The logger has an optional mutex at the logger level, but printer and file paths
contain explicit `TODO: lock this` comments. A thread-safe logger wrapper does not
make every printer implementation thread-safe. Concurrent file writes and rotation
can race.

The logger's synchronization is narrower than its name suggests
([`gd_log_logger.h`](../../external/gd/source/gd_log_logger.h)):

- **The default logger takes no lock.** `logger<iLoggerKey, bThread>` defaults
  `bThread` to `false`, and `gd::log::get_s()` returns that unsynchronized
  `logger<0>`.
- **Opting in locks only printing.** With `bThread = true`, the `print` and
  `print_always` paths lock a static mutex. `append`, printer removal and `clear`,
  `set_severity`, and `error_pop` do not, while `check_severity` reads the severity
  unlocked and printing pushes to the same error vector that `error_pop` removes from.
- **The printer mutex is unused.** `printer_get_mutex_g()` is defined in
  [`gd_log_logger_printer.cpp`](../../external/gd/source/gd_log_logger_printer.cpp) but
  never called.
- **Timestamps use non-reentrant `localtime`.** Message time and date helpers in
  [`gd_log_logger.cpp`](../../external/gd/source/gd_log_logger.cpp) and the rotation
  helpers in [`gd_file_rotate.cpp`](../../external/gd/source/gd_file_rotate.cpp) call
  `localtime`, which returns a pointer to process-wide static storage. The logger mutex
  does not cover other loggers (each `iLoggerKey` has its own mutex), file rotation, or
  application code calling `localtime`. `localtime_r` or `localtime_s` avoid this.

gd-rs does not include a logger. Expected failures use typed `Result` values and produce
no output. Applications own instrumentation, subscriber selection and rolling-file
appenders. Likewise:

- use `std::fs`, `std::path`, `Read`, `Write`, and `Seek` for files and archives;
- use `clap` directly for CLI parsing;
- use `crossterm` or `indicatif` for optional terminal behavior;
- use ordinary Rust containers; applications select specialized storage if needed.

The POSIX console path includes an explicitly unimplemented operation. Platform APIs
must have platform-specific tests; unsupported operations should return a typed
error, not assert.

The SQLite adapter does not compile with Clang 18 against libstdc++ 13 on Ubuntu 24.04.
[`gd_com.h:110`](../../external/gd/source/gd_com.h), included through
`gd_database.h`, uses an unqualified `nullptr_t` default template argument. The
standard guarantees only `std::nullptr_t`; the unqualified name works only where some
header happens to declare it in the global namespace, as with GCC 13 on the same host
and Apple Clang on macOS.

These facilities are intentionally not modules in `gd-rs`. CLI schemas belong in an
application's `clap::Command`; file and path operations use `std`; rotation belongs to
the selected logging sink; COM-like routing is replaced by application traits and
standard `Arc` ownership. Pure SQL construction is an application-level concern outside
this crate. This avoids adding wrapper APIs whose only job is to rename maintained Rust
facilities.

## Assertion-based validation and unchecked typed access

The source extensively uses assertions, `reinterpret_cast` and direct memory-copy
operations. Textual occurrence counts are not counts of unsafe executed paths.
Assertions often validate public inputs such as names, row bounds, types, and parser
states. In release builds, failed assertions disappear, potentially allowing invalid
indexes or pointer arithmetic to continue.

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
| Random UUID generation uses one unsynchronized static engine | Data race; global state; undefined behavior | ☑ | Corrupted engine state or duplicate "random" UUIDs under concurrent generation |
| Argument storage has several overlapping packed/owned/borrowed implementations | Duplication; architecture; maintainability | ☐ | Divergent invariants, codecs, ownership rules, and bug fixes |
| Named argument lookup linearly decodes the packed entries | Algorithmic complexity | ☐ | **O(k n)** work when `k` names are independently looked up among `n` entries |
| Shared argument buffers use a non-atomic reference count | Data race; ownership/lifetime; undefined behavior | ☑ | Leak, double delete, or use-after-free during concurrent copy/drop |
| Shared-argument move construction releases the static empty buffer | Ownership/lifetime; global state; memory-unsafe access; undefined behavior | ☑ | Every move construction, including `std::vector` growth, asserts or deletes a static object |
| Argument serializers reuse stale buffers and can alias an input view with its output | Lifetime/invalidation; memory-unsafe access; serialization correctness; undefined behavior | ☑ | Corrupted URI fields, invalid JSON names, or reads through an invalidated view |
| Table implementations duplicate layouts and cast between sibling classes | Type safety; architecture; undefined-behavior risk | ☑ | A layout change silently invalidates offset and cast assumptions |
| The documented columnar table is actually a packed row store | Documentation mismatch; space/cache efficiency | ☐ | Strided column scans and avoidable cache traffic for wide rows |
| Named table access repeatedly scans schema metadata | Algorithmic complexity | ☐ | One named access per row costs **O(r c)**; named access for every cell costs **O(r c²)** |
| Table row sorting uses selection/bubble-style physical swaps | Algorithmic complexity; write amplification | ☐ | **O(r²)** comparisons and commonly **O(r² w)** byte movement |
| The selection-sort range assertion checks `uFrom + uFrom` | Missing argument/range validation | ☐ | Invalid ranges can pass while valid ranges can be rejected |
| Scalar integer/string table index lookup accepts any non-end `lower_bound` result | Missing result validation; correctness | ☐ | A missing key is reported as the next greater key; composite lookup checks equality |
| String indexes retain views without mutation invalidation or a generation check | Ownership/lifetime; stale reference; memory-unsafe access | ☑ | Table growth or destruction leaves dangling index keys |
| Shared table-column metadata uses a non-atomic reference count | Data race; ownership/lifetime; undefined behavior | ☑ | Premature deletion, double deletion, or use-after-free; reproduced on ARM64 and x86_64 with per-thread tables |
| Internal-table copies do not retain their shared column metadata | Ownership/lifetime; memory-unsafe access; undefined behavior | ☑ | The first copy destroyed can leave the other with a dangling schema pointer and cause use-after-free or double release |
| Table JSON skips alternating columns and omits the outer array | Serialization correctness; missing output validation | ☐ | Silent data loss and output that is not one complete JSON value |
| Table CSV inserts a comma between records | Serialization correctness | ☐ | Extra empty fields and inconsistent record widths |
| Text handling duplicates UTF traversal, escaping, and parsing primitives | Duplication; architecture; validation risk | ☐ | Inconsistent boundary rules and a broad memory-safety audit surface |
| Default UTF-8 strings share copy-on-write buffers through a non-atomic count | Data race; ownership/lifetime; undefined behavior | ☑ | Apparently independent copies leak, double free, or clone freed memory across threads |
| UTF-8 string move assignment shares a buffer without retaining it | Ownership/lifetime; memory-unsafe access; undefined behavior | ☑ | Heap use-after-free on destruction and a leak of the destination's previous buffer |
| URI decoding uses reserved vector capacity without increasing size | Container/buffer contract; portability; bounds/lifetime risk | ☑ | Raw output relies on storage beyond the range guaranteed by `vector::data`; the size mismatch alone does not establish UB on every implementation |
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
| Logger printer/file/rotation state is not consistently synchronized; the default logger takes no lock and the opt-in lock covers only printing | Data race; I/O correctness; undefined behavior | ☑ | Interleaved writes, corrupted rotation state, or races on printer lists, severity, and the error stack |
| Logger and rotation timestamps use non-reentrant `localtime` | Data race; non-reentrant API; global state | ☑ | Wrong or torn timestamps when other threads or loggers call `localtime` concurrently |
| A POSIX console operation is explicitly unimplemented | Missing platform implementation; API completeness | ☐ | Platform-dependent failure or assertion instead of a reported error |
| `gd_com.h` uses an unqualified `nullptr_t` | Portability; standard conformance | ☐ | The SQLite adapter fails to compile with Clang 18 and libstdc++ 13 |
| Public-input checks rely extensively on release-disabled assertions | Missing argument/state/bounds validation; undefined-behavior exposure | ☐ | Invalid indexes or pointer arithmetic continue unchecked in release builds |
| Packed buffers use typed cast loads without a general alignment proof | Alignment; memory-unsafe access; undefined behavior | ☑ | Systemic platform-dependent misaligned access beyond the named examples |

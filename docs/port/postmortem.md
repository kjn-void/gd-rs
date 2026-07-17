# Architecture postmortem: preserving the three-object core

The author of the C++ library described `Variant`, `Arguments`, and `Table` as a
small framework of foundational data structures rather than three isolated classes.
The important architectural choice is to concentrate difficult representation and
ownership problems in those objects so that most application code can remain simple.

That observation maps closely to this port. `gd-rs` does not try to replace the
three-object model with a large hierarchy of narrowly specialized containers. It
preserves three owned roots:

```text
Value
Arguments
Table
```

Rust does not make the underlying domain simpler. Its contribution is to encode more
of the contracts around those roots in the type system, while retaining the original
concentration of responsibility.

## Concept mapping

| C++ concept | Rust mapping | Role |
|---|---|---|
| `Variant` | `Value` | owned dynamic value |
| variant type metadata | `DataType` | closed logical type descriptor |
| non-owning variant | `ValueRef<'a>` | lifetime-bound borrowed value |
| `Arguments` | `Arguments` | ordered named and positional values |
| argument entry | `Argument` | optional name plus owned value |
| companion lookup structure | `ArgumentIndex<'a>` | optional borrowed name index |
| `Table` | `Table` | owned typed column storage |
| table schema metadata | `Arc<Schema>`, `ColumnSpec` | shared immutable column contracts |
| table accessors | `Row<'a>`, `Column<'a>` | lightweight borrowed views |
| indexes and sorting | `ColumnIndex<'a>`, `RowOrder<'a>` | optional borrowed results |

The supporting types are not alternative ownership models. `ValueRef`, `Row`, and
`Column` are views returned by the owned roots. The index and ordering types are
optional accelerators or results that borrow their source. `Arc<Schema>` separates
shared immutable metadata from mutable row storage so the two cannot silently
diverge, while many small tables can reuse one layout.

There are deliberately no parallel `OwnedValue`, `TypedValue`, `ArgumentsBuilder`,
`TableMut`, `RowMut`, `Cell`, or `CellMut` families. Mutation remains directly on
`Arguments` and `Table`; ordinary construction does not require builders. The
[ownership model](../high-level/architecture.md#ownership-model) shows how these types remain
centered on the three roots.

## Current public surface

The C++ discussion speculated that the central classes might expose hundreds of
methods while still presenting only a few concepts to users. The Rust surface is
currently much narrower. Counting inherent public method declarations in the source:

| Owned root | Methods on root | Methods in its complete supporting family |
|---|---:|---:|
| `Value` | 6 | 16 across `DataType`, `Value`, and `ValueRef` |
| `Arguments` | 19 | 36 across `Argument`, `Arguments`, and `ArgumentIndex` |
| `Table` | 19 | 53 across schema, table, views, index, and row-order types |

These figures exclude trait implementations such as `From<T>`. They are a snapshot
of the current crate, not evidence that Rust expresses every C++ convenience method
with fewer declarations. The port intentionally implements a smaller semantic core;
it has not established method-for-method parity with the complete C++ API.

## What the original observation gets right

The productive part of the design is architectural rather than language-specific.
The Rust implementation still concentrates dynamic values, ordered arguments, and
tabular data in the same three places. Other facilities orbit those objects:

- formatting consumes `Arguments` and `Table`;
- expression evaluation crosses a `Value` boundary;
- SQLite binds `Value`/`Arguments` and materializes `Table`;
- text and binary modules provide checked boundary operations.

The core remains non-trivial, especially the typed column implementation. Rust did
not remove the need to define nullability, conversion, ordering, indexing, ownership,
and error behavior. A mature C++ core also has years of production experience that a
new implementation cannot acquire from tests or static guarantees alone.

The benchmark results reinforce the same point: there is no universal language win.
Performance follows the representation, algorithm, ownership boundary, and exact work
performed.

## What Rust changes

The port concentrates safety at the core boundaries instead of distributing a large
set of competing domain abstractions throughout user code:

- `Value` is a closed sum type, so a runtime tag cannot disagree with its payload.
- `ValueRef` cannot outlive the string, byte buffer, or value that it borrows.
- an `ArgumentIndex` or `ColumnIndex` immutably borrows its owner, preventing
  structural mutation that would invalidate names or stored positions;
- schema width, value type, and nullability are validated before a row mutation;
- null is explicit rather than an uninitialized non-null payload state;
- binary cursor failures return typed errors and leave the cursor unchanged;
- serializers reject or preserve information according to documented policies rather
  than silently omitting or corrupting values.

This creates a real ergonomic tradeoff. Code cannot retain an index or view and then
mutate its source in the same scope. Some C++ usage patterns therefore require shorter
borrow scopes or rebuilding an index. That restriction replaces states in which C++
offsets, pointers, or string views could become stale; it is not an additional data
model that users must learn.

The detailed accepted differences and rejected defects are recorded in
[compatibility decisions](compatibility.md).

## Performance evidence

The matched benchmarks show a mixed result rather than an automatic Rust advantage:

- eleven-field argument reads are about 1.5 times faster by linear name lookup and
  about 2 times faster positionally, while building the Rust hash index is slower;
- ordinary table construction in the current fixture is about 0.65 times as fast,
  largely because of the fixture's integer-to-string formatting path;
- with group strings prepared, insertion is about 1.9 times faster in Rust;
- the typed column scan is about 2 times faster;
- row ordering is dramatically faster because Rust builds an `O(n log n)` stable
  permutation while the characterized C++ path destructively performs `O(n²)` work;
- dynamic-value results vary by payload size, with neither language uniformly ahead.

The row-order comparison is particularly important: most of the gain comes from a
different algorithm and post-sort contract, not from translating identical code into
Rust. The complete methodology and caveats are in
[the performance report](../high-level/performance.md).

No retained-memory or allocation-count conclusion follows from the timing results.
Those measurements are still required before claiming that either representation is
more memory efficient.

## Scope and maturity limits

This crate is a semantic port of the portable data and expression core, not an ABI-
compatible or drop-in replacement for all of `gd`. CLI handling, filesystem policy,
logging, console behavior, COM-like routing, generic database interfaces, ODBC, and
several other integration layers are intentionally outside the crate.

The existing compatibility matrix maps behavior areas and important edge cases. The
examples demonstrate common workflows. Neither is an exhaustive map of every C++
public method or of the operations used by downstream applications. Consequently:

- a drop-in replacement for the full mature C++ convenience surface has not been
  demonstrated;
- preservation of the three-object architecture with stronger ownership and validity
  contracts has been demonstrated;
- production maturity and downstream ergonomics remain empirical questions.

The next useful parity study would inventory real C++ call sites, rank the methods
actually used by applications, and map those workflows to Rust. That would test the
claim that most user code needs only a small subset of a broad convenience API more
directly than raw public-method counts.

## Conclusion

The Rust port preserves the C++ author's central architectural insight rather than
trying to replace it. Its value proposition is not that Rust discovers a simpler
domain model. It is that the same three pillars can expose narrower interfaces while
making ownership, lifetime, type, nullability, and mutation contracts mechanically
enforceable.

If the goal is a method-for-method, production-mature replacement, the work is not
complete. If the goal is the same concentrated architecture expressed as a safer,
idiomatic Rust core, that is what has been built.

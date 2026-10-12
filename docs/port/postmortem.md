# Architecture: values, arguments, and tables

`gd-rs` concentrates dynamic values, ordered arguments, and tabular storage in
three owned roots:

```text
Value
Arguments
Table
```

These types own representation and validation so application code can work with
values, entries, and rows without managing tags, raw pointers, or storage lifetimes.
Borrowed views, shared schema metadata, and indexes expose specific operations
without transferring ownership.

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
| table accessors and selections | `Row<'a>`, `Column<'a>`, `TableSelection<'a>`, `SelectedRow<'a>` | borrowed cells with optional owned selection metadata |
| indexes and sorting | `ColumnIndex<'a>`, `CompositeIndex<'a, N>`, `RowOrder<'a>` | optional borrowed results |
| concurrent ingestion | `ConcurrentTableBuilder` | validates complete rows, then materializes an owned `Table` |
| typed shared records | `SharedRecordTable<S>` | optional `Arc<S>` storage with copy-on-write editing, outside the dynamic schema/adapters |

The supporting types are not alternative ownership models. `ValueRef`, `Row`, and
`Column` are views returned by the owned roots. The index and ordering types are
optional accelerators or results that borrow their source. `Arc<Schema>` separates
shared immutable metadata from mutable row storage so the two cannot silently
diverge, while many small tables can reuse one layout.

Mutation is available directly on `Arguments` and `Table`;
`RowMut`, `RowsMut`, `ColumnMut`, and `FixedStringCellMut` are borrowed views for
editing existing storage, including disjoint parallel partitions. Ordinary
construction does not require builders. The
[ownership model](../high-level/architecture.md#ownership-model) shows how these types
relate to the three roots.

## Public surface

| Root | Operations | Supporting types |
|---|---|---|
| `Value` | construction, borrowing, typed access, numeric conversion | `DataType`, `ValueRef` |
| `Arguments` | ordered insertion/removal, positional and named access, reusable lookup | `Argument`, `ArgumentIndex` |
| `Table` | schema-checked writes, row/column views, selection, append, indexing, ordering, tombstones and compaction | schema and converter types, mutable views, selections, indexes, `RowOrder`, `RowCompaction`, `ConcurrentTableBuilder` |

`SharedRecordTable<S>` provides a typed alternative for filtering and copying
shared records. It uses `Arc<S>` handles and copy-on-write editing; it does not
participate in the dynamic `Table` schema, formatting, or SQLite adapters.

See the [API guides](../api/index.md) for the complete supported operations and
[source statistics](source-stats.md) for measured implementation size.

## Responsibility boundaries

Other facilities use the three roots through explicit boundaries:

- formatting consumes `Arguments` and `Table`;
- expression evaluation crosses a `Value` boundary;
- SQLite binds `Value`/`Arguments` and materializes `Table`;
- text and binary modules provide checked boundary operations.

The typed column implementation defines nullability, conversion, ordering,
indexing, ownership, and error behavior. Tests and static guarantees cover these
contracts; production maturity also depends on downstream use.

The benchmark results reinforce the same point: there is no universal language win.
Performance follows the representation, algorithm, ownership boundary, and exact work
performed.

## Ownership and validity contracts

The port concentrates safety at the core boundaries instead of distributing a large
set of competing domain abstractions throughout user code:

- `Value` is a closed sum type, so a runtime tag cannot disagree with its payload.
- `ValueRef` cannot outlive the string, byte buffer, or value that it borrows.
- an `ArgumentIndex`, `ColumnIndex`, `CompositeIndex` or `TableSelection` immutably borrows its owner, preventing
  structural mutation that would invalidate names or stored positions;
- schema width, value type, and nullability are checked by row insertion and cell
  setters; mapped append stages a whole batch before changing the destination;
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

The maintained [order workflow](../high-level/order-workflow.md) measures database
loading, validation, a three-table join, slices and parameterized outputs, including
concurrent frozen-input runs. Its measurements also apply to the concrete revisions and
implementations recorded there, rather than proving the speed of every API.

The current Rust row-order API builds an `O(n log n)` stable permutation; GD's DTO
selection/bubble sorting uses `O(n²)` comparisons and physically moves rows. That
algorithm and contract difference must be considered when comparing timings.

Timing results do not establish retained-memory or allocation costs. Use the
memory and layout measurements for those comparisons, with their documented
schemas and ownership policies.

## Scope and maturity limits

This crate is a semantic port of the portable data and expression core, not an ABI-
compatible or drop-in replacement for all of `gd`. CLI handling, filesystem policy,
logging, console behavior, COM-like routing, generic database interfaces, ODBC, and
several other integration layers are intentionally outside the crate.

The [feature matrix](feature-matrix.md) inventories current capabilities and the
compatibility matrix maps behavior areas and important edge cases. The examples
demonstrate common workflows. Neither is an exhaustive map of every C++ public method or
of the operations used by downstream applications. Consequently:

- a drop-in replacement for the full mature C++ convenience surface has not been
  demonstrated;
- preservation of the three-object architecture with stronger ownership and validity
  contracts has been demonstrated;
- production maturity and downstream ergonomics remain empirical questions.

Downstream method-for-method parity is not established. The feature matrix covers
library capabilities, while the order workflow demonstrates one concrete application;
neither inventories every call site in downstream GD projects.

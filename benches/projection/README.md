# Shared table storage and column projections

The [C++ example](../cpp-reference/flyweight_projection.cpp) uses the GD member table
`gd::table::table`. The [Rust example](../../examples/flyweight_projection.rs) uses
`gd::Table` and native `TableSelection` views. Both have the same two rows, the same
four columns (`key`, `name`, `qty`, `cost`), and overlapping projections:

- A: `key`, `name`, `qty`;
- B: `name`, `qty`, `cost`.

The Rust fixture builds application-owned handles around borrowed selections.
For the library's owning immutable selections and synchronized mutable
projections, see the [table API guide](../../docs/api/tables.md#owning-immutable-selections).

Run both from the repository root:

```sh
python3 benches/projection/verify.py
```

Select one implementation with `--implementation cpp` or `--implementation rust`.
The runner needs a C++20 compiler and Cargo. It builds the unmodified GD submodule
into `target/projection-example` without CMake, SQLite, or Google Benchmark.
The Rust example can also be run directly:

```sh
cargo run --locked --no-default-features --example flyweight_projection
```

The C++ reference is pinned to `cb11cff90d05260a88d59a30c9da421cd4e19c34`.
Its native member table shares `detail::columns`, while each prepared table owns
its own cell allocation. `new_columns_s(source, indexes, count)` copies selected
column definitions; preparing a destination and using mapped `append` copies
the selected values. The example verifies these distinctions through actual
cell addresses and by changing the source after creating the copied projection.
It also tries `table(subset_columns, source_table, first_row, row_count)` in an
isolated process: the pinned GD implementation rejects different source and
destination column counts with an assertion. With the full schema, that
constructor copies the cells rather than sharing their allocation.

Sharing cells through different projections is implemented in the C++ example by
the small application class `Projection`: it holds a `std::shared_ptr` to one GD
member table and a column map. This is an application extension, not a native GD
projection-table constructor. The example does not alias internal GD buffers or
change GD ownership fields.

The Rust owning descriptor holds `Rc<RefCell<Table>>` and a column map. It creates
native `project_view` selections while reading, then releases those borrows before
writing. Both implementations verify that the projections share string bytes,
that changing A's quantity from 10 to 99 is visible in the source and B, that the
materialized copy remains 10, and that dropping the original owning handle keeps
the views valid. The Rust example also reads both projections on separate threads
using `Arc<Table>`.

These examples share cell storage, rather than just schemas. GD's C++ `Projection`
retains only its owner handle and column map. Rust's temporary `project_view`
additionally allocates row positions and a projected schema; no cell payloads are
copied. `Rc<RefCell<Table>>` is used for sequential edits, and `Arc<Table>` for
immutable sharing across threads. Applications can use `SharedTableProjection`
for synchronized writes; this fixture uses sequential `RefCell` access.

The checks are correctness examples, not performance measurements.

## Files and storage in the projection examples

The diagrams show the mutable example after A changes the first row's quantity
to 99. `name0` and `name1` denote the stored Ada and Grace string payloads.

```mermaid
flowchart TB
    subgraph app["C++ example: flyweight_projection.cpp"]
        A["Application Projection A<br/>column map: 0, 1, 2"]
        B["Application Projection B<br/>column map: 1, 2, 3"]
    end
    A -->|shared_ptr| T
    B -->|shared_ptr| T
    T["One gd::table::table<br/>gd_table_table.h / .cpp"]
    T -->|shared column definitions| S["detail::columns<br/>gd_table_column.h / .cpp<br/>key, name, qty, cost<br/>offsets: 0, 8, 16, 24"]
    T -->|owns m_puData| R["Packed rows: AoS<br/>row 0: key=1, name-index=0, qty=99, cost=100<br/>row 1: key=2, name-index=1, qty=20, cost=200<br/>32 bytes per row; no null metadata in this example"]
    T -->|owns m_references| H["references<br/>gd_table.h / .cpp<br/>index 0 → name0 bytes<br/>index 1 → name1 bytes"]
    R -.->|resolve name indexes| H
```

The application projections retain one existing GD table; they are not separate
native GD tables pointing at the same `m_puData` allocation. A second native
member table sharing the `detail::columns` object has its own row buffer.

```mermaid
flowchart TB
    subgraph app["Rust example: flyweight_projection.rs"]
        A["Owning Projection A<br/>column map: 0, 1, 2"]
        B["Owning Projection B<br/>column map: 1, 2, 3"]
        O["One Rc&lt;RefCell&lt;Table&gt;&gt;"]
        A -->|Rc clone| O
        B -->|Rc clone| O
    end
    O -->|owns| T["One gd::Table<br/>src/table.rs"]
    T -->|Arc schema| S["Schema / ColumnSpec<br/>src/table/schema.rs<br/>types, names, aliases, name lookup"]
    T -->|owns ColumnStorage| C["Separate typed column vectors: SoA<br/>src/table/storage.rs<br/>key: Vec&lt;u64&gt; = 1, 2<br/>name: Vec&lt;CompactString&gt; = desc0, desc1<br/>qty: Vec&lt;u64&gt; = 99, 20<br/>cost: Vec&lt;u64&gt; = 100, 200"]
    C -->|long strings own heap bytes| H["name0 bytes / name1 bytes"]
    A -->|project_view during a read borrow| VA["Native TableSelection A<br/>src/table/selection_view.rs<br/>owns row indexes, column map, projected schema"]
    B -->|project_view during a read borrow| VB["Native TableSelection B<br/>src/table/selection_view.rs<br/>owns row indexes, column map, projected schema"]
    VA -.->|borrows cells| T
    VB -.->|borrows cells| T
```

The Rust selections are released before the example edits the source. The
`Arc<Table>` worker example uses the same SoA storage layout with immutable
ownership across threads. A separate `Table` sharing only `Arc<Schema>` has its
own column vectors.

Verified on 2026-10-13 with Apple Clang 21.0.0 and Rust 1.99.0 on macOS ARM64.
Both examples ran successfully. The isolated GD subset-constructor probe
reached the expected equal-column-count assertion in `append`.

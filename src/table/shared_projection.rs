//! Live column projections over one synchronized mutable table.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use thiserror::Error;

use super::{ColumnSpec, Schema, SelectedRow, Table, TableError, TableSelection};
use crate::{Value, ValueRef};

/// A shared projection's lock, schema, or checked table operation failed.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum SharedProjectionError {
    /// A thread panicked while holding the source's write lock.
    #[error("shared projection source lock is poisoned")]
    LockPoisoned,
    /// The source schema no longer matches the projection's captured schema.
    #[error("shared projection source schema changed; create a new projection")]
    SchemaChanged,
    /// A checked table operation rejected the request.
    #[error(transparent)]
    Table(#[from] TableError),
}

/// An owning, live column projection over an `Arc<RwLock<Table>>`.
///
/// Clones and overlapping projections share writes. Every projection includes all
/// current physical rows: appends, removals and compaction are visible to the next
/// guard. Row positions are current physical positions, not stable row identities.
/// Reads borrow cells from a read or write guard, which prevents conflicting
/// access until it is dropped. No lock is acquired for each cell within a guard.
///
/// Column metadata is fixed at construction. Each guard validates the complete
/// source schema; replacement with an unequal schema returns
/// [`SharedProjectionError::SchemaChanged`]. Filtered or explicit row selections
/// can be borrowed inside a guard using [`TableProjectionReadGuard::as_borrowed`].
/// They cannot outlive that guard. [`Self::materialize`] copies an independent
/// snapshot, suitable for conversion to [`super::SharedTableSelection`].
///
/// ```
/// use gd::{ColumnSpec, DataType, Schema, Table, ValueRef};
/// let mut table = Table::new(Schema::new([
///     ColumnSpec::new("id", DataType::I64),
///     ColumnSpec::new("qty", DataType::I64),
/// ])?);
/// table.push_row([1_i64.into(), 10_i64.into()])?;
/// let shared = table.into_shared_mut();
/// let a = shared.project(&[0, 1])?;
/// let b = shared.project(&[1])?;
/// a.set_cell(0, 1, 99_i64)?;
/// assert_eq!(b.read()?.cell(0, 0)?, ValueRef::I64(99));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// Do not acquire this source's lock again while holding a guard, including from
/// a converter invoked by a write. Recursive acquisition or read-to-write upgrades
/// can deadlock. A write guard serializes its operations but does not roll back
/// earlier successful writes if a later operation fails.
#[derive(Clone, Debug)]
pub struct SharedTableProjection {
    source: Arc<RwLock<Table>>,
    plan: Arc<ProjectionPlan>,
}

#[derive(Debug)]
struct ProjectionPlan {
    source_schema: Arc<Schema>,
    schema: Arc<Schema>,
    columns: Vec<usize>,
}

impl ProjectionPlan {
    fn validate_schema(&self, table: &Table) -> Result<(), SharedProjectionError> {
        if std::ptr::eq(table.schema(), self.source_schema.as_ref())
            || table.schema() == self.source_schema.as_ref()
        {
            Ok(())
        } else {
            Err(SharedProjectionError::SchemaChanged)
        }
    }

    fn source_column(&self, column: usize) -> Result<usize, TableError> {
        self.columns
            .get(column)
            .copied()
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.columns.len(),
            })
    }

    fn row<'a>(&'a self, table: &'a Table, row: usize) -> Option<SelectedRow<'a>> {
        Some(SelectedRow::new(
            table.row(row)?,
            &self.columns,
            &self.schema,
        ))
    }

    fn cell<'a>(
        &self,
        table: &'a Table,
        row: usize,
        column: usize,
    ) -> Result<ValueRef<'a>, TableError> {
        table.cell(row, self.source_column(column)?)
    }

    fn as_borrowed<'a>(&self, table: &'a Table) -> TableSelection<'a> {
        TableSelection::from_parts(
            table,
            (0..table.row_count()).collect(),
            self.columns.clone(),
            Arc::clone(&self.schema),
        )
    }
}

impl SharedTableProjection {
    /// Moves a table into a new lock and selects every fixed column.
    ///
    /// Cell payloads are not copied. Construction allocates the lock, column map
    /// and projection plan. Cloning an existing projection shares these allocations.
    #[must_use]
    pub fn new(table: Table) -> Self {
        let schema = table.schema_arc();
        Self::from_parts(Arc::new(RwLock::new(table)), schema)
    }

    /// Selects every fixed column of an existing shared mutable table.
    ///
    /// Acquires one read lock to capture the schema. The caller can retain the
    /// same `Arc` for structural edits, which become visible to future guards.
    ///
    /// # Errors
    /// Returns [`SharedProjectionError::LockPoisoned`] for a poisoned source lock.
    pub fn from_shared(source: Arc<RwLock<Table>>) -> Result<Self, SharedProjectionError> {
        let schema = source
            .read()
            .map_err(|_| SharedProjectionError::LockPoisoned)?
            .schema_arc();
        Ok(Self::from_parts(source, schema))
    }

    fn from_parts(source: Arc<RwLock<Table>>, schema: Arc<Schema>) -> Self {
        let plan = Arc::new(ProjectionPlan {
            columns: (0..schema.len()).collect(),
            source_schema: Arc::clone(&schema),
            schema,
        });
        Self { source, plan }
    }

    /// Clones the source handle for explicit whole-table or structural operations.
    ///
    /// Replacing the source with a different schema invalidates existing
    /// projections. No lock is acquired by this method.
    #[must_use]
    pub fn source_arc(&self) -> Arc<RwLock<Table>> {
        Arc::clone(&self.source)
    }

    /// Returns physical source column positions in projection order.
    #[must_use]
    pub fn columns(&self) -> &[usize] {
        &self.plan.columns
    }

    /// Returns the captured projected schema without acquiring a lock.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.plan.schema
    }

    /// Returns the fixed column count of this projection.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.plan.columns.len()
    }

    /// Projects positions relative to this projection, retaining all current rows.
    ///
    /// Allocates column/schema metadata but copies no cells. This uses captured
    /// metadata without locking; source-schema validation happens on guard acquisition.
    ///
    /// # Errors
    /// Returns a column-bounds or duplicate-column-name error.
    pub fn project(&self, columns: &[usize]) -> Result<Self, TableError> {
        let columns = columns
            .iter()
            .map(|&column| self.plan.source_column(column))
            .collect::<Result<Vec<_>, _>>()?;
        let specs = columns
            .iter()
            .map(|&column| {
                self.plan.source_schema.column(column).cloned().ok_or(
                    TableError::InternalInvariant {
                        detail: "projection column is missing from its captured schema",
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let schema =
            Schema::new(specs)?.with_unknown_fields(self.plan.source_schema.unknown_fields());
        Ok(Self {
            source: Arc::clone(&self.source),
            plan: Arc::new(ProjectionPlan {
                source_schema: Arc::clone(&self.plan.source_schema),
                schema: schema.into(),
                columns,
            }),
        })
    }

    /// Acquires a shared read lock and validates the captured source schema.
    ///
    /// The returned guard prevents writes/reallocation and retains one consistent
    /// table state for a scan. Multiple readers can hold guards concurrently.
    ///
    /// # Errors
    /// Returns a poisoned-lock or changed-schema error, without exposing cells.
    pub fn read(&self) -> Result<TableProjectionReadGuard<'_>, SharedProjectionError> {
        let table = self
            .source
            .read()
            .map_err(|_| SharedProjectionError::LockPoisoned)?;
        self.plan.validate_schema(&table)?;
        Ok(TableProjectionReadGuard {
            table,
            plan: &self.plan,
        })
    }

    /// Acquires an exclusive write lock and validates the captured source schema.
    ///
    /// One guard can perform multiple reads/writes without relocking. Other readers
    /// and writers wait until it is dropped. Earlier writes are not rolled back
    /// if a later checked operation returns an error.
    ///
    /// # Errors
    /// Returns a poisoned-lock or changed-schema error before any write occurs.
    pub fn write(&self) -> Result<TableProjectionWriteGuard<'_>, SharedProjectionError> {
        let table = self
            .source
            .write()
            .map_err(|_| SharedProjectionError::LockPoisoned)?;
        self.plan.validate_schema(&table)?;
        Ok(TableProjectionWriteGuard {
            table,
            plan: &self.plan,
        })
    }

    /// Sets one projected cell under one exclusive lock.
    ///
    /// # Errors
    /// Returns a lock/schema error or the checked setter's validation error.
    /// Bounds for the requested column are projection-relative; errors from the
    /// source setter report physical source column positions.
    pub fn set_cell(
        &self,
        row: usize,
        column: usize,
        value: impl Into<Value>,
    ) -> Result<(), SharedProjectionError> {
        Ok(self.write()?.set_cell(row, column, value)?)
    }

    /// Sets a projected name/alias or row-local extra under one exclusive lock.
    ///
    /// # Errors
    /// Returns a lock/schema or checked setter error. Omitted fixed columns
    /// return [`TableError::ColumnNotFound`], even on an open schema.
    pub fn set_named(
        &self,
        row: usize,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), SharedProjectionError> {
        Ok(self.write()?.set_named(row, name, value)?)
    }

    /// Copies one consistent projection snapshot into an independent table.
    ///
    /// Copies cells, extras, properties and tombstones while holding a read lock;
    /// converters are not rerun. Convert the result with [`Table::into_shared`]
    /// to retain an owning immutable snapshot across future writes.
    ///
    /// # Errors
    /// Returns a lock/schema or internal table projection error.
    pub fn materialize(&self) -> Result<Table, SharedProjectionError> {
        Ok(self.read()?.materialize()?)
    }
}

/// A read lock retaining the cells and row positions of one live projection.
///
/// Rows, values and borrowed selections cannot outlive this guard.
///
/// ```compile_fail
/// use gd::{ColumnSpec, DataType, Schema, Table};
/// let mut table = Table::new(Schema::new([
///     ColumnSpec::new("name", DataType::String),
/// ]).unwrap());
/// table.push_row(["Ada".into()]).unwrap();
/// let projection = table.into_shared_mut();
/// let guard = projection.read().unwrap();
/// let value = guard.cell(0, 0).unwrap();
/// drop(guard);
/// println!("{value:?}");
/// ```
#[derive(Debug)]
pub struct TableProjectionReadGuard<'a> {
    table: RwLockReadGuard<'a, Table>,
    plan: &'a ProjectionPlan,
}

impl TableProjectionReadGuard<'_> {
    /// Returns the complete source table through this read lock.
    #[must_use]
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Returns the projected schema.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.plan.schema
    }

    /// Returns physical source column positions in projection order.
    #[must_use]
    pub fn columns(&self) -> &[usize] {
        &self.plan.columns
    }

    /// Returns one projected column's specification.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<&ColumnSpec> {
        self.schema().column(position)
    }

    /// Returns the current physical row count, including tombstones.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.table.row_count()
    }

    /// Returns the projected fixed column count.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.plan.columns.len()
    }

    /// Returns whether there are no physical rows, independent of column count.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Reads one row by its current physical source position.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<SelectedRow<'_>> {
        self.plan.row(&self.table, row)
    }

    /// Reads one cell by physical row and projection-relative column.
    ///
    /// # Errors
    /// Returns a row/column bounds error.
    pub fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, TableError> {
        self.plan.cell(&self.table, row, column)
    }

    /// Iterates all current physical rows, including tombstones.
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = SelectedRow<'_>> + DoubleEndedIterator {
        self.table
            .rows()
            .map(|row| SelectedRow::new(row, &self.plan.columns, &self.plan.schema))
    }

    /// Iterates current live rows without copying cells or allocating positions.
    pub fn live_rows(&self) -> impl Iterator<Item = SelectedRow<'_>> {
        self.rows().filter(|row| !row.is_tombstoned())
    }

    /// Borrows all current rows through the existing selection API.
    ///
    /// Allocates row/column position vectors, shares projected schema metadata,
    /// and borrows cells under this guard. Supports filtering and existing serializers.
    #[must_use]
    pub fn as_borrowed(&self) -> TableSelection<'_> {
        self.plan.as_borrowed(&self.table)
    }

    /// Copies this guarded state into an independent mutable table.
    ///
    /// # Errors
    /// Returns any internal projection consistency error.
    pub fn materialize(&self) -> Result<Table, TableError> {
        self.table.project(&self.plan.columns)
    }
}

/// An exclusive lock for checked reads and writes through one column projection.
///
/// Borrowed rows/values/selections must be released before this guard can mutate
/// again. Each setter validates its own operation; the guard is not a transaction.
///
/// ```compile_fail
/// use gd::{ColumnSpec, DataType, Schema, Table};
/// let mut table = Table::new(Schema::new([
///     ColumnSpec::new("name", DataType::String),
/// ]).unwrap());
/// table.push_row(["Ada".into()]).unwrap();
/// let projection = table.into_shared_mut();
/// let mut guard = projection.write().unwrap();
/// let value = guard.cell(0, 0).unwrap();
/// guard.set_cell(0, 0, "Grace").unwrap();
/// println!("{value:?}");
/// ```
#[derive(Debug)]
pub struct TableProjectionWriteGuard<'a> {
    table: RwLockWriteGuard<'a, Table>,
    plan: &'a ProjectionPlan,
}

impl TableProjectionWriteGuard<'_> {
    /// Returns the complete source table immutably through this write lock.
    #[must_use]
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Returns the current physical row count, including tombstones.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.table.row_count()
    }

    /// Reads one projected row by current physical source position.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<SelectedRow<'_>> {
        self.plan.row(&self.table, row)
    }

    /// Reads a cell without reacquiring the lock.
    ///
    /// # Errors
    /// Returns a row/column bounds error.
    pub fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, TableError> {
        self.plan.cell(&self.table, row, column)
    }

    /// Borrows this guarded state using the existing selection API.
    ///
    /// Allocates row/column positions but copies no cells. The borrow must end
    /// before calling a setter or dropping this guard.
    #[must_use]
    pub fn as_borrowed(&self) -> TableSelection<'_> {
        self.plan.as_borrowed(&self.table)
    }

    /// Replaces one projected cell using the source's checked setter.
    ///
    /// # Errors
    /// Returns projection-relative column bounds or the source setter's row,
    /// type, conversion, nullability or capacity error. Source setter errors
    /// identify physical source columns. The rejected cell remains unchanged.
    pub fn set_cell(
        &mut self,
        row: usize,
        column: usize,
        value: impl Into<Value>,
    ) -> Result<(), TableError> {
        self.table
            .set_cell(row, self.plan.source_column(column)?, value.into())
    }

    /// Replaces a projected name/alias or a permitted row-local extra.
    ///
    /// # Errors
    /// Returns [`TableError::ColumnNotFound`] for an omitted fixed column or
    /// a disallowed extra, or the source setter's bounds/validation error.
    pub fn set_named(
        &mut self,
        row: usize,
        name: &str,
        value: impl Into<Value>,
    ) -> Result<(), TableError> {
        if let Some(column) = self.plan.schema.column_index(name) {
            self.set_cell(row, column, value)
        } else if self.plan.source_schema.column_index(name).is_some() {
            Err(TableError::ColumnNotFound(name.into()))
        } else {
            self.table.set_named(row, name, value)
        }
    }

    /// Marks a current physical source row as logically deleted.
    ///
    /// # Errors
    /// Returns [`TableError::RowOutOfBounds`] for an invalid row.
    pub fn tombstone_row(&mut self, row: usize) -> Result<bool, TableError> {
        self.table.tombstone_row(row)
    }

    /// Restores a current physical source row to the live set.
    ///
    /// # Errors
    /// Returns [`TableError::RowOutOfBounds`] for an invalid row.
    pub fn restore_row(&mut self, row: usize) -> Result<bool, TableError> {
        self.table.restore_row(row)
    }
}

impl Table {
    /// Moves this table into a shared mutable projection of every fixed column.
    ///
    /// Cell payloads are not copied. Cloned or derived projections observe writes
    /// through the same lock. Use [`SharedTableProjection::from_shared`] for an
    /// existing `Arc<RwLock<Table>>`.
    #[must_use]
    pub fn into_shared_mut(self) -> SharedTableProjection {
        SharedTableProjection::new(self)
    }
}

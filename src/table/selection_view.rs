//! Borrowed table selections: owned positions and schema metadata, borrowed cells.

use std::sync::Arc;

use super::{ColumnSpec, Row, Schema, Table, TableError};
use crate::ValueRef;

/// An immutable selection borrowing its source table's cell storage.
///
/// Row/column positions and projected schema metadata are owned; payloads, extras,
/// and properties remain borrowed. Rows may repeat. Holding a used selection
/// prevents source mutation, so its positions cannot become stale. Materializing
/// explicitly copies cells, extras, tombstones and properties.
///
/// ```
/// use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef, selection_to_json};
/// let mut table = Table::new(Schema::new([
///     ColumnSpec::new("id", DataType::I64),
///     ColumnSpec::new("name", DataType::String),
/// ])?);
/// table.push_row([Value::I64(7), Value::from("Ada")])?;
/// let view = table.filter_view(|row| row.get(0) == Some(ValueRef::I64(7)));
/// let names = view.project(&[1])?;
/// assert_eq!(selection_to_json(&names)?, "[{\"name\":\"Ada\"}]");
/// let independent = names.materialize()?;
/// assert_eq!(independent.column_count(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ```compile_fail
/// use gd::{Schema, Table};
/// let mut table = Table::new(Schema::new([]).unwrap());
/// let view = table.project_view(&[]).unwrap();
/// table.compact(); // cannot mutate while the view is used
/// let _ = view.row_count();
/// ```
#[derive(Clone, Debug)]
pub struct TableSelection<'a> {
    table: &'a Table,
    positions: Vec<usize>,
    columns: Vec<usize>,
    schema: Arc<Schema>,
}

/// A row in a projected selection. Column positions are selection-relative;
/// [`Self::position`] is the original physical source position.
#[derive(Clone, Copy, Debug)]
pub struct SelectedRow<'a> {
    source: Row<'a>,
    columns: &'a [usize],
    schema: &'a Schema,
}

impl<'a> SelectedRow<'a> {
    /// Returns the physical source row position.
    #[must_use]
    pub const fn position(self) -> usize {
        self.source.position()
    }

    /// Returns whether the physical source row is logically deleted.
    #[must_use]
    pub fn is_tombstoned(self) -> bool {
        self.source.is_tombstoned()
    }

    /// Returns the projected column count.
    #[must_use]
    pub const fn len(self) -> usize {
        self.columns.len()
    }

    /// Returns whether the row has no projected fixed columns.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.columns.is_empty()
    }

    /// Reads a cell by selection-relative column position.
    #[must_use]
    pub fn get(self, column: usize) -> Option<ValueRef<'a>> {
        self.source.get(*self.columns.get(column)?)
    }

    /// Reads a projected column by name/alias. Row-local extras are accessible;
    /// fixed source columns omitted from the projection are not.
    #[must_use]
    pub fn get_named(self, name_or_alias: &str) -> Option<ValueRef<'a>> {
        if let Some(column) = self.schema.column_index(name_or_alias) {
            self.get(column)
        } else if self
            .source
            .table
            .schema()
            .column_index(name_or_alias)
            .is_none()
        {
            self.source.get_named(name_or_alias)
        } else {
            None
        }
    }

    /// Iterates fixed cells in projected column order, without copying payloads.
    #[must_use]
    pub fn iter(self) -> impl ExactSizeIterator<Item = ValueRef<'a>> + DoubleEndedIterator {
        self.columns
            .iter()
            .map(move |&column| self.source.get(column).unwrap_or(ValueRef::Null))
    }
}

impl<'a> TableSelection<'a> {
    /// Returns the borrowed source table.
    #[must_use]
    pub const fn table(&self) -> &'a Table {
        self.table
    }

    /// Returns selected physical source positions, including duplicates.
    #[must_use]
    pub fn positions(&self) -> &[usize] {
        &self.positions
    }

    /// Returns selected physical source column positions.
    #[must_use]
    pub fn columns(&self) -> &[usize] {
        &self.columns
    }

    /// Returns the projected schema.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Returns a projected column specification by selection-relative position.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<&ColumnSpec> {
        self.schema.column(position)
    }

    /// Returns the selected physical row count, including tombstones/duplicates.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.positions.len()
    }

    /// Returns the projected fixed column count.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Returns whether no row is selected (independent of projection width).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Reads one row by selection-relative position.
    #[must_use]
    pub fn row(&self, position: usize) -> Option<SelectedRow<'_>> {
        Some(SelectedRow {
            source: self.table.row(*self.positions.get(position)?)?,
            columns: &self.columns,
            schema: &self.schema,
        })
    }

    /// Iterates every selected physical row, in selection order.
    #[must_use]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = SelectedRow<'_>> + DoubleEndedIterator {
        self.positions.iter().map(|&position| SelectedRow {
            source: Row {
                table: self.table,
                row: position,
            },
            columns: &self.columns,
            schema: &self.schema,
        })
    }

    /// Iterates selected live rows only, preserving order and duplicates.
    pub fn live_rows(&self) -> impl Iterator<Item = SelectedRow<'_>> {
        self.rows().filter(|row| !row.is_tombstoned())
    }

    /// Filters live selected rows without copying their cells. The predicate sees
    /// projected columns; tombstones are excluded before the predicate runs.
    #[must_use]
    pub fn filter_rows(&self, mut predicate: impl FnMut(SelectedRow<'_>) -> bool) -> Self {
        Self {
            table: self.table,
            positions: self
                .live_rows()
                .filter(|&row| predicate(row))
                .map(SelectedRow::position)
                .collect(),
            columns: self.columns.clone(),
            schema: Arc::clone(&self.schema),
        }
    }

    /// Projects selection-relative columns, retaining all selected rows.
    ///
    /// # Errors
    ///
    /// Returns a column-bounds or duplicate-column-name error.
    pub fn project(&self, columns: &[usize]) -> Result<Self, TableError> {
        let mapped = columns
            .iter()
            .map(|&column| {
                self.columns
                    .get(column)
                    .copied()
                    .ok_or(TableError::ColumnOutOfBounds {
                        column,
                        column_count: self.column_count(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.table.select_view(&self.positions, &mapped)
    }

    /// Copies the selected data into an independent table. Extras, properties and
    /// tombstones follow [`Table::select`]; no converter is rerun.
    ///
    /// # Errors
    ///
    /// Returns any internal selection consistency error.
    pub fn materialize(&self) -> Result<Table, TableError> {
        self.table.select(&self.positions, &self.columns)
    }
}

impl Table {
    /// Borrows selected physical rows and columns, preserving order, duplicates
    /// and tombstones. Column duplication is rejected like [`Self::select`].
    ///
    /// # Errors
    ///
    /// Returns a row/column bounds or duplicate-column-name error.
    pub fn select_view(
        &self,
        rows: &[usize],
        columns: &[usize],
    ) -> Result<TableSelection<'_>, TableError> {
        let schema = self.projection_schema(columns)?;
        if let Some(&row) = rows.iter().find(|&&row| row >= self.row_count()) {
            return Err(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count(),
            });
        }
        Ok(TableSelection {
            table: self,
            positions: rows.to_vec(),
            columns: columns.to_vec(),
            schema: schema.into(),
        })
    }

    /// Borrows all physical rows with a column projection, without copying cells.
    ///
    /// # Errors
    ///
    /// Returns a column-bounds or duplicate-column-name error.
    pub fn project_view(&self, columns: &[usize]) -> Result<TableSelection<'_>, TableError> {
        Ok(TableSelection {
            table: self,
            positions: (0..self.row_count()).collect(),
            columns: columns.to_vec(),
            schema: self.projection_schema(columns)?.into(),
        })
    }

    /// Borrows live rows matching a predicate, retaining all fixed columns.
    /// Cell payloads are not copied; O(selected rows + columns) positions are owned.
    #[must_use]
    pub fn filter_view(&self, predicate: impl FnMut(Row<'_>) -> bool) -> TableSelection<'_> {
        TableSelection {
            table: self,
            positions: self.select_rows(predicate),
            columns: (0..self.column_count()).collect(),
            schema: self.schema_arc(),
        }
    }
}

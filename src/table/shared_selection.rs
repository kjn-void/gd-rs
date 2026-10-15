//! Owning immutable selections with shared source storage and selection metadata.

use std::{ops::Range, sync::Arc};

use super::{ColumnSpec, Schema, SelectedRow, Table, TableError, TableSelection};

/// An immutable selection that keeps its source table alive through [`Arc`].
///
/// Cloning shares both the source and the selection metadata in constant time.
/// Whole-table and contiguous selections store a range rather than one position
/// per row. Filtering and explicit row selection allocate positions, never cell
/// payloads. Even an empty selection retains the complete source table.
///
/// Reads borrow from this handle. There is no shared mutation; [`Self::materialize`]
/// creates an independently mutable table. Rows and values cannot outlive the
/// handle they borrow, although cloned handles can be moved to other threads.
///
/// ```
/// use gd::{ColumnSpec, DataType, Schema, Table, ValueRef, shared_selection_to_json};
/// let mut table = Table::new(Schema::new([
///     ColumnSpec::new("id", DataType::I64),
///     ColumnSpec::new("name", DataType::String),
/// ])?);
/// table.push_row([7_i64.into(), "Ada".into()])?;
/// let shared = table.into_shared();
/// let names = shared.project(&[1])?;
/// drop(shared);
/// assert_eq!(names.row(0).unwrap().get(0), Some(ValueRef::String("Ada")));
/// assert_eq!(shared_selection_to_json(&names)?, "[{\"name\":\"Ada\"}]");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// ```compile_fail
/// use gd::{Schema, Table};
/// let shared = Table::new(Schema::new([]).unwrap()).into_shared();
/// let row = shared.row(0);
/// drop(shared); // a borrowed row cannot outlive this handle
/// let _ = row.unwrap().len();
/// ```
///
/// ```compile_fail
/// use gd::{Schema, Table};
/// let shared = Table::new(Schema::new([]).unwrap()).into_shared();
/// shared.table().compact(); // selections expose immutable source access
/// ```
#[derive(Clone, Debug)]
pub struct SharedTableSelection {
    source: Arc<Table>,
    plan: Arc<SelectionPlan>,
}

#[derive(Debug)]
struct SelectionPlan {
    rows: RowSelection,
    columns: Arc<[usize]>,
    schema: Arc<Schema>,
}

#[derive(Clone, Debug)]
enum RowSelection {
    Range(Range<usize>),
    Indices {
        positions: Arc<[usize]>,
        range: Range<usize>,
    },
}

impl RowSelection {
    fn indices(positions: Vec<usize>) -> Self {
        let range = 0..positions.len();
        Self::Indices {
            positions: positions.into(),
            range,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Range(range) | Self::Indices { range, .. } => range.len(),
        }
    }

    fn position(&self, index: usize) -> usize {
        match self {
            Self::Range(range) => range.start + index,
            Self::Indices { positions, range } => positions[range.start + index],
        }
    }

    // The caller validates this selection-relative range before slicing it.
    fn slice(&self, selected: Range<usize>) -> Self {
        match self {
            Self::Range(range) => {
                Self::Range(range.start + selected.start..range.start + selected.end)
            }
            Self::Indices { positions, range } => Self::Indices {
                positions: Arc::clone(positions),
                range: range.start + selected.start..range.start + selected.end,
            },
        }
    }
}

impl SharedTableSelection {
    /// Selects every physical row and fixed column of an owned or shared table.
    ///
    /// Passing a [`Table`] moves its storage into an [`Arc`] without cloning cell
    /// payloads. Passing an `Arc<Table>` reuses that allocation. The source schema
    /// is shared; only the column-position map and selection plan are allocated.
    #[must_use]
    pub fn new(table: impl Into<Arc<Table>>) -> Self {
        let source = table.into();
        let plan = Arc::new(SelectionPlan {
            rows: RowSelection::Range(0..source.row_count()),
            columns: (0..source.column_count()).collect(),
            schema: source.schema_arc(),
        });
        Self { source, plan }
    }

    /// Returns the complete immutable source table, including omitted rows/columns.
    #[must_use]
    pub fn table(&self) -> &Table {
        &self.source
    }

    /// Iterates physical source positions in selection order, including duplicates.
    ///
    /// Unlike [`TableSelection::positions`], this returns an iterator so ranges
    /// need no position-vector allocation.
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn positions(&self) -> impl ExactSizeIterator<Item = usize> + DoubleEndedIterator + '_ {
        (0..self.row_count()).map(|index| self.plan.rows.position(index))
    }

    /// Returns selected physical source column positions in projection order.
    #[must_use]
    pub fn columns(&self) -> &[usize] {
        &self.plan.columns
    }

    /// Returns the projected schema.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.plan.schema
    }

    /// Returns a projected column specification by selection-relative position.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<&ColumnSpec> {
        self.schema().column(position)
    }

    /// Returns the selected physical row count, including tombstones/duplicates.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.plan.rows.len()
    }

    /// Returns the projected fixed column count.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns().len()
    }

    /// Returns whether no row is selected, independent of projection width.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.row_count() == 0
    }

    /// Reads a row by selection-relative position, borrowing its projected cells.
    #[must_use]
    pub fn row(&self, position: usize) -> Option<SelectedRow<'_>> {
        if position >= self.row_count() {
            return None;
        }
        Some(self.selected_row(self.plan.rows.position(position)))
    }

    fn selected_row(&self, position: usize) -> SelectedRow<'_> {
        SelectedRow::new(
            super::Row {
                table: &self.source,
                row: position,
            },
            self.columns(),
            self.schema(),
        )
    }

    /// Iterates every selected physical row, in selection order.
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = SelectedRow<'_>> + DoubleEndedIterator {
        self.positions().map(|position| self.selected_row(position))
    }

    /// Iterates selected live rows only, preserving order and duplicates.
    pub fn live_rows(&self) -> impl Iterator<Item = SelectedRow<'_>> {
        self.rows().filter(|row| !row.is_tombstoned())
    }

    /// Filters live selected rows without copying cells. The predicate sees
    /// projected columns; tombstones are excluded before the predicate runs.
    ///
    /// Allocates O(matched rows) positions and shares column/schema metadata.
    #[must_use]
    pub fn filter_rows(&self, mut predicate: impl FnMut(SelectedRow<'_>) -> bool) -> Self {
        self.with_rows(RowSelection::indices(
            self.live_rows()
                .filter(|&row| predicate(row))
                .map(SelectedRow::position)
                .collect(),
        ))
    }

    fn with_rows(&self, rows: RowSelection) -> Self {
        Self {
            source: Arc::clone(&self.source),
            plan: Arc::new(SelectionPlan {
                rows,
                columns: Arc::clone(&self.plan.columns),
                schema: Arc::clone(&self.plan.schema),
            }),
        }
    }

    /// Selects a contiguous range of selection-relative rows in constant time.
    ///
    /// Preserves tombstones and duplicates. Slicing even an indexed selection
    /// shares its position allocation rather than copying positions.
    ///
    /// # Errors
    /// Returns [`TableError::RowRangeOutOfBounds`] for a reversed range or an end
    /// beyond this selection's row count.
    pub fn slice(&self, rows: Range<usize>) -> Result<Self, TableError> {
        if rows.start > rows.end || rows.end > self.row_count() {
            return Err(TableError::RowRangeOutOfBounds {
                start: rows.start,
                end: rows.end,
                row_count: self.row_count(),
            });
        }
        Ok(self.with_rows(self.plan.rows.slice(rows)))
    }

    /// Projects selection-relative columns, retaining all selected rows.
    ///
    /// Allocates O(selected columns) metadata and shares the row selection.
    ///
    /// # Errors
    /// Returns a column-bounds or duplicate-column-name error.
    pub fn project(&self, columns: &[usize]) -> Result<Self, TableError> {
        let columns = self.map_columns(columns)?;
        let schema = self.source.projection_schema(&columns)?.into();
        Ok(Self {
            source: Arc::clone(&self.source),
            plan: Arc::new(SelectionPlan {
                rows: self.plan.rows.clone(),
                columns: columns.into(),
                schema,
            }),
        })
    }

    /// Selects selection-relative rows and columns, preserving order and row
    /// duplicates, including tombstones. Column duplication is rejected.
    ///
    /// Allocates O(selected rows + selected columns) metadata, copying no cells.
    ///
    /// # Errors
    /// Returns a row/column bounds or duplicate-column-name error.
    pub fn select(&self, rows: &[usize], columns: &[usize]) -> Result<Self, TableError> {
        let columns = self.map_columns(columns)?;
        let schema = self.source.projection_schema(&columns)?.into();
        let positions = rows
            .iter()
            .map(|&row| {
                if row >= self.row_count() {
                    return Err(TableError::RowOutOfBounds {
                        row,
                        row_count: self.row_count(),
                    });
                }
                Ok(self.plan.rows.position(row))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            source: Arc::clone(&self.source),
            plan: Arc::new(SelectionPlan {
                rows: RowSelection::indices(positions),
                columns: columns.into(),
                schema,
            }),
        })
    }

    fn map_columns(&self, columns: &[usize]) -> Result<Vec<usize>, TableError> {
        columns
            .iter()
            .map(|&column| {
                self.columns()
                    .get(column)
                    .copied()
                    .ok_or(TableError::ColumnOutOfBounds {
                        column,
                        column_count: self.column_count(),
                    })
            })
            .collect()
    }

    /// Borrows this selection through the existing [`TableSelection`] API.
    ///
    /// Allocates O(selected rows + selected columns) positions to satisfy that
    /// API's slice contracts. The projected schema and all cell storage are shared.
    #[must_use]
    pub fn as_borrowed(&self) -> TableSelection<'_> {
        TableSelection::from_parts(
            &self.source,
            self.positions().collect(),
            self.columns().to_vec(),
            Arc::clone(&self.plan.schema),
        )
    }

    /// Copies selected cells, extras, properties and tombstones into an independent
    /// table. Converters are not rerun; the source remains immutable.
    ///
    /// # Errors
    /// Returns any internal selection consistency error.
    pub fn materialize(&self) -> Result<Table, TableError> {
        self.source
            .select(&self.positions().collect::<Vec<_>>(), self.columns())
    }
}

impl Table {
    /// Moves this table into an owning immutable selection of every row/column.
    ///
    /// Cell payloads are not copied. Use [`SharedTableSelection::new`] to retain
    /// an existing `Arc<Table>` instead. [`SharedTableSelection::materialize`]
    /// creates an independent mutable table when needed.
    #[must_use]
    pub fn into_shared(self) -> SharedTableSelection {
        SharedTableSelection::new(self)
    }
}

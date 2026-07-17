//! Schema-driven typed column storage.

mod index;
mod ordering;
mod row_mut;
mod schema;
mod storage;
mod views;

pub use index::{ColumnIndex, IndexKeyRef};
pub use ordering::{NullOrder, RowOrder, SortDirection};
pub use row_mut::{RowMut, RowsMut};
pub use schema::{ColumnSpec, Schema, UnknownFields};
pub use views::{Column, ColumnElement, ColumnMut, ColumnSliceError, Row};

use compact_str::CompactString;
use std::sync::Arc;
#[cfg(test)]
use storage::ColumnData;
use storage::{ColumnStorage, ExtrasStorage, RowExtras};
use thiserror::Error;

use crate::{DataType, Value, ValueRef};

/// A table schema, storage, or access error.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum TableError {
    /// Two columns claim the same name or alias.
    #[error("duplicate column name or alias: {0}")]
    DuplicateColumnName(CompactString),
    /// A row contains a different number of values than the schema.
    #[error("expected {expected} row values, found {actual}")]
    RowWidth {
        /// Required value count.
        expected: usize,
        /// Supplied value count.
        actual: usize,
    },
    /// A cell value does not match its column type.
    #[error("column {column} expects {expected}, found {actual}")]
    TypeMismatch {
        /// Positional column index.
        column: usize,
        /// Schema type.
        expected: DataType,
        /// Supplied type.
        actual: DataType,
    },
    /// A null value was supplied to a non-nullable column.
    #[error("column {column} is not nullable")]
    NullNotAllowed {
        /// Positional column index.
        column: usize,
    },
    /// A row position is outside the table.
    #[error("row {row} is out of bounds for {row_count} rows")]
    RowOutOfBounds {
        /// Requested row.
        row: usize,
        /// Current row count.
        row_count: usize,
    },
    /// A column position is outside the schema.
    #[error("column {column} is out of bounds for {column_count} columns")]
    ColumnOutOfBounds {
        /// Requested column.
        column: usize,
        /// Current column count.
        column_count: usize,
    },
    /// No primary name or alias matches the query.
    #[error("column not found: {0}")]
    ColumnNotFound(CompactString),
    /// A value supplied as an extra uses a declared column name or alias.
    #[error("extra field conflicts with schema column or alias: {0}")]
    ExtraFieldConflictsWithColumn(CompactString),
    /// Hash indexes do not support this logical type.
    #[error("columns of type {0} cannot be indexed")]
    UnsupportedIndexType(DataType),
}

/// A schema-driven table whose fixed columns store their primitive types
/// directly and whose optional unknown fields are owned by individual rows.
///
/// Required columns store contiguous `T` values directly. Nullable columns use
/// `Option<T>` to represent null cells. Schemas that reject unknown fields do not
/// allocate per-row extras storage. The immutable schema is shared through
/// [`Arc`], so tables with the same layout do not duplicate schema metadata.
#[derive(Clone, Debug)]
pub struct Table {
    schema: Arc<Schema>,
    columns: Vec<ColumnStorage>,
    extras: ExtrasStorage,
    row_count: usize,
}

impl Table {
    /// Creates an empty table from an owned or shared schema.
    #[must_use]
    pub fn new(schema: impl Into<Arc<Schema>>) -> Self {
        Self::with_capacity(schema, 0)
    }

    /// Creates an empty table with per-column capacity for `capacity` rows from
    /// an owned or shared schema.
    #[must_use]
    pub fn with_capacity(schema: impl Into<Arc<Schema>>, capacity: usize) -> Self {
        let schema = schema.into();
        let columns = schema
            .iter()
            .map(|column| ColumnStorage::new(column.data_type(), column.is_nullable(), capacity))
            .collect();
        let extras = ExtrasStorage::with_capacity(schema.unknown_fields(), capacity);
        Self {
            schema,
            columns,
            extras,
            row_count: 0,
        }
    }

    /// Returns the immutable schema.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        self.schema.as_ref()
    }

    /// Clones the shared schema handle.
    ///
    /// The schema metadata is not copied.
    #[must_use]
    pub fn schema_arc(&self) -> Arc<Schema> {
        Arc::clone(&self.schema)
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.row_count
    }

    /// Returns the number of columns.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.schema.len()
    }

    /// Returns whether the table has no rows.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Appends one complete row after validating every value.
    ///
    /// The operation is atomic with respect to type/null validation: no column
    /// changes if any supplied value is invalid.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::RowWidth`], [`TableError::TypeMismatch`], or
    /// [`TableError::NullNotAllowed`] when the row does not match the schema.
    pub fn push_row<const N: usize>(&mut self, values: [Value; N]) -> Result<usize, TableError> {
        self.validate_row(&values)?;
        Ok(self.push_validated_row(values))
    }

    /// Appends one complete fixed row together with row-local extra fields.
    ///
    /// Extra names must not match a declared column or alias. Repeated extra
    /// names replace the earlier value, matching [`Table::set_named`]. The
    /// complete operation is validated before the table changes.
    ///
    /// # Errors
    ///
    /// Returns the ordinary row-validation errors, [`TableError::ColumnNotFound`]
    /// when the schema rejects unknown fields, or
    /// [`TableError::ExtraFieldConflictsWithColumn`] for a declared name.
    pub fn push_row_with_extras<const N: usize, I, K, V>(
        &mut self,
        values: [Value; N],
        extras: I,
    ) -> Result<usize, TableError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<CompactString>,
        V: Into<Value>,
    {
        self.validate_row(&values)?;
        let extras = self.collect_extras(extras)?;
        let row = self.push_validated_row(values);
        if !extras.is_empty() {
            self.extras.set(row, extras);
        }
        Ok(row)
    }

    /// Appends one runtime-width owned row after validating every value.
    ///
    /// Use [`Table::push_row`] when the row width is known at the call site.
    /// This method consumes an existing vector and does not allocate a second
    /// staging buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::RowWidth`], [`TableError::TypeMismatch`], or
    /// [`TableError::NullNotAllowed`] when the row does not match the schema.
    pub fn push_row_vec(&mut self, values: Vec<Value>) -> Result<usize, TableError> {
        self.validate_row(&values)?;
        Ok(self.push_validated_row(values))
    }

    fn validate_row(&self, values: &[Value]) -> Result<(), TableError> {
        if values.len() != self.column_count() {
            return Err(TableError::RowWidth {
                expected: self.column_count(),
                actual: values.len(),
            });
        }
        for (column, (spec, value)) in self.schema.iter().zip(values.iter()).enumerate() {
            validate_cell(spec, value, column)?;
        }
        Ok(())
    }

    fn collect_extras<I, K, V>(&self, extras: I) -> Result<RowExtras, TableError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<CompactString>,
        V: Into<Value>,
    {
        let extras = extras.into_iter();
        let mut collected = RowExtras::with_capacity(extras.size_hint().0);
        for (name, value) in extras {
            let name = name.into();
            if self.schema.column_index(&name).is_some() {
                return Err(TableError::ExtraFieldConflictsWithColumn(name));
            }
            if self.schema.unknown_fields() == UnknownFields::Reject {
                return Err(TableError::ColumnNotFound(name));
            }
            collected.set(name, value.into());
        }
        Ok(collected)
    }

    fn push_validated_row(&mut self, values: impl IntoIterator<Item = Value>) -> usize {
        let row = self.row_count;
        for (storage, value) in self.columns.iter_mut().zip(values) {
            storage.push_validated(value);
        }
        self.extras.push_empty();
        self.row_count += 1;
        debug_assert!(
            self.columns
                .iter()
                .all(|column| column.len() == self.row_count)
        );
        debug_assert!(self.extras.len_matches(self.row_count));
        row
    }

    /// Removes and discards the last row, returning whether a row existed.
    pub fn pop_row(&mut self) -> bool {
        if self.row_count == 0 {
            return false;
        }
        for column in &mut self.columns {
            column.pop();
        }
        self.extras.pop();
        self.row_count -= 1;
        true
    }

    /// Reads a cell by row and column position.
    ///
    /// # Errors
    ///
    /// Returns an out-of-bounds error for an invalid row or column.
    pub fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, TableError> {
        self.validate_position(row, column)?;
        self.columns
            .get(column)
            .and_then(|storage| storage.get(row))
            .ok_or(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count,
            })
    }

    /// Reads a cell by primary column name, alias, or stored row-local name.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnNotFound`] when neither the fixed schema nor
    /// the selected row contains the name, or an out-of-bounds row error.
    pub fn cell_named(&self, row: usize, name_or_alias: &str) -> Result<ValueRef<'_>, TableError> {
        if let Some(column) = self.schema.column_index(name_or_alias) {
            return self.cell(row, column);
        }
        if self.schema.unknown_fields() == UnknownFields::Reject {
            return Err(TableError::ColumnNotFound(name_or_alias.into()));
        }
        self.validate_row_position(row)?;
        self.extras
            .get(row)
            .and_then(|extras| extras.get(name_or_alias))
            .map(Value::as_ref)
            .ok_or_else(|| TableError::ColumnNotFound(name_or_alias.into()))
    }

    /// Replaces one cell after exact type and nullability validation.
    ///
    /// # Errors
    ///
    /// Returns a position, type, or nullability error without changing the cell.
    pub fn set_cell(&mut self, row: usize, column: usize, value: Value) -> Result<(), TableError> {
        self.validate_position(row, column)?;
        let spec = self
            .schema
            .column(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            })?;
        validate_cell(spec, &value, column)?;
        self.columns[column].set_validated(row, value);
        Ok(())
    }

    /// Replaces a fixed cell or stores an unknown name as a row-local value.
    ///
    /// Declared names and aliases retain exact schema type/null validation.
    /// Unknown names are accepted only when the schema uses
    /// [`UnknownFields::Store`]. Setting an existing extra replaces its value.
    ///
    /// # Errors
    ///
    /// Returns a row, type, or nullability error for fixed cells, or
    /// [`TableError::ColumnNotFound`] when an unknown name is rejected.
    pub fn set_named(
        &mut self,
        row: usize,
        name_or_alias: &str,
        value: impl Into<Value>,
    ) -> Result<(), TableError> {
        let value = value.into();
        if let Some(column) = self.schema.column_index(name_or_alias) {
            return self.set_cell(row, column, value);
        }
        if self.schema.unknown_fields() == UnknownFields::Reject {
            return Err(TableError::ColumnNotFound(name_or_alias.into()));
        }
        self.validate_row_position(row)?;
        self.extras
            .get_or_insert(row)
            .set(name_or_alias.into(), value);
        Ok(())
    }

    /// Returns a borrowing column view by position.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<Column<'_>> {
        Some(Column {
            spec: self.schema.column(position)?,
            storage: self.columns.get(position)?,
        })
    }

    /// Returns a borrowing column view by primary name or alias.
    #[must_use]
    pub fn column_named(&self, name_or_alias: &str) -> Option<Column<'_>> {
        self.column(self.schema.column_index(name_or_alias)?)
    }

    /// Borrows one column immutably and a distinct column mutably.
    ///
    /// This is the safe bulk-transform counterpart to [`Table::column`]. It is
    /// useful when values from one column are mapped directly into another
    /// column, including with parallel slice iterators. The returned views can
    /// each perform their normal runtime type and nullability check once.
    ///
    /// Returns `None` when either position is out of bounds or both positions
    /// identify the same column.
    #[must_use]
    pub fn column_pair_mut(
        &mut self,
        source: usize,
        target: usize,
    ) -> Option<(Column<'_>, ColumnMut<'_>)> {
        if source == target {
            return None;
        }
        let source_spec = self.schema.column(source)?;
        let target_spec = self.schema.column(target)?;
        let (source_storage, target_storage) = if source < target {
            let (before_target, target_and_after) = self.columns.split_at_mut(target);
            (before_target.get(source)?, target_and_after.first_mut()?)
        } else {
            let (before_source, source_and_after) = self.columns.split_at_mut(source);
            (source_and_after.first()?, before_source.get_mut(target)?)
        };
        Some((
            Column {
                spec: source_spec,
                storage: source_storage,
            },
            ColumnMut {
                spec: target_spec,
                storage: target_storage,
            },
        ))
    }

    /// Returns one borrowing row view.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<Row<'_>> {
        (row < self.row_count).then_some(Row { table: self, row })
    }

    /// Iterates over borrowing row views.
    #[must_use]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = Row<'_>> + DoubleEndedIterator {
        (0..self.row_count).map(|row| Row { table: self, row })
    }

    /// Builds an `ahash` index for one supported column.
    ///
    /// # Errors
    ///
    /// Returns a column bounds error or [`TableError::UnsupportedIndexType`].
    pub fn index(&self, column: usize) -> Result<ColumnIndex<'_>, TableError> {
        ColumnIndex::new(self, column)
    }

    /// Builds a stable row permutation ordered by one column.
    ///
    /// The table is not mutated or copied. Equal keys retain insertion order,
    /// null placement is independent of direction, and floating-point columns
    /// use [`f32::total_cmp`] or [`f64::total_cmp`]. The operation takes
    /// **O(r log r)** time and **O(r)** space for `r` rows.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnOutOfBounds`] for an invalid column.
    pub fn row_order(
        &self,
        column: usize,
        direction: SortDirection,
        null_order: NullOrder,
    ) -> Result<RowOrder<'_>, TableError> {
        let storage = self
            .columns
            .get(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            })?;
        let mut positions: Vec<_> = (0..self.row_count).collect();
        positions.sort_by(|left, right| storage.compare_rows(*left, *right, direction, null_order));
        Ok(RowOrder {
            table: self,
            positions,
        })
    }

    /// Builds a stable row permutation ordered by a column name or alias.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnNotFound`] when the name is unknown.
    pub fn row_order_named(
        &self,
        name_or_alias: &str,
        direction: SortDirection,
        null_order: NullOrder,
    ) -> Result<RowOrder<'_>, TableError> {
        let column = self
            .schema
            .column_index(name_or_alias)
            .ok_or_else(|| TableError::ColumnNotFound(name_or_alias.into()))?;
        self.row_order(column, direction, null_order)
    }

    fn validate_position(&self, row: usize, column: usize) -> Result<(), TableError> {
        self.validate_row_position(row)?;
        if column >= self.column_count() {
            return Err(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            });
        }
        Ok(())
    }

    fn validate_row_position(&self, row: usize) -> Result<(), TableError> {
        if row >= self.row_count {
            return Err(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count,
            });
        }
        Ok(())
    }
}

fn validate_cell(spec: &ColumnSpec, value: &Value, column: usize) -> Result<(), TableError> {
    let actual = value.data_type();
    if actual == DataType::Null {
        if spec.is_nullable() {
            return Ok(());
        }
        return Err(TableError::NullNotAllowed { column });
    }
    if actual != spec.data_type() {
        return Err(TableError::TypeMismatch {
            column,
            expected: spec.data_type(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        ColumnData, ColumnSpec, ColumnStorage, DataType, ExtrasStorage, Schema, Table,
        UnknownFields, Value,
    };

    #[test]
    fn column_storage_follows_schema_nullability() {
        assert!(matches!(
            ColumnStorage::new(DataType::I64, false, 8),
            ColumnStorage::I64(ColumnData::Required(values)) if values.capacity() >= 8
        ));
        assert!(matches!(
            ColumnStorage::new(DataType::I64, true, 8),
            ColumnStorage::I64(ColumnData::Nullable(values)) if values.capacity() >= 8
        ));
        assert!(matches!(
            ColumnStorage::new(DataType::String, false, 8),
            ColumnStorage::String(ColumnData::Required(values)) if values.capacity() >= 8
        ));
        assert!(matches!(
            ColumnStorage::new(DataType::String, true, 8),
            ColumnStorage::String(ColumnData::Nullable(values)) if values.capacity() >= 8
        ));
    }

    #[test]
    fn extras_storage_follows_schema_policy() {
        let strict_schema = Schema::new([ColumnSpec::new("id", DataType::U64)]).unwrap();
        let mut strict = Table::with_capacity(strict_schema, 8);
        assert!(matches!(&strict.extras, ExtrasStorage::Disabled));
        strict.push_row([Value::U64(1)]).unwrap();
        assert!(matches!(&strict.extras, ExtrasStorage::Disabled));
        assert!(strict.pop_row());

        let open_schema = Schema::new([ColumnSpec::new("id", DataType::U64)])
            .unwrap()
            .with_unknown_fields(UnknownFields::Store);
        let mut open = Table::with_capacity(open_schema, 8);
        assert!(matches!(
            &open.extras,
            ExtrasStorage::Enabled(rows) if rows.is_empty() && rows.capacity() >= 8
        ));
        let row = open.push_row([Value::U64(1)]).unwrap();
        assert!(matches!(
            &open.extras,
            ExtrasStorage::Enabled(rows) if rows.len() == 1 && rows[0].is_none()
        ));
        open.set_named(row, "dynamic", "value").unwrap();
        assert!(matches!(
            &open.extras,
            ExtrasStorage::Enabled(rows) if rows[0].is_some()
        ));
    }

    #[test]
    fn tables_share_arc_schema_ownership() {
        let schema = Arc::new(
            Schema::new([
                ColumnSpec::new("id", DataType::U64),
                ColumnSpec::new("enabled", DataType::Bool),
            ])
            .unwrap(),
        );

        let first = Table::new(Arc::clone(&schema));
        let second = Table::with_capacity(Arc::clone(&schema), 8);
        let from_table = first.schema_arc();

        assert!(std::ptr::eq(first.schema(), schema.as_ref()));
        assert!(std::ptr::eq(second.schema(), schema.as_ref()));
        assert!(Arc::ptr_eq(&schema, &from_table));
        assert_eq!(Arc::strong_count(&schema), 4);
    }
}

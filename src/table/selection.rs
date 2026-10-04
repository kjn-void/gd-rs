//! Filtering, projection, and indexed join row selection.

use super::{ColumnIndex, IndexKeyRef, Row, Schema, Table, TableError};

impl Table {
    /// Selects live row positions in source order using a predicate.
    ///
    /// The predicate runs once per live row; tombstoned rows are skipped. Positions
    /// are a snapshot and must not be reused after compaction or row removal.
    pub fn select_rows(&self, mut predicate: impl FnMut(Row<'_>) -> bool) -> Vec<usize> {
        self.live_rows()
            .filter_map(|row| predicate(row).then_some(row.position()))
            .collect()
    }

    /// Copies matching live rows into independent storage, preserving source order.
    ///
    /// Schema metadata is shared; values, extras, and table properties are copied.
    /// This is selection followed by column-wise gathering, not per-cell insertion.
    ///
    /// # Errors
    ///
    /// Returns a table error if gathering the selected rows fails.
    pub fn filter_rows(&self, predicate: impl FnMut(Row<'_>) -> bool) -> Result<Self, TableError> {
        self.copy_rows(&self.select_rows(predicate))
    }

    /// Copies the requested columns in selection order into independent storage.
    ///
    /// All physical rows, tombstones, extras, and properties are preserved. Column
    /// specifications retain names, aliases, nullability, and converters. An empty
    /// projection retains the row count. Duplicate columns are rejected because
    /// their schema names would collide. Use [`Self::filter_rows`] to select live rows.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnOutOfBounds`] for an invalid column, or
    /// [`TableError::DuplicateColumnName`] for repeated columns.
    pub fn project(&self, columns: &[usize]) -> Result<Self, TableError> {
        let schema = self.projection_schema(columns)?;
        Ok(Self {
            schema: schema.into(),
            columns: columns
                .iter()
                .map(|&column| self.columns[column].clone())
                .collect(),
            extras: self.extras.clone(),
            tombstones: self.tombstones.clone(),
            properties: self.properties.clone(),
            row_count: self.row_count,
        })
    }

    /// Copies selected rows and columns without an intermediate full-width table.
    ///
    /// Row order and repeated row positions are preserved, including tombstones.
    /// Column order, metadata, extras, and properties follow [`Self::project`].
    /// The result owns independent cell storage.
    ///
    /// # Errors
    ///
    /// Returns a row/column bounds error or a duplicate column name error.
    pub fn select(&self, rows: &[usize], columns: &[usize]) -> Result<Self, TableError> {
        let schema = self.projection_schema(columns)?;
        if let Some(&row) = rows.iter().find(|&&row| row >= self.row_count) {
            return Err(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count,
            });
        }
        Ok(Self {
            schema: schema.into(),
            columns: columns
                .iter()
                .map(|&column| self.columns[column].copy_rows(rows))
                .collect(),
            extras: self.extras.copy_rows(rows),
            tombstones: self.tombstones.copy_rows(rows),
            properties: self.properties.clone(),
            row_count: rows.len(),
        })
    }

    fn projection_schema(&self, columns: &[usize]) -> Result<Schema, TableError> {
        let specs = columns
            .iter()
            .map(|&column| {
                self.schema
                    .column(column)
                    .cloned()
                    .ok_or(TableError::ColumnOutOfBounds {
                        column,
                        column_count: self.column_count(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Schema::new(specs)?.with_unknown_fields(self.schema.unknown_fields()))
    }

    /// Selects row pairs for a left equality join against a reusable right index.
    ///
    /// Only live rows participate. Null never equals null. Each left row produces
    /// all matching right rows in right source order, or one pair with `None` if
    /// unmatched. Left source order is preserved. Both columns must have the same
    /// logical type, supported by [`ColumnIndex`]. Cell payloads are not copied.
    ///
    /// Returned positions are a snapshot; do not reuse them after removing or
    /// compacting rows in either table. The right index borrows its source table,
    /// preventing mutation while it is in use.
    ///
    /// # Errors
    ///
    /// Returns a column bounds, type mismatch, or unsupported index type error.
    pub fn left_join_rows(
        &self,
        column: usize,
        right: &ColumnIndex<'_>,
    ) -> Result<Vec<(usize, Option<usize>)>, TableError> {
        let spec = self
            .schema
            .column(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            })?;
        let expected = right
            .table()
            .schema()
            .column(right.column())
            .ok_or(TableError::InternalInvariant {
                detail: "index column is missing",
            })?
            .data_type();
        if spec.data_type() != expected {
            return Err(TableError::TypeMismatch {
                column,
                expected,
                actual: spec.data_type(),
            });
        }
        let mut pairs = Vec::with_capacity(self.live_row_count());
        for row in self.live_rows() {
            let matches = row
                .get(column)
                .and_then(IndexKeyRef::from_value)
                .map_or(&[][..], |key| right.rows(key));
            if matches.is_empty() {
                pairs.push((row.position(), None));
            } else {
                pairs.extend(matches.iter().map(|&other| (row.position(), Some(other))));
            }
        }
        Ok(pairs)
    }
}

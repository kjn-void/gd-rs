//! Borrowing composite hash indexes with fixed-width tuple keys.

use ahash::AHashMap;
use smallvec::SmallVec;

use super::{IndexKeyRef, Row, Table, TableError};
use crate::DataType;

/// A borrowing hash index over `N` ordered key columns.
///
/// Any null component excludes a live row from key buckets. Tombstoned rows are
/// tracked separately. Supported component types match [`super::ColumnIndex`];
/// floats and `Null`-typed columns are rejected. Keys borrow text/bytes without
/// copying payloads. The table cannot be mutated while the index is in use.
///
/// ```compile_fail
/// use gd::{ColumnSpec, DataType, Schema, Table, Value};
/// let mut table = Table::new(Schema::new([
///     ColumnSpec::new("id", DataType::I64),
/// ]).unwrap());
/// let index = table.composite_index([0]).unwrap();
/// table.push_row([Value::I64(1)]).unwrap();
/// let _ = index.distinct_key_count();
/// ```
#[derive(Clone, Debug)]
pub struct CompositeIndex<'a, const N: usize> {
    table: &'a Table,
    columns: [usize; N],
    by_key: AHashMap<[IndexKeyRef<'a>; N], usize>,
    matches: Vec<SmallVec<[usize; 1]>>,
    null_rows: Vec<usize>,
    tombstoned_rows: Vec<usize>,
}

impl<'a, const N: usize> CompositeIndex<'a, N> {
    fn new(table: &'a Table, columns: [usize; N]) -> Result<Self, TableError> {
        if N == 0 {
            return Err(TableError::EmptyIndexKey);
        }
        for column in columns {
            let spec = table
                .schema()
                .column(column)
                .ok_or(TableError::ColumnOutOfBounds {
                    column,
                    column_count: table.column_count(),
                })?;
            if matches!(
                spec.data_type(),
                DataType::Null | DataType::F32 | DataType::F64
            ) {
                return Err(TableError::UnsupportedIndexType(spec.data_type()));
            }
        }
        let mut index = Self {
            table,
            columns,
            by_key: AHashMap::with_capacity(table.live_row_count()),
            matches: Vec::new(),
            null_rows: Vec::new(),
            tombstoned_rows: Vec::new(),
        };
        for row in table.rows() {
            if row.is_tombstoned() {
                index.tombstoned_rows.push(row.position());
            } else if let Some(key) = row_key(row, columns) {
                let bucket = *index.by_key.entry(key).or_insert_with(|| {
                    index.matches.push(SmallVec::new());
                    index.matches.len() - 1
                });
                index.matches[bucket].push(row.position());
            } else {
                index.null_rows.push(row.position());
            }
        }
        Ok(index)
    }

    /// Returns the source table.
    #[must_use]
    pub const fn table(&self) -> &'a Table {
        self.table
    }

    /// Returns key column positions in tuple order.
    #[must_use]
    pub const fn columns(&self) -> &[usize; N] {
        &self.columns
    }

    /// Returns every exact match in source order. Component kinds must match;
    /// signed and unsigned keys are distinct and no text conversion occurs.
    #[must_use]
    pub fn rows(&self, key: [IndexKeyRef<'_>; N]) -> &[usize] {
        let bucket = self.by_key.get(&key).copied();
        bucket.map_or(&[], |position| self.matches[position].as_slice())
    }

    /// Returns live rows with at least one null component.
    #[must_use]
    pub fn null_rows(&self) -> &[usize] {
        &self.null_rows
    }

    /// Returns tombstoned rows excluded from the index.
    #[must_use]
    pub fn tombstoned_rows(&self) -> &[usize] {
        &self.tombstoned_rows
    }

    /// Returns the number of distinct complete non-null keys.
    #[must_use]
    pub fn distinct_key_count(&self) -> usize {
        self.by_key.len()
    }
}

fn row_key<const N: usize>(row: Row<'_>, columns: [usize; N]) -> Option<[IndexKeyRef<'_>; N]> {
    // Initialize on the stack, then replace every component. No key allocation.
    let mut key = [IndexKeyRef::Bool(false); N];
    for (slot, column) in key.iter_mut().zip(columns) {
        *slot = IndexKeyRef::from_value(row.get(column)?)?;
    }
    Some(key)
}

impl Table {
    /// Builds an immutable hash index over an ordered tuple of columns.
    ///
    /// Construction is expected O(rows × N); duplicate matches retain physical
    /// source order. Repeated columns are allowed. Nulls never form a key.
    ///
    /// ```
    /// use gd::{ColumnSpec, DataType, Schema, Table, Value};
    /// let mut table = Table::new(Schema::new([
    ///     ColumnSpec::new("customer", DataType::String),
    ///     ColumnSpec::new("order", DataType::I64),
    /// ])?);
    /// table.push_row([Value::from("Ada"), Value::I64(7)])?;
    /// let index = table.composite_index([0, 1])?;
    /// assert_eq!(index.rows(["Ada".into(), 7_i64.into()]), &[0]);
    /// # Ok::<(), gd::TableError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an empty-key, column-bounds, or unsupported-component-type error,
    /// including when the table has no rows.
    pub fn composite_index<const N: usize>(
        &self,
        columns: [usize; N],
    ) -> Result<CompositeIndex<'_, N>, TableError> {
        CompositeIndex::new(self, columns)
    }

    /// Lazily joins live rows against a composite index, borrowing both inputs.
    ///
    /// Each left row produces all right matches in source order, or one `None`.
    /// A null in any component never matches. Each component's logical type must
    /// match exactly. Cell data and result pairs are not materialized. Iteration
    /// keeps both sources borrowed; stop iteration to cap result production.
    ///
    /// # Errors
    ///
    /// Returns column-bounds or component-type mismatch errors before iteration.
    pub fn left_join_composite<'a, const N: usize>(
        &'a self,
        columns: [usize; N],
        right: &'a CompositeIndex<'_, N>,
    ) -> Result<impl Iterator<Item = (Row<'a>, Option<Row<'a>>)> + 'a, TableError> {
        for (left, &other) in columns.iter().zip(right.columns()) {
            let spec = self
                .schema()
                .column(*left)
                .ok_or(TableError::ColumnOutOfBounds {
                    column: *left,
                    column_count: self.column_count(),
                })?;
            let expected = right
                .table()
                .schema()
                .column(other)
                .ok_or(TableError::InternalInvariant {
                    detail: "composite index column is missing",
                })?
                .data_type();
            if spec.data_type() != expected {
                return Err(TableError::TypeMismatch {
                    column: *left,
                    expected,
                    actual: spec.data_type(),
                });
            }
        }
        Ok(self.live_rows().flat_map(move |row| {
            let matches = row_key(row, columns).map_or(&[][..], |key| right.rows(key));
            let unmatched = matches.is_empty().then_some((row, None));
            unmatched.into_iter().chain(
                matches
                    .iter()
                    .map(move |&position| (row, right.table().row(position))),
            )
        }))
    }

    /// Collects physical row pairs from [`Self::left_join_composite`].
    ///
    /// The owned positions do not retain source borrows and become stale after
    /// removal/compaction. Prefer the iterator when accessing joined row payloads.
    ///
    /// # Errors
    ///
    /// Returns the join's column-bounds or component-type mismatch errors.
    pub fn left_join_rows_composite<const N: usize>(
        &self,
        columns: [usize; N],
        right: &CompositeIndex<'_, N>,
    ) -> Result<Vec<(usize, Option<usize>)>, TableError> {
        Ok(self
            .left_join_composite(columns, right)?
            .map(|(left, right)| (left.position(), right.map(Row::position)))
            .collect())
    }
}

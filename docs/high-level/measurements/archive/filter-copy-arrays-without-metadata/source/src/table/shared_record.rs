//! A typed table with one column of shared record handles.

use std::sync::Arc;

use super::TableError;

/// A single-column table whose cells are `Arc<S>` handles to typed records.
///
/// Filtering and copying clone handles rather than record payloads. A destination
/// remains valid after the source is dropped, but initially shares its records.
/// [`Self::get_mut`] uses copy-on-write through [`Arc::make_mut`]. Interior
/// mutability or sharing inside `S` follows that type's own semantics.
///
/// Struct fields are accessed through Rust rather than a dynamic [`super::Schema`].
/// This type does not extend [`crate::Value`] or the SQL/formatting adapters.
#[derive(Debug)]
pub struct SharedRecordTable<S> {
    records: Vec<Arc<S>>,
}

impl<S> Default for SharedRecordTable<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Clone for SharedRecordTable<S> {
    fn clone(&self) -> Self {
        Self {
            records: self.records.clone(),
        }
    }
}

impl<S> From<Vec<Arc<S>>> for SharedRecordTable<S> {
    fn from(records: Vec<Arc<S>>) -> Self {
        Self { records }
    }
}

impl<S> SharedRecordTable<S> {
    /// Creates an empty shared-record table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Reserves space for the specified number of record handles.
    #[must_use]
    pub fn with_capacity(rows: usize) -> Self {
        Self {
            records: Vec::with_capacity(rows),
        }
    }

    /// Returns the number of records.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.records.len()
    }

    /// Returns the single shared-record column's handle slice.
    #[must_use]
    pub fn as_slice(&self) -> &[Arc<S>] {
        &self.records
    }

    /// Returns a record without cloning its handle.
    #[must_use]
    pub fn get(&self, row: usize) -> Option<&S> {
        self.records.get(row).map(AsRef::as_ref)
    }

    /// Returns exclusive access, cloning a shared record before editing it.
    ///
    /// The clone depth follows `S::clone`; an unshared record needs no clone.
    pub fn get_mut(&mut self, row: usize) -> Option<&mut S>
    where
        S: Clone,
    {
        self.records.get_mut(row).map(Arc::make_mut)
    }

    /// Allocates one shared record and appends its handle.
    pub fn push(&mut self, record: S) {
        self.records.push(Arc::new(record));
    }

    /// Appends an existing shared handle without cloning it.
    pub fn push_shared(&mut self, record: Arc<S>) {
        self.records.push(record);
    }

    fn validate_rows(&self, rows: &[usize]) -> Result<(), TableError> {
        if let Some(&row) = rows.iter().find(|&&row| row >= self.row_count()) {
            return Err(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count(),
            });
        }
        Ok(())
    }

    /// Copies selected handles in order, preserving duplicate positions.
    ///
    /// # Errors
    /// Returns [`TableError::RowOutOfBounds`] for the first invalid position.
    pub fn copy_rows(&self, rows: &[usize]) -> Result<Self, TableError> {
        self.validate_rows(rows)?;
        Ok(Self::from(
            rows.iter()
                .map(|&row| Arc::clone(&self.records[row]))
                .collect::<Vec<_>>(),
        ))
    }

    /// Filters records in source order into one table of cloned handles.
    ///
    /// Reserves the source row count as an upper bound to avoid repeated target
    /// growth; only matched handles are constructed and dropped.
    #[must_use]
    pub fn filter(&self, mut predicate: impl FnMut(&S) -> bool) -> Self {
        let mut records = Vec::with_capacity(self.row_count());
        for record in &self.records {
            if predicate(record) {
                records.push(Arc::clone(record));
            }
        }
        Self::from(records)
    }
}

#[cfg(feature = "rayon")]
impl<S: Send + Sync> SharedRecordTable<S> {
    /// Copies selected handles in parallel using the current Rayon pool.
    ///
    /// The result is one table, with selection order and duplicates preserved.
    ///
    /// # Errors
    /// Returns [`TableError::RowOutOfBounds`] for the first invalid position.
    pub fn par_copy_rows(&self, rows: &[usize]) -> Result<Self, TableError> {
        use rayon::prelude::*;
        self.validate_rows(rows)?;
        Ok(Self::from(
            rows.par_iter()
                .map(|&row| Arc::clone(&self.records[row]))
                .collect::<Vec<_>>(),
        ))
    }

    /// Filters in parallel into one table, preserving source order.
    ///
    /// Rayon filters and clones handles together into local buffers, then
    /// concatenates them into one destination vector in source order. Records
    /// and strings are not cloned. Pool control remains with the caller.
    #[must_use]
    pub fn par_filter(&self, predicate: impl Fn(&S) -> bool + Send + Sync) -> Self {
        use rayon::prelude::*;
        Self::from(
            self.records
                .par_iter()
                .filter_map(|record| predicate(record).then(|| Arc::clone(record)))
                .collect::<Vec<_>>(),
        )
    }
}

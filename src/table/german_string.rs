//! Borrowing views over German/Umbra string columns.

use german_str::GermanStr;
use thiserror::Error;

use super::storage::{CellValue, ColumnData};

/// An invalid write through a German string view.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GermanStringError {
    /// The requested row is outside this view.
    #[error("row {row} is out of bounds for {row_count} rows")]
    RowOutOfBounds {
        /// Requested relative row position.
        row: usize,
        /// Number of rows in the view.
        row_count: usize,
    },
    /// The UTF-8 encoding exceeds the layout's 32-bit byte length.
    #[error("string uses {actual} bytes, exceeding the German string layout limit")]
    TooLong {
        /// Supplied UTF-8 byte length.
        actual: usize,
    },
    /// A null write was attempted on required storage.
    #[error("the string column does not allow nulls")]
    NullNotAllowed,
}

/// A borrowed German/Umbra string column. Nulls and empty strings are distinct.
///
/// The underlying cells use a 16-byte layout with an inline prefix and either
/// eight inline bytes or a heap pointer. Reading a cell never allocates.
#[derive(Clone, Copy, Debug)]
pub struct GermanStrings<'a> {
    values: &'a ColumnData<GermanStr>,
}

impl<'a> GermanStrings<'a> {
    pub(super) const fn new(values: &'a ColumnData<GermanStr>) -> Self {
        Self { values }
    }

    /// Returns the number of physical cells, including tombstones.
    #[must_use]
    pub fn len(self) -> usize {
        self.values.len()
    }

    /// Returns whether this view has no cells.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.values.is_empty()
    }

    /// Reads a cell: outer `None` means out of bounds, inner `None` means null.
    #[must_use]
    pub fn get(self, row: usize) -> Option<Option<&'a str>> {
        match self.values.get(row) {
            CellValue::OutOfBounds => None,
            CellValue::Null => Some(None),
            CellValue::Value(value) => Some(Some(value.as_str())),
        }
    }

    /// Iterates strings in physical row order without allocating.
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn iter(self) -> impl ExactSizeIterator<Item = Option<&'a str>> + DoubleEndedIterator {
        (0..self.len()).map(move |row| self.get(row).flatten())
    }
}

/// A mutable German/Umbra string column view.
#[derive(Debug)]
pub struct GermanStringsMut<'a> {
    values: &'a mut ColumnData<GermanStr>,
}

impl<'a> GermanStringsMut<'a> {
    pub(super) const fn new(values: &'a mut ColumnData<GermanStr>) -> Self {
        Self { values }
    }

    /// Returns the number of physical cells in this view.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether this view has no cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Borrows this view for allocation-free reading.
    #[must_use]
    pub fn as_view(&self) -> GermanStrings<'_> {
        GermanStrings::new(self.values)
    }

    /// Reads a relative cell, distinguishing out-of-bounds, null, and empty text.
    #[must_use]
    pub fn get(&self, row: usize) -> Option<Option<&str>> {
        self.as_view().get(row)
    }

    /// Replaces one cell without changing the row offset.
    ///
    /// # Errors
    ///
    /// Returns a row-bounds, nullability, or 32-bit length error before changing
    /// the cell.
    pub fn set(&mut self, row: usize, value: Option<&str>) -> Result<(), GermanStringError> {
        let row_count = self.len();
        if row >= row_count {
            return Err(GermanStringError::RowOutOfBounds { row, row_count });
        }
        let value = if let Some(text) = value {
            if text.len() > u32::MAX as usize {
                return Err(GermanStringError::TooLong { actual: text.len() });
            }
            Some(
                GermanStr::new(text)
                    .map_err(|_| GermanStringError::TooLong { actual: text.len() })?,
            )
        } else {
            if matches!(self.values, ColumnData::Required(_)) {
                return Err(GermanStringError::NullNotAllowed);
            }
            None
        };
        self.values
            .set(row, value)
            .map_err(|_| GermanStringError::RowOutOfBounds { row, row_count })
    }
}

//! Fixed-capacity UTF-8 cells addressed through offsets into a column buffer.

use thiserror::Error;

const NULL_LENGTH: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
struct StringIndex {
    offset: usize,
    length: usize,
}

/// An invalid write to a fixed-capacity string view.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FixedStringError {
    /// The requested row is outside the borrowed view.
    #[error("row {row} is out of bounds for {row_count} rows")]
    RowOutOfBounds {
        /// Requested relative row position.
        row: usize,
        /// Number of rows in the view.
        row_count: usize,
    },
    /// The UTF-8 encoding exceeds the slot capacity; no bytes were changed.
    #[error("string uses {actual} bytes, exceeding the {capacity}-byte slot")]
    TooLong {
        /// Slot capacity in bytes.
        capacity: usize,
        /// Supplied UTF-8 byte length.
        actual: usize,
    },
    /// A null write was attempted on required storage.
    #[error("the string column does not allow nulls")]
    NullNotAllowed,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FixedStringData {
    capacity: usize,
    nullable: bool,
    indices: Vec<StringIndex>,
    buffer: Vec<u8>,
}

impl FixedStringData {
    pub(super) fn new(capacity: usize, nullable: bool, rows: usize) -> Self {
        Self {
            capacity,
            nullable,
            indices: Vec::with_capacity(rows),
            buffer: Vec::with_capacity(
                rows.checked_mul(capacity)
                    .expect("string buffer capacity overflow"),
            ),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.indices.len()
    }

    pub(super) fn view(&self) -> FixedStrings<'_> {
        FixedStrings {
            indices: &self.indices,
            buffer: &self.buffer,
            capacity: self.capacity,
            base: 0,
        }
    }

    pub(super) fn view_mut(&mut self) -> FixedStringsMut<'_> {
        FixedStringsMut {
            indices: &mut self.indices,
            buffer: &mut self.buffer,
            capacity: self.capacity,
            nullable: self.nullable,
            base: 0,
        }
    }

    pub(super) fn push(&mut self, value: Option<&str>) {
        debug_assert!(value.is_some() || self.nullable);
        debug_assert!(value.is_none_or(|text| text.len() <= self.capacity));
        let offset = self.buffer.len();
        self.buffer.resize(
            offset
                .checked_add(self.capacity)
                .expect("string buffer capacity overflow"),
            0,
        );
        if let Some(text) = value {
            self.buffer[offset..offset + text.len()].copy_from_slice(text.as_bytes());
        }
        self.indices.push(StringIndex {
            offset,
            length: value.map_or(NULL_LENGTH, str::len),
        });
    }

    pub(super) fn pop(&mut self) {
        if let Some(index) = self.indices.pop() {
            self.buffer.truncate(index.offset);
        }
    }

    pub(super) fn can_append(&self, other: &Self) -> bool {
        self.capacity == other.capacity && self.nullable == other.nullable
    }

    pub(super) fn append(&mut self, mut other: Self) {
        let base = self.buffer.len();
        for index in &mut other.indices {
            index.offset += base;
        }
        self.indices.extend(other.indices);
        self.buffer.extend(other.buffer);
    }

    pub(super) fn copy_rows(&self, rows: impl IntoIterator<Item = usize>, count: usize) -> Self {
        let mut result = Self::new(self.capacity, self.nullable, count);
        for row in rows {
            result.push(self.view().get(row).expect("validated string row"));
        }
        result
    }

    pub(super) fn retain_live(&mut self, tombstoned: &[bool]) {
        let mut destination = 0;
        for (row, deleted) in tombstoned.iter().copied().enumerate() {
            if !deleted {
                let offset = destination * self.capacity;
                let source = self.indices[row];
                self.buffer
                    .copy_within(source.offset..source.offset + self.capacity, offset);
                self.indices[destination] = StringIndex {
                    offset,
                    length: source.length,
                };
                destination += 1;
            }
        }
        self.indices.truncate(destination);
        self.buffer.truncate(destination * self.capacity);
    }
}

/// A borrowed string column with fixed-capacity slots in one shared byte buffer.
///
/// Offsets and lengths are private so callers cannot invalidate UTF-8 or alias
/// slots. Null and empty strings are distinct. Positions include tombstoned rows.
#[derive(Clone, Copy, Debug)]
pub struct FixedStrings<'a> {
    indices: &'a [StringIndex],
    buffer: &'a [u8],
    capacity: usize,
    base: usize,
}

impl<'a> FixedStrings<'a> {
    /// Returns the number of cells in this view.
    #[must_use]
    pub fn len(self) -> usize {
        self.indices.len()
    }
    /// Returns whether this view has no cells.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.indices.is_empty()
    }
    /// Returns each slot's UTF-8 capacity in bytes.
    #[must_use]
    pub const fn capacity(self) -> usize {
        self.capacity
    }
    /// Reads a relative row: outer `None` means out of bounds, inner `None` null.
    #[must_use]
    // Private descriptors and safe setters guarantee UTF-8 and in-bounds slices.
    #[allow(clippy::missing_panics_doc)]
    pub fn get(self, row: usize) -> Option<Option<&'a str>> {
        let index = self.indices.get(row)?;
        if index.length == NULL_LENGTH {
            return Some(None);
        }
        let begin = index.offset - self.base;
        Some(Some(
            std::str::from_utf8(&self.buffer[begin..begin + index.length])
                .expect("validated UTF-8 slot"),
        ))
    }
    /// Iterates over physical cells in row order without allocating strings.
    #[must_use]
    // The iterator generates only in-bounds positions.
    #[allow(clippy::missing_panics_doc)]
    pub fn iter(self) -> impl ExactSizeIterator<Item = Option<&'a str>> + DoubleEndedIterator {
        (0..self.len()).map(move |row| self.get(row).expect("in-bounds string row"))
    }
}

/// A mutable string column view that can be split into disjoint row ranges.
#[derive(Debug)]
pub struct FixedStringsMut<'a> {
    indices: &'a mut [StringIndex],
    buffer: &'a mut [u8],
    capacity: usize,
    nullable: bool,
    base: usize,
}

impl<'a> FixedStringsMut<'a> {
    /// Returns the number of cells in this view.
    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }
    /// Returns whether this view has no cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    /// Returns each slot's UTF-8 capacity in bytes.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
    /// Borrows the view for reading.
    #[must_use]
    pub fn as_view(&self) -> FixedStrings<'_> {
        FixedStrings {
            indices: self.indices,
            buffer: self.buffer,
            capacity: self.capacity,
            base: self.base,
        }
    }
    /// Reads a relative row, distinguishing out-of-bounds, null, and empty cells.
    #[must_use]
    pub fn get(&self, row: usize) -> Option<Option<&str>> {
        self.as_view().get(row)
    }
    /// Borrows one slot for mutation without permitting invalid UTF-8.
    #[must_use]
    pub fn cell_mut(&mut self, row: usize) -> Option<FixedStringCellMut<'_>> {
        let index = self.indices.get_mut(row)?;
        let begin = index.offset - self.base;
        Some(FixedStringCellMut {
            length: &mut index.length,
            slot: &mut self.buffer[begin..begin + self.capacity],
            nullable: self.nullable,
        })
    }
    pub(super) fn into_cell_mut(self, row: usize) -> Option<FixedStringCellMut<'a>> {
        let index = self.indices.get_mut(row)?;
        let begin = index.offset - self.base;
        Some(FixedStringCellMut {
            length: &mut index.length,
            slot: &mut self.buffer[begin..begin + self.capacity],
            nullable: self.nullable,
        })
    }

    /// Replaces a relative cell, rejecting oversized or disallowed null input.
    ///
    /// # Errors
    /// Returns [`FixedStringError`] without changing the cell on failure.
    pub fn set(&mut self, row: usize, value: Option<&str>) -> Result<(), FixedStringError> {
        let row_count = self.len();
        self.cell_mut(row)
            .ok_or(FixedStringError::RowOutOfBounds { row, row_count })?
            .set(value)
    }
    /// Splits slots and their index descriptors for independent scoped workers.
    ///
    /// # Panics
    /// Panics if `mid > self.len()`.
    #[must_use]
    pub fn split_at(self, mid: usize) -> (Self, Self) {
        let (left_indices, right_indices) = self.indices.split_at_mut(mid);
        let bytes = mid * self.capacity;
        let (left_buffer, right_buffer) = self.buffer.split_at_mut(bytes);
        (
            Self {
                indices: left_indices,
                buffer: left_buffer,
                capacity: self.capacity,
                nullable: self.nullable,
                base: self.base,
            },
            Self {
                indices: right_indices,
                buffer: right_buffer,
                capacity: self.capacity,
                nullable: self.nullable,
                base: self.base + bytes,
            },
        )
    }
    /// Iterates over distinct mutable slots in physical row order.
    pub fn iter_mut(
        &mut self,
    ) -> impl ExactSizeIterator<Item = FixedStringCellMut<'_>> + DoubleEndedIterator {
        let nullable = self.nullable;
        self.indices
            .iter_mut()
            .zip(self.buffer.chunks_exact_mut(self.capacity))
            .map(move |(index, slot)| FixedStringCellMut {
                length: &mut index.length,
                slot,
                nullable,
            })
    }
}

/// An exclusively borrowed fixed-capacity UTF-8 cell.
#[derive(Debug)]
pub struct FixedStringCellMut<'a> {
    length: &'a mut usize,
    slot: &'a mut [u8],
    nullable: bool,
}

impl FixedStringCellMut<'_> {
    /// Reads the current value, with `None` representing null.
    #[must_use]
    // Safe setters maintain the private length and UTF-8 invariants.
    #[allow(clippy::missing_panics_doc)]
    pub fn get(&self) -> Option<&str> {
        (*self.length != NULL_LENGTH)
            .then(|| std::str::from_utf8(&self.slot[..*self.length]).expect("validated UTF-8 slot"))
    }
    /// Borrows the current UTF-8 value for operations that preserve byte length.
    ///
    /// Safe `str` mutation, such as `make_ascii_uppercase`, preserves UTF-8.
    #[must_use]
    // Safe str mutation cannot invalidate the private UTF-8 invariant.
    #[allow(clippy::missing_panics_doc)]
    pub fn as_str_mut(&mut self) -> Option<&mut str> {
        if *self.length == NULL_LENGTH {
            return None;
        }
        Some(std::str::from_utf8_mut(&mut self.slot[..*self.length]).expect("validated UTF-8 slot"))
    }
    /// Replaces the value without allocating, truncating, or changing its offset.
    ///
    /// # Errors
    /// Returns [`FixedStringError::TooLong`] or [`FixedStringError::NullNotAllowed`]
    /// before changing any bytes or metadata.
    pub fn set(&mut self, value: Option<&str>) -> Result<(), FixedStringError> {
        match value {
            Some(text) => {
                if text.len() > self.slot.len() {
                    return Err(FixedStringError::TooLong {
                        capacity: self.slot.len(),
                        actual: text.len(),
                    });
                }
                self.slot[..text.len()].copy_from_slice(text.as_bytes());
                *self.length = text.len();
            }
            None if self.nullable => *self.length = NULL_LENGTH,
            None => return Err(FixedStringError::NullNotAllowed),
        }
        Ok(())
    }
}

//! Lazily allocated per-row tombstone flags.

use std::ops::Range;

/// Per-row tombstone flags for logically deleted rows.
///
/// The flag vector is allocated on the first tombstone and released again once
/// every tombstoned row has been restored. While the vector is absent, every row
/// is live and a table pays no metadata cost.
#[derive(Clone, Debug, Default)]
pub(super) struct Tombstones {
    flags: Vec<bool>,
    count: usize,
}

impl Tombstones {
    /// Returns the number of tombstoned rows.
    pub(super) const fn count(&self) -> usize {
        self.count
    }

    /// Returns whether one physical row is tombstoned.
    pub(super) fn is_tombstoned(&self, row: usize) -> bool {
        self.flags.get(row).copied().unwrap_or(false)
    }

    /// Sets or clears one row's flag and returns whether the state changed.
    pub(super) fn set(&mut self, row: usize, tombstoned: bool, row_count: usize) -> bool {
        debug_assert!(row < row_count);
        if self.flags.is_empty() {
            if !tombstoned {
                return false;
            }
            self.flags.resize(row_count, false);
        }
        if self.flags[row] == tombstoned {
            return false;
        }
        self.flags[row] = tombstoned;
        if tombstoned {
            self.count += 1;
        } else {
            self.count -= 1;
            if self.count == 0 {
                self.flags.clear();
            }
        }
        true
    }

    /// Extends the flags for one appended live row.
    pub(super) fn push_live(&mut self) {
        if !self.flags.is_empty() {
            self.flags.push(false);
        }
    }

    /// Removes the flag for the last physical row.
    pub(super) fn pop(&mut self) {
        if self.flags.pop() == Some(true) {
            self.count -= 1;
            if self.count == 0 {
                self.flags.clear();
            }
        }
    }

    /// Restores every row and returns the number that was tombstoned.
    pub(super) fn clear(&mut self) -> usize {
        let count = self.count;
        self.flags.clear();
        self.count = 0;
        count
    }

    /// Returns the flag slice, or `None` when no row is tombstoned.
    pub(super) fn flags(&self) -> Option<&[bool]> {
        (!self.flags.is_empty()).then_some(self.flags.as_slice())
    }

    /// Returns tombstoned physical row positions in ascending order.
    pub(super) fn iter_tombstoned(&self) -> impl Iterator<Item = usize> + '_ {
        self.flags
            .iter()
            .enumerate()
            .filter_map(|(row, &tombstoned)| tombstoned.then_some(row))
    }

    /// Copies flags for one contiguous physical range.
    pub(super) fn copy_range(&self, rows: Range<usize>) -> Self {
        if self.count == 0 {
            return Self::default();
        }
        let mut flags = Vec::with_capacity(rows.len());
        let mut count = 0;
        for row in rows {
            let tombstoned = self.is_tombstoned(row);
            count += usize::from(tombstoned);
            flags.push(tombstoned);
        }
        Self::from_copied(flags, count)
    }

    /// Copies flags for selected physical rows, preserving order and duplicates.
    pub(super) fn copy_rows(&self, rows: &[usize]) -> Self {
        if self.count == 0 {
            return Self::default();
        }
        let mut flags = Vec::with_capacity(rows.len());
        let mut count = 0;
        for &row in rows {
            let tombstoned = self.is_tombstoned(row);
            count += usize::from(tombstoned);
            flags.push(tombstoned);
        }
        Self::from_copied(flags, count)
    }

    fn from_copied(flags: Vec<bool>, count: usize) -> Self {
        if count == 0 {
            Self::default()
        } else {
            Self { flags, count }
        }
    }
}

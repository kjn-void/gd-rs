//! Position mapping produced by physically removing tombstoned rows.

/// The outcome of [`super::Table::compact`]: which physical rows were removed and
/// where each surviving row moved.
///
/// Surviving rows keep their relative order, so a retained row moves down by the
/// number of removed rows that preceded it. The mapping stores only the removed
/// positions; translating one old position is a binary search over them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RowCompaction {
    removed: Vec<usize>,
    previous_row_count: usize,
}

impl RowCompaction {
    pub(super) const fn new(removed: Vec<usize>, previous_row_count: usize) -> Self {
        Self {
            removed,
            previous_row_count,
        }
    }

    /// Returns the removed physical positions, in ascending order, as they were
    /// numbered before compaction.
    #[must_use]
    pub fn removed_rows(&self) -> &[usize] {
        &self.removed
    }

    /// Returns the number of removed rows.
    #[must_use]
    pub fn removed_count(&self) -> usize {
        self.removed.len()
    }

    /// Returns whether compaction removed no row and therefore moved no row.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty()
    }

    /// Returns the physical row count before compaction.
    #[must_use]
    pub const fn previous_row_count(&self) -> usize {
        self.previous_row_count
    }

    /// Translates a pre-compaction position to its position after compaction.
    ///
    /// Returns `None` when the row was removed or the position was outside the
    /// table before compaction.
    #[must_use]
    pub fn new_position(&self, previous_position: usize) -> Option<usize> {
        if previous_position >= self.previous_row_count {
            return None;
        }
        match self.removed.binary_search(&previous_position) {
            Ok(_) => None,
            Err(removed_before) => Some(previous_position - removed_before),
        }
    }
}

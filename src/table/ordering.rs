//! Stable row ordering over table columns.

use std::cmp::Ordering;

#[cfg(feature = "german-strings")]
use german_str::GermanStr;

use super::storage::{ColumnData, ColumnStorage, NumericData};
use super::{Row, Table};

/// Direction used when ordering table rows.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SortDirection {
    /// Smaller non-null values precede larger values.
    Ascending,
    /// Larger non-null values precede smaller values.
    Descending,
}

/// Placement of null cells in an ordered row view.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NullOrder {
    /// Null cells precede every non-null value.
    First,
    /// Null cells follow every non-null value.
    Last,
}

impl ColumnStorage {
    pub(super) fn ordered_positions(
        &self,
        direction: SortDirection,
        null_order: NullOrder,
    ) -> Vec<usize> {
        // Resolve the physical layout once. Numeric keys are copied; string,
        // byte and UUID keys borrow their column. Comparisons never dispatch on
        // ColumnStorage or perform fallible cell lookup.
        macro_rules! numeric {
            ($values:expr, $compare:expr) => {
                order_numeric($values, direction, null_order, $compare)
            };
        }
        macro_rules! borrowed {
            ($values:expr, $compare:expr) => {
                order_column($values, direction, null_order, $compare)
            };
        }
        match self {
            Self::Null(count) => (0..*count).collect(),
            Self::Bool(values) => borrowed!(values, Ord::cmp),
            Self::I8(values) => numeric!(values, Ord::cmp),
            Self::I16(values) => numeric!(values, Ord::cmp),
            Self::I32(values) => numeric!(values, Ord::cmp),
            Self::I64(values) => numeric!(values, Ord::cmp),
            Self::U8(values) => numeric!(values, Ord::cmp),
            Self::U16(values) => numeric!(values, Ord::cmp),
            Self::U32(values) => numeric!(values, Ord::cmp),
            Self::U64(values) => numeric!(values, Ord::cmp),
            Self::F32(values) => numeric!(values, f32::total_cmp),
            Self::F64(values) => numeric!(values, f64::total_cmp),
            Self::String(values) => borrowed!(values, Ord::cmp),
            #[cfg(feature = "german-strings")]
            Self::GermanString(values) => borrowed!(values, compare_german),
            Self::InternedString(values) => {
                order_keys(values.view().iter(), direction, null_order, Ord::cmp)
            }
            Self::FixedString(values) => {
                order_keys(values.view().iter(), direction, null_order, Ord::cmp)
            }
            Self::Bytes(values) => borrowed!(values, Ord::cmp),
            Self::Uuid(values) => borrowed!(values, Ord::cmp),
        }
    }
}

fn order_numeric<T: Copy>(
    values: &NumericData<T>,
    direction: SortDirection,
    null_order: NullOrder,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Vec<usize> {
    match values {
        NumericData::Required(values) => order_keys(
            values.iter().copied().map(Some),
            direction,
            null_order,
            compare,
        ),
        NumericData::Nullable(numbers) => order_keys(
            numbers
                .values
                .iter()
                .zip(&numbers.valid)
                .map(|(&value, &valid)| valid.then_some(value)),
            direction,
            null_order,
            compare,
        ),
    }
}

fn order_column<T>(
    values: &ColumnData<T>,
    direction: SortDirection,
    null_order: NullOrder,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Vec<usize> {
    let compare = |a: &&T, b: &&T| compare(a, b);
    match values {
        ColumnData::Required(values) => {
            order_keys(values.iter().map(Some), direction, null_order, compare)
        }
        ColumnData::Nullable(values) => order_keys(
            values.iter().map(Option::as_ref),
            direction,
            null_order,
            compare,
        ),
    }
}

fn order_keys<K: Copy>(
    values: impl Iterator<Item = Option<K>>,
    direction: SortDirection,
    null_order: NullOrder,
    compare: impl Fn(&K, &K) -> Ordering,
) -> Vec<usize> {
    let mut keys = Vec::with_capacity(values.size_hint().0);
    let mut nulls = Vec::new();
    for (row, value) in values.enumerate() {
        if let Some(value) = value {
            keys.push((value, row));
        } else {
            nulls.push(row);
        }
    }
    // Stable sorting applies only to the populated keys. Nulls were collected
    // in insertion order and are placed independently of the sort direction.
    match direction {
        SortDirection::Ascending => keys.sort_by(|a, b| compare(&a.0, &b.0)),
        SortDirection::Descending => keys.sort_by(|a, b| compare(&b.0, &a.0)),
    }
    let mut positions: Vec<_> = keys.into_iter().map(|(_, row)| row).collect();
    match null_order {
        NullOrder::First if !nulls.is_empty() => {
            nulls.extend(positions);
            nulls
        }
        _ => {
            positions.extend(nulls);
            positions
        }
    }
}

#[cfg(feature = "german-strings")]
fn compare_german(left: &GermanStr, right: &GermanStr) -> Ordering {
    // The zero-padded four-byte prefix decides most comparisons without
    // following a heap pointer. Fall back to the complete text when it ties,
    // which also resolves interior-NUL and short-prefix cases correctly.
    left.prefix_bytes_array()
        .cmp(&right.prefix_bytes_array())
        .then_with(|| left.as_str().cmp(right.as_str()))
}

/// A stable ordered view of rows in an immutably borrowed table.
///
/// The borrow prevents mutation from invalidating positions while the order is
/// used. The returned view stores row positions and may reuse the key allocation,
/// retaining spare capacity. Construction also uses typed key and stable-sort
/// buffers. Iterating the returned view does not allocate. The permutation covers every physical row, including tombstoned
/// rows; use [`RowOrder::live_rows`] for the live ordered view.
#[derive(Clone, Debug)]
pub struct RowOrder<'a> {
    pub(super) table: &'a Table,
    pub(super) positions: Vec<usize>,
}

impl<'a> RowOrder<'a> {
    /// Returns the source table.
    #[must_use]
    pub const fn table(&self) -> &'a Table {
        self.table
    }

    /// Returns ordered original row positions.
    #[must_use]
    pub fn positions(&self) -> &[usize] {
        &self.positions
    }

    /// Returns the number of ordered rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Returns whether the order contains no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Iterates over borrowing row views in key order.
    ///
    /// Every physical row is yielded, including tombstoned rows.
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = Row<'a>> + DoubleEndedIterator + '_ {
        self.positions.iter().copied().map(|row| Row {
            table: self.table,
            row,
        })
    }

    /// Iterates over live rows in key order, skipping tombstoned rows.
    pub fn live_rows(&self) -> impl Iterator<Item = Row<'a>> + '_ {
        self.positions
            .iter()
            .copied()
            .filter(|&row| !self.table.row_is_tombstoned(row))
            .map(|row| Row {
                table: self.table,
                row,
            })
    }
}

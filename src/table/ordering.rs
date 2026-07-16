//! Stable row ordering over table columns.

use std::cmp::Ordering;

use super::storage::ColumnStorage;
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
    pub(super) fn compare_rows(
        &self,
        left: usize,
        right: usize,
        direction: SortDirection,
        null_order: NullOrder,
    ) -> Ordering {
        macro_rules! ordered {
            ($values:expr) => {
                compare_optional(
                    $values.value(left),
                    $values.value(right),
                    direction,
                    null_order,
                    Ord::cmp,
                )
            };
        }
        match self {
            Self::Null(_) => Ordering::Equal,
            Self::Bool(values) => ordered!(values),
            Self::I8(values) => ordered!(values),
            Self::I16(values) => ordered!(values),
            Self::I32(values) => ordered!(values),
            Self::I64(values) => ordered!(values),
            Self::U8(values) => ordered!(values),
            Self::U16(values) => ordered!(values),
            Self::U32(values) => ordered!(values),
            Self::U64(values) => ordered!(values),
            Self::F32(values) => compare_optional(
                values.value(left),
                values.value(right),
                direction,
                null_order,
                f32::total_cmp,
            ),
            Self::F64(values) => compare_optional(
                values.value(left),
                values.value(right),
                direction,
                null_order,
                f64::total_cmp,
            ),
            Self::String(values) => ordered!(values),
            Self::Bytes(values) => ordered!(values),
            Self::Uuid(values) => ordered!(values),
        }
    }
}

fn compare_optional<T>(
    left: Option<&T>,
    right: Option<&T>,
    direction: SortDirection,
    null_order: NullOrder,
    compare: impl FnOnce(&T, &T) -> Ordering,
) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => match null_order {
            NullOrder::First => Ordering::Less,
            NullOrder::Last => Ordering::Greater,
        },
        (Some(_), None) => match null_order {
            NullOrder::First => Ordering::Greater,
            NullOrder::Last => Ordering::Less,
        },
        (Some(left), Some(right)) => match direction {
            SortDirection::Ascending => compare(left, right),
            SortDirection::Descending => compare(left, right).reverse(),
        },
    }
}

/// A stable ordered view of rows in an immutably borrowed table.
///
/// The borrow prevents mutation from invalidating positions while the order is
/// used. Creating the view allocates one `usize` per row; iterating it does not
/// allocate.
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
    #[must_use]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = Row<'a>> + DoubleEndedIterator + '_ {
        self.positions.iter().copied().map(|row| Row {
            table: self.table,
            row,
        })
    }
}

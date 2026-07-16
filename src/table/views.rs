//! Borrowing row and column views.

use std::fmt;

use compact_str::CompactString;
use thiserror::Error;
use uuid::Uuid;

use crate::{DataType, ValueRef};

use super::storage::{ColumnData, ColumnStorage};
use super::{ColumnSpec, Table};

/// Error returned when requesting a required typed slice from a column.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ColumnSliceError {
    /// The requested Rust element type does not match the schema type.
    #[error("expected a {expected} column, found {actual}")]
    TypeMismatch {
        /// Logical type corresponding to the requested Rust type.
        expected: DataType,
        /// Logical type declared by the column schema.
        actual: DataType,
    },
    /// Nullable storage cannot be represented as a dense `&[T]`.
    #[error("the {data_type} column is nullable and has no dense required-value slice")]
    Nullable {
        /// Logical type declared by the column schema.
        data_type: DataType,
    },
}

mod column_element_private {
    use super::{ColumnData, ColumnMut, ColumnStorage, DataType};
    use uuid::Uuid;

    pub trait Sealed: Sized {
        const DATA_TYPE: DataType;

        fn required_values(column: super::Column<'_>) -> Result<&[Self], super::ColumnSliceError>;

        fn required_values_mut(
            column: ColumnMut<'_>,
        ) -> Result<&mut [Self], super::ColumnSliceError>;
    }

    macro_rules! impl_column_element {
        ($type:ty, $data_type:ident, $storage:ident) => {
            impl Sealed for $type {
                const DATA_TYPE: DataType = DataType::$data_type;

                fn required_values(
                    column: super::Column<'_>,
                ) -> Result<&[Self], super::ColumnSliceError> {
                    match column.storage {
                        ColumnStorage::$storage(ColumnData::Required(values)) => Ok(values),
                        ColumnStorage::$storage(ColumnData::Nullable(_)) => {
                            Err(super::ColumnSliceError::Nullable {
                                data_type: column.spec.data_type(),
                            })
                        }
                        _ => Err(super::ColumnSliceError::TypeMismatch {
                            expected: Self::DATA_TYPE,
                            actual: column.spec.data_type(),
                        }),
                    }
                }

                fn required_values_mut(
                    column: ColumnMut<'_>,
                ) -> Result<&mut [Self], super::ColumnSliceError> {
                    match column.storage {
                        ColumnStorage::$storage(ColumnData::Required(values)) => Ok(values),
                        ColumnStorage::$storage(ColumnData::Nullable(_)) => {
                            Err(super::ColumnSliceError::Nullable {
                                data_type: column.spec.data_type(),
                            })
                        }
                        _ => Err(super::ColumnSliceError::TypeMismatch {
                            expected: Self::DATA_TYPE,
                            actual: column.spec.data_type(),
                        }),
                    }
                }
            }
        };
    }

    impl_column_element!(bool, Bool, Bool);
    impl_column_element!(i8, I8, I8);
    impl_column_element!(i16, I16, I16);
    impl_column_element!(i32, I32, I32);
    impl_column_element!(i64, I64, I64);
    impl_column_element!(u8, U8, U8);
    impl_column_element!(u16, U16, U16);
    impl_column_element!(u32, U32, U32);
    impl_column_element!(u64, U64, U64);
    impl_column_element!(f32, F32, F32);
    impl_column_element!(f64, F64, F64);
    impl_column_element!(Uuid, Uuid, Uuid);
}

/// A fixed-width Rust type that can borrow a required table column as a slice.
///
/// This trait is sealed. It is implemented for `bool`, the fixed-width integer
/// and floating-point primitives, and [`Uuid`].
pub trait ColumnElement: column_element_private::Sealed {}

impl ColumnElement for bool {}
impl ColumnElement for i8 {}
impl ColumnElement for i16 {}
impl ColumnElement for i32 {}
impl ColumnElement for i64 {}
impl ColumnElement for u8 {}
impl ColumnElement for u16 {}
impl ColumnElement for u32 {}
impl ColumnElement for u64 {}
impl ColumnElement for f32 {}
impl ColumnElement for f64 {}
impl ColumnElement for Uuid {}

/// A borrowing view over one typed column.
#[derive(Clone, Copy)]
pub struct Column<'a> {
    pub(super) spec: &'a ColumnSpec,
    pub(super) storage: &'a ColumnStorage,
}

impl fmt::Debug for Column<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Column")
            .field("spec", self.spec)
            .field("len", &self.len())
            .finish()
    }
}

impl<'a> Column<'a> {
    /// Returns this column's schema definition.
    #[must_use]
    pub const fn spec(self) -> &'a ColumnSpec {
        self.spec
    }

    /// Returns the number of cells.
    #[must_use]
    pub fn len(self) -> usize {
        self.storage.len()
    }

    /// Returns whether the column has no cells.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Reads one cell.
    #[must_use]
    pub fn get(self, row: usize) -> Option<ValueRef<'a>> {
        self.storage.get(row)
    }

    /// Borrows a required fixed-width column as one contiguous typed slice.
    ///
    /// Type and nullability are checked once before returning the slice. Loops
    /// over the result contain no per-cell table lookup, dynamic value tag, or
    /// null discriminant, making this the preferred interface for numerical
    /// column operations and compiler auto-vectorization.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Nullable`] for a nullable column.
    pub fn as_slice<T: ColumnElement>(self) -> Result<&'a [T], ColumnSliceError> {
        <T as column_element_private::Sealed>::required_values(self)
    }

    /// Iterates contiguously over this column.
    #[must_use]
    pub fn iter(self) -> impl ExactSizeIterator<Item = ValueRef<'a>> + DoubleEndedIterator {
        (0..self.len()).map(move |row| self.get(row).unwrap_or(ValueRef::Null))
    }

    /// Calls `operation` for every cell after dispatching the column's storage
    /// type and nullability once.
    ///
    /// Unlike [`Column::iter`], this avoids repeating the `ColumnStorage` and
    /// `ColumnData` matches and bounds check for every cell. Values remain
    /// dynamically represented as [`ValueRef`]; use [`Column::as_slice`] when
    /// the fixed-width type is known and a fully typed loop is preferred.
    pub fn for_each_value(self, mut operation: impl FnMut(ValueRef<'a>)) {
        macro_rules! copied {
            ($values:expr, $variant:ident) => {
                match $values {
                    ColumnData::Required(values) => {
                        for value in values {
                            operation(ValueRef::$variant(*value));
                        }
                    }
                    ColumnData::Nullable(values) => {
                        for value in values {
                            operation(
                                value
                                    .as_ref()
                                    .map_or(ValueRef::Null, |value| ValueRef::$variant(*value)),
                            );
                        }
                    }
                }
            };
        }

        macro_rules! borrowed {
            ($values:expr, $variant:ident, $borrow:expr) => {
                match $values {
                    ColumnData::Required(values) => {
                        for value in values {
                            operation(ValueRef::$variant($borrow(value)));
                        }
                    }
                    ColumnData::Nullable(values) => {
                        for value in values {
                            operation(value.as_ref().map_or(ValueRef::Null, |value| {
                                ValueRef::$variant($borrow(value))
                            }));
                        }
                    }
                }
            };
        }

        match self.storage {
            ColumnStorage::Null(len) => {
                for _ in 0..*len {
                    operation(ValueRef::Null);
                }
            }
            ColumnStorage::Bool(values) => copied!(values, Bool),
            ColumnStorage::I8(values) => copied!(values, I8),
            ColumnStorage::I16(values) => copied!(values, I16),
            ColumnStorage::I32(values) => copied!(values, I32),
            ColumnStorage::I64(values) => copied!(values, I64),
            ColumnStorage::U8(values) => copied!(values, U8),
            ColumnStorage::U16(values) => copied!(values, U16),
            ColumnStorage::U32(values) => copied!(values, U32),
            ColumnStorage::U64(values) => copied!(values, U64),
            ColumnStorage::F32(values) => copied!(values, F32),
            ColumnStorage::F64(values) => copied!(values, F64),
            ColumnStorage::String(values) => {
                borrowed!(values, String, |value: &'a CompactString| value.as_str());
            }
            ColumnStorage::Bytes(values) => {
                borrowed!(values, Bytes, |value: &'a [u8]| value);
            }
            ColumnStorage::Uuid(values) => copied!(values, Uuid),
        }
    }
}

/// A mutable borrowing view over one typed column.
pub struct ColumnMut<'a> {
    pub(super) spec: &'a ColumnSpec,
    pub(super) storage: &'a mut ColumnStorage,
}

impl fmt::Debug for ColumnMut<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ColumnMut")
            .field("spec", self.spec)
            .field("len", &self.storage.len())
            .finish()
    }
}

impl<'a> ColumnMut<'a> {
    /// Returns this column's schema definition.
    #[must_use]
    pub const fn spec(&self) -> &'a ColumnSpec {
        self.spec
    }

    /// Returns the number of cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Returns whether the column has no cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows a required fixed-width column as one contiguous mutable slice.
    ///
    /// Type and nullability are checked once before returning the slice. The
    /// exclusive borrow prevents table access while callers mutate its values.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Nullable`] for a nullable column.
    pub fn as_mut_slice<T: ColumnElement>(self) -> Result<&'a mut [T], ColumnSliceError> {
        <T as column_element_private::Sealed>::required_values_mut(self)
    }
}

/// A borrowing view over one table row.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub(super) table: &'a Table,
    pub(super) row: usize,
}

impl<'a> Row<'a> {
    /// Returns the row position.
    #[must_use]
    pub const fn position(self) -> usize {
        self.row
    }

    /// Returns the number of cells.
    #[must_use]
    pub fn len(self) -> usize {
        self.table.column_count()
    }

    /// Returns whether this row has no cells.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Reads one cell by column position.
    #[must_use]
    pub fn get(self, column: usize) -> Option<ValueRef<'a>> {
        self.table.columns.get(column)?.get(self.row)
    }

    /// Reads one cell by primary column name, alias, or stored row-local name.
    #[must_use]
    pub fn get_named(self, name_or_alias: &str) -> Option<ValueRef<'a>> {
        self.table.cell_named(self.row, name_or_alias).ok()
    }

    /// Iterates over the row's cells in schema order.
    #[must_use]
    pub fn iter(self) -> impl ExactSizeIterator<Item = ValueRef<'a>> + DoubleEndedIterator {
        (0..self.len()).map(move |column| self.get(column).unwrap_or(ValueRef::Null))
    }
}

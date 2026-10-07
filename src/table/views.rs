//! Borrowing row and column views.

use std::fmt;

use compact_str::CompactString;
use thiserror::Error;
use uuid::Uuid;

use crate::{DataType, ValueRef};

use super::storage::{ColumnData, ColumnStorage};
use super::{ColumnSpec, FixedStrings, FixedStringsMut, Table};

/// Error returned when requesting a typed slice from a column.
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
    /// Fixed string buffers cannot be represented as `CompactString` slices.
    #[error("the string column uses fixed buffer storage; use fixed_strings instead")]
    FixedStringBuffer,
    /// Nullable storage cannot be represented as a dense `&[T]`.
    #[error("the {data_type} column is nullable and has no dense required-value slice")]
    Nullable {
        /// Logical type declared by the column schema.
        data_type: DataType,
    },
    /// Required storage cannot be borrowed as a slice of optional values.
    #[error("the {data_type} column is required and has no nullable-value slice")]
    Required {
        /// Logical type declared by the column schema.
        data_type: DataType,
    },
}

mod column_element_private {
    use super::{ColumnData, ColumnMut, ColumnStorage, DataType};
    use compact_str::CompactString;
    use uuid::Uuid;

    pub trait Sealed: Sized {
        const DATA_TYPE: DataType;

        fn required_values(column: super::Column<'_>) -> Result<&[Self], super::ColumnSliceError>;

        fn required_values_mut(
            column: ColumnMut<'_>,
        ) -> Result<&mut [Self], super::ColumnSliceError>;

        fn nullable_values(
            column: super::Column<'_>,
        ) -> Result<&[Option<Self>], super::ColumnSliceError>;

        fn nullable_values_mut(
            column: ColumnMut<'_>,
        ) -> Result<&mut [Option<Self>], super::ColumnSliceError>;
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
                        ColumnStorage::FixedString(_) if Self::DATA_TYPE == DataType::String => {
                            Err(super::ColumnSliceError::FixedStringBuffer)
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
                        ColumnStorage::FixedString(_) if Self::DATA_TYPE == DataType::String => {
                            Err(super::ColumnSliceError::FixedStringBuffer)
                        }
                        _ => Err(super::ColumnSliceError::TypeMismatch {
                            expected: Self::DATA_TYPE,
                            actual: column.spec.data_type(),
                        }),
                    }
                }

                fn nullable_values(
                    column: super::Column<'_>,
                ) -> Result<&[Option<Self>], super::ColumnSliceError> {
                    match column.storage {
                        ColumnStorage::$storage(ColumnData::Nullable(values)) => Ok(values),
                        ColumnStorage::$storage(ColumnData::Required(_)) => {
                            Err(super::ColumnSliceError::Required {
                                data_type: column.spec.data_type(),
                            })
                        }
                        ColumnStorage::FixedString(_) if Self::DATA_TYPE == DataType::String => {
                            Err(super::ColumnSliceError::FixedStringBuffer)
                        }
                        _ => Err(super::ColumnSliceError::TypeMismatch {
                            expected: Self::DATA_TYPE,
                            actual: column.spec.data_type(),
                        }),
                    }
                }

                fn nullable_values_mut(
                    column: ColumnMut<'_>,
                ) -> Result<&mut [Option<Self>], super::ColumnSliceError> {
                    match column.storage {
                        ColumnStorage::$storage(ColumnData::Nullable(values)) => Ok(values),
                        ColumnStorage::$storage(ColumnData::Required(_)) => {
                            Err(super::ColumnSliceError::Required {
                                data_type: column.spec.data_type(),
                            })
                        }
                        ColumnStorage::FixedString(_) if Self::DATA_TYPE == DataType::String => {
                            Err(super::ColumnSliceError::FixedStringBuffer)
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

    impl_column_element!(CompactString, String, String);
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

/// A Rust storage element that can borrow a table column as a typed slice.
///
/// This trait is sealed. It is implemented for `bool`, the fixed-width integer
/// and floating-point primitives, [`Uuid`], and [`CompactString`] for ordinary
/// string storage. Fixed string buffers use [`Column::fixed_strings`] instead.
pub trait ColumnElement: column_element_private::Sealed {}

impl ColumnElement for CompactString {}
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

    /// Borrows fixed-capacity UTF-8 storage, or returns `None` for other layouts.
    #[must_use]
    pub fn fixed_strings(self) -> Option<FixedStrings<'a>> {
        match self.storage {
            ColumnStorage::FixedString(values) => Some(values.view()),
            _ => None,
        }
    }

    /// Borrows a required column as one contiguous typed storage slice.
    ///
    /// Type and nullability are checked once before returning the slice. Loops
    /// over the result contain no per-cell table lookup, dynamic value tag, or
    /// null discriminant, making this the preferred interface for numerical
    /// column operations and compiler auto-vectorization.
    /// Ordinary strings use `T = CompactString`; fixed string buffers use
    /// [`Self::fixed_strings`] instead.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Nullable`] for a nullable column.
    /// Returns [`ColumnSliceError::FixedStringBuffer`] when requesting string
    /// descriptors from a fixed string buffer.
    pub fn as_slice<T: ColumnElement>(self) -> Result<&'a [T], ColumnSliceError> {
        <T as column_element_private::Sealed>::required_values(self)
    }

    /// Borrows a nullable column as a contiguous slice of typed storage options.
    ///
    /// Type and nullability are checked once. Each physical row has one element:
    /// `None` represents null, and `Some(value)` contains the typed value. The
    /// slice includes tombstoned rows and does not copy or wrap cells in `ValueRef`.
    /// A nullable column uses this interface even when every cell is populated.
    ///
    /// ```
    /// use gd::{ColumnSpec, DataType, Schema, Table, Value};
    /// let schema = Schema::new([
    ///     ColumnSpec::new("amount", DataType::I64).nullable(true),
    /// ])?;
    /// let mut table = Table::new(schema);
    /// table.push_row([Value::I64(25)])?;
    /// table.push_row([Value::Null])?;
    /// let amounts = table.column(0).unwrap().as_nullable_slice::<i64>()?;
    /// assert_eq!(amounts, &[Some(25), None]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Required`] for a required column.
    /// Returns [`ColumnSliceError::FixedStringBuffer`] for fixed string storage
    /// requested as `CompactString` descriptors.
    pub fn as_nullable_slice<T: ColumnElement>(self) -> Result<&'a [Option<T>], ColumnSliceError> {
        <T as column_element_private::Sealed>::nullable_values(self)
    }

    /// Iterates contiguously over this column.
    #[must_use]
    pub fn iter(self) -> impl ExactSizeIterator<Item = ValueRef<'a>> + DoubleEndedIterator {
        (0..self.len()).map(move |row| self.get(row).unwrap_or(ValueRef::Null))
    }

    /// Calls `operation` for every cell after dispatching the column's storage
    /// type and nullability once.
    ///
    /// Unlike [`Column::iter`], this avoids repeating storage-kind and
    /// nullability dispatch for every cell. Values remain
    /// dynamically represented as [`ValueRef`]; use [`Column::as_slice`] or
    /// [`Column::as_nullable_slice`] for a fully typed loop, or
    /// [`Column::fixed_strings`] for fixed string buffers.
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
            ColumnStorage::FixedString(values) => {
                for value in values.view().iter() {
                    operation(value.map_or(ValueRef::Null, ValueRef::String));
                }
            }
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
    /// Borrows fixed-capacity UTF-8 storage for allocation-free reads and writes.
    /// Returns `None` for other storage layouts.
    #[must_use]
    pub fn fixed_strings_mut(self) -> Option<FixedStringsMut<'a>> {
        match self.storage {
            ColumnStorage::FixedString(values) => Some(values.view_mut()),
            _ => None,
        }
    }

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

    /// Borrows a required column as one contiguous mutable storage slice.
    ///
    /// Type and nullability are checked once before returning the slice. The
    /// exclusive borrow prevents table access while callers mutate its values.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Nullable`] for a nullable column.
    /// Returns [`ColumnSliceError::FixedStringBuffer`] for fixed string storage
    /// requested as `CompactString` descriptors.
    pub fn as_mut_slice<T: ColumnElement>(self) -> Result<&'a mut [T], ColumnSliceError> {
        <T as column_element_private::Sealed>::required_values_mut(self)
    }

    /// Borrows a nullable column as a mutable slice of typed storage options.
    ///
    /// Type and nullability are checked once, then callers can replace values
    /// with `Some(value)` or `None` without dynamic cell setters. The exclusive
    /// borrow prevents table access while the slice is in use. All physical
    /// rows are included; changing a value does not change its tombstone flag.
    ///
    /// ```
    /// use gd::{ColumnSpec, DataType, Schema, Table, Value, ValueRef};
    /// let schema = Schema::new([
    ///     ColumnSpec::new("amount", DataType::I64).nullable(true),
    /// ])?;
    /// let mut table = Table::new(schema);
    /// table.push_row([Value::I64(25)])?;
    /// table.push_row([Value::Null])?;
    /// let (_, [amounts]) = table.columns_io([], [0])?;
    /// for amount in amounts.as_nullable_mut_slice::<i64>()? {
    ///     *amount = amount.map(|value| value * 2);
    /// }
    /// assert_eq!(table.cell(0, 0)?, ValueRef::I64(50));
    /// assert_eq!(table.cell(1, 0)?, ValueRef::Null);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Required`] for a required column.
    /// Returns [`ColumnSliceError::FixedStringBuffer`] for fixed string storage
    /// requested as `CompactString` descriptors.
    pub fn as_nullable_mut_slice<T: ColumnElement>(
        self,
    ) -> Result<&'a mut [Option<T>], ColumnSliceError> {
        <T as column_element_private::Sealed>::nullable_values_mut(self)
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

    /// Returns whether this physical row is tombstoned.
    #[must_use]
    pub fn is_tombstoned(self) -> bool {
        self.table.row_is_tombstoned(self.row)
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

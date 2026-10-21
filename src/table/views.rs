//! Borrowing row and column views.

use std::fmt;

use compact_str::CompactString;
use thiserror::Error;
use uuid::Uuid;

use crate::{DataType, ValueRef};

use super::float_sum;
use super::storage::{ColumnData, ColumnStorage, NumericData};
use super::{
    ColumnSpec, FixedStrings, FixedStringsMut, InternedStrings, InternedStringsMut, Table,
};

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
    /// Fixed or interned string storage cannot be represented as `CompactString` slices.
    ///
    /// This variant covers both layouts. Use the corresponding
    /// `fixed_strings` or `interned_strings` view instead.
    #[error(
        "the string column uses buffer storage; use its fixed_strings or interned_strings view"
    )]
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
    pub trait Sealed {}
}

/// A Rust storage element that can borrow a table column as a typed slice.
///
/// This trait is sealed. It is implemented for `bool`, the fixed-width integer
/// and floating-point primitives, [`Uuid`], and [`CompactString`] for ordinary
/// string storage. Fixed and interned strings use their dedicated string views instead.
///
/// Nullable storage depends on the element type. Fixed-width numbers keep dense values
/// with one validity byte per row and borrow as [`NullableValues`]; `bool`, `Uuid`, and
/// `CompactString` keep one `Option` per row and borrow as `&[Option<T>]`.
pub trait ColumnElement: column_element_private::Sealed + Sized {
    /// The view returned by [`Column::as_nullable_slice`].
    type Nullable<'a>: Copy
    where
        Self: 'a;

    /// The view returned by [`ColumnMut::as_nullable_mut_slice`].
    type NullableMut<'a>
    where
        Self: 'a;

    #[doc(hidden)]
    const DATA_TYPE: DataType;

    #[doc(hidden)]
    fn required_values(column: Column<'_>) -> Result<&[Self], ColumnSliceError>;

    #[doc(hidden)]
    fn required_values_mut(column: ColumnMut<'_>) -> Result<&mut [Self], ColumnSliceError>;

    #[doc(hidden)]
    fn nullable_values(column: Column<'_>) -> Result<Self::Nullable<'_>, ColumnSliceError>;

    #[doc(hidden)]
    fn nullable_values_mut(
        column: ColumnMut<'_>,
    ) -> Result<Self::NullableMut<'_>, ColumnSliceError>;
}

macro_rules! impl_column_element {
    (option, $type:ty, $data_type:ident, $storage:ident) => {
        impl_column_element!(
            @impl $type, $data_type, $storage,
            [&'a [Option<$type>]], [&'a mut [Option<$type>]]
        );
    };
    (numeric, $type:ty, $data_type:ident, $storage:ident) => {
        impl_column_element!(
            @impl $type, $data_type, $storage,
            [NullableValues<'a, $type>], [NullableValuesMut<'a, $type>]
        );
    };
    (@impl $type:ty, $data_type:ident, $storage:ident, [$($nullable:tt)*], [$($nullable_mut:tt)*]) => {
        impl column_element_private::Sealed for $type {}

        impl ColumnElement for $type {
            type Nullable<'a> = $($nullable)*;
            type NullableMut<'a> = $($nullable_mut)*;

            const DATA_TYPE: DataType = DataType::$data_type;

            fn required_values(column: Column<'_>) -> Result<&[Self], ColumnSliceError> {
                match column.storage {
                    ColumnStorage::$storage(data) => {
                        data.required_slice().ok_or(ColumnSliceError::Nullable {
                            data_type: column.spec.data_type(),
                        })
                    }
                    other => Err(mismatch(other, Self::DATA_TYPE, column.spec)),
                }
            }

            fn required_values_mut(
                column: ColumnMut<'_>,
            ) -> Result<&mut [Self], ColumnSliceError> {
                let spec = column.spec;
                match column.storage {
                    ColumnStorage::$storage(data) => {
                        data.required_slice_mut().ok_or(ColumnSliceError::Nullable {
                            data_type: spec.data_type(),
                        })
                    }
                    other => Err(mismatch(other, Self::DATA_TYPE, spec)),
                }
            }

            fn nullable_values(
                column: Column<'_>,
            ) -> Result<Self::Nullable<'_>, ColumnSliceError> {
                match column.storage {
                    ColumnStorage::$storage(data) => {
                        data.nullable_slice().ok_or(ColumnSliceError::Required {
                            data_type: column.spec.data_type(),
                        })
                    }
                    other => Err(mismatch(other, Self::DATA_TYPE, column.spec)),
                }
            }

            fn nullable_values_mut(
                column: ColumnMut<'_>,
            ) -> Result<Self::NullableMut<'_>, ColumnSliceError> {
                let spec = column.spec;
                match column.storage {
                    ColumnStorage::$storage(data) => {
                        data.nullable_slice_mut().ok_or(ColumnSliceError::Required {
                            data_type: spec.data_type(),
                        })
                    }
                    other => Err(mismatch(other, Self::DATA_TYPE, spec)),
                }
            }
        }
    };
}

impl_column_element!(option, CompactString, String, String);
impl_column_element!(option, bool, Bool, Bool);
impl_column_element!(numeric, i8, I8, I8);
impl_column_element!(numeric, i16, I16, I16);
impl_column_element!(numeric, i32, I32, I32);
impl_column_element!(numeric, i64, I64, I64);
impl_column_element!(numeric, u8, U8, U8);
impl_column_element!(numeric, u16, U16, U16);
impl_column_element!(numeric, u32, U32, U32);
impl_column_element!(numeric, u64, U64, U64);
impl_column_element!(numeric, f32, F32, F32);
impl_column_element!(numeric, f64, F64, F64);
impl_column_element!(option, Uuid, Uuid, Uuid);

/// Builds the error for a typed slice request against a column of another kind.
fn mismatch(storage: &ColumnStorage, expected: DataType, spec: &ColumnSpec) -> ColumnSliceError {
    match storage {
        ColumnStorage::FixedString(_) | ColumnStorage::InternedString(_)
            if expected == DataType::String =>
        {
            ColumnSliceError::FixedStringBuffer
        }
        _ => ColumnSliceError::TypeMismatch {
            expected,
            actual: spec.data_type(),
        },
    }
}

impl<T> ColumnData<T> {
    fn required_slice(&self) -> Option<&[T]> {
        match self {
            Self::Required(values) => Some(values),
            Self::Nullable(_) => None,
        }
    }

    fn required_slice_mut(&mut self) -> Option<&mut [T]> {
        match self {
            Self::Required(values) => Some(values),
            Self::Nullable(_) => None,
        }
    }

    fn nullable_slice(&self) -> Option<&[Option<T>]> {
        match self {
            Self::Required(_) => None,
            Self::Nullable(values) => Some(values),
        }
    }

    fn nullable_slice_mut(&mut self) -> Option<&mut [Option<T>]> {
        match self {
            Self::Required(_) => None,
            Self::Nullable(values) => Some(values),
        }
    }
}

impl<T> NumericData<T> {
    fn required_slice(&self) -> Option<&[T]> {
        match self {
            Self::Required(values) => Some(values),
            Self::Nullable(_) => None,
        }
    }

    fn required_slice_mut(&mut self) -> Option<&mut [T]> {
        match self {
            Self::Required(values) => Some(values),
            Self::Nullable(_) => None,
        }
    }

    fn nullable_slice(&self) -> Option<NullableValues<'_, T>> {
        match self {
            Self::Required(_) => None,
            Self::Nullable(numbers) => Some(NullableValues {
                values: &numbers.values,
                valid: &numbers.valid,
            }),
        }
    }

    fn nullable_slice_mut(&mut self) -> Option<NullableValuesMut<'_, T>> {
        match self {
            Self::Required(_) => None,
            Self::Nullable(numbers) => Some(NullableValuesMut {
                values: &mut numbers.values,
                valid: &mut numbers.valid,
            }),
        }
    }
}

/// A borrowed nullable fixed-width numeric column.
///
/// The column stores dense values and one validity byte per physical row,
/// including tombstoned rows. [`Self::values`] and [`Self::validity`] borrow
/// both slices directly for branch-free or vectorized loops. A value is
/// meaningful only where the corresponding validity byte is `true`; null rows
/// hold an unspecified placeholder.
pub struct NullableValues<'a, T> {
    values: &'a [T],
    valid: &'a [bool],
}

impl<T> Clone for NullableValues<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for NullableValues<'_, T> {}

impl<T: Copy + fmt::Debug> fmt::Debug for NullableValues<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, T: Copy> NullableValues<'a, T> {
    /// Returns the number of physical rows.
    #[must_use]
    pub fn len(self) -> usize {
        self.valid.len()
    }

    /// Returns whether the column has no rows.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.valid.is_empty()
    }

    /// Borrows the dense values. Entries for null rows are placeholders.
    #[must_use]
    pub const fn values(self) -> &'a [T] {
        self.values
    }

    /// Borrows the per-row validity bytes; `false` marks a null row.
    #[must_use]
    pub const fn validity(self) -> &'a [bool] {
        self.valid
    }

    /// Returns `None` for an out-of-bounds row, `Some(None)` for null, and
    /// `Some(Some(value))` otherwise.
    #[must_use]
    pub fn get(self, row: usize) -> Option<Option<T>> {
        let valid = *self.valid.get(row)?;
        Some(valid.then(|| self.values[row]))
    }

    /// Copies one row's value, returning `None` for null.
    ///
    /// This is the nullable counterpart of indexing a `&[T]`.
    ///
    /// # Panics
    ///
    /// Panics when `row` is out of bounds.
    #[must_use]
    #[inline]
    pub fn value(self, row: usize) -> Option<T> {
        let value = self.values[row];
        self.valid[row].then_some(value)
    }

    /// Iterates over every row in order, yielding `None` for null rows.
    pub fn iter(self) -> impl ExactSizeIterator<Item = Option<T>> + DoubleEndedIterator + 'a {
        self.values
            .iter()
            .zip(self.valid)
            .map(|(&value, &valid)| valid.then_some(value))
    }

    /// Copies every row into a vector of options.
    #[must_use]
    pub fn to_vec(self) -> Vec<Option<T>> {
        self.iter().collect()
    }
}

impl<T: Copy + PartialEq> PartialEq for NullableValues<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl<T: Copy + PartialEq> PartialEq<[Option<T>]> for NullableValues<'_, T> {
    fn eq(&self, other: &[Option<T>]) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter().copied())
    }
}

impl<T: Copy + PartialEq, const N: usize> PartialEq<[Option<T>; N]> for NullableValues<'_, T> {
    fn eq(&self, other: &[Option<T>; N]) -> bool {
        *self == other[..]
    }
}

impl<T: Copy + PartialEq, const N: usize> PartialEq<&[Option<T>; N]> for NullableValues<'_, T> {
    fn eq(&self, other: &&[Option<T>; N]) -> bool {
        *self == other[..]
    }
}

/// A mutable nullable fixed-width numeric column.
///
/// See [`NullableValues`] for the layout. [`Self::into_parts`] borrows the
/// values and validity mutably for bulk transforms: setting a validity byte to
/// `true` makes the stored value visible, and `false` makes the row null.
pub struct NullableValuesMut<'a, T> {
    values: &'a mut [T],
    valid: &'a mut [bool],
}

impl<T: Copy + fmt::Debug> fmt::Debug for NullableValuesMut<'_, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_values().fmt(formatter)
    }
}

impl<T: Copy + PartialEq> PartialEq for NullableValuesMut<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_values() == other.as_values()
    }
}

impl<'a, T: Copy> NullableValuesMut<'a, T> {
    /// Reborrows this column immutably.
    #[must_use]
    pub fn as_values(&self) -> NullableValues<'_, T> {
        NullableValues {
            values: self.values,
            valid: self.valid,
        }
    }

    /// Returns the number of physical rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.valid.len()
    }

    /// Returns whether the column has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.valid.is_empty()
    }

    /// Reads one row; see [`NullableValues::get`].
    #[must_use]
    pub fn get(&self, row: usize) -> Option<Option<T>> {
        self.as_values().get(row)
    }

    /// Replaces one row, returning `false` without a change when `row` is out of bounds.
    pub fn set(&mut self, row: usize, value: Option<T>) -> bool {
        let Some(valid) = self.valid.get_mut(row) else {
            return false;
        };
        *valid = value.is_some();
        if let Some(value) = value {
            self.values[row] = value;
        }
        true
    }

    /// Splits the view into its dense values and per-row validity bytes.
    #[must_use]
    pub fn into_parts(self) -> (&'a mut [T], &'a mut [bool]) {
        (self.values, self.valid)
    }
}

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

    /// Borrows interned UTF-8 storage, or returns `None` for other layouts.
    #[must_use]
    pub fn interned_strings(self) -> Option<InternedStrings<'a>> {
        match self.storage {
            ColumnStorage::InternedString(values) => Some(values.view()),
            _ => None,
        }
    }

    /// Borrows a required column as one contiguous typed storage slice.
    ///
    /// Type and nullability are checked once before returning the slice. Loops
    /// over the result contain no per-cell table lookup, dynamic value tag, or
    /// null discriminant, making this the preferred interface for numerical
    /// column operations and compiler auto-vectorization.
    /// Ordinary strings use `T = CompactString`; fixed and interned strings use
    /// [`Self::fixed_strings`] and [`Self::interned_strings`] respectively.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] if `T` does not match the
    /// schema type, or [`ColumnSliceError::Nullable`] for a nullable column.
    /// Returns [`ColumnSliceError::FixedStringBuffer`] when requesting string
    /// descriptors from fixed or interned string storage.
    pub fn as_slice<T: ColumnElement>(self) -> Result<&'a [T], ColumnSliceError> {
        T::required_values(self)
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
    /// Returns [`ColumnSliceError::FixedStringBuffer`] for fixed
    /// or interned string storage requested as `CompactString` descriptors.
    pub fn as_nullable_slice<T: ColumnElement>(self) -> Result<T::Nullable<'a>, ColumnSliceError> {
        T::nullable_values(self)
    }

    /// Sums a required or nullable `F64` column using 16 independent accumulators.
    ///
    /// A sequential floating-point sum such as `iter().sum()` is one dependency
    /// chain: every addition waits for the previous one, and the compiler may not
    /// reorder IEEE additions. This method instead adds element `i` of the leading
    /// `len - len % 16` elements to accumulator `i % 16`, combines the accumulators
    /// in order, and then adds the trailing elements in row order. The independent
    /// chains overlap in the CPU pipeline and auto-vectorize without fast-math.
    ///
    /// The summation order is fixed, so the result is deterministic for the same
    /// cells, but its rounding can differ from a sequential sum. NaN and infinities
    /// propagate as in any IEEE sum. Null cells contribute nothing, and an empty or
    /// all-null column sums to `+0.0`. Like [`Self::as_slice`], the sum covers every
    /// physical row, including tombstoned rows.
    ///
    /// ```
    /// use gd::{ColumnSpec, DataType, Schema, Table, Value};
    /// let schema = Schema::new([
    ///     ColumnSpec::new("price", DataType::F64),
    ///     ColumnSpec::new("discount", DataType::F64).nullable(true),
    /// ])?;
    /// let mut table = Table::new(schema);
    /// table.push_row([Value::F64(1.5), Value::F64(0.25)])?;
    /// table.push_row([Value::F64(2.5), Value::Null])?;
    /// assert_eq!(table.column(0).unwrap().sum_f64()?, 4.0);
    /// assert_eq!(table.column(1).unwrap().sum_f64()?, 0.25);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] unless the column type is
    /// [`DataType::F64`].
    pub fn sum_f64(self) -> Result<f64, ColumnSliceError> {
        match self.storage {
            ColumnStorage::F64(NumericData::Required(values)) => {
                Ok(float_sum::sum_required(values))
            }
            ColumnStorage::F64(NumericData::Nullable(numbers)) => {
                Ok(float_sum::sum_nullable_f64(&numbers.values, &numbers.valid))
            }
            _ => Err(ColumnSliceError::TypeMismatch {
                expected: DataType::F64,
                actual: self.spec.data_type(),
            }),
        }
    }

    /// Sums a required or nullable `F32` column using 16 independent `f32`
    /// accumulators.
    ///
    /// The summation order, null handling, and tombstone behavior are the same as
    /// for [`Self::sum_f64`]. Accumulation stays in `f32`; for a wider
    /// accumulator, convert the values from [`Self::as_slice`] explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnSliceError::TypeMismatch`] unless the column type is
    /// [`DataType::F32`].
    pub fn sum_f32(self) -> Result<f32, ColumnSliceError> {
        match self.storage {
            ColumnStorage::F32(NumericData::Required(values)) => {
                Ok(float_sum::sum_required(values))
            }
            ColumnStorage::F32(NumericData::Nullable(numbers)) => {
                Ok(float_sum::sum_nullable_f32(&numbers.values, &numbers.valid))
            }
            _ => Err(ColumnSliceError::TypeMismatch {
                expected: DataType::F32,
                actual: self.spec.data_type(),
            }),
        }
    }

    /// Iterates contiguously over this column.
    #[must_use = "iterators are lazy and must be consumed"]
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
    /// [`Column::fixed_strings`] or [`Column::interned_strings`] for string views.
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

        macro_rules! numeric {
            ($values:expr, $variant:ident) => {
                match $values {
                    NumericData::Required(values) => {
                        for value in values {
                            operation(ValueRef::$variant(*value));
                        }
                    }
                    NumericData::Nullable(numbers) => {
                        for (value, &valid) in numbers.values.iter().zip(&numbers.valid) {
                            operation(if valid {
                                ValueRef::$variant(*value)
                            } else {
                                ValueRef::Null
                            });
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
            ColumnStorage::I8(values) => numeric!(values, I8),
            ColumnStorage::I16(values) => numeric!(values, I16),
            ColumnStorage::I32(values) => numeric!(values, I32),
            ColumnStorage::I64(values) => numeric!(values, I64),
            ColumnStorage::U8(values) => numeric!(values, U8),
            ColumnStorage::U16(values) => numeric!(values, U16),
            ColumnStorage::U32(values) => numeric!(values, U32),
            ColumnStorage::U64(values) => numeric!(values, U64),
            ColumnStorage::F32(values) => numeric!(values, F32),
            ColumnStorage::F64(values) => numeric!(values, F64),
            ColumnStorage::FixedString(values) => {
                for value in values.view().iter() {
                    operation(value.map_or(ValueRef::Null, ValueRef::String));
                }
            }
            ColumnStorage::String(values) => {
                borrowed!(values, String, |value: &'a CompactString| value.as_str());
            }
            ColumnStorage::InternedString(values) => {
                for value in values.view().iter() {
                    operation(value.map_or(ValueRef::Null, ValueRef::String));
                }
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

    /// Borrows interned strings for checked replacement and disjoint row splits.
    /// Returns `None` for other storage layouts.
    #[must_use]
    pub fn interned_strings_mut(self) -> Option<InternedStringsMut<'a>> {
        match self.storage {
            ColumnStorage::InternedString(values) => Some(values.view_mut()),
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
    /// Returns [`ColumnSliceError::FixedStringBuffer`] for fixed
    /// or interned string storage requested as `CompactString` descriptors.
    pub fn as_mut_slice<T: ColumnElement>(self) -> Result<&'a mut [T], ColumnSliceError> {
        T::required_values_mut(self)
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
    /// let (values, valid) = amounts
    ///     .as_nullable_mut_slice::<i64>()?
    ///     .into_parts();
    /// for (value, &valid) in values.iter_mut().zip(valid.iter()) {
    ///     if valid {
    ///         *value *= 2;
    ///     }
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
    /// Returns [`ColumnSliceError::FixedStringBuffer`] for fixed
    /// or interned string storage requested as `CompactString` descriptors.
    pub fn as_nullable_mut_slice<T: ColumnElement>(
        self,
    ) -> Result<T::NullableMut<'a>, ColumnSliceError> {
        T::nullable_values_mut(self)
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
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn iter(self) -> impl ExactSizeIterator<Item = ValueRef<'a>> + DoubleEndedIterator {
        (0..self.len()).map(move |column| self.get(column).unwrap_or(ValueRef::Null))
    }
}

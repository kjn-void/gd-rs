//! Mutable row views and safely splittable row ranges.

use std::fmt;

use compact_str::CompactString;
#[cfg(feature = "german-strings")]
use german_str::GermanStr;
use uuid::Uuid;

use crate::{Value, ValueRef};

use super::storage::{CellValue, ColumnData, ColumnStorage, ExtrasStorage, NumericData, RowExtras};
use super::{
    FixedStringCellMut, FixedStringsMut, InternedStringCellMut, InternedStringsMut, Schema, Table,
    TableError, UnknownFields, prepare_cell,
};

enum ColumnSliceMut<'a, T> {
    Required(&'a mut [T]),
    Nullable(&'a mut [Option<T>]),
    /// Nullable numeric storage: parallel values and per-row validity bytes.
    Split {
        values: &'a mut [T],
        valid: &'a mut [bool],
    },
}

impl<T> ColumnSliceMut<'_, T> {
    fn split_at(self, mid: usize) -> (Self, Self) {
        match self {
            Self::Required(values) => {
                let (left, right) = values.split_at_mut(mid);
                (Self::Required(left), Self::Required(right))
            }
            Self::Nullable(values) => {
                let (left, right) = values.split_at_mut(mid);
                (Self::Nullable(left), Self::Nullable(right))
            }
            Self::Split { values, valid } => {
                let (left_values, right_values) = values.split_at_mut(mid);
                let (left_valid, right_valid) = valid.split_at_mut(mid);
                (
                    Self::Split {
                        values: left_values,
                        valid: left_valid,
                    },
                    Self::Split {
                        values: right_values,
                        valid: right_valid,
                    },
                )
            }
        }
    }

    fn get(&self, row: usize) -> CellValue<'_, T> {
        match self {
            Self::Required(values) => values
                .get(row)
                .map_or(CellValue::OutOfBounds, CellValue::Value),
            Self::Nullable(values) => match values.get(row) {
                None => CellValue::OutOfBounds,
                Some(None) => CellValue::Null,
                Some(Some(value)) => CellValue::Value(value),
            },
            Self::Split { values, valid } => match (values.get(row), valid.get(row)) {
                (Some(value), Some(true)) => CellValue::Value(value),
                (Some(_), Some(false)) => CellValue::Null,
                _ => CellValue::OutOfBounds,
            },
        }
    }

    fn cell_mut(&mut self, row: usize) -> Option<CellSlotMut<'_, T>> {
        match self {
            Self::Required(values) => values.get_mut(row).map(CellSlotMut::Required),
            Self::Nullable(values) => values.get_mut(row).map(CellSlotMut::Nullable),
            Self::Split { values, valid } => Some(CellSlotMut::Split {
                value: values.get_mut(row)?,
                valid: valid.get_mut(row)?,
            }),
        }
    }
}

impl<T> ColumnData<T> {
    fn row_slice_mut(&mut self) -> ColumnSliceMut<'_, T> {
        match self {
            Self::Required(values) => ColumnSliceMut::Required(values),
            Self::Nullable(values) => ColumnSliceMut::Nullable(values),
        }
    }
}

impl<T> NumericData<T> {
    fn row_slice_mut(&mut self) -> ColumnSliceMut<'_, T> {
        match self {
            Self::Required(values) => ColumnSliceMut::Required(values),
            Self::Nullable(numbers) => ColumnSliceMut::Split {
                values: &mut numbers.values,
                valid: &mut numbers.valid,
            },
        }
    }
}

enum CellSlotMut<'a, T> {
    Required(&'a mut T),
    Nullable(&'a mut Option<T>),
    Split {
        value: &'a mut T,
        valid: &'a mut bool,
    },
}

impl<T> CellSlotMut<'_, T> {
    fn set(&mut self, value: Option<T>) -> Result<(), TableError> {
        match self {
            Self::Required(current) => {
                let Some(value) = value else {
                    return Err(TableError::InternalInvariant {
                        detail: "null was assigned to a required cell",
                    });
                };
                **current = value;
                Ok(())
            }
            Self::Nullable(current) => {
                **current = value;
                Ok(())
            }
            Self::Split { value: slot, valid } => {
                // A null keeps the previous slot contents; only validity is observable.
                **valid = value.is_some();
                if let Some(value) = value {
                    **slot = value;
                }
                Ok(())
            }
        }
    }
}

enum CellMut<'a> {
    Null,
    Bool(CellSlotMut<'a, bool>),
    I8(CellSlotMut<'a, i8>),
    I16(CellSlotMut<'a, i16>),
    I32(CellSlotMut<'a, i32>),
    I64(CellSlotMut<'a, i64>),
    U8(CellSlotMut<'a, u8>),
    U16(CellSlotMut<'a, u16>),
    U32(CellSlotMut<'a, u32>),
    U64(CellSlotMut<'a, u64>),
    F32(CellSlotMut<'a, f32>),
    F64(CellSlotMut<'a, f64>),
    String(CellSlotMut<'a, CompactString>),
    #[cfg(feature = "german-strings")]
    GermanString(CellSlotMut<'a, GermanStr>),
    InternedString(InternedStringCellMut<'a>),
    FixedString(FixedStringCellMut<'a>),
    Bytes(CellSlotMut<'a, Box<[u8]>>),
    Uuid(CellSlotMut<'a, Uuid>),
}

impl CellMut<'_> {
    fn set_validated(&mut self, value: Value) -> Result<(), TableError> {
        macro_rules! set {
            ($slot:expr, $variant:ident) => {
                $slot.set(match value {
                    Value::Null => None,
                    Value::$variant(value) => Some(value),
                    _ => {
                        return Err(TableError::InternalInvariant {
                            detail: "value type did not match its column",
                        });
                    }
                })
            };
        }
        match self {
            Self::Null => {
                if value != Value::Null {
                    return Err(TableError::InternalInvariant {
                        detail: "non-null value was assigned to a null cell",
                    });
                }
                Ok(())
            }
            Self::Bool(slot) => set!(slot, Bool),
            Self::I8(slot) => set!(slot, I8),
            Self::I16(slot) => set!(slot, I16),
            Self::I32(slot) => set!(slot, I32),
            Self::I64(slot) => set!(slot, I64),
            Self::U8(slot) => set!(slot, U8),
            Self::U16(slot) => set!(slot, U16),
            Self::U32(slot) => set!(slot, U32),
            Self::U64(slot) => set!(slot, U64),
            Self::F32(slot) => set!(slot, F32),
            Self::F64(slot) => set!(slot, F64),
            Self::String(slot) => set!(slot, String),
            #[cfg(feature = "german-strings")]
            Self::GermanString(slot) => {
                let value = match &value {
                    Value::Null => None,
                    Value::String(text) => Some(GermanStr::new(text.as_str()).map_err(|_| {
                        TableError::InternalInvariant {
                            detail: "validated German string exceeded layout limit",
                        }
                    })?),
                    _ => {
                        return Err(TableError::InternalInvariant {
                            detail: "German string input type differs",
                        });
                    }
                };
                slot.set(value)
            }
            Self::InternedString(slot) => slot
                .set(match &value {
                    Value::Null => None,
                    Value::String(text) => Some(text.as_str()),
                    _ => {
                        return Err(TableError::InternalInvariant {
                            detail: "interned string input type differs",
                        });
                    }
                })
                .map_err(|_| TableError::InternalInvariant {
                    detail: "validated interned string row write failed",
                }),
            Self::FixedString(slot) => slot
                .set(match &value {
                    Value::Null => None,
                    Value::String(text) => Some(text.as_str()),
                    _ => {
                        return Err(TableError::InternalInvariant {
                            detail: "fixed string input type differs",
                        });
                    }
                })
                .map_err(|_| TableError::InternalInvariant {
                    detail: "validated fixed string row write failed",
                }),
            Self::Bytes(slot) => set!(slot, Bytes),
            Self::Uuid(slot) => set!(slot, Uuid),
        }
    }
}

enum ColumnRangeMut<'a> {
    Null(usize),
    Bool(ColumnSliceMut<'a, bool>),
    I8(ColumnSliceMut<'a, i8>),
    I16(ColumnSliceMut<'a, i16>),
    I32(ColumnSliceMut<'a, i32>),
    I64(ColumnSliceMut<'a, i64>),
    U8(ColumnSliceMut<'a, u8>),
    U16(ColumnSliceMut<'a, u16>),
    U32(ColumnSliceMut<'a, u32>),
    U64(ColumnSliceMut<'a, u64>),
    F32(ColumnSliceMut<'a, f32>),
    F64(ColumnSliceMut<'a, f64>),
    String(ColumnSliceMut<'a, CompactString>),
    #[cfg(feature = "german-strings")]
    GermanString(ColumnSliceMut<'a, GermanStr>),
    InternedString(InternedStringsMut<'a>),
    FixedString(FixedStringsMut<'a>),
    Bytes(ColumnSliceMut<'a, Box<[u8]>>),
    Uuid(ColumnSliceMut<'a, Uuid>),
}

impl ColumnRangeMut<'_> {
    fn split_at(self, mid: usize) -> (Self, Self) {
        macro_rules! split {
            ($values:expr, $variant:ident) => {{
                let (left, right) = $values.split_at(mid);
                (Self::$variant(left), Self::$variant(right))
            }};
        }
        match self {
            Self::Null(len) => (Self::Null(mid), Self::Null(len - mid)),
            Self::Bool(values) => split!(values, Bool),
            Self::I8(values) => split!(values, I8),
            Self::I16(values) => split!(values, I16),
            Self::I32(values) => split!(values, I32),
            Self::I64(values) => split!(values, I64),
            Self::U8(values) => split!(values, U8),
            Self::U16(values) => split!(values, U16),
            Self::U32(values) => split!(values, U32),
            Self::U64(values) => split!(values, U64),
            Self::F32(values) => split!(values, F32),
            Self::F64(values) => split!(values, F64),
            Self::String(values) => split!(values, String),
            #[cfg(feature = "german-strings")]
            Self::GermanString(values) => split!(values, GermanString),
            Self::InternedString(values) => split!(values, InternedString),
            Self::FixedString(values) => split!(values, FixedString),
            Self::Bytes(values) => split!(values, Bytes),
            Self::Uuid(values) => split!(values, Uuid),
        }
    }

    fn get(&self, row: usize) -> Option<ValueRef<'_>> {
        macro_rules! copied {
            ($values:expr, $variant:ident) => {
                match $values.get(row) {
                    CellValue::OutOfBounds => None,
                    CellValue::Null => Some(ValueRef::Null),
                    CellValue::Value(value) => Some(ValueRef::$variant(*value)),
                }
            };
        }
        match self {
            Self::Null(len) => (row < *len).then_some(ValueRef::Null),
            Self::Bool(values) => copied!(values, Bool),
            Self::I8(values) => copied!(values, I8),
            Self::I16(values) => copied!(values, I16),
            Self::I32(values) => copied!(values, I32),
            Self::I64(values) => copied!(values, I64),
            Self::U8(values) => copied!(values, U8),
            Self::U16(values) => copied!(values, U16),
            Self::U32(values) => copied!(values, U32),
            Self::U64(values) => copied!(values, U64),
            Self::F32(values) => copied!(values, F32),
            Self::F64(values) => copied!(values, F64),
            Self::String(values) => match values.get(row) {
                CellValue::OutOfBounds => None,
                CellValue::Null => Some(ValueRef::Null),
                CellValue::Value(value) => Some(ValueRef::String(value)),
            },
            #[cfg(feature = "german-strings")]
            Self::GermanString(values) => match values.get(row) {
                CellValue::OutOfBounds => None,
                CellValue::Null => Some(ValueRef::Null),
                CellValue::Value(value) => Some(ValueRef::String(value.as_str())),
            },
            Self::InternedString(values) => values
                .get(row)
                .map(|value| value.map_or(ValueRef::Null, ValueRef::String)),
            Self::FixedString(values) => values
                .get(row)
                .map(|value| value.map_or(ValueRef::Null, ValueRef::String)),
            Self::Bytes(values) => match values.get(row) {
                CellValue::OutOfBounds => None,
                CellValue::Null => Some(ValueRef::Null),
                CellValue::Value(value) => Some(ValueRef::Bytes(value)),
            },
            Self::Uuid(values) => copied!(values, Uuid),
        }
    }

    fn cell_mut(&mut self, row: usize) -> Option<CellMut<'_>> {
        macro_rules! cell {
            ($values:expr, $variant:ident) => {
                $values.cell_mut(row).map(CellMut::$variant)
            };
        }
        match self {
            Self::Null(len) => (row < *len).then_some(CellMut::Null),
            Self::Bool(values) => cell!(values, Bool),
            Self::I8(values) => cell!(values, I8),
            Self::I16(values) => cell!(values, I16),
            Self::I32(values) => cell!(values, I32),
            Self::I64(values) => cell!(values, I64),
            Self::U8(values) => cell!(values, U8),
            Self::U16(values) => cell!(values, U16),
            Self::U32(values) => cell!(values, U32),
            Self::U64(values) => cell!(values, U64),
            Self::F32(values) => cell!(values, F32),
            Self::F64(values) => cell!(values, F64),
            Self::String(values) => cell!(values, String),
            #[cfg(feature = "german-strings")]
            Self::GermanString(values) => cell!(values, GermanString),
            Self::InternedString(values) => values.cell_mut(row).map(CellMut::InternedString),
            Self::FixedString(values) => values.cell_mut(row).map(CellMut::FixedString),
            Self::Bytes(values) => cell!(values, Bytes),
            Self::Uuid(values) => cell!(values, Uuid),
        }
    }
}

impl ColumnStorage {
    fn row_range_mut(&mut self) -> ColumnRangeMut<'_> {
        macro_rules! range {
            ($values:expr, $variant:ident) => {
                ColumnRangeMut::$variant($values.row_slice_mut())
            };
        }
        match self {
            Self::Null(len) => ColumnRangeMut::Null(*len),
            Self::Bool(values) => range!(values, Bool),
            Self::I8(values) => range!(values, I8),
            Self::I16(values) => range!(values, I16),
            Self::I32(values) => range!(values, I32),
            Self::I64(values) => range!(values, I64),
            Self::U8(values) => range!(values, U8),
            Self::U16(values) => range!(values, U16),
            Self::U32(values) => range!(values, U32),
            Self::U64(values) => range!(values, U64),
            Self::F32(values) => range!(values, F32),
            Self::F64(values) => range!(values, F64),
            Self::String(values) => range!(values, String),
            #[cfg(feature = "german-strings")]
            Self::GermanString(values) => range!(values, GermanString),
            Self::InternedString(values) => ColumnRangeMut::InternedString(values.view_mut()),
            Self::FixedString(values) => ColumnRangeMut::FixedString(values.view_mut()),
            Self::Bytes(values) => range!(values, Bytes),
            Self::Uuid(values) => range!(values, Uuid),
        }
    }
}

enum ExtrasRangeMut<'a> {
    Disabled,
    Enabled(&'a mut [Option<Box<RowExtras>>]),
}

impl ExtrasRangeMut<'_> {
    fn split_at(self, mid: usize) -> (Self, Self) {
        match self {
            Self::Disabled => (Self::Disabled, Self::Disabled),
            Self::Enabled(rows) => {
                let (left, right) = rows.split_at_mut(mid);
                (Self::Enabled(left), Self::Enabled(right))
            }
        }
    }

    fn row_mut(&mut self, row: usize) -> Option<RowExtrasMut<'_>> {
        match self {
            Self::Disabled => Some(RowExtrasMut::Disabled),
            Self::Enabled(rows) => rows.get_mut(row).map(RowExtrasMut::Enabled),
        }
    }
}

enum RowExtrasMut<'a> {
    Disabled,
    Enabled(&'a mut Option<Box<RowExtras>>),
}

impl RowExtrasMut<'_> {
    fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Disabled => None,
            Self::Enabled(extras) => extras.as_deref().and_then(|extras| extras.get(name)),
        }
    }

    fn set(&mut self, name: CompactString, value: Value) -> Result<(), TableError> {
        match self {
            Self::Disabled => Err(TableError::InternalInvariant {
                detail: "row extras were stored in a closed schema",
            }),
            Self::Enabled(extras) => {
                extras
                    .get_or_insert_with(|| Box::new(RowExtras::default()))
                    .set(name, value);
                Ok(())
            }
        }
    }
}

impl ExtrasStorage {
    fn row_range_mut(&mut self) -> ExtrasRangeMut<'_> {
        match self {
            Self::Disabled => ExtrasRangeMut::Disabled,
            Self::Enabled(rows) => ExtrasRangeMut::Enabled(rows),
        }
    }

    fn row_cell_mut(&mut self, row: usize) -> Option<RowExtrasMut<'_>> {
        match self {
            Self::Disabled => Some(RowExtrasMut::Disabled),
            Self::Enabled(rows) => rows.get_mut(row).map(RowExtrasMut::Enabled),
        }
    }
}

/// Column storage that a [`RowMut`] reads and writes one row of on demand.
///
/// Resolving cells lazily keeps row-view construction independent of the column
/// count; only the cells an operation touches are dispatched.
trait RowCells {
    fn column_count(&self) -> usize;
    fn cell(&self, row: usize, column: usize) -> Option<ValueRef<'_>>;
    fn set_cell_validated(
        &mut self,
        row: usize,
        column: usize,
        value: Value,
    ) -> Result<(), TableError>;
}

impl RowCells for Vec<ColumnStorage> {
    fn column_count(&self) -> usize {
        self.len()
    }

    fn cell(&self, row: usize, column: usize) -> Option<ValueRef<'_>> {
        self.as_slice().get(column)?.get(row)
    }

    fn set_cell_validated(
        &mut self,
        row: usize,
        column: usize,
        value: Value,
    ) -> Result<(), TableError> {
        let column_count = self.len();
        self.as_mut_slice()
            .get_mut(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count,
            })?
            .set_validated(row, value)
    }
}

impl RowCells for Vec<ColumnRangeMut<'_>> {
    fn column_count(&self) -> usize {
        self.len()
    }

    fn cell(&self, row: usize, column: usize) -> Option<ValueRef<'_>> {
        self.as_slice().get(column)?.get(row)
    }

    fn set_cell_validated(
        &mut self,
        row: usize,
        column: usize,
        value: Value,
    ) -> Result<(), TableError> {
        let column_count = self.len();
        self.as_mut_slice()
            .get_mut(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count,
            })?
            .cell_mut(row)
            .ok_or(TableError::InternalInvariant {
                detail: "row view position exceeded its column range",
            })?
            .set_validated(value)
    }
}

/// A mutable borrowing view over one table row.
///
/// The view exclusively borrows the row's column storage and resolves a cell
/// only when it is read or written, so constructing a view does not depend on
/// the column count. It can change values but cannot add or remove rows or fixed
/// columns.
pub struct RowMut<'a> {
    schema: &'a Schema,
    cells: &'a mut (dyn RowCells + 'a),
    row: usize,
    extras: RowExtrasMut<'a>,
    position: usize,
    tombstoned: bool,
}

impl fmt::Debug for RowMut<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RowMut")
            .field("position", &self.position)
            .field("len", &self.cells.column_count())
            .field("tombstoned", &self.tombstoned)
            .finish_non_exhaustive()
    }
}

impl RowMut<'_> {
    /// Returns the original table row position.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Returns whether this physical row is tombstoned.
    ///
    /// Tombstoning is metadata and does not lock cell writes; structural changes
    /// such as [`Table::tombstone_row`] require the table itself.
    #[must_use]
    pub const fn is_tombstoned(&self) -> bool {
        self.tombstoned
    }

    /// Returns the number of fixed-schema cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.column_count()
    }

    /// Returns whether the fixed schema has no columns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.column_count() == 0
    }

    /// Reads one fixed cell by column position.
    #[must_use]
    pub fn get(&self, column: usize) -> Option<ValueRef<'_>> {
        self.cells.cell(self.row, column)
    }

    /// Reads one cell by primary name, alias, or stored row-local name.
    #[must_use]
    pub fn get_named(&self, name_or_alias: &str) -> Option<ValueRef<'_>> {
        if let Some(column) = self.schema.column_index(name_or_alias) {
            return self.get(column);
        }
        self.extras.get(name_or_alias).map(Value::as_ref)
    }

    /// Replaces one fixed cell after schema-directed conversion and validation.
    ///
    /// # Errors
    ///
    /// Returns a column bounds, conversion, type, nullability, or fixed-string
    /// capacity error without changing the cell.
    pub fn set(&mut self, column: usize, mut value: Value) -> Result<(), TableError> {
        let spec = self
            .schema
            .column(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.cells.column_count(),
            })?;
        prepare_cell(spec, &mut value, column)?;
        self.cells.set_cell_validated(self.row, column, value)
    }

    /// Replaces a fixed cell or stores an unknown name as a row-local value.
    ///
    /// # Errors
    ///
    /// Returns a conversion, type, nullability, or fixed-string capacity error for
    /// declared columns, or [`TableError::ColumnNotFound`] for a rejected unknown name.
    pub fn set_named(
        &mut self,
        name_or_alias: &str,
        value: impl Into<Value>,
    ) -> Result<(), TableError> {
        let value = value.into();
        if let Some(column) = self.schema.column_index(name_or_alias) {
            return self.set(column, value);
        }
        if self.schema.unknown_fields() == UnknownFields::Reject {
            return Err(TableError::ColumnNotFound(name_or_alias.into()));
        }
        self.extras.set(name_or_alias.into(), value)?;
        Ok(())
    }
}

/// A mutable row range whose storage can be divided at row boundaries.
///
/// Splitting partitions every typed column and the open-schema sidecar at the
/// same position. The two returned ranges therefore contain no overlapping
/// mutable references and are safe to process on different threads.
pub struct RowsMut<'a> {
    schema: &'a Schema,
    columns: Vec<ColumnRangeMut<'a>>,
    extras: ExtrasRangeMut<'a>,
    tombstones: Option<&'a [bool]>,
    start: usize,
    len: usize,
}

impl fmt::Debug for RowsMut<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RowsMut")
            .field("start", &self.start)
            .field("len", &self.len)
            .field("columns", &self.columns.len())
            .finish_non_exhaustive()
    }
}

impl RowsMut<'_> {
    /// Returns the number of rows in this range.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether this range contains no rows.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the first original table row position represented by this range.
    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    /// Returns one mutable row using a position relative to this range.
    #[must_use]
    pub fn row_mut(&mut self, row: usize) -> Option<RowMut<'_>> {
        if row >= self.len {
            return None;
        }
        let extras = self.extras.row_mut(row)?;
        let tombstoned = self
            .tombstones
            .and_then(|flags| flags.get(row))
            .copied()
            .unwrap_or(false);
        Some(RowMut {
            schema: self.schema,
            cells: &mut self.columns,
            row,
            extras,
            position: self.start + row,
            tombstoned,
        })
    }

    /// Splits this range before `mid`, using a position relative to the range.
    ///
    /// # Panics
    ///
    /// Panics when `mid > self.len()`.
    #[must_use]
    pub fn split_at(self, mid: usize) -> (Self, Self) {
        assert!(mid <= self.len, "row split index is out of bounds");
        let mut left_columns = Vec::with_capacity(self.columns.len());
        let mut right_columns = Vec::with_capacity(self.columns.len());
        for column in self.columns {
            let (left, right) = column.split_at(mid);
            left_columns.push(left);
            right_columns.push(right);
        }
        let (left_extras, right_extras) = self.extras.split_at(mid);
        let (left_tombstones, right_tombstones) = match self.tombstones {
            Some(flags) => {
                let (left, right) = flags.split_at(mid);
                (Some(left), Some(right))
            }
            None => (None, None),
        };
        (
            Self {
                schema: self.schema,
                columns: left_columns,
                extras: left_extras,
                tombstones: left_tombstones,
                start: self.start,
                len: mid,
            },
            Self {
                schema: self.schema,
                columns: right_columns,
                extras: right_extras,
                tombstones: right_tombstones,
                start: self.start + mid,
                len: self.len - mid,
            },
        )
    }

    /// Applies an operation to every row in this range in position order.
    pub fn for_each<F>(mut self, mut operation: F)
    where
        F: for<'row> FnMut(RowMut<'row>),
    {
        for row in 0..self.len {
            if let Some(row) = self.row_mut(row) {
                operation(row);
            }
        }
    }

    /// Applies an operation in parallel after recursively splitting row storage.
    ///
    /// `min_rows` is the splitting threshold: a range is split while it holds more
    /// than `min_rows` rows, so scheduled ranges never exceed that size and halving
    /// can produce ranges about half as large. Enable the crate's `rayon` feature
    /// to use this method.
    ///
    /// # Panics
    ///
    /// Panics when `min_rows` is zero.
    #[cfg(feature = "rayon")]
    pub fn par_for_each<F>(self, min_rows: usize, operation: F)
    where
        F: for<'row> Fn(RowMut<'row>) + Send + Sync,
    {
        assert!(min_rows > 0, "parallel row grain size must be non-zero");
        self.par_for_each_inner(min_rows, &operation);
    }

    #[cfg(feature = "rayon")]
    fn par_for_each_inner<F>(self, min_rows: usize, operation: &F)
    where
        F: for<'row> Fn(RowMut<'row>) + Send + Sync,
    {
        if self.len <= min_rows {
            self.for_each(operation);
            return;
        }
        let mid = self.len / 2;
        let (left, right) = self.split_at(mid);
        rayon::join(
            || left.par_for_each_inner(min_rows, operation),
            || right.par_for_each_inner(min_rows, operation),
        );
    }
}

impl Table {
    /// Returns one mutable row view.
    ///
    /// The view can read and replace fixed cells and, for an open schema,
    /// row-local extra fields. It cannot change the table's row or column count.
    #[must_use]
    pub fn row_mut(&mut self, row: usize) -> Option<RowMut<'_>> {
        if row >= self.row_count {
            return None;
        }
        let extras = self.extras.row_cell_mut(row)?;
        Some(RowMut {
            schema: &self.schema,
            cells: &mut self.columns,
            row,
            extras,
            position: row,
            tombstoned: self.tombstones.is_tombstoned(row),
        })
    }

    /// Borrows all rows mutably as a safely splittable range.
    ///
    /// [`RowsMut::split_at`] partitions every column, the optional open-schema
    /// sidecar, and the tombstone flags at the same row boundary, so the
    /// resulting ranges can be sent to separate scoped threads without locks or
    /// overlapping mutable references.
    #[must_use]
    pub fn rows_mut(&mut self) -> RowsMut<'_> {
        RowsMut {
            schema: &self.schema,
            columns: self
                .columns
                .iter_mut()
                .map(ColumnStorage::row_range_mut)
                .collect(),
            extras: self.extras.row_range_mut(),
            tombstones: self.tombstones.flags(),
            start: 0,
            len: self.row_count,
        }
    }

    /// Applies a mutable row operation in parallel using Rayon.
    ///
    /// `min_rows` is the splitting threshold: a range is split while it holds more
    /// than `min_rows` rows, so scheduled ranges never exceed that size. Use a
    /// non-trivial threshold to amortize range splitting and task scheduling. Enable
    /// the crate's `rayon` feature to use this method.
    ///
    /// # Panics
    ///
    /// Panics when `min_rows` is zero.
    #[cfg(feature = "rayon")]
    pub fn par_for_each_row_mut<F>(&mut self, min_rows: usize, operation: F)
    where
        F: for<'row> Fn(RowMut<'row>) + Send + Sync,
    {
        self.rows_mut().par_for_each(min_rows, operation);
    }
}

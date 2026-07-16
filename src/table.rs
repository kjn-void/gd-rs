//! Schema-driven typed column storage.

mod row_mut;

pub use row_mut::{RowMut, RowsMut};

use std::{cmp::Ordering, fmt};

use ahash::AHashMap;
use compact_str::CompactString;
use smallvec::SmallVec;
use thiserror::Error;
use uuid::Uuid;

use crate::{DataType, Value, ValueRef};

/// A schema column's name, optional alias, type, and nullability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnSpec {
    name: CompactString,
    alias: Option<CompactString>,
    data_type: DataType,
    nullable: bool,
}

/// Policy for names that are not declared as schema columns or aliases.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnknownFields {
    /// Reject unknown names. This is the default for a strict schema.
    #[default]
    Reject,
    /// Store unknown names as owned, row-local dynamic values.
    Store,
}

impl ColumnSpec {
    /// Creates a non-nullable column, except that a `Null` column is always nullable.
    pub fn new(name: impl Into<CompactString>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            alias: None,
            data_type,
            nullable: data_type == DataType::Null,
        }
    }

    /// Sets an alternate lookup name.
    #[must_use]
    pub fn with_alias(mut self, alias: impl Into<CompactString>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Sets whether this column accepts [`Value::Null`].
    ///
    /// A column whose type is [`DataType::Null`] remains nullable.
    #[must_use]
    pub const fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable || matches!(self.data_type, DataType::Null);
        self
    }

    /// Returns the primary column name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the optional alternate lookup name.
    #[must_use]
    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    /// Returns the column's logical value type.
    #[must_use]
    pub const fn data_type(&self) -> DataType {
        self.data_type
    }

    /// Returns whether the column accepts null values.
    #[must_use]
    pub const fn is_nullable(&self) -> bool {
        self.nullable
    }
}

/// An immutable table schema with O(1)-expected name/alias lookup and an
/// explicit policy for row-local unknown fields.
#[derive(Clone, Debug)]
pub struct Schema {
    columns: Vec<ColumnSpec>,
    by_name: AHashMap<CompactString, usize>,
    unknown_fields: UnknownFields,
}

impl Schema {
    /// Validates and constructs a schema.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::DuplicateColumnName`] if a name or alias is already
    /// assigned to a different column.
    pub fn new(columns: impl IntoIterator<Item = ColumnSpec>) -> Result<Self, TableError> {
        let columns: Vec<_> = columns.into_iter().collect();
        let mut by_name = AHashMap::with_capacity(columns.len().saturating_mul(2));
        for (position, column) in columns.iter().enumerate() {
            insert_schema_name(&mut by_name, column.name(), position)?;
            if let Some(alias) = column.alias() {
                insert_schema_name(&mut by_name, alias, position)?;
            }
        }
        Ok(Self {
            columns,
            by_name,
            unknown_fields: UnknownFields::Reject,
        })
    }

    /// Sets how tables using this schema handle names absent from the schema.
    #[must_use]
    pub const fn with_unknown_fields(mut self, unknown_fields: UnknownFields) -> Self {
        self.unknown_fields = unknown_fields;
        self
    }

    /// Returns the policy for names absent from the schema.
    #[must_use]
    pub const fn unknown_fields(&self) -> UnknownFields {
        self.unknown_fields
    }

    /// Returns the number of columns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Returns whether the schema contains no columns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Returns a column definition by position.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<&ColumnSpec> {
        self.columns.get(position)
    }

    /// Resolves a primary name or alias in expected O(name length).
    #[must_use]
    pub fn column_index(&self, name_or_alias: &str) -> Option<usize> {
        self.by_name.get(name_or_alias).copied()
    }

    /// Returns column definitions in positional order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ColumnSpec> + DoubleEndedIterator {
        self.columns.iter()
    }
}

fn insert_schema_name(
    names: &mut AHashMap<CompactString, usize>,
    name: &str,
    position: usize,
) -> Result<(), TableError> {
    if let Some(previous) = names.get(name) {
        if *previous != position {
            return Err(TableError::DuplicateColumnName(name.into()));
        }
        return Ok(());
    }
    names.insert(name.into(), position);
    Ok(())
}

/// A table schema, storage, or access error.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum TableError {
    /// Two columns claim the same name or alias.
    #[error("duplicate column name or alias: {0}")]
    DuplicateColumnName(CompactString),
    /// A row contains a different number of values than the schema.
    #[error("expected {expected} row values, found {actual}")]
    RowWidth {
        /// Required value count.
        expected: usize,
        /// Supplied value count.
        actual: usize,
    },
    /// A cell value does not match its column type.
    #[error("column {column} expects {expected}, found {actual}")]
    TypeMismatch {
        /// Positional column index.
        column: usize,
        /// Schema type.
        expected: DataType,
        /// Supplied type.
        actual: DataType,
    },
    /// A null value was supplied to a non-nullable column.
    #[error("column {column} is not nullable")]
    NullNotAllowed {
        /// Positional column index.
        column: usize,
    },
    /// A row position is outside the table.
    #[error("row {row} is out of bounds for {row_count} rows")]
    RowOutOfBounds {
        /// Requested row.
        row: usize,
        /// Current row count.
        row_count: usize,
    },
    /// A column position is outside the schema.
    #[error("column {column} is out of bounds for {column_count} columns")]
    ColumnOutOfBounds {
        /// Requested column.
        column: usize,
        /// Current column count.
        column_count: usize,
    },
    /// No primary name or alias matches the query.
    #[error("column not found: {0}")]
    ColumnNotFound(CompactString),
    /// A value supplied as an extra uses a declared column name or alias.
    #[error("extra field conflicts with schema column or alias: {0}")]
    ExtraFieldConflictsWithColumn(CompactString),
    /// Hash indexes do not support this logical type.
    #[error("columns of type {0} cannot be indexed")]
    UnsupportedIndexType(DataType),
}

const ROW_EXTRAS_HASH_THRESHOLD: usize = 4;

#[derive(Clone, Debug)]
enum RowExtras {
    Inline(SmallVec<[(CompactString, Value); 2]>),
    Hashed(AHashMap<CompactString, Value>),
}

impl Default for RowExtras {
    fn default() -> Self {
        Self::Inline(SmallVec::new())
    }
}

impl RowExtras {
    fn with_capacity(capacity: usize) -> Self {
        if capacity > ROW_EXTRAS_HASH_THRESHOLD {
            Self::Hashed(AHashMap::with_capacity(capacity))
        } else {
            Self::Inline(SmallVec::with_capacity(capacity))
        }
    }

    fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Inline(entries) => entries
                .iter()
                .find_map(|(entry_name, value)| (entry_name == name).then_some(value)),
            Self::Hashed(entries) => entries.get(name),
        }
    }

    fn set(&mut self, name: CompactString, value: Value) {
        match self {
            Self::Inline(entries) => {
                if let Some((_, current)) = entries
                    .iter_mut()
                    .find(|(entry_name, _)| entry_name.as_str() == name.as_str())
                {
                    *current = value;
                    return;
                }
                if entries.len() < ROW_EXTRAS_HASH_THRESHOLD {
                    entries.push((name, value));
                    return;
                }

                let mut hashed = AHashMap::with_capacity(entries.len() + 1);
                hashed.extend(entries.drain(..));
                hashed.insert(name, value);
                *self = Self::Hashed(hashed);
            }
            Self::Hashed(entries) => {
                entries.insert(name, value);
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Inline(entries) => entries.is_empty(),
            Self::Hashed(entries) => entries.is_empty(),
        }
    }
}

#[derive(Clone, Debug)]
enum ExtrasStorage {
    Disabled,
    Enabled(Vec<Option<Box<RowExtras>>>),
}

impl ExtrasStorage {
    fn with_capacity(unknown_fields: UnknownFields, capacity: usize) -> Self {
        match unknown_fields {
            UnknownFields::Reject => Self::Disabled,
            UnknownFields::Store => Self::Enabled(Vec::with_capacity(capacity)),
        }
    }

    fn push_empty(&mut self) {
        if let Self::Enabled(rows) = self {
            rows.push(None);
        }
    }

    fn set(&mut self, row: usize, extras: RowExtras) {
        match self {
            Self::Enabled(rows) => rows[row] = Some(Box::new(extras)),
            Self::Disabled => unreachable!("closed schemas cannot store extra fields"),
        }
    }

    fn get(&self, row: usize) -> Option<&RowExtras> {
        match self {
            Self::Disabled => None,
            Self::Enabled(rows) => rows.get(row)?.as_deref(),
        }
    }

    fn get_or_insert(&mut self, row: usize) -> &mut RowExtras {
        match self {
            Self::Enabled(rows) => rows[row]
                .get_or_insert_with(|| Box::new(RowExtras::default()))
                .as_mut(),
            Self::Disabled => unreachable!("closed schemas cannot store extra fields"),
        }
    }

    fn pop(&mut self) {
        if let Self::Enabled(rows) = self {
            let _ = rows.pop();
        }
    }

    fn len_matches(&self, row_count: usize) -> bool {
        match self {
            Self::Disabled => true,
            Self::Enabled(rows) => rows.len() == row_count,
        }
    }
}

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

#[derive(Clone, Debug, PartialEq)]
enum ColumnData<T> {
    Required(Vec<T>),
    Nullable(Vec<Option<T>>),
}

enum CellValue<'a, T> {
    OutOfBounds,
    Null,
    Value(&'a T),
}

impl<T> ColumnData<T> {
    #[inline]
    fn with_capacity(nullable: bool, capacity: usize) -> Self {
        if nullable {
            Self::Nullable(Vec::with_capacity(capacity))
        } else {
            Self::Required(Vec::with_capacity(capacity))
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match self {
            Self::Required(values) => values.len(),
            Self::Nullable(values) => values.len(),
        }
    }

    #[inline]
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
        }
    }

    #[inline]
    fn value(&self, row: usize) -> Option<&T> {
        match self.get(row) {
            CellValue::OutOfBounds => panic!("column lengths match row count"),
            CellValue::Null => None,
            CellValue::Value(value) => Some(value),
        }
    }

    #[inline]
    fn push(&mut self, value: Option<T>) {
        match self {
            Self::Required(values) => {
                values.push(value.expect("required column values were validated"));
            }
            Self::Nullable(values) => values.push(value),
        }
    }

    #[inline]
    fn set(&mut self, row: usize, value: Option<T>) {
        match self {
            Self::Required(values) => {
                values[row] = value.expect("required column values were validated");
            }
            Self::Nullable(values) => values[row] = value,
        }
    }

    #[inline]
    fn pop(&mut self) {
        match self {
            Self::Required(values) => drop(values.pop()),
            Self::Nullable(values) => drop(values.pop()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ColumnStorage {
    Null(usize),
    Bool(ColumnData<bool>),
    I8(ColumnData<i8>),
    I16(ColumnData<i16>),
    I32(ColumnData<i32>),
    I64(ColumnData<i64>),
    U8(ColumnData<u8>),
    U16(ColumnData<u16>),
    U32(ColumnData<u32>),
    U64(ColumnData<u64>),
    F32(ColumnData<f32>),
    F64(ColumnData<f64>),
    String(ColumnData<CompactString>),
    Bytes(ColumnData<Box<[u8]>>),
    Uuid(ColumnData<Uuid>),
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

impl ColumnStorage {
    #[inline]
    fn new(data_type: DataType, nullable: bool, capacity: usize) -> Self {
        match data_type {
            DataType::Null => Self::Null(0),
            DataType::Bool => Self::Bool(ColumnData::with_capacity(nullable, capacity)),
            DataType::I8 => Self::I8(ColumnData::with_capacity(nullable, capacity)),
            DataType::I16 => Self::I16(ColumnData::with_capacity(nullable, capacity)),
            DataType::I32 => Self::I32(ColumnData::with_capacity(nullable, capacity)),
            DataType::I64 => Self::I64(ColumnData::with_capacity(nullable, capacity)),
            DataType::U8 => Self::U8(ColumnData::with_capacity(nullable, capacity)),
            DataType::U16 => Self::U16(ColumnData::with_capacity(nullable, capacity)),
            DataType::U32 => Self::U32(ColumnData::with_capacity(nullable, capacity)),
            DataType::U64 => Self::U64(ColumnData::with_capacity(nullable, capacity)),
            DataType::F32 => Self::F32(ColumnData::with_capacity(nullable, capacity)),
            DataType::F64 => Self::F64(ColumnData::with_capacity(nullable, capacity)),
            DataType::String => Self::String(ColumnData::with_capacity(nullable, capacity)),
            DataType::Bytes => Self::Bytes(ColumnData::with_capacity(nullable, capacity)),
            DataType::Uuid => Self::Uuid(ColumnData::with_capacity(nullable, capacity)),
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match self {
            Self::Null(len) => *len,
            Self::Bool(values) => values.len(),
            Self::I8(values) => values.len(),
            Self::I16(values) => values.len(),
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
            Self::U8(values) => values.len(),
            Self::U16(values) => values.len(),
            Self::U32(values) => values.len(),
            Self::U64(values) => values.len(),
            Self::F32(values) => values.len(),
            Self::F64(values) => values.len(),
            Self::String(values) => values.len(),
            Self::Bytes(values) => values.len(),
            Self::Uuid(values) => values.len(),
        }
    }

    #[inline]
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
            Self::Bytes(values) => match values.get(row) {
                CellValue::OutOfBounds => None,
                CellValue::Null => Some(ValueRef::Null),
                CellValue::Value(value) => Some(ValueRef::Bytes(value)),
            },
            Self::Uuid(values) => copied!(values, Uuid),
        }
    }

    #[inline]
    fn push_validated(&mut self, value: Value) {
        macro_rules! push {
            ($values:expr, $value:expr, $variant:ident) => {
                $values.push(match $value {
                    Value::Null => None,
                    Value::$variant(value) => Some(value),
                    _ => unreachable!("value was validated against its column"),
                })
            };
        }
        match self {
            Self::Null(len) => {
                debug_assert_eq!(value, Value::Null);
                *len += 1;
            }
            Self::Bool(values) => push!(values, value, Bool),
            Self::I8(values) => push!(values, value, I8),
            Self::I16(values) => push!(values, value, I16),
            Self::I32(values) => push!(values, value, I32),
            Self::I64(values) => push!(values, value, I64),
            Self::U8(values) => push!(values, value, U8),
            Self::U16(values) => push!(values, value, U16),
            Self::U32(values) => push!(values, value, U32),
            Self::U64(values) => push!(values, value, U64),
            Self::F32(values) => push!(values, value, F32),
            Self::F64(values) => push!(values, value, F64),
            Self::String(values) => push!(values, value, String),
            Self::Bytes(values) => push!(values, value, Bytes),
            Self::Uuid(values) => push!(values, value, Uuid),
        }
    }

    #[inline]
    fn set_validated(&mut self, row: usize, value: Value) {
        macro_rules! set {
            ($values:expr, $value:expr, $variant:ident) => {
                $values.set(
                    row,
                    match $value {
                        Value::Null => None,
                        Value::$variant(value) => Some(value),
                        _ => unreachable!("value was validated against its column"),
                    },
                )
            };
        }
        match self {
            Self::Null(_) => debug_assert_eq!(value, Value::Null),
            Self::Bool(values) => set!(values, value, Bool),
            Self::I8(values) => set!(values, value, I8),
            Self::I16(values) => set!(values, value, I16),
            Self::I32(values) => set!(values, value, I32),
            Self::I64(values) => set!(values, value, I64),
            Self::U8(values) => set!(values, value, U8),
            Self::U16(values) => set!(values, value, U16),
            Self::U32(values) => set!(values, value, U32),
            Self::U64(values) => set!(values, value, U64),
            Self::F32(values) => set!(values, value, F32),
            Self::F64(values) => set!(values, value, F64),
            Self::String(values) => set!(values, value, String),
            Self::Bytes(values) => set!(values, value, Bytes),
            Self::Uuid(values) => set!(values, value, Uuid),
        }
    }

    #[inline]
    fn pop(&mut self) {
        match self {
            Self::Null(len) => *len -= 1,
            Self::Bool(values) => values.pop(),
            Self::I8(values) => values.pop(),
            Self::I16(values) => values.pop(),
            Self::I32(values) => values.pop(),
            Self::I64(values) => values.pop(),
            Self::U8(values) => values.pop(),
            Self::U16(values) => values.pop(),
            Self::U32(values) => values.pop(),
            Self::U64(values) => values.pop(),
            Self::F32(values) => values.pop(),
            Self::F64(values) => values.pop(),
            Self::String(values) => values.pop(),
            Self::Bytes(values) => values.pop(),
            Self::Uuid(values) => values.pop(),
        }
    }

    fn compare_rows(
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

/// A schema-driven table whose fixed columns store their primitive types
/// directly and whose optional unknown fields are owned by individual rows.
///
/// Required columns store contiguous `T` values directly. Nullable columns use
/// `Option<T>` to represent null cells. Schemas that reject unknown fields do not
/// allocate per-row extras storage.
#[derive(Clone, Debug)]
pub struct Table {
    schema: Schema,
    columns: Vec<ColumnStorage>,
    extras: ExtrasStorage,
    row_count: usize,
}

impl Table {
    /// Creates an empty table.
    #[must_use]
    pub fn new(schema: Schema) -> Self {
        Self::with_capacity(schema, 0)
    }

    /// Creates an empty table with per-column capacity for `capacity` rows.
    #[must_use]
    pub fn with_capacity(schema: Schema, capacity: usize) -> Self {
        let columns = schema
            .iter()
            .map(|column| ColumnStorage::new(column.data_type(), column.is_nullable(), capacity))
            .collect();
        let extras = ExtrasStorage::with_capacity(schema.unknown_fields(), capacity);
        Self {
            schema,
            columns,
            extras,
            row_count: 0,
        }
    }

    /// Returns the immutable schema.
    #[must_use]
    pub const fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Returns the number of rows.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.row_count
    }

    /// Returns the number of columns.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.schema.len()
    }

    /// Returns whether the table has no rows.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Appends one complete row after validating every value.
    ///
    /// The operation is atomic with respect to type/null validation: no column
    /// changes if any supplied value is invalid.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::RowWidth`], [`TableError::TypeMismatch`], or
    /// [`TableError::NullNotAllowed`] when the row does not match the schema.
    pub fn push_row<const N: usize>(&mut self, values: [Value; N]) -> Result<usize, TableError> {
        self.validate_row(&values)?;
        Ok(self.push_validated_row(values))
    }

    /// Appends one complete fixed row together with row-local extra fields.
    ///
    /// Extra names must not match a declared column or alias. Repeated extra
    /// names replace the earlier value, matching [`Table::set_named`]. The
    /// complete operation is validated before the table changes.
    ///
    /// # Errors
    ///
    /// Returns the ordinary row-validation errors, [`TableError::ColumnNotFound`]
    /// when the schema rejects unknown fields, or
    /// [`TableError::ExtraFieldConflictsWithColumn`] for a declared name.
    pub fn push_row_with_extras<const N: usize, I, K, V>(
        &mut self,
        values: [Value; N],
        extras: I,
    ) -> Result<usize, TableError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<CompactString>,
        V: Into<Value>,
    {
        self.validate_row(&values)?;
        let extras = self.collect_extras(extras)?;
        let row = self.push_validated_row(values);
        if !extras.is_empty() {
            self.extras.set(row, extras);
        }
        Ok(row)
    }

    /// Appends one runtime-width owned row after validating every value.
    ///
    /// Use [`Table::push_row`] when the row width is known at the call site.
    /// This method consumes an existing vector and does not allocate a second
    /// staging buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::RowWidth`], [`TableError::TypeMismatch`], or
    /// [`TableError::NullNotAllowed`] when the row does not match the schema.
    pub fn push_row_vec(&mut self, values: Vec<Value>) -> Result<usize, TableError> {
        self.validate_row(&values)?;
        Ok(self.push_validated_row(values))
    }

    fn validate_row(&self, values: &[Value]) -> Result<(), TableError> {
        if values.len() != self.column_count() {
            return Err(TableError::RowWidth {
                expected: self.column_count(),
                actual: values.len(),
            });
        }
        for (column, (spec, value)) in self.schema.iter().zip(values.iter()).enumerate() {
            validate_cell(spec, value, column)?;
        }
        Ok(())
    }

    fn collect_extras<I, K, V>(&self, extras: I) -> Result<RowExtras, TableError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<CompactString>,
        V: Into<Value>,
    {
        let extras = extras.into_iter();
        let mut collected = RowExtras::with_capacity(extras.size_hint().0);
        for (name, value) in extras {
            let name = name.into();
            if self.schema.column_index(&name).is_some() {
                return Err(TableError::ExtraFieldConflictsWithColumn(name));
            }
            if self.schema.unknown_fields() == UnknownFields::Reject {
                return Err(TableError::ColumnNotFound(name));
            }
            collected.set(name, value.into());
        }
        Ok(collected)
    }

    fn push_validated_row(&mut self, values: impl IntoIterator<Item = Value>) -> usize {
        let row = self.row_count;
        for (storage, value) in self.columns.iter_mut().zip(values) {
            storage.push_validated(value);
        }
        self.extras.push_empty();
        self.row_count += 1;
        debug_assert!(
            self.columns
                .iter()
                .all(|column| column.len() == self.row_count)
        );
        debug_assert!(self.extras.len_matches(self.row_count));
        row
    }

    /// Removes and discards the last row, returning whether a row existed.
    pub fn pop_row(&mut self) -> bool {
        if self.row_count == 0 {
            return false;
        }
        for column in &mut self.columns {
            column.pop();
        }
        self.extras.pop();
        self.row_count -= 1;
        true
    }

    /// Reads a cell by row and column position.
    ///
    /// # Errors
    ///
    /// Returns an out-of-bounds error for an invalid row or column.
    pub fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, TableError> {
        self.validate_position(row, column)?;
        self.columns
            .get(column)
            .and_then(|storage| storage.get(row))
            .ok_or(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count,
            })
    }

    /// Reads a cell by primary column name, alias, or stored row-local name.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnNotFound`] when neither the fixed schema nor
    /// the selected row contains the name, or an out-of-bounds row error.
    pub fn cell_named(&self, row: usize, name_or_alias: &str) -> Result<ValueRef<'_>, TableError> {
        if let Some(column) = self.schema.column_index(name_or_alias) {
            return self.cell(row, column);
        }
        if self.schema.unknown_fields() == UnknownFields::Reject {
            return Err(TableError::ColumnNotFound(name_or_alias.into()));
        }
        self.validate_row_position(row)?;
        self.extras
            .get(row)
            .and_then(|extras| extras.get(name_or_alias))
            .map(Value::as_ref)
            .ok_or_else(|| TableError::ColumnNotFound(name_or_alias.into()))
    }

    /// Replaces one cell after exact type and nullability validation.
    ///
    /// # Errors
    ///
    /// Returns a position, type, or nullability error without changing the cell.
    pub fn set_cell(&mut self, row: usize, column: usize, value: Value) -> Result<(), TableError> {
        self.validate_position(row, column)?;
        let spec = self
            .schema
            .column(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            })?;
        validate_cell(spec, &value, column)?;
        self.columns[column].set_validated(row, value);
        Ok(())
    }

    /// Replaces a fixed cell or stores an unknown name as a row-local value.
    ///
    /// Declared names and aliases retain exact schema type/null validation.
    /// Unknown names are accepted only when the schema uses
    /// [`UnknownFields::Store`]. Setting an existing extra replaces its value.
    ///
    /// # Errors
    ///
    /// Returns a row, type, or nullability error for fixed cells, or
    /// [`TableError::ColumnNotFound`] when an unknown name is rejected.
    pub fn set_named(
        &mut self,
        row: usize,
        name_or_alias: &str,
        value: impl Into<Value>,
    ) -> Result<(), TableError> {
        let value = value.into();
        if let Some(column) = self.schema.column_index(name_or_alias) {
            return self.set_cell(row, column, value);
        }
        if self.schema.unknown_fields() == UnknownFields::Reject {
            return Err(TableError::ColumnNotFound(name_or_alias.into()));
        }
        self.validate_row_position(row)?;
        self.extras
            .get_or_insert(row)
            .set(name_or_alias.into(), value);
        Ok(())
    }

    /// Returns a borrowing column view by position.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<Column<'_>> {
        Some(Column {
            spec: self.schema.column(position)?,
            storage: self.columns.get(position)?,
        })
    }

    /// Returns a borrowing column view by primary name or alias.
    #[must_use]
    pub fn column_named(&self, name_or_alias: &str) -> Option<Column<'_>> {
        self.column(self.schema.column_index(name_or_alias)?)
    }

    /// Borrows one column immutably and a distinct column mutably.
    ///
    /// This is the safe bulk-transform counterpart to [`Table::column`]. It is
    /// useful when values from one column are mapped directly into another
    /// column, including with parallel slice iterators. The returned views can
    /// each perform their normal runtime type and nullability check once.
    ///
    /// Returns `None` when either position is out of bounds or both positions
    /// identify the same column.
    #[must_use]
    pub fn column_pair_mut(
        &mut self,
        source: usize,
        target: usize,
    ) -> Option<(Column<'_>, ColumnMut<'_>)> {
        if source == target {
            return None;
        }
        let source_spec = self.schema.column(source)?;
        let target_spec = self.schema.column(target)?;
        let (source_storage, target_storage) = if source < target {
            let (before_target, target_and_after) = self.columns.split_at_mut(target);
            (before_target.get(source)?, target_and_after.first_mut()?)
        } else {
            let (before_source, source_and_after) = self.columns.split_at_mut(source);
            (source_and_after.first()?, before_source.get_mut(target)?)
        };
        Some((
            Column {
                spec: source_spec,
                storage: source_storage,
            },
            ColumnMut {
                spec: target_spec,
                storage: target_storage,
            },
        ))
    }

    /// Returns one borrowing row view.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<Row<'_>> {
        (row < self.row_count).then_some(Row { table: self, row })
    }

    /// Iterates over borrowing row views.
    #[must_use]
    pub fn rows(&self) -> impl ExactSizeIterator<Item = Row<'_>> + DoubleEndedIterator {
        (0..self.row_count).map(|row| Row { table: self, row })
    }

    /// Builds an `ahash` index for one supported column.
    ///
    /// # Errors
    ///
    /// Returns a column bounds error or [`TableError::UnsupportedIndexType`].
    pub fn index(&self, column: usize) -> Result<ColumnIndex<'_>, TableError> {
        ColumnIndex::new(self, column)
    }

    /// Builds a stable row permutation ordered by one column.
    ///
    /// The table is not mutated or copied. Equal keys retain insertion order,
    /// null placement is independent of direction, and floating-point columns
    /// use [`f32::total_cmp`] or [`f64::total_cmp`]. The operation takes
    /// **O(r log r)** time and **O(r)** space for `r` rows.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnOutOfBounds`] for an invalid column.
    pub fn row_order(
        &self,
        column: usize,
        direction: SortDirection,
        null_order: NullOrder,
    ) -> Result<RowOrder<'_>, TableError> {
        let storage = self
            .columns
            .get(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            })?;
        let mut positions: Vec<_> = (0..self.row_count).collect();
        positions.sort_by(|left, right| storage.compare_rows(*left, *right, direction, null_order));
        Ok(RowOrder {
            table: self,
            positions,
        })
    }

    /// Builds a stable row permutation ordered by a column name or alias.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::ColumnNotFound`] when the name is unknown.
    pub fn row_order_named(
        &self,
        name_or_alias: &str,
        direction: SortDirection,
        null_order: NullOrder,
    ) -> Result<RowOrder<'_>, TableError> {
        let column = self
            .schema
            .column_index(name_or_alias)
            .ok_or_else(|| TableError::ColumnNotFound(name_or_alias.into()))?;
        self.row_order(column, direction, null_order)
    }

    fn validate_position(&self, row: usize, column: usize) -> Result<(), TableError> {
        self.validate_row_position(row)?;
        if column >= self.column_count() {
            return Err(TableError::ColumnOutOfBounds {
                column,
                column_count: self.column_count(),
            });
        }
        Ok(())
    }

    fn validate_row_position(&self, row: usize) -> Result<(), TableError> {
        if row >= self.row_count {
            return Err(TableError::RowOutOfBounds {
                row,
                row_count: self.row_count,
            });
        }
        Ok(())
    }
}

/// A stable ordered view of rows in an immutably borrowed table.
///
/// The borrow prevents mutation from invalidating positions while the order is
/// used. Creating the view allocates one `usize` per row; iterating it does not
/// allocate.
#[derive(Clone, Debug)]
pub struct RowOrder<'a> {
    table: &'a Table,
    positions: Vec<usize>,
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

fn validate_cell(spec: &ColumnSpec, value: &Value, column: usize) -> Result<(), TableError> {
    let actual = value.data_type();
    if actual == DataType::Null {
        if spec.is_nullable() {
            return Ok(());
        }
        return Err(TableError::NullNotAllowed { column });
    }
    if actual != spec.data_type() {
        return Err(TableError::TypeMismatch {
            column,
            expected: spec.data_type(),
            actual,
        });
    }
    Ok(())
}

/// A borrowing view over one typed column.
#[derive(Clone, Copy)]
pub struct Column<'a> {
    spec: &'a ColumnSpec,
    storage: &'a ColumnStorage,
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
    spec: &'a ColumnSpec,
    storage: &'a mut ColumnStorage,
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
    table: &'a Table,
    row: usize,
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

/// A hashable borrowed table-index key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IndexKeyRef<'a> {
    /// A Boolean key.
    Bool(bool),
    /// Any signed integer key, widened without loss.
    Signed(i64),
    /// Any unsigned integer key, widened without loss.
    Unsigned(u64),
    /// A UTF-8 string key.
    String(&'a str),
    /// An arbitrary byte sequence key.
    Bytes(&'a [u8]),
    /// A UUID key.
    Uuid(Uuid),
}

impl<'a> IndexKeyRef<'a> {
    fn from_value(value: ValueRef<'a>) -> Option<Self> {
        Some(match value {
            ValueRef::Null | ValueRef::F32(_) | ValueRef::F64(_) => return None,
            ValueRef::Bool(value) => Self::Bool(value),
            ValueRef::I8(value) => Self::Signed(i64::from(value)),
            ValueRef::I16(value) => Self::Signed(i64::from(value)),
            ValueRef::I32(value) => Self::Signed(i64::from(value)),
            ValueRef::I64(value) => Self::Signed(value),
            ValueRef::U8(value) => Self::Unsigned(u64::from(value)),
            ValueRef::U16(value) => Self::Unsigned(u64::from(value)),
            ValueRef::U32(value) => Self::Unsigned(u64::from(value)),
            ValueRef::U64(value) => Self::Unsigned(value),
            ValueRef::String(value) => Self::String(value),
            ValueRef::Bytes(value) => Self::Bytes(value),
            ValueRef::Uuid(value) => Self::Uuid(value),
        })
    }
}

impl<'a> From<&'a str> for IndexKeyRef<'a> {
    fn from(value: &'a str) -> Self {
        Self::String(value)
    }
}

impl<'a> From<&'a [u8]> for IndexKeyRef<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Bytes(value)
    }
}

impl From<bool> for IndexKeyRef<'_> {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<Uuid> for IndexKeyRef<'_> {
    fn from(value: Uuid) -> Self {
        Self::Uuid(value)
    }
}

macro_rules! impl_index_integer {
    (signed: $($signed:ty),*; unsigned: $($unsigned:ty),*) => {
        $(
            impl From<$signed> for IndexKeyRef<'_> {
                fn from(value: $signed) -> Self {
                    Self::Signed(i64::from(value))
                }
            }
        )*
        $(
            impl From<$unsigned> for IndexKeyRef<'_> {
                fn from(value: $unsigned) -> Self {
                    Self::Unsigned(u64::from(value))
                }
            }
        )*
    };
}

impl_index_integer!(signed: i8, i16, i32, i64; unsigned: u8, u16, u32, u64);

/// An immutable hash index borrowing keys from one table column.
///
/// Null rows are tracked separately. Floating-point columns are rejected
/// because NaN and signed-zero equality need an explicit application policy.
#[derive(Clone, Debug)]
pub struct ColumnIndex<'a> {
    table: &'a Table,
    column: usize,
    by_key: IndexStorage<'a>,
    null_rows: SmallVec<[usize; 1]>,
}

#[derive(Clone, Debug)]
enum IndexStorage<'a> {
    Bool(AHashMap<bool, SmallVec<[usize; 1]>>),
    Signed(AHashMap<i64, SmallVec<[usize; 1]>>),
    Unsigned(AHashMap<u64, SmallVec<[usize; 1]>>),
    String(AHashMap<&'a str, SmallVec<[usize; 1]>>),
    Bytes(AHashMap<&'a [u8], SmallVec<[usize; 1]>>),
    Uuid(AHashMap<Uuid, SmallVec<[usize; 1]>>),
}

impl<'a> IndexStorage<'a> {
    fn new(data_type: DataType, capacity: usize) -> Self {
        match data_type {
            DataType::Bool => Self::Bool(AHashMap::with_capacity(capacity)),
            DataType::I8 | DataType::I16 | DataType::I32 | DataType::I64 => {
                Self::Signed(AHashMap::with_capacity(capacity))
            }
            DataType::U8 | DataType::U16 | DataType::U32 | DataType::U64 => {
                Self::Unsigned(AHashMap::with_capacity(capacity))
            }
            DataType::String => Self::String(AHashMap::with_capacity(capacity)),
            DataType::Bytes => Self::Bytes(AHashMap::with_capacity(capacity)),
            DataType::Uuid => Self::Uuid(AHashMap::with_capacity(capacity)),
            DataType::Null | DataType::F32 | DataType::F64 => {
                unreachable!("unsupported index type was rejected")
            }
        }
    }

    fn insert(&mut self, value: ValueRef<'a>, row: usize) {
        macro_rules! insert {
            ($map:expr, $key:expr) => {
                $map.entry($key).or_default().push(row)
            };
        }
        match (self, IndexKeyRef::from_value(value)) {
            (Self::Bool(map), Some(IndexKeyRef::Bool(key))) => insert!(map, key),
            (Self::Signed(map), Some(IndexKeyRef::Signed(key))) => insert!(map, key),
            (Self::Unsigned(map), Some(IndexKeyRef::Unsigned(key))) => insert!(map, key),
            (Self::String(map), Some(IndexKeyRef::String(key))) => insert!(map, key),
            (Self::Bytes(map), Some(IndexKeyRef::Bytes(key))) => insert!(map, key),
            (Self::Uuid(map), Some(IndexKeyRef::Uuid(key))) => insert!(map, key),
            _ => unreachable!("value type matches index storage"),
        }
    }

    fn rows(&self, key: IndexKeyRef<'_>) -> &[usize] {
        match (self, key) {
            (Self::Bool(map), IndexKeyRef::Bool(key)) => map.get(&key),
            (Self::Signed(map), IndexKeyRef::Signed(key)) => map.get(&key),
            (Self::Unsigned(map), IndexKeyRef::Unsigned(key)) => map.get(&key),
            (Self::String(map), IndexKeyRef::String(key)) => map.get(key),
            (Self::Bytes(map), IndexKeyRef::Bytes(key)) => map.get(key),
            (Self::Uuid(map), IndexKeyRef::Uuid(key)) => map.get(&key),
            _ => None,
        }
        .map_or(&[], SmallVec::as_slice)
    }

    fn len(&self) -> usize {
        match self {
            Self::Bool(map) => map.len(),
            Self::Signed(map) => map.len(),
            Self::Unsigned(map) => map.len(),
            Self::String(map) => map.len(),
            Self::Bytes(map) => map.len(),
            Self::Uuid(map) => map.len(),
        }
    }
}

impl<'a> ColumnIndex<'a> {
    fn new(table: &'a Table, column: usize) -> Result<Self, TableError> {
        let spec = table
            .schema
            .column(column)
            .ok_or(TableError::ColumnOutOfBounds {
                column,
                column_count: table.column_count(),
            })?;
        if matches!(
            spec.data_type(),
            DataType::Null | DataType::F32 | DataType::F64
        ) {
            return Err(TableError::UnsupportedIndexType(spec.data_type()));
        }
        let mut by_key = IndexStorage::new(spec.data_type(), table.row_count());
        let mut null_rows = SmallVec::new();
        for row in 0..table.row_count() {
            let value = table.columns[column]
                .get(row)
                .expect("column lengths match row count");
            if value == ValueRef::Null {
                null_rows.push(row);
            } else {
                by_key.insert(value, row);
            }
        }
        Ok(Self {
            table,
            column,
            by_key,
            null_rows,
        })
    }

    /// Returns the indexed column position.
    #[must_use]
    pub const fn column(&self) -> usize {
        self.column
    }

    /// Returns the source table.
    #[must_use]
    pub const fn table(&self) -> &'a Table {
        self.table
    }

    /// Returns all rows exactly matching `key` in insertion order.
    #[must_use]
    pub fn rows(&self, key: IndexKeyRef<'_>) -> &[usize] {
        self.by_key.rows(key)
    }

    /// Returns all rows whose indexed cell is null.
    #[must_use]
    pub fn null_rows(&self) -> &[usize] {
        &self.null_rows
    }

    /// Returns the number of distinct non-null keys.
    #[must_use]
    pub fn distinct_key_count(&self) -> usize {
        self.by_key.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ColumnData, ColumnSpec, ColumnStorage, DataType, ExtrasStorage, Schema, Table,
        UnknownFields, Value,
    };

    #[test]
    fn column_storage_follows_schema_nullability() {
        assert!(matches!(
            ColumnStorage::new(DataType::I64, false, 8),
            ColumnStorage::I64(ColumnData::Required(values)) if values.capacity() >= 8
        ));
        assert!(matches!(
            ColumnStorage::new(DataType::I64, true, 8),
            ColumnStorage::I64(ColumnData::Nullable(values)) if values.capacity() >= 8
        ));
        assert!(matches!(
            ColumnStorage::new(DataType::String, false, 8),
            ColumnStorage::String(ColumnData::Required(values)) if values.capacity() >= 8
        ));
        assert!(matches!(
            ColumnStorage::new(DataType::String, true, 8),
            ColumnStorage::String(ColumnData::Nullable(values)) if values.capacity() >= 8
        ));
    }

    #[test]
    fn extras_storage_follows_schema_policy() {
        let strict_schema = Schema::new([ColumnSpec::new("id", DataType::U64)]).unwrap();
        let mut strict = Table::with_capacity(strict_schema, 8);
        assert!(matches!(&strict.extras, ExtrasStorage::Disabled));
        strict.push_row([Value::U64(1)]).unwrap();
        assert!(matches!(&strict.extras, ExtrasStorage::Disabled));
        assert!(strict.pop_row());

        let open_schema = Schema::new([ColumnSpec::new("id", DataType::U64)])
            .unwrap()
            .with_unknown_fields(UnknownFields::Store);
        let mut open = Table::with_capacity(open_schema, 8);
        assert!(matches!(
            &open.extras,
            ExtrasStorage::Enabled(rows) if rows.is_empty() && rows.capacity() >= 8
        ));
        let row = open.push_row([Value::U64(1)]).unwrap();
        assert!(matches!(
            &open.extras,
            ExtrasStorage::Enabled(rows) if rows.len() == 1 && rows[0].is_none()
        ));
        open.set_named(row, "dynamic", "value").unwrap();
        assert!(matches!(
            &open.extras,
            ExtrasStorage::Enabled(rows) if rows[0].is_some()
        ));
    }
}

//! Typed column storage and open-schema row sidecars.

use ahash::AHashMap;
use compact_str::CompactString;
use smallvec::SmallVec;
use uuid::Uuid;

use crate::{DataType, Value, ValueRef};

use super::UnknownFields;

const ROW_EXTRAS_HASH_THRESHOLD: usize = 4;

#[derive(Clone, Debug)]
pub(super) enum RowExtras {
    Inline(SmallVec<[(CompactString, Value); 2]>),
    Hashed(AHashMap<CompactString, Value>),
}

impl Default for RowExtras {
    fn default() -> Self {
        Self::Inline(SmallVec::new())
    }
}

impl RowExtras {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        if capacity > ROW_EXTRAS_HASH_THRESHOLD {
            Self::Hashed(AHashMap::with_capacity(capacity))
        } else {
            Self::Inline(SmallVec::with_capacity(capacity))
        }
    }

    pub(super) fn get(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Inline(entries) => entries
                .iter()
                .find_map(|(entry_name, value)| (entry_name == name).then_some(value)),
            Self::Hashed(entries) => entries.get(name),
        }
    }

    pub(super) fn set(&mut self, name: CompactString, value: Value) {
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

    pub(super) fn is_empty(&self) -> bool {
        match self {
            Self::Inline(entries) => entries.is_empty(),
            Self::Hashed(entries) => entries.is_empty(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum ExtrasStorage {
    Disabled,
    Enabled(Vec<Option<Box<RowExtras>>>),
}

impl ExtrasStorage {
    pub(super) fn with_capacity(unknown_fields: UnknownFields, capacity: usize) -> Self {
        match unknown_fields {
            UnknownFields::Reject => Self::Disabled,
            UnknownFields::Store => Self::Enabled(Vec::with_capacity(capacity)),
        }
    }

    pub(super) fn push_empty(&mut self) {
        if let Self::Enabled(rows) = self {
            rows.push(None);
        }
    }

    pub(super) fn set(&mut self, row: usize, extras: RowExtras) {
        self.set_box(row, Box::new(extras));
    }

    pub(super) fn set_box(&mut self, row: usize, extras: Box<RowExtras>) {
        match self {
            Self::Enabled(rows) => rows[row] = Some(extras),
            Self::Disabled => unreachable!("closed schemas cannot store extra fields"),
        }
    }

    pub(super) fn get(&self, row: usize) -> Option<&RowExtras> {
        match self {
            Self::Disabled => None,
            Self::Enabled(rows) => rows.get(row)?.as_deref(),
        }
    }

    pub(super) fn get_or_insert(&mut self, row: usize) -> &mut RowExtras {
        match self {
            Self::Enabled(rows) => rows[row]
                .get_or_insert_with(|| Box::new(RowExtras::default()))
                .as_mut(),
            Self::Disabled => unreachable!("closed schemas cannot store extra fields"),
        }
    }

    pub(super) fn pop(&mut self) {
        if let Self::Enabled(rows) = self {
            let _ = rows.pop();
        }
    }

    pub(super) fn len_matches(&self, row_count: usize) -> bool {
        match self {
            Self::Disabled => true,
            Self::Enabled(rows) => rows.len() == row_count,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ColumnData<T> {
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
    pub(super) fn with_capacity(nullable: bool, capacity: usize) -> Self {
        if nullable {
            Self::Nullable(Vec::with_capacity(capacity))
        } else {
            Self::Required(Vec::with_capacity(capacity))
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
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
    pub(super) fn value(&self, row: usize) -> Option<&T> {
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
    pub(super) fn set(&mut self, row: usize, value: Option<T>) {
        match self {
            Self::Required(values) => {
                values[row] = value.expect("required column values were validated");
            }
            Self::Nullable(values) => values[row] = value,
        }
    }

    #[inline]
    pub(super) fn pop(&mut self) {
        match self {
            Self::Required(values) => drop(values.pop()),
            Self::Nullable(values) => drop(values.pop()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ColumnStorage {
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

impl ColumnStorage {
    #[inline]
    pub(super) fn new(data_type: DataType, nullable: bool, capacity: usize) -> Self {
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
    pub(super) fn len(&self) -> usize {
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
    pub(super) fn get(&self, row: usize) -> Option<ValueRef<'_>> {
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
    pub(super) fn push_validated(&mut self, value: Value) {
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
    pub(super) fn set_validated(&mut self, row: usize, value: Value) {
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
    pub(super) fn pop(&mut self) {
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
}

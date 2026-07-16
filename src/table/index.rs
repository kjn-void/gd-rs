//! Borrowing hash indexes over supported table columns.

use ahash::AHashMap;
use smallvec::SmallVec;
use uuid::Uuid;

use crate::{DataType, ValueRef};

use super::{Table, TableError};

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
    pub(super) fn new(table: &'a Table, column: usize) -> Result<Self, TableError> {
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

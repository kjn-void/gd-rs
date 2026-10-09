//! Borrowed dynamically typed values.

use uuid::Uuid;

use super::{DataType, Value, ValueError};

/// A borrowed dynamically typed value.
///
/// Primitive payloads are copied into the view. String and byte payloads are
/// borrowed for `'a`, preventing a view from outliving its storage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueRef<'a> {
    /// An absent value.
    Null,
    /// A Boolean value.
    Bool(bool),
    /// An 8-bit signed integer.
    I8(i8),
    /// A 16-bit signed integer.
    I16(i16),
    /// A 32-bit signed integer.
    I32(i32),
    /// A 64-bit signed integer.
    I64(i64),
    /// An 8-bit unsigned integer.
    U8(u8),
    /// A 16-bit unsigned integer.
    U16(u16),
    /// A 32-bit unsigned integer.
    U32(u32),
    /// A 64-bit unsigned integer.
    U64(u64),
    /// A 32-bit IEEE-754 floating-point number.
    F32(f32),
    /// A 64-bit IEEE-754 floating-point number.
    F64(f64),
    /// Borrowed valid UTF-8 text.
    String(&'a str),
    /// Borrowed arbitrary bytes.
    Bytes(&'a [u8]),
    /// A UUID copied inline.
    Uuid(Uuid),
}

impl<'a> ValueRef<'a> {
    /// Returns the value's logical type in constant time.
    #[must_use]
    pub const fn data_type(self) -> DataType {
        match self {
            Self::Null => DataType::Null,
            Self::Bool(_) => DataType::Bool,
            Self::I8(_) => DataType::I8,
            Self::I16(_) => DataType::I16,
            Self::I32(_) => DataType::I32,
            Self::I64(_) => DataType::I64,
            Self::U8(_) => DataType::U8,
            Self::U16(_) => DataType::U16,
            Self::U32(_) => DataType::U32,
            Self::U64(_) => DataType::U64,
            Self::F32(_) => DataType::F32,
            Self::F64(_) => DataType::F64,
            Self::String(_) => DataType::String,
            Self::Bytes(_) => DataType::Bytes,
            Self::Uuid(_) => DataType::Uuid,
        }
    }

    /// Copies the borrowed value into an owned [`Value`].
    #[must_use]
    pub fn to_owned(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(value),
            Self::I8(value) => Value::I8(value),
            Self::I16(value) => Value::I16(value),
            Self::I32(value) => Value::I32(value),
            Self::I64(value) => Value::I64(value),
            Self::U8(value) => Value::U8(value),
            Self::U16(value) => Value::U16(value),
            Self::U32(value) => Value::U32(value),
            Self::U64(value) => Value::U64(value),
            Self::F32(value) => Value::F32(value),
            Self::F64(value) => Value::F64(value),
            Self::String(value) => Value::String(value.into()),
            Self::Bytes(value) => Value::Bytes(value.into()),
            Self::Uuid(value) => Value::Uuid(value),
        }
    }

    /// Returns the contained string.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] when this is not a string.
    pub const fn as_str(self) -> Result<&'a str, ValueError> {
        match self {
            Self::String(value) => Ok(value),
            _ => Err(ValueError::TypeMismatch {
                expected: DataType::String,
                actual: self.data_type(),
            }),
        }
    }

    /// Converts any integer variant to `i64` with range checking.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] for a non-integer variant, or
    /// [`ValueError::OutOfRange`] when an unsigned value exceeds `i64::MAX`.
    pub fn to_i64(self) -> Result<i64, ValueError> {
        match self {
            Self::I8(value) => Ok(i64::from(value)),
            Self::I16(value) => Ok(i64::from(value)),
            Self::I32(value) => Ok(i64::from(value)),
            Self::I64(value) => Ok(value),
            Self::U8(value) => Ok(i64::from(value)),
            Self::U16(value) => Ok(i64::from(value)),
            Self::U32(value) => Ok(i64::from(value)),
            Self::U64(value) => i64::try_from(value).map_err(|_| ValueError::OutOfRange {
                value: value.to_string(),
                target: DataType::I64,
            }),
            _ => Err(ValueError::TypeMismatch {
                expected: DataType::I64,
                actual: self.data_type(),
            }),
        }
    }

    /// Converts any numeric variant to `f64`.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] when this is not numeric.
    #[allow(clippy::cast_precision_loss)]
    pub fn to_f64(self) -> Result<f64, ValueError> {
        match self {
            Self::I8(value) => Ok(f64::from(value)),
            Self::I16(value) => Ok(f64::from(value)),
            Self::I32(value) => Ok(f64::from(value)),
            Self::I64(value) => Ok(value as f64),
            Self::U8(value) => Ok(f64::from(value)),
            Self::U16(value) => Ok(f64::from(value)),
            Self::U32(value) => Ok(f64::from(value)),
            Self::U64(value) => Ok(value as f64),
            Self::F32(value) => Ok(f64::from(value)),
            Self::F64(value) => Ok(value),
            _ => Err(ValueError::TypeMismatch {
                expected: DataType::F64,
                actual: self.data_type(),
            }),
        }
    }

    /// Returns the contained bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] when this is not a byte sequence.
    pub const fn as_bytes(self) -> Result<&'a [u8], ValueError> {
        match self {
            Self::Bytes(value) => Ok(value),
            _ => Err(ValueError::TypeMismatch {
                expected: DataType::Bytes,
                actual: self.data_type(),
            }),
        }
    }
}

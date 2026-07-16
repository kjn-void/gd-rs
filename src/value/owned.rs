//! Owned dynamically typed values.

use compact_str::CompactString;
use uuid::Uuid;

use super::{DataType, ValueError, ValueRef};

/// An owned dynamically typed value.
///
/// This is a Rust sum type. Its discriminant and payload cannot disagree, and
/// dynamic payloads use standard owned Rust containers.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
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
    /// Owned valid UTF-8 text, stored inline when it is short enough.
    String(CompactString),
    /// Owned arbitrary bytes.
    Bytes(Box<[u8]>),
    /// A UUID stored inline.
    Uuid(Uuid),
}

impl Value {
    /// Returns the value's logical type in constant time.
    #[must_use]
    pub const fn data_type(&self) -> DataType {
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

    /// Borrows the value without allocating.
    #[must_use]
    pub fn as_ref(&self) -> ValueRef<'_> {
        match self {
            Self::Null => ValueRef::Null,
            Self::Bool(value) => ValueRef::Bool(*value),
            Self::I8(value) => ValueRef::I8(*value),
            Self::I16(value) => ValueRef::I16(*value),
            Self::I32(value) => ValueRef::I32(*value),
            Self::I64(value) => ValueRef::I64(*value),
            Self::U8(value) => ValueRef::U8(*value),
            Self::U16(value) => ValueRef::U16(*value),
            Self::U32(value) => ValueRef::U32(*value),
            Self::U64(value) => ValueRef::U64(*value),
            Self::F32(value) => ValueRef::F32(*value),
            Self::F64(value) => ValueRef::F64(*value),
            Self::String(value) => ValueRef::String(value),
            Self::Bytes(value) => ValueRef::Bytes(value),
            Self::Uuid(value) => ValueRef::Uuid(*value),
        }
    }

    /// Returns the contained string.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] when this is not a string.
    pub fn as_str(&self) -> Result<&str, ValueError> {
        self.as_ref().as_str()
    }

    /// Returns the contained bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] when this is not a byte sequence.
    pub fn as_bytes(&self) -> Result<&[u8], ValueError> {
        self.as_ref().as_bytes()
    }

    /// Converts any numeric variant to `i64` with range checking.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] for a non-integer variant, or
    /// [`ValueError::OutOfRange`] when an unsigned value exceeds `i64::MAX`.
    pub fn to_i64(&self) -> Result<i64, ValueError> {
        self.as_ref().to_i64()
    }

    /// Converts any numeric variant to `f64`.
    ///
    /// # Errors
    ///
    /// Returns [`ValueError::TypeMismatch`] when this is not numeric.
    pub fn to_f64(&self) -> Result<f64, ValueError> {
        self.as_ref().to_f64()
    }
}

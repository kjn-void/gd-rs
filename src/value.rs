//! Owned and borrowed dynamically typed values.

use std::fmt;

use compact_str::CompactString;
use thiserror::Error;
use uuid::Uuid;

/// The logical type of a [`Value`] or [`ValueRef`].
///
/// Ownership is deliberately not encoded in `DataType`: `Value::String` and
/// `ValueRef::String` have the same logical type while Rust's types and
/// lifetimes describe their different storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DataType {
    /// An absent value.
    Null,
    /// A Boolean value.
    Bool,
    /// An 8-bit signed integer.
    I8,
    /// A 16-bit signed integer.
    I16,
    /// A 32-bit signed integer.
    I32,
    /// A 64-bit signed integer.
    I64,
    /// An 8-bit unsigned integer.
    U8,
    /// A 16-bit unsigned integer.
    U16,
    /// A 32-bit unsigned integer.
    U32,
    /// A 64-bit unsigned integer.
    U64,
    /// A 32-bit IEEE-754 floating-point number.
    F32,
    /// A 64-bit IEEE-754 floating-point number.
    F64,
    /// Valid UTF-8 text.
    String,
    /// Arbitrary bytes.
    Bytes,
    /// A 128-bit UUID.
    Uuid,
}

impl DataType {
    /// Returns the stable Rust-facing name of this type.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::String => "string",
            Self::Bytes => "bytes",
            Self::Uuid => "uuid",
        }
    }

    /// Returns whether the type is an integer or floating-point number.
    #[must_use]
    pub const fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::F32
                | Self::F64
        )
    }

    /// Returns whether the type is a signed or unsigned integer.
    #[must_use]
    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
        )
    }

    /// Returns the fixed payload width, or `None` for variable-sized types.
    #[must_use]
    pub const fn fixed_width(self) -> Option<usize> {
        match self {
            Self::Null => Some(0),
            Self::Bool | Self::I8 | Self::U8 => Some(1),
            Self::I16 | Self::U16 => Some(2),
            Self::I32 | Self::U32 | Self::F32 => Some(4),
            Self::I64 | Self::U64 | Self::F64 => Some(8),
            Self::Uuid => Some(16),
            Self::String | Self::Bytes => None,
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// An error produced while reading or converting a dynamic value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ValueError {
    /// The value has a different logical type than the operation accepts.
    #[error("expected {expected}, found {actual}")]
    TypeMismatch {
        /// The requested logical type.
        expected: DataType,
        /// The value's actual logical type.
        actual: DataType,
    },
    /// A numeric value cannot be represented by the requested destination.
    #[error("{value} is outside the range of {target}")]
    OutOfRange {
        /// A display form of the rejected value.
        value: String,
        /// The requested destination type.
        target: DataType,
    },
}

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

macro_rules! impl_from_primitive {
    ($source:ty, $variant:ident) => {
        impl From<$source> for Value {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }

        impl<'a> From<$source> for ValueRef<'a> {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

impl_from_primitive!(bool, Bool);
impl_from_primitive!(i8, I8);
impl_from_primitive!(i16, I16);
impl_from_primitive!(i32, I32);
impl_from_primitive!(i64, I64);
impl_from_primitive!(u8, U8);
impl_from_primitive!(u16, U16);
impl_from_primitive!(u32, U32);
impl_from_primitive!(u64, U64);
impl_from_primitive!(f32, F32);
impl_from_primitive!(f64, F64);
impl_from_primitive!(Uuid, Uuid);

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}

impl<'a> From<&'a str> for ValueRef<'a> {
    fn from(value: &'a str) -> Self {
        Self::String(value)
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value.into_boxed_slice())
    }
}

impl From<&[u8]> for Value {
    fn from(value: &[u8]) -> Self {
        Self::Bytes(value.into())
    }
}

impl<'a> From<&'a [u8]> for ValueRef<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self::Bytes(value)
    }
}

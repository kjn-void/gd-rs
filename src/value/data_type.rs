//! Logical dynamic-value types.

use std::fmt;

/// The logical type of a [`crate::Value`] or [`crate::ValueRef`].
///
/// Ownership is deliberately not encoded in `DataType`: [`crate::Value::String`] and
/// [`crate::ValueRef::String`] have the same logical type while Rust's types and
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

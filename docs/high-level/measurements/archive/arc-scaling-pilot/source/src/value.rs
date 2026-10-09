//! Owned and borrowed dynamically typed values.

mod borrowed;
mod data_type;
mod owned;

pub use borrowed::ValueRef;
pub use data_type::DataType;
pub use owned::Value;

use thiserror::Error;
use uuid::Uuid;

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

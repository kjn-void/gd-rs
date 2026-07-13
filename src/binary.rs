//! Bounds-checked binary encoding, decoding, hex, and byte search.

use std::str::{self, Utf8Error};

use hex_simd::{AsciiCase, Error as HexError};
use memchr::memmem;
use thiserror::Error;

/// Byte order used by multi-byte reader and writer methods.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Endian {
    /// Most-significant byte first.
    Big,
    /// Least-significant byte first.
    Little,
    /// The target platform's native byte order.
    Native,
}

/// A binary cursor, length-prefix, hex, or UTF-8 error.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum BinaryError {
    /// An operation requires more bytes than remain in the cursor.
    #[error("need {needed} bytes at position {position}, but only {remaining} remain")]
    UnexpectedEof {
        /// Cursor position where the operation began.
        position: usize,
        /// Bytes required by the operation.
        needed: usize,
        /// Bytes available at the position.
        remaining: usize,
    },
    /// A seek target lies beyond the buffer.
    #[error("position {position} is out of bounds for a {length}-byte buffer")]
    PositionOutOfBounds {
        /// Requested cursor position.
        position: usize,
        /// Total buffer length.
        length: usize,
    },
    /// A payload cannot be represented by a 32-bit length prefix.
    #[error("payload length exceeds a u32 prefix")]
    LengthOverflow,
    /// Hexadecimal text is malformed.
    #[error("invalid hexadecimal text")]
    InvalidHex,
    /// A length-prefixed string is not valid UTF-8.
    #[error(transparent)]
    InvalidUtf8(#[from] Utf8Error),
}

impl From<HexError> for BinaryError {
    fn from(_: HexError) -> Self {
        Self::InvalidHex
    }
}

/// Decodes an even-length hexadecimal string.
///
/// Both lowercase and uppercase digits are accepted. Unlike the C++ validator,
/// an empty string is a valid encoding of an empty byte sequence.
///
/// # Errors
///
/// Returns [`BinaryError::InvalidHex`] for an odd length or non-hex digit.
pub fn decode_hex(text: &str) -> Result<Vec<u8>, BinaryError> {
    Ok(hex_simd::decode_to_vec(text)?)
}

/// Encodes bytes using lowercase hexadecimal digits.
#[must_use]
pub fn encode_hex(bytes: &[u8]) -> String {
    hex_simd::encode_to_string(bytes, AsciiCase::Lower)
}

/// Encodes bytes using uppercase hexadecimal digits.
#[must_use]
pub fn encode_hex_upper(bytes: &[u8]) -> String {
    hex_simd::encode_to_string(bytes, AsciiCase::Upper)
}

/// Finds the first occurrence of `needle` at or after `offset`.
///
/// The implementation delegates to `memchr`'s architecture-specific substring
/// search. An empty needle matches at `offset` when `offset <= haystack.len()`.
#[must_use]
pub fn find_bytes(haystack: &[u8], needle: &[u8], offset: usize) -> Option<usize> {
    let tail = haystack.get(offset..)?;
    memmem::find(tail, needle).map(|position| position + offset)
}

/// Finds the last occurrence of `needle`.
///
/// An empty needle matches at `haystack.len()`.
#[must_use]
pub fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    memmem::rfind(haystack, needle)
}

/// A non-owning, bounds-checked cursor over immutable bytes.
///
/// Failed operations do not change the cursor position.
#[derive(Clone, Copy, Debug)]
pub struct BinaryReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BinaryReader<'a> {
    /// Creates a cursor at the start of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// Returns the total buffer length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the underlying buffer is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the current byte position.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Returns the number of unread bytes.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    /// Returns whether no unread bytes remain.
    #[must_use]
    pub const fn is_eof(&self) -> bool {
        self.position == self.bytes.len()
    }

    /// Returns the next byte without advancing.
    #[must_use]
    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    /// Moves to an absolute position.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::PositionOutOfBounds`] if `position > self.len()`.
    pub fn seek(&mut self, position: usize) -> Result<(), BinaryError> {
        if position > self.len() {
            return Err(BinaryError::PositionOutOfBounds {
                position,
                length: self.len(),
            });
        }
        self.position = position;
        Ok(())
    }

    /// Advances by exactly `count` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] without advancing when too few
    /// bytes remain.
    pub fn skip(&mut self, count: usize) -> Result<(), BinaryError> {
        let _ = self.read_exact(count)?;
        Ok(())
    }

    /// Borrows and advances past exactly `count` bytes.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] without advancing when too few
    /// bytes remain.
    pub fn read_exact(&mut self, count: usize) -> Result<&'a [u8], BinaryError> {
        let end = self
            .position
            .checked_add(count)
            .filter(|end| *end <= self.len());
        let Some(end) = end else {
            return Err(self.eof_error(count));
        };
        let result = &self.bytes[self.position..end];
        self.position = end;
        Ok(result)
    }

    /// Reads one unsigned byte.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] at end of input.
    pub fn read_u8(&mut self) -> Result<u8, BinaryError> {
        Ok(self.read_exact(1)?[0])
    }

    /// Reads one signed byte.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] at end of input.
    pub fn read_i8(&mut self) -> Result<i8, BinaryError> {
        Ok(i8::from_ne_bytes([self.read_u8()?]))
    }

    /// Reads a `u16`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than two bytes remain.
    pub fn read_u16(&mut self, endian: Endian) -> Result<u16, BinaryError> {
        Ok(read_array(
            self.read_exact(2)?,
            endian,
            u16::from_be_bytes,
            u16::from_le_bytes,
            u16::from_ne_bytes,
        ))
    }

    /// Reads an `i16`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than two bytes remain.
    pub fn read_i16(&mut self, endian: Endian) -> Result<i16, BinaryError> {
        Ok(i16::from_ne_bytes(self.read_u16(endian)?.to_ne_bytes()))
    }

    /// Reads a `u32`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than four bytes remain.
    pub fn read_u32(&mut self, endian: Endian) -> Result<u32, BinaryError> {
        Ok(read_array(
            self.read_exact(4)?,
            endian,
            u32::from_be_bytes,
            u32::from_le_bytes,
            u32::from_ne_bytes,
        ))
    }

    /// Reads an `i32`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than four bytes remain.
    pub fn read_i32(&mut self, endian: Endian) -> Result<i32, BinaryError> {
        Ok(i32::from_ne_bytes(self.read_u32(endian)?.to_ne_bytes()))
    }

    /// Reads a `u64`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than eight bytes remain.
    pub fn read_u64(&mut self, endian: Endian) -> Result<u64, BinaryError> {
        Ok(read_array(
            self.read_exact(8)?,
            endian,
            u64::from_be_bytes,
            u64::from_le_bytes,
            u64::from_ne_bytes,
        ))
    }

    /// Reads an `i64`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than eight bytes remain.
    pub fn read_i64(&mut self, endian: Endian) -> Result<i64, BinaryError> {
        Ok(i64::from_ne_bytes(self.read_u64(endian)?.to_ne_bytes()))
    }

    /// Reads an IEEE-754 `f32` without changing its bit pattern.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than four bytes remain.
    pub fn read_f32(&mut self, endian: Endian) -> Result<f32, BinaryError> {
        Ok(f32::from_bits(self.read_u32(endian)?))
    }

    /// Reads an IEEE-754 `f64` without changing its bit pattern.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when fewer than eight bytes remain.
    pub fn read_f64(&mut self, endian: Endian) -> Result<f64, BinaryError> {
        Ok(f64::from_bits(self.read_u64(endian)?))
    }

    /// Reads a `u32`-length-prefixed byte slice atomically.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] without changing position if the
    /// prefix or complete payload is unavailable.
    pub fn read_bytes_u32(&mut self, endian: Endian) -> Result<&'a [u8], BinaryError> {
        let start = self.position;
        let result = (|| {
            let length =
                usize::try_from(self.read_u32(endian)?).map_err(|_| BinaryError::LengthOverflow)?;
            self.read_exact(length)
        })();
        if result.is_err() {
            self.position = start;
        }
        result
    }

    /// Reads a `u32`-length-prefixed UTF-8 string atomically.
    ///
    /// # Errors
    ///
    /// Returns a bounds or UTF-8 error without changing position.
    pub fn read_str_u32(&mut self, endian: Endian) -> Result<&'a str, BinaryError> {
        let start = self.position;
        let result = self
            .read_bytes_u32(endian)
            .and_then(|bytes| Ok(str::from_utf8(bytes)?));
        if result.is_err() {
            self.position = start;
        }
        result
    }

    fn eof_error(&self, needed: usize) -> BinaryError {
        BinaryError::UnexpectedEof {
            position: self.position,
            needed,
            remaining: self.remaining(),
        }
    }
}

fn read_array<T, const N: usize>(
    bytes: &[u8],
    endian: Endian,
    big_endian: fn([u8; N]) -> T,
    little_endian: fn([u8; N]) -> T,
    native_endian: fn([u8; N]) -> T,
) -> T {
    let array: [u8; N] = bytes.try_into().expect("caller provided N bytes");
    match endian {
        Endian::Big => big_endian(array),
        Endian::Little => little_endian(array),
        Endian::Native => native_endian(array),
    }
}

macro_rules! endian_bytes {
    ($value:expr, $endian:expr) => {
        match $endian {
            Endian::Big => $value.to_be_bytes(),
            Endian::Little => $value.to_le_bytes(),
            Endian::Native => $value.to_ne_bytes(),
        }
    };
}

/// A non-owning, bounds-checked cursor over mutable bytes.
///
/// Failed operations do not change the cursor position or buffer.
#[derive(Debug)]
pub struct BinaryWriter<'a> {
    bytes: &'a mut [u8],
    position: usize,
}

impl<'a> BinaryWriter<'a> {
    /// Creates a cursor at the start of `bytes`.
    pub const fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// Returns the total buffer length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the underlying buffer is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the current byte position.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Returns the number of unwritten bytes.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    /// Returns the prefix written so far.
    #[must_use]
    pub fn written(&self) -> &[u8] {
        &self.bytes[..self.position]
    }

    /// Moves to an absolute position.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::PositionOutOfBounds`] if `position > self.len()`.
    pub fn seek(&mut self, position: usize) -> Result<(), BinaryError> {
        if position > self.len() {
            return Err(BinaryError::PositionOutOfBounds {
                position,
                length: self.len(),
            });
        }
        self.position = position;
        Ok(())
    }

    /// Writes all bytes atomically.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] without changing the cursor or
    /// buffer when insufficient space remains.
    pub fn write_exact(&mut self, value: &[u8]) -> Result<(), BinaryError> {
        let end = self
            .position
            .checked_add(value.len())
            .filter(|end| *end <= self.len());
        let Some(end) = end else {
            return Err(self.eof_error(value.len()));
        };
        self.bytes[self.position..end].copy_from_slice(value);
        self.position = end;
        Ok(())
    }

    /// Writes one byte.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when no space remains.
    pub fn write_u8(&mut self, value: u8) -> Result<(), BinaryError> {
        self.write_exact(&[value])
    }

    /// Writes one signed byte.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when no space remains.
    pub fn write_i8(&mut self, value: i8) -> Result<(), BinaryError> {
        self.write_exact(&value.to_ne_bytes())
    }

    /// Writes a `u16`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_u16(&mut self, value: u16, endian: Endian) -> Result<(), BinaryError> {
        self.write_exact(&endian_bytes!(value, endian))
    }

    /// Writes an `i16`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_i16(&mut self, value: i16, endian: Endian) -> Result<(), BinaryError> {
        self.write_exact(&endian_bytes!(value, endian))
    }

    /// Writes a `u32`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_u32(&mut self, value: u32, endian: Endian) -> Result<(), BinaryError> {
        self.write_exact(&endian_bytes!(value, endian))
    }

    /// Writes an `i32`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_i32(&mut self, value: i32, endian: Endian) -> Result<(), BinaryError> {
        self.write_exact(&endian_bytes!(value, endian))
    }

    /// Writes a `u64`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_u64(&mut self, value: u64, endian: Endian) -> Result<(), BinaryError> {
        self.write_exact(&endian_bytes!(value, endian))
    }

    /// Writes an `i64`.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_i64(&mut self, value: i64, endian: Endian) -> Result<(), BinaryError> {
        self.write_exact(&endian_bytes!(value, endian))
    }

    /// Writes an IEEE-754 `f32` without changing its bit pattern.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_f32(&mut self, value: f32, endian: Endian) -> Result<(), BinaryError> {
        self.write_u32(value.to_bits(), endian)
    }

    /// Writes an IEEE-754 `f64` without changing its bit pattern.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::UnexpectedEof`] when insufficient space remains.
    pub fn write_f64(&mut self, value: f64, endian: Endian) -> Result<(), BinaryError> {
        self.write_u64(value.to_bits(), endian)
    }

    /// Writes a `u32` length prefix and payload atomically.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::LengthOverflow`] or [`BinaryError::UnexpectedEof`]
    /// without changing the cursor or buffer.
    pub fn write_bytes_u32(&mut self, value: &[u8], endian: Endian) -> Result<(), BinaryError> {
        let length = u32::try_from(value.len()).map_err(|_| BinaryError::LengthOverflow)?;
        let needed = 4_usize
            .checked_add(value.len())
            .ok_or(BinaryError::LengthOverflow)?;
        if needed > self.remaining() {
            return Err(self.eof_error(needed));
        }
        self.write_u32(length, endian)?;
        self.write_exact(value)
    }

    /// Writes a `u32` length prefix and a UTF-8 string atomically.
    ///
    /// The prefix stores the UTF-8 byte length, not the Unicode scalar count.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError::LengthOverflow`] or [`BinaryError::UnexpectedEof`]
    /// without changing the cursor or buffer.
    pub fn write_str_u32(&mut self, value: &str, endian: Endian) -> Result<(), BinaryError> {
        self.write_bytes_u32(value.as_bytes(), endian)
    }

    fn eof_error(&self, needed: usize) -> BinaryError {
        BinaryError::UnexpectedEof {
            position: self.position,
            needed,
            remaining: self.remaining(),
        }
    }
}

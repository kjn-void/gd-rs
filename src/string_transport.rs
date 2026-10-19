//! UTF-8 string construction in caller storage with automatic heap fallback.

// Expose caller-known copy lengths, including through formatting/operator adapters.
#![allow(clippy::inline_always)]

use std::{fmt, ops::AddAssign};

enum Storage<'a> {
    Borrowed(&'a mut [u8]),
    Owned(Box<[u8]>),
}

/// A UTF-8 builder that borrows a byte buffer until its contents outgrow it.
///
/// One byte is reserved for a trailing NUL. When the text no longer fits,
/// the builder copies its used prefix to the heap and remains owned. Each
/// owned append requests exactly the resulting byte length plus one; use
/// [`String`] instead when amortized heap growth is preferable.
///
/// Interior NUL characters are allowed. Ordinary text access excludes the trailing
/// terminator. Converting a reference into a [`crate::Value`] copies the text,
/// allowing the builder to be reused independently of a table storing that value.
///
/// ```
/// use gd::{StringTransport, Value};
///
/// let mut buffer = [0_u8; 150];
/// let mut text = StringTransport::new(&mut buffer);
/// text.assign("12345");
/// text += " 12345";
/// text.append(" 12345");
/// let value = Value::from(&text);
/// text.assign("reused");
/// assert_eq!(value.as_str().unwrap(), "12345 12345 12345");
/// ```
pub struct StringTransport<'a> {
    storage: Storage<'a>,
    length: usize,
}

impl<'a> StringTransport<'a> {
    /// Borrows `buffer`, initializing it to empty without allocating.
    ///
    /// A buffer of length `n` holds at most `n.saturating_sub(1)` text bytes.
    /// Empty buffers are accepted and spill on the first append or assignment.
    #[must_use]
    pub fn new(buffer: &'a mut [u8]) -> Self {
        if let Some(first) = buffer.first_mut() {
            *first = 0;
        }
        Self {
            storage: Storage::Borrowed(buffer),
            length: 0,
        }
    }

    /// Creates an empty owned builder without allocating until the first write.
    #[must_use]
    pub fn owned() -> Self {
        Self {
            storage: Storage::Owned(Box::default()),
            length: 0,
        }
    }

    /// Replaces the text, retaining owned mode if the builder has already spilled.
    ///
    /// # Panics
    ///
    /// Panics if the required allocation exceeds the platform's capacity limit.
    #[inline(always)]
    pub fn assign(&mut self, text: &str) {
        self.length = 0;
        self.append(text);
    }

    /// Appends UTF-8 text, moving to owned storage if the borrowed buffer is full.
    ///
    /// # Panics
    ///
    /// Panics if the required allocation exceeds the platform's capacity limit.
    #[inline(always)]
    pub fn append(&mut self, text: &str) {
        // Each length is at most isize::MAX, so their sum fits usize.
        let end = self.length + text.len();
        match &mut self.storage {
            Storage::Borrowed(buffer) if end < buffer.len() => {
                buffer[self.length..end].copy_from_slice(text.as_bytes());
                buffer[end] = 0;
            }
            _ => self.append_owned(text, end),
        }
        // Keep this update visible to callers across the allocation helper.
        self.length = end;
    }

    #[cold]
    #[inline(never)]
    fn append_owned(&mut self, text: &str, end: usize) {
        let required = end.checked_add(1).expect("String too long");
        let mut next = vec![0_u8; required].into_boxed_slice();
        next[..self.length].copy_from_slice(self.as_bytes());
        next[self.length..end].copy_from_slice(text.as_bytes());
        self.storage = Storage::Owned(next);
    }

    /// Returns the text length in UTF-8 bytes, excluding the terminator.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.length
    }

    /// Returns whether the builder contains no text.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Returns the storage capacity in bytes, including space for the terminator.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.bytes().len()
    }

    /// Returns whether the builder still uses the caller's buffer.
    #[must_use]
    pub fn is_borrowed(&self) -> bool {
        matches!(self.storage, Storage::Borrowed(_))
    }

    /// Returns the initialized text bytes, excluding the trailing NUL.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes()[..self.length]
    }

    /// Returns the text as UTF-8, excluding the trailing NUL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.valid_text()
    }

    /// Returns the text, matching the original transport helper's accessor.
    #[must_use]
    pub fn text(&self) -> &str {
        self.as_str()
    }

    /// Returns the text and its trailing NUL, including any interior NUL bytes.
    ///
    /// An empty builder without physical storage returns a static NUL byte.
    #[must_use]
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        if self.bytes().is_empty() {
            b"\0"
        } else {
            &self.bytes()[..=self.length]
        }
    }

    fn bytes(&self) -> &[u8] {
        match &self.storage {
            Storage::Borrowed(buffer) => buffer,
            Storage::Owned(buffer) => buffer,
        }
    }

    fn valid_text(&self) -> &str {
        // Writes accept only valid UTF-8 and never expose mutable byte access.
        std::str::from_utf8(self.as_bytes()).expect("StringTransport preserves UTF-8")
    }
}

impl Default for StringTransport<'_> {
    fn default() -> Self {
        Self::owned()
    }
}

impl AsRef<str> for StringTransport<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for StringTransport<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Debug for StringTransport<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StringTransport")
            .field("text", &self.as_str())
            .field("capacity", &self.capacity())
            .field("borrowed", &self.is_borrowed())
            .finish()
    }
}

impl fmt::Write for StringTransport<'_> {
    #[inline(always)]
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.append(text);
        Ok(())
    }
}

impl AddAssign<&str> for StringTransport<'_> {
    #[inline(always)]
    fn add_assign(&mut self, text: &str) {
        self.append(text);
    }
}

//! UTF conversion and text-format boundaries.

use std::{borrow::Cow, str::Utf8Error, string::FromUtf16Error};

use percent_encoding::{AsciiSet, CONTROLS, percent_decode, utf8_percent_encode};
use thiserror::Error;

/// Characters escaped by the characterized C++ URI-component encoder.
///
/// Its unescaped punctuation is: exclamation mark, apostrophe, parentheses,
/// asterisk, hyphen, period, underscore, and grave accent. This differs from common URL
/// component sets by escaping `~`.
const GD_COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'~');

/// A UTF-8, UTF-16, JSON, or percent-decoding error.
#[derive(Debug, Error)]
pub enum TextError {
    /// A byte boundary did not contain valid UTF-8.
    #[error(transparent)]
    InvalidUtf8(#[from] Utf8Error),
    /// UTF-16 contained an unpaired surrogate.
    #[error(transparent)]
    InvalidUtf16(#[from] FromUtf16Error),
    /// A percent sign was not followed by two hexadecimal digits.
    #[error("invalid percent escape at byte {position}")]
    InvalidPercent {
        /// Byte offset of the malformed percent sign.
        position: usize,
    },
    /// A complete JSON string literal was malformed or had another JSON type.
    #[error(transparent)]
    InvalidJson(#[from] serde_json::Error),
}

/// Validates bytes as UTF-8 and returns the same storage as a string slice.
///
/// This is the codec-boundary counterpart of accepting `&str` in the rest of
/// the public API.
///
/// # Errors
///
/// Returns [`Utf8Error`] at the first malformed sequence.
pub fn validate_utf8(bytes: &[u8]) -> Result<&str, Utf8Error> {
    std::str::from_utf8(bytes)
}

/// Decodes UTF-16 into an owned UTF-8 string.
///
/// # Errors
///
/// Returns [`TextError::InvalidUtf16`] for an unpaired surrogate.
pub fn decode_utf16(units: &[u16]) -> Result<String, TextError> {
    Ok(String::from_utf16(units)?)
}

/// Encodes text as a complete JSON string literal, including its quotes.
///
/// All JSON control characters are escaped. Astral code points are preserved;
/// `serde_json` may keep them as UTF-8 rather than spelling surrogate escapes.
///
/// # Panics
///
/// String serialization is infallible in `serde_json`; a panic would indicate
/// that this upstream contract changed.
#[must_use]
pub fn encode_json_string(text: &str) -> String {
    serde_json::to_string(text).expect("serializing a string cannot fail")
}

/// Decodes a complete JSON string literal.
///
/// # Errors
///
/// Returns [`TextError::InvalidJson`] for malformed JSON or a value that is not
/// a JSON string.
pub fn decode_json_string(literal: &str) -> Result<String, TextError> {
    Ok(serde_json::from_str(literal)?)
}

/// Percent-encodes one URI component using the characterized `gd` allow-list.
///
/// Hexadecimal digits are uppercase. The operation is **O(n)** in input bytes
/// and returns newly allocated text.
#[must_use]
pub fn encode_percent_component(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    push_percent_component(&mut encoded, text);
    encoded
}

/// Decodes one percent-encoded URI component as UTF-8.
///
/// `+` decodes to a space to retain the characterized C++ behavior. Plain text
/// without escapes is returned as a borrowed value.
///
/// # Errors
///
/// Returns [`TextError::InvalidPercent`] for a truncated or non-hex escape and
/// [`TextError::InvalidUtf8`] when decoded bytes are not UTF-8.
pub fn decode_percent_component(text: &str) -> Result<Cow<'_, str>, TextError> {
    let (has_percent, has_plus) = validate_percent_escapes(text.as_bytes())?;

    if !has_percent && !has_plus {
        return Ok(Cow::Borrowed(text));
    }

    if has_plus {
        let mut with_spaces = text.as_bytes().to_vec();
        for byte in &mut with_spaces {
            if *byte == b'+' {
                *byte = b' ';
            }
        }
        if !has_percent {
            let decoded = String::from_utf8(with_spaces).map_err(|error| error.utf8_error())?;
            return Ok(Cow::Owned(decoded));
        }
        return Ok(Cow::Owned(
            percent_decode(&with_spaces).decode_utf8()?.into_owned(),
        ));
    }

    Ok(percent_decode(text.as_bytes()).decode_utf8()?)
}

pub(crate) fn push_percent_component(output: &mut String, text: &str) {
    for segment in utf8_percent_encode(text, GD_COMPONENT) {
        output.push_str(segment);
    }
}

/// Escapes the five XML predefined entities in text content or an attribute.
///
/// The original string is borrowed when no escaping is necessary. This function
/// does not add element or attribute delimiters and does not validate XML 1.0
/// control-character restrictions.
#[must_use]
pub fn escape_xml(text: &str) -> Cow<'_, str> {
    let Some(first) = text.find(['&', '<', '>', '"', '\'']) else {
        return Cow::Borrowed(text);
    };

    let mut escaped = String::with_capacity(text.len() + 8);
    escaped.push_str(&text[..first]);
    for character in text[first..].chars() {
        escaped.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&apos;",
            _ => {
                escaped.push(character);
                continue;
            }
        });
    }
    Cow::Owned(escaped)
}

/// Splits text on `separator`, treating a doubled separator as one literal byte.
///
/// A trailing single separator produces a final empty part. A doubled separator
/// at the end belongs to the final part. The separator may be any Unicode scalar
/// value. The operation is **O(n)** time and output space.
#[must_use]
pub fn split_escaped(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if character == separator {
            if characters.peek() == Some(&separator) {
                part.push(separator);
                characters.next();
                continue;
            }
            parts.push(std::mem::take(&mut part));
        } else {
            part.push(character);
        }
    }
    parts.push(part);
    parts
}

/// Returns at most the first `count` Unicode scalar values without allocating.
///
/// This counts scalar values, not grapheme clusters. It runs in **O(count)** time
/// up to the end of `text`.
#[must_use]
pub fn prefix_chars(text: &str, count: usize) -> &str {
    let end = text
        .char_indices()
        .nth(count)
        .map_or(text.len(), |(offset, _)| offset);
    &text[..end]
}

/// Trims leading and trailing ASCII control bytes and spaces (`U+0000..=U+0020`).
///
/// This preserves the C++ byte-classification rule and intentionally differs
/// from [`str::trim`], which recognizes Unicode whitespace.
#[must_use]
pub fn trim_ascii_control(text: &str) -> &str {
    text.trim_matches(|character| character <= '\u{20}')
}

fn validate_percent_escapes(bytes: &[u8]) -> Result<(bool, bool), TextError> {
    let mut position = 0;
    let mut has_percent = false;
    let mut has_plus = false;
    while position < bytes.len() {
        if bytes[position] == b'%' {
            if bytes
                .get(position + 1)
                .is_none_or(|byte| !byte.is_ascii_hexdigit())
                || bytes
                    .get(position + 2)
                    .is_none_or(|byte| !byte.is_ascii_hexdigit())
            {
                return Err(TextError::InvalidPercent { position });
            }
            has_percent = true;
            position += 3;
        } else {
            has_plus |= bytes[position] == b'+';
            position += 1;
        }
    }
    Ok((has_percent, has_plus))
}

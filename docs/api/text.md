# Text and encoding

Rust strings are always valid UTF-8, so `gd-rs` does not reproduce the C++ UTF-8
pointer iterators or mutable byte-string utilities. The public functions live at
codec and interchange boundaries where validation or escaping is still necessary.

## UTF boundaries

```rust
use gd::{decode_utf16, validate_utf8};

assert_eq!(validate_utf8(b"Gr\xC3\xA4nssnitt").unwrap(), "Gränssnitt");
assert_eq!(decode_utf16(&[0x0041, 0xD83D, 0xDE80]).unwrap(), "A🚀");
```

`validate_utf8` returns a borrowed `&str` over the input bytes and allocates nothing.
`decode_utf16` returns an owned `String` and rejects unpaired surrogates. Most APIs
accept `&str` directly and therefore need neither operation.

## JSON strings

```rust
use gd::{decode_json_string, encode_json_string};

let literal = encode_json_string("line one\n\"line two\"");
assert_eq!(literal, "\"line one\\n\\\"line two\\\"\"");
assert_eq!(decode_json_string(&literal).unwrap(), "line one\n\"line two\"");
```

These functions consume and produce a complete JSON string literal, including its
quotes. Decoding rejects malformed JSON and valid JSON values of any other type.

## Percent encoding

```rust
use gd::{decode_percent_component, encode_percent_component};

assert_eq!(encode_percent_component("rust & c++"), "rust%20%26%20c%2B%2B");
assert_eq!(decode_percent_component("rust+and%20gd").unwrap(), "rust and gd");
```

The encoder preserves the punctuation characterized from C++ gd: `! ' ( ) * - . _`,
plus the grave accent. It escapes `~`, uses uppercase hexadecimal digits, and returns
newly allocated text.
The decoder also treats `+` as a space for compatibility. It strictly rejects a
truncated or non-hex percent escape and decoded bytes that are not UTF-8. When the
input contains neither `%` nor `+`, decoding returns a borrowed `Cow::Borrowed`.

## XML escaping

```rust
use std::borrow::Cow;

use gd::escape_xml;

assert_eq!(escape_xml("<tag a='b'>&"), "&lt;tag a=&apos;b&apos;&gt;&amp;");
assert!(matches!(escape_xml("plain text"), Cow::Borrowed(_)));
```

`escape_xml` replaces the five predefined entities: ampersand, less-than,
greater-than, double quote, and apostrophe. It adds no surrounding XML markup and
does not validate whether control characters are legal in an XML document.

## Small text operations

```rust
use gd::{prefix_chars, split_escaped, trim_ascii_control};

assert_eq!(split_escaped("one;;literal;three", ';'), ["one;literal", "three"]);
assert_eq!(prefix_chars("a🚀b", 2), "a🚀");
assert_eq!(trim_ascii_control("\0  value\n"), "value");
```

`split_escaped` treats a doubled separator as one literal separator. `prefix_chars`
counts Unicode scalar values, not bytes or user-perceived grapheme clusters.
`trim_ascii_control` trims only `U+0000..=U+0020`, preserving the byte-classification
rule from C++; it intentionally differs from `str::trim` and Unicode whitespace.

Normalization, locale-sensitive case conversion, grapheme segmentation, and general
Unicode classification are better supplied by dedicated Rust crates when needed.

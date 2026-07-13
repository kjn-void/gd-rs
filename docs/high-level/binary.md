# Binary data

The `binary` module provides bounds-checked cursors, integer and floating-point
encoding, hexadecimal conversion, and byte substring search. It is an ordinary Rust
API rather than a representation-compatible port of the C++ reader and writer.

## Cursors

`BinaryReader<'a>` borrows immutable bytes and returns borrowed slices for fixed-size
and length-prefixed payloads. `BinaryWriter<'a>` borrows mutable bytes. Both cursors
use an absolute byte position and leave that position unchanged when an operation
fails.

```rust
use gd::{BinaryError, BinaryReader, BinaryWriter, Endian};

let mut storage = [0_u8; 16];
let mut writer = BinaryWriter::new(&mut storage);
writer.write_u32(0x1234_5678, Endian::Big)?;
writer.write_f32(1.5, Endian::Little)?;

let mut reader = BinaryReader::new(&storage[..writer.position()]);
assert_eq!(reader.read_u32(Endian::Big)?, 0x1234_5678);
assert_eq!(reader.read_f32(Endian::Little)?, 1.5);
# Ok::<(), BinaryError>(())
```

Integer methods accept [`Endian`](../../src/binary.rs). Floating-point values are
transferred through `to_bits` and `from_bits`, preserving NaN payloads, infinities,
and signed zero. No alignment assumption is made about the input buffer.

The `read_bytes_u32`, `read_str_u32`, `write_bytes_u32`, and `write_str_u32` methods
use a `u32` byte-length prefix. Prefix and payload are one logical operation: a
truncated read or undersized destination does not consume or write the prefix.

## Hex and byte search

`encode_hex`, `encode_hex_upper`, and `decode_hex` delegate to `hex-simd`. Empty hex
text decodes to an empty vector; odd-length or malformed input returns
`BinaryError::InvalidHex`.

`find_bytes` and `rfind_bytes` delegate to `memchr::memmem`. `find_bytes` accepts a
start offset, and an empty needle follows Rust slice-search conventions: it matches at
the requested offset when that offset is in bounds.

These crate-backed functions replace the C++ scalar hexadecimal loops and naive
substring search. Comparative measurements are recorded in
[Benchmark methodology and results](performance.md).

## Errors

`BinaryError` distinguishes insufficient input/output, an invalid seek target, a
payload too large for a `u32` prefix, malformed hexadecimal text, and invalid UTF-8.
Expected input failures are returned to the caller; the module has no sticky hidden
error flag and does not log.

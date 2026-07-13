//! Integration and property tests for binary cursors, hex, and byte search.

use gd::{
    BinaryError, BinaryReader, BinaryWriter, Endian, decode_hex, encode_hex, encode_hex_upper,
    find_bytes, rfind_bytes,
};
use proptest::prelude::*;

#[test]
fn hex_and_search_match_characterized_behavior() {
    let bytes = decode_hex("00a1B2ff").unwrap();
    assert_eq!(bytes, [0x00, 0xa1, 0xb2, 0xff]);
    assert_eq!(encode_hex(&bytes), "00a1b2ff");
    assert_eq!(encode_hex_upper(&bytes), "00A1B2FF");
    assert!(decode_hex("abc").is_err());
    assert!(decode_hex("0g").is_err());

    let haystack = [1, 2, 3, 2, 3, 4, 2, 3];
    assert_eq!(find_bytes(&haystack, &[2, 3], 0), Some(1));
    assert_eq!(find_bytes(&haystack, &[2, 3], 2), Some(3));
    assert_eq!(rfind_bytes(&haystack, &[2, 3]), Some(6));
}

#[test]
fn failed_cursor_operations_are_atomic() {
    let mut reader = BinaryReader::new(&[1, 2]);
    assert!(matches!(
        reader.read_u32(Endian::Big),
        Err(BinaryError::UnexpectedEof { .. })
    ));
    assert_eq!(reader.position(), 0);

    let mut bytes = [0xaa, 0xbb];
    let mut writer = BinaryWriter::new(&mut bytes);
    assert!(writer.write_u32(42, Endian::Big).is_err());
    assert_eq!(writer.position(), 0);
    assert_eq!(bytes, [0xaa, 0xbb]);
}

#[test]
fn length_prefixed_strings_are_atomic_and_borrowed() {
    let mut bytes = [0_u8; 32];
    let length;
    {
        let mut writer = BinaryWriter::new(&mut bytes);
        writer.write_str_u32("hello", Endian::Big).unwrap();
        length = writer.position();
    }
    let mut reader = BinaryReader::new(&bytes[..length]);
    assert_eq!(reader.read_str_u32(Endian::Big), Ok("hello"));
    assert!(reader.is_eof());

    let mut truncated = BinaryReader::new(&bytes[..length - 1]);
    assert!(truncated.read_str_u32(Endian::Big).is_err());
    assert_eq!(truncated.position(), 0);
}

proptest! {
    #[test]
    fn integer_and_float_bits_round_trip(
        unsigned in any::<u64>(),
        signed in any::<i64>(),
        float_bits in any::<u64>(),
        little in any::<bool>(),
    ) {
        let endian = if little { Endian::Little } else { Endian::Big };
        let mut bytes = [0_u8; 24];
        let mut writer = BinaryWriter::new(&mut bytes);
        writer.write_u64(unsigned, endian).unwrap();
        writer.write_i64(signed, endian).unwrap();
        writer.write_f64(f64::from_bits(float_bits), endian).unwrap();

        let mut reader = BinaryReader::new(&bytes);
        prop_assert_eq!(reader.read_u64(endian).unwrap(), unsigned);
        prop_assert_eq!(reader.read_i64(endian).unwrap(), signed);
        prop_assert_eq!(reader.read_f64(endian).unwrap().to_bits(), float_bits);
    }

    #[test]
    fn hex_round_trip(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        let encoded = encode_hex(&bytes);
        prop_assert_eq!(decode_hex(&encoded).unwrap(), bytes);
    }
}

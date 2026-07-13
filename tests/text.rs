//! Integration and property tests for UTF and text-format boundaries.

use std::borrow::Cow;

use gd::{
    TextError, decode_json_string, decode_percent_component, decode_utf16, encode_json_string,
    encode_percent_component, escape_xml, prefix_chars, split_escaped, trim_ascii_control,
    validate_utf8,
};
use proptest::prelude::*;

#[test]
fn validates_utf8_and_utf16_at_byte_boundaries() {
    assert_eq!(validate_utf8("Grüße 😀".as_bytes()).unwrap(), "Grüße 😀");
    assert!(validate_utf8(&[0xf0, 0x28, 0x8c, 0x28]).is_err());
    assert_eq!(decode_utf16(&[0x0041, 0xd83d, 0xde00]).unwrap(), "A😀");
    assert!(decode_utf16(&[0xd800]).is_err());
}

#[test]
fn json_handles_controls_and_astral_characters() {
    let input = "quote: \" nul: \0 emoji: 😀";
    let encoded = encode_json_string(input);
    assert_eq!(decode_json_string(&encoded).unwrap(), input);
    assert!(encoded.contains("\\u0000"));
    assert!(matches!(
        decode_json_string("not a literal"),
        Err(TextError::InvalidJson(_))
    ));
}

#[test]
fn percent_codec_matches_component_policy() {
    let encoded = encode_percent_component("hello world/é~!");
    assert_eq!(encoded, "hello%20world%2F%C3%A9%7E!");
    assert_eq!(
        decode_percent_component(&encoded).unwrap(),
        "hello world/é~!"
    );
    assert_eq!(decode_percent_component("a+b").unwrap(), "a b");
    assert!(matches!(
        decode_percent_component("bad%2"),
        Err(TextError::InvalidPercent { position: 3 })
    ));
    assert!(matches!(
        decode_percent_component("%FF"),
        Err(TextError::InvalidUtf8(_))
    ));
    assert!(matches!(
        decode_percent_component("plain"),
        Ok(Cow::Borrowed("plain"))
    ));
}

#[test]
fn xml_split_prefix_and_trim_have_explicit_edges() {
    assert_eq!(
        escape_xml("<tag a='x'>&\""),
        "&lt;tag a=&apos;x&apos;&gt;&amp;&quot;"
    );
    assert!(matches!(escape_xml("plain"), Cow::Borrowed("plain")));
    assert_eq!(
        split_escaped("one,,two,three,", ','),
        ["one,two", "three", ""]
    );
    assert_eq!(
        split_escaped("one😀😀two😀three", '😀'),
        ["one😀two", "three"]
    );
    assert_eq!(split_escaped("", ','), [""]);
    assert_eq!(prefix_chars("A😀éZ", 3), "A😀é");
    assert_eq!(prefix_chars("A😀éZ", usize::MAX), "A😀éZ");
    assert_eq!(trim_ascii_control("\0\t value \r\n"), "value");
    assert_eq!(
        trim_ascii_control("\u{2003}value\u{2003}"),
        "\u{2003}value\u{2003}"
    );
}

proptest! {
    #[test]
    fn json_round_trip(text in ".{0,1024}") {
        let literal = encode_json_string(&text);
        prop_assert_eq!(decode_json_string(&literal).unwrap(), text);
    }

    #[test]
    fn percent_round_trip(text in ".{0,1024}") {
        let encoded = encode_percent_component(&text);
        prop_assert_eq!(decode_percent_component(&encoded).unwrap(), text);
    }
}

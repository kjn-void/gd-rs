//! Caller-buffer construction, heap fallback, and table ownership boundaries.

use std::fmt::Write;

use gd::{ColumnSpec, DataType, Schema, StringTransport, Table, Value, ValueRef};

#[test]
fn every_small_capacity_preserves_utf8_and_exact_owned_growth() {
    for capacity in 0..=80 {
        let mut buffer = vec![0xAA; capacity];
        let mut text = StringTransport::new(&mut buffer);
        let mut expected = String::new();
        let mut borrowed = true;
        assert!(text.is_empty());
        assert_eq!(text.as_bytes_with_nul(), b"\0");
        for piece in ["", "a", "é", "😀", "0123456789!", " 0123456789!", "", "尾"] {
            text.append(piece);
            expected.push_str(piece);
            borrowed &= expected.len() < capacity;
            assert_eq!(text.as_str(), expected);
            assert_eq!(text.text(), expected);
            assert_eq!(text.as_bytes(), expected.as_bytes());
            assert_eq!(text.len(), expected.len());
            assert_eq!(text.is_borrowed(), borrowed);
            assert_eq!(text.as_bytes_with_nul().last(), Some(&0));
            if !borrowed {
                assert_eq!(text.capacity(), expected.len() + 1);
            }
        }
    }
}

#[test]
fn exact_fit_and_spill_preserve_the_caller_buffer() {
    let mut buffer = [0xAA; 6];
    {
        let mut text = StringTransport::new(&mut buffer);
        text.assign("12345");
        assert!(text.is_borrowed());
        text += "é";
        assert!(!text.is_borrowed());
        assert_eq!(text.as_str(), "12345é");
        assert_eq!(text.capacity(), 8);
        text.append("😀");
        assert_eq!(text.capacity(), 12);
        text.assign("x");
        assert!(!text.is_borrowed());
        assert_eq!(text.capacity(), 2);
        assert_eq!(text.as_bytes_with_nul(), b"x\0");
        text.assign("");
        assert!(text.is_empty());
        assert_eq!(text.capacity(), 1);
    }
    assert_eq!(&buffer, b"12345\0");
}

#[test]
fn owned_empty_formatting_and_interior_nuls() {
    let mut text = StringTransport::default();
    assert!(!text.is_borrowed());
    assert_eq!(text.capacity(), 0);
    assert_eq!(text.as_bytes_with_nul(), b"\0");
    write!(text, "café {}", 42).unwrap();
    assert_eq!(text.as_ref(), "café 42");
    assert_eq!(text.to_string(), "café 42");
    text.assign("a\0b");
    assert_eq!(text.as_str(), "a\0b");
    assert_eq!(text.as_bytes_with_nul(), b"a\0b\0");
}

#[test]
fn table_values_survive_builder_reuse_spill_and_drop() {
    let mut table = Table::new(Schema::new([ColumnSpec::new("label", DataType::String)]).unwrap());
    let mut buffer = [0_u8; 8];
    {
        let mut text = StringTransport::new(&mut buffer);
        text.assign("first");
        let view = ValueRef::from(&text);
        assert_eq!(view, ValueRef::String("first"));
        assert_eq!(view.as_str().unwrap().as_ptr(), text.as_bytes().as_ptr());
        table.push_row([Value::from(&text)]).unwrap();
        text.assign("second row spills to the heap");
        assert!(!text.is_borrowed());
        table.push_row([Value::from(&text)]).unwrap();
        text.assign("updated");
        table.set_named(0, "label", &text).unwrap();
        text.assign("consumed");
        table.push_row([text.into()]).unwrap();
    }
    buffer.fill(0xFF);
    assert_eq!(table.cell(0, 0), Ok(ValueRef::String("updated")));
    assert_eq!(
        table.cell(1, 0),
        Ok(ValueRef::String("second row spills to the heap"))
    );
    assert_eq!(table.cell(2, 0), Ok(ValueRef::String("consumed")));
}

#[test]
fn owned_repeated_growth_preserves_utf8() {
    let mut text = StringTransport::owned();
    for _ in 0..100 {
        text.append("é😀");
    }
    assert_eq!(text.len(), 600);
    assert_eq!(text.capacity(), 601);
    assert_eq!(Value::from(&text).as_str().unwrap(), "é😀".repeat(100));
}

//! Integration tests for owned and borrowed GD values.

use std::mem::size_of;

use gd::{DataType, Value, ValueError, ValueRef};
use proptest::prelude::*;

#[test]
fn owned_string_is_independent_of_its_source() {
    let mut source = String::from("alpha");
    let value = Value::from(source.clone());
    source.replace_range(..1, "A");

    assert_eq!(value.as_str(), Ok("alpha"));
}

#[test]
fn borrowed_string_uses_the_source_storage() {
    let source = String::from("alpha");
    let value = ValueRef::from(source.as_str());

    assert_eq!(value.as_str(), Ok("alpha"));
    assert_eq!(value.data_type(), DataType::String);
}

#[test]
fn primitive_widths_are_preserved() {
    assert_eq!(Value::from(-7_i8).data_type(), DataType::I8);
    assert_eq!(Value::from(-7_i64).data_type(), DataType::I64);
    assert_eq!(Value::from(7_u64).data_type(), DataType::U64);
    assert_eq!(Value::from(3.5_f64).data_type(), DataType::F64);
}

#[test]
fn numeric_conversions_check_range_and_type() {
    assert_eq!(Value::from(42_i32).to_f64(), Ok(42.0));
    assert_eq!(Value::from(42_u32).to_i64(), Ok(42));
    assert!(matches!(
        Value::from(u64::MAX).to_i64(),
        Err(ValueError::OutOfRange { .. })
    ));
    assert!(matches!(
        Value::from("42").to_i64(),
        Err(ValueError::TypeMismatch { .. })
    ));
}

#[test]
fn representation_sizes_are_tracked() {
    // These are diagnostic guardrails, not ABI promises. A change requires a
    // benchmark and retained-memory comparison.
    assert!(size_of::<Value>() <= 32);
    assert!(size_of::<ValueRef<'_>>() <= 32);
}

proptest! {
    #[test]
    fn borrowed_string_round_trips_to_owned(source in ".{0,4096}") {
        let borrowed = ValueRef::from(source.as_str());
        prop_assert_eq!(borrowed.to_owned(), Value::from(source));
    }

    #[test]
    fn borrowed_bytes_round_trip_to_owned(source in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let borrowed = ValueRef::from(source.as_slice());
        prop_assert_eq!(borrowed.to_owned(), Value::from(source));
    }
}

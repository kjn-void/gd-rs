//! Stable cached-key ordering across every storage family and null policy.

use gd::{ColumnSpec, DataType, NullOrder, Schema, SortDirection, Table, Value};

#[test]
fn every_storage_family_preserves_ties_and_independent_null_placement() {
    let mut cases = Vec::new();
    macro_rules! numeric {
        ($kind:ident, $small:expr, $large:expr) => {
            cases.push((
                ColumnSpec::new("key", DataType::$kind),
                Value::from($small),
                Value::from($large),
            ));
        };
    }
    numeric!(Bool, false, true);
    numeric!(I8, i8::MIN, i8::MAX);
    numeric!(I16, i16::MIN, i16::MAX);
    numeric!(I32, i32::MIN, i32::MAX);
    numeric!(I64, i64::MIN, i64::MAX);
    numeric!(U8, 0_u8, u8::MAX);
    numeric!(U16, 0_u16, u16::MAX);
    numeric!(U32, 0_u32, u32::MAX);
    numeric!(U64, 0_u64, u64::MAX);
    numeric!(F32, f32::NEG_INFINITY, f32::INFINITY);
    numeric!(F64, f64::NEG_INFINITY, f64::INFINITY);
    numeric!(Bytes, vec![0_u8], vec![255_u8]);
    numeric!(Uuid, uuid::Uuid::nil(), uuid::Uuid::from_u128(u128::MAX));
    for spec in [
        ColumnSpec::new("key", DataType::String),
        ColumnSpec::fixed_string("key", 64),
        ColumnSpec::interned_string("key"),
    ] {
        cases.push((
            spec,
            Value::from("a"),
            Value::from("a long string which lives on the heap"),
        ));
    }
    #[cfg(feature = "german-strings")]
    cases.push((
        ColumnSpec::german_string("key"),
        Value::from("a"),
        Value::from("a long string which lives on the heap"),
    ));

    for (spec, small, large) in cases {
        for nullable in [false, true] {
            let mut table = Table::new(Schema::new([spec.clone().nullable(nullable)]).unwrap());
            let input = if nullable {
                vec![
                    large.clone(),
                    Value::Null,
                    small.clone(),
                    large.clone(),
                    Value::Null,
                ]
            } else {
                vec![large.clone(), small.clone(), large.clone()]
            };
            for value in input {
                table.push_row([value]).unwrap();
            }
            for direction in [SortDirection::Ascending, SortDirection::Descending] {
                for nulls in [NullOrder::First, NullOrder::Last] {
                    let populated = match (nullable, direction) {
                        (false, SortDirection::Ascending) => vec![1, 0, 2],
                        (false, SortDirection::Descending) => vec![0, 2, 1],
                        (true, SortDirection::Ascending) => vec![2, 0, 3],
                        (true, SortDirection::Descending) => vec![0, 3, 2],
                    };
                    let expected: Vec<_> = if !nullable {
                        populated
                    } else if nulls == NullOrder::First {
                        [vec![1, 4], populated].concat()
                    } else {
                        [populated, vec![1, 4]].concat()
                    };
                    assert_eq!(
                        table.row_order(0, direction, nulls).unwrap().positions(),
                        expected,
                        "{spec:?}, {direction:?}, {nulls:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn null_only_and_empty_orders_preserve_insertion_order() {
    for count in [0, 1, 16] {
        for kind in [DataType::Null, DataType::I64] {
            let mut table =
                Table::new(Schema::new([ColumnSpec::new("key", kind).nullable(true)]).unwrap());
            for _ in 0..count {
                table.push_row([Value::Null]).unwrap();
            }
            for direction in [SortDirection::Ascending, SortDirection::Descending] {
                for nulls in [NullOrder::First, NullOrder::Last] {
                    assert_eq!(
                        table.row_order(0, direction, nulls).unwrap().positions(),
                        (0..count).collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn floating_cached_keys_preserve_nan_payloads_signed_zero_and_stability() {
    let values = [
        Some(f64::from_bits(0xfff8_0000_0000_0001)),
        Some(f64::NEG_INFINITY),
        Some(-0.0),
        None,
        Some(0.0),
        Some(f64::INFINITY),
        Some(f64::from_bits(0x7ff8_0000_0000_0001)),
        Some(-0.0),
        None,
    ];
    let mut table =
        Table::new(Schema::new([ColumnSpec::new("key", DataType::F64).nullable(true)]).unwrap());
    for value in values {
        table
            .push_row([value.map_or(Value::Null, Value::F64)])
            .unwrap();
    }
    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        for nulls in [NullOrder::First, NullOrder::Last] {
            let mut expected: Vec<_> = (0..values.len()).collect();
            expected.sort_by(|&a, &b| match (values[a], values[b]) {
                (Some(a), Some(b)) => {
                    if direction == SortDirection::Ascending {
                        a.total_cmp(&b)
                    } else {
                        b.total_cmp(&a)
                    }
                }
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => {
                    if nulls == NullOrder::First {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                }
                (Some(_), None) => {
                    if nulls == NullOrder::First {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Less
                    }
                }
            });
            assert_eq!(
                table.row_order(0, direction, nulls).unwrap().positions(),
                expected
            );
        }
    }
}

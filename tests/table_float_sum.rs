//! Tests for the lane-split `Column::sum_f64` and `Column::sum_f32` reductions.

use gd::{ColumnSliceError, ColumnSpec, DataType, Schema, Table, Value};
use proptest::prelude::*;

const LANES: usize = 16;

/// The documented summation order, written independently of the library.
fn model_f64(values: &[Option<f64>]) -> f64 {
    let split = values.len() - values.len() % LANES;
    let mut lanes = [0.0_f64; LANES];
    for (row, value) in values[..split].iter().enumerate() {
        if let Some(value) = value {
            lanes[row % LANES] += value;
        }
    }
    let mut total = 0.0;
    for lane in lanes {
        total += lane;
    }
    for value in values[split..].iter().flatten() {
        total += value;
    }
    total
}

fn model_f32(values: &[Option<f32>]) -> f32 {
    let split = values.len() - values.len() % LANES;
    let mut lanes = [0.0_f32; LANES];
    for (row, value) in values[..split].iter().enumerate() {
        if let Some(value) = value {
            lanes[row % LANES] += value;
        }
    }
    let mut total = 0.0;
    for lane in lanes {
        total += lane;
    }
    for value in values[split..].iter().flatten() {
        total += value;
    }
    total
}

fn f64_table(nullable: bool, values: &[Option<f64>]) -> Table {
    let schema = Schema::new([ColumnSpec::new("value", DataType::F64).nullable(nullable)]).unwrap();
    let mut table = Table::new(schema);
    for value in values {
        table
            .push_row([value.map_or(Value::Null, Value::F64)])
            .unwrap();
    }
    table
}

fn f32_table(nullable: bool, values: &[Option<f32>]) -> Table {
    let schema = Schema::new([ColumnSpec::new("value", DataType::F32).nullable(nullable)]).unwrap();
    let mut table = Table::new(schema);
    for value in values {
        table
            .push_row([value.map_or(Value::Null, Value::F32)])
            .unwrap();
    }
    table
}

fn sum_f64(table: &Table) -> f64 {
    table.column(0).unwrap().sum_f64().unwrap()
}

fn sum_f32(table: &Table) -> f32 {
    table.column(0).unwrap().sum_f32().unwrap()
}

#[test]
fn integer_valued_sums_are_exact_across_chunk_boundaries() {
    for len in [0_u16, 1, 15, 16, 17, 31, 32, 33, 1_000] {
        // Every partial sum stays below 2^24, so both orders are exact in f32 too.
        let expected = u32::from(len) * (u32::from(len) + 1) / 2;

        let values: Vec<Option<f64>> = (1..=len).map(|value| Some(f64::from(value))).collect();
        let expected_f64 = f64::from(expected);
        assert_eq!(
            sum_f64(&f64_table(false, &values)),
            expected_f64,
            "f64 len {len}"
        );
        assert_eq!(
            sum_f64(&f64_table(true, &values)),
            expected_f64,
            "nullable f64 len {len}"
        );

        let values: Vec<Option<f32>> = (1..=len).map(|value| Some(f32::from(value))).collect();
        let expected_f32: f32 = values.iter().flatten().sum();
        assert_eq!(f64::from(expected_f32), expected_f64);
        assert_eq!(
            sum_f32(&f32_table(false, &values)),
            expected_f32,
            "f32 len {len}"
        );
        assert_eq!(
            sum_f32(&f32_table(true, &values)),
            expected_f32,
            "nullable f32 len {len}"
        );
    }
}

#[test]
fn nulls_contribute_nothing() {
    let rows = 0_u16..40;
    let values: Vec<Option<f64>> = rows
        .clone()
        .map(|row| (row % 3 != 0).then_some(f64::from(row)))
        .collect();
    let expected: f64 = values.iter().flatten().sum();
    assert_eq!(sum_f64(&f64_table(true, &values)), expected);

    let values: Vec<Option<f32>> = rows
        .map(|row| (row % 3 != 0).then_some(f32::from(row)))
        .collect();
    let expected: f32 = values.iter().flatten().sum();
    assert_eq!(sum_f32(&f32_table(true, &values)), expected);
}

#[test]
fn empty_and_all_null_columns_sum_to_positive_zero() {
    for table in [f64_table(false, &[]), f64_table(true, &[None; 20])] {
        assert_eq!(sum_f64(&table).to_bits(), 0.0_f64.to_bits());
    }
    for table in [f32_table(false, &[]), f32_table(true, &[None; 20])] {
        assert_eq!(sum_f32(&table).to_bits(), 0.0_f32.to_bits());
    }
}

#[test]
fn nan_and_infinities_propagate() {
    let mut values = vec![Some(1.0); 40];
    values[5] = Some(f64::NAN);
    assert!(sum_f64(&f64_table(false, &values)).is_nan());

    values[5] = Some(f64::INFINITY);
    assert_eq!(sum_f64(&f64_table(false, &values)), f64::INFINITY);

    values[37] = Some(f64::NEG_INFINITY);
    assert!(sum_f64(&f64_table(true, &values)).is_nan());
}

#[test]
fn rounding_follows_the_documented_lane_order() {
    // 1e16 + 1.0 rounds back to 1e16. Sequentially, each 1.0 is lost; with lanes,
    // the 1.0s in lanes 1 to 15 accumulate before meeting the large value.
    let mut values = vec![Some(1.0_f64); 32];
    values[0] = Some(1e16);
    let lane_sum = sum_f64(&f64_table(false, &values));
    let sequential: f64 = values.iter().flatten().sum();
    assert_eq!(lane_sum, model_f64(&values));
    assert_eq!(lane_sum, 1e16 + 30.0);
    assert_eq!(sequential, 1e16);
}

#[test]
fn tombstoned_rows_are_included_like_typed_slices() {
    let mut table = f64_table(false, &[Some(1.0), Some(2.0), Some(4.0)]);
    assert!(table.tombstone_row(1).unwrap());
    assert_eq!(sum_f64(&table), 7.0);
}

#[test]
fn sums_check_the_column_type() {
    let schema = Schema::new([
        ColumnSpec::new("f32", DataType::F32),
        ColumnSpec::new("f64", DataType::F64).nullable(true),
        ColumnSpec::new("i64", DataType::I64),
    ])
    .unwrap();
    let table = Table::new(schema);
    assert_eq!(
        table.column(0).unwrap().sum_f64(),
        Err(ColumnSliceError::TypeMismatch {
            expected: DataType::F64,
            actual: DataType::F32,
        })
    );
    assert_eq!(
        table.column(1).unwrap().sum_f32(),
        Err(ColumnSliceError::TypeMismatch {
            expected: DataType::F32,
            actual: DataType::F64,
        })
    );
    assert_eq!(
        table.column(2).unwrap().sum_f64(),
        Err(ColumnSliceError::TypeMismatch {
            expected: DataType::F64,
            actual: DataType::I64,
        })
    );
}

proptest! {
    #[test]
    fn f64_sums_match_the_documented_order_bit_for_bit(
        values in prop::collection::vec(prop::option::weighted(0.8, -1e6_f64..1e6), 0..300)
    ) {
        let expected = model_f64(&values).to_bits();
        prop_assert_eq!(sum_f64(&f64_table(true, &values)).to_bits(), expected);
        if values.iter().all(Option::is_some) {
            prop_assert_eq!(sum_f64(&f64_table(false, &values)).to_bits(), expected);
        }
    }

    #[test]
    fn f32_sums_match_the_documented_order_bit_for_bit(
        values in prop::collection::vec(prop::option::weighted(0.8, -1e4_f32..1e4), 0..300)
    ) {
        let expected = model_f32(&values).to_bits();
        prop_assert_eq!(sum_f32(&f32_table(true, &values)).to_bits(), expected);
        if values.iter().all(Option::is_some) {
            prop_assert_eq!(sum_f32(&f32_table(false, &values)).to_bits(), expected);
        }
    }
}

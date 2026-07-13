//! Integration and property tests for ordered arguments and name indexes.

use gd::{Argument, Arguments, Value};
use proptest::prelude::*;

#[test]
fn preserves_order_names_and_primitive_types() {
    let mut values = Arguments::new();
    values.push_named("name", "Ada");
    values.push_named("age", 37_i32);
    values.push_positional(99_u64);

    assert_eq!(values.len(), 3);
    assert_eq!(
        values.get_named("name").unwrap().value().as_str(),
        Ok("Ada")
    );
    assert_eq!(values.get_named("age").unwrap().value(), &Value::I32(37));
    assert_eq!(values.get(2).unwrap().name(), None);
    assert_eq!(values.get(2).unwrap().value(), &Value::U64(99));
}

#[test]
fn preserves_duplicate_names_and_occurrences() {
    let values: Arguments = [10_i32, 20, 30]
        .into_iter()
        .map(|value| Argument::named("item", value))
        .collect();

    assert_eq!(
        values.get_nth_named("item", 0).unwrap().value(),
        &Value::I32(10)
    );
    assert_eq!(
        values.get_nth_named("item", 2).unwrap().value(),
        &Value::I32(30)
    );
    assert!(values.get_nth_named("item", 3).is_none());
}

#[test]
fn index_handles_named_unnamed_and_duplicate_entries() {
    let mut values = Arguments::new();
    values.push_named("item", 10_i32);
    values.push_positional(false);
    values.push_named("item", 20_i32);
    values.push_named("other", 30_i32);

    let index = values.index();
    assert_eq!(index.len(), 4);
    assert_eq!(index.distinct_name_count(), 2);
    assert_eq!(index.positions("item"), &[0, 2]);
    assert_eq!(
        index.get_nth_named("item", 1).unwrap().value(),
        &Value::I32(20)
    );
    assert!(index.get_named("missing").is_none());
}

#[test]
fn removal_compacts_without_changing_relative_order() {
    let mut values = Arguments::new();
    values.push_named("first", 1_i32);
    values.push_named("middle", 2_i32);
    values.push_named("last", 3_i32);

    let removed = values.remove_named("middle").unwrap();
    assert_eq!(removed.value(), &Value::I32(2));
    assert_eq!(values.len(), 2);
    assert_eq!(values.get(0).unwrap().name(), Some("first"));
    assert_eq!(values.get(1).unwrap().name(), Some("last"));
}

proptest! {
    #[test]
    fn hash_index_matches_linear_lookup(
        entries in prop::collection::vec(("[a-z]{0,8}", any::<i32>()), 0..128),
        query in "[a-z]{0,8}",
    ) {
        let values: Arguments = entries
            .into_iter()
            .map(|(name, value)| Argument::named(name, value))
            .collect();
        let index = values.index();

        prop_assert_eq!(
            index.get_named(&query).map(Argument::value),
            values.get_named(&query).map(Argument::value),
        );
        let expected: Vec<_> = values
            .iter()
            .enumerate()
            .filter_map(|(position, argument)| (argument.name() == Some(query.as_str())).then_some(position))
            .collect();
        prop_assert_eq!(index.positions(&query), expected);
    }
}

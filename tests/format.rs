//! Integration tests for argument and table interchange formats.

use gd::{
    Arguments, ColumnSpec, DataType, FormatError, NullOrder, Schema, SortDirection, Table, Value,
    arguments_to_json, arguments_to_uri, row_order_to_json, table_to_csv, table_to_json,
};

fn table() -> Table {
    let schema = Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("name", DataType::String),
        ColumnSpec::new("note", DataType::String).nullable(true),
    ])
    .unwrap();
    let mut table = Table::new(schema);
    table
        .push_row([Value::U64(2), Value::from("A, B"), Value::Null])
        .unwrap();
    table
        .push_row([
            Value::U64(1),
            Value::from("quote \""),
            Value::from("line\n2"),
        ])
        .unwrap();
    table
}

#[test]
fn arguments_have_explicit_json_and_uri_representability() {
    let mut arguments = Arguments::new();
    arguments.push_named("query name", "café & tea");
    arguments.push_named("limit", 10_u32);
    let json: serde_json::Value =
        serde_json::from_str(&arguments_to_json(&arguments).unwrap()).unwrap();
    assert_eq!(json["query name"], "café & tea");
    assert_eq!(json["limit"], 10);
    assert_eq!(
        arguments_to_uri(&arguments).unwrap(),
        "query%20name=caf%C3%A9%20%26%20tea&limit=10"
    );

    arguments.push_named("limit", 20_u32);
    assert!(matches!(
        arguments_to_json(&arguments),
        Err(FormatError::DuplicateArgumentName(_))
    ));
    assert!(arguments_to_uri(&arguments).unwrap().ends_with("&limit=20"));

    arguments.push_positional(true);
    assert!(matches!(
        arguments_to_uri(&arguments),
        Err(FormatError::UnnamedArgument { position: 3 })
    ));
}

#[test]
fn table_json_and_csv_escape_values() {
    let table = table();
    let json: serde_json::Value = serde_json::from_str(&table_to_json(&table).unwrap()).unwrap();
    assert_eq!(json[0]["id"], 2);
    assert_eq!(json[0]["name"], "A, B");
    assert!(json[0]["note"].is_null());
    assert_eq!(json[1]["note"], "line\n2");

    assert_eq!(
        table_to_csv(&table, true).unwrap(),
        "id,name,note\n2,\"A, B\",\n1,\"quote \"\"\",\"line\n2\"\n"
    );
}

#[test]
fn ordered_json_uses_the_borrowed_permutation() {
    let table = table();
    let order = table
        .row_order_named("id", SortDirection::Ascending, NullOrder::Last)
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&row_order_to_json(&order).unwrap()).unwrap();
    assert_eq!(json[0]["id"], 1);
    assert_eq!(json[1]["id"], 2);
}

#[test]
fn json_rejects_non_finite_numbers() {
    let schema = Schema::new([ColumnSpec::new("value", DataType::F64)]).unwrap();
    let mut table = Table::new(schema);
    table.push_row([Value::F64(f64::NAN)]).unwrap();
    assert!(matches!(
        table_to_json(&table),
        Err(FormatError::NonFiniteFloat)
    ));
}

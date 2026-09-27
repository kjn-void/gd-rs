//! Integration tests for argument and table interchange formats.

use gd::{
    Arguments, ColumnConverter, ColumnSpec, DataType, FormatError, ImportError, NullOrder, Schema,
    SortDirection, Table, TableError, UnknownFields, Value, ValueRef, arguments_to_json,
    arguments_to_uri, row_order_to_json, table_from_csv, table_from_json, table_to_csv,
    table_to_json,
};
use proptest::prelude::*;
use std::sync::Arc;
use uuid::Uuid;

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
fn interchange_formats_exclude_tombstoned_rows() {
    let mut table = table();
    table.tombstone_row(0).unwrap();

    let json: serde_json::Value = serde_json::from_str(&table_to_json(&table).unwrap()).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
    assert_eq!(json[0]["id"], 1);

    assert_eq!(
        table_to_csv(&table, true).unwrap(),
        "id,name,note\n1,\"quote \"\"\",\"line\n2\"\n"
    );

    let order = table
        .row_order_named("id", SortDirection::Ascending, NullOrder::Last)
        .unwrap();
    assert_eq!(order.positions(), &[1, 0]);
    let ordered: serde_json::Value =
        serde_json::from_str(&row_order_to_json(&order).unwrap()).unwrap();
    assert_eq!(ordered.as_array().unwrap().len(), 1);
    assert_eq!(ordered[0]["id"], 1);

    table.restore_row(0).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&table_to_json(&table).unwrap())
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
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

fn every_type_schema() -> Schema {
    Schema::new([
        ColumnSpec::new("nothing", DataType::Null),
        ColumnSpec::new("flag", DataType::Bool),
        ColumnSpec::new("i8", DataType::I8),
        ColumnSpec::new("i16", DataType::I16),
        ColumnSpec::new("i32", DataType::I32),
        ColumnSpec::new("i64", DataType::I64),
        ColumnSpec::new("u8", DataType::U8),
        ColumnSpec::new("u16", DataType::U16),
        ColumnSpec::new("u32", DataType::U32),
        ColumnSpec::new("u64", DataType::U64),
        ColumnSpec::new("f32", DataType::F32),
        ColumnSpec::new("f64", DataType::F64),
        ColumnSpec::new("text", DataType::String),
        ColumnSpec::new("bytes", DataType::Bytes),
        ColumnSpec::new("uuid", DataType::Uuid),
        ColumnSpec::new("maybe", DataType::I32).nullable(true),
        ColumnSpec::new("note", DataType::String).nullable(true),
    ])
    .unwrap()
}

fn every_type_table() -> Table {
    let mut table = Table::new(every_type_schema());
    table
        .push_row([
            Value::Null,
            Value::Bool(true),
            Value::I8(i8::MIN),
            Value::I16(i16::MIN),
            Value::I32(i32::MIN),
            Value::I64(i64::MIN),
            Value::U8(u8::MAX),
            Value::U16(u16::MAX),
            Value::U32(u32::MAX),
            Value::U64(u64::MAX),
            Value::F32(0.1),
            Value::F64(0.1 + 0.2),
            Value::from("quote \", comma, and line\nbreak é"),
            Value::from(vec![0x00, 0xab, 0xff]),
            Value::Uuid(Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef)),
            Value::I32(-7),
            Value::from("present"),
        ])
        .unwrap();
    table
        .push_row([
            Value::Null,
            Value::Bool(false),
            Value::I8(i8::MAX),
            Value::I16(i16::MAX),
            Value::I32(i32::MAX),
            Value::I64(i64::MAX),
            Value::U8(0),
            Value::U16(0),
            Value::U32(0),
            Value::U64(0),
            Value::F32(f32::MIN_POSITIVE),
            Value::F64(f64::MAX),
            Value::from(""),
            Value::from(Vec::new()),
            Value::Uuid(Uuid::nil()),
            Value::Null,
            Value::Null,
        ])
        .unwrap();
    table
}

fn assert_same_cells(actual: &Table, expected: &Table) {
    assert_eq!(actual.row_count(), expected.row_count());
    for row in 0..expected.row_count() {
        assert_eq!(actual.row(row).unwrap().iter().collect::<Vec<_>>(), {
            expected.row(row).unwrap().iter().collect::<Vec<_>>()
        });
    }
}

#[test]
fn json_round_trips_every_column_type_exactly() {
    let table = every_type_table();
    let json = table_to_json(&table).unwrap();
    let imported = table_from_json(table.schema_arc(), &json).unwrap();
    assert_same_cells(&imported, &table);
    assert_eq!(table_to_json(&imported).unwrap(), json);
}

#[test]
fn csv_round_trips_every_column_type_with_and_without_headers() {
    let table = every_type_table();
    for headers in [true, false] {
        let csv = table_to_csv(&table, headers).unwrap();
        let imported = table_from_csv(table.schema_arc(), &csv, headers).unwrap();
        // A required string keeps its empty value; a nullable one would read null.
        assert_same_cells(&imported, &table);
        assert_eq!(table_to_csv(&imported, headers).unwrap(), csv);
    }
}

#[test]
fn csv_round_trips_non_finite_floats_and_single_column_nulls() {
    let schema = Schema::new([ColumnSpec::new("value", DataType::F64).nullable(true)]).unwrap();
    let mut table = Table::new(schema);
    for value in [
        Value::F64(f64::NAN),
        Value::F64(f64::INFINITY),
        Value::F64(f64::NEG_INFINITY),
        Value::Null,
    ] {
        table.push_row([value]).unwrap();
    }
    let csv = table_to_csv(&table, false).unwrap();
    let imported = table_from_csv(table.schema_arc(), &csv, false).unwrap();
    assert_eq!(imported.row_count(), 4);
    assert!(matches!(imported.cell(0, 0), Ok(ValueRef::F64(value)) if value.is_nan()));
    assert_eq!(imported.cell(1, 0), Ok(ValueRef::F64(f64::INFINITY)));
    assert_eq!(imported.cell(2, 0), Ok(ValueRef::F64(f64::NEG_INFINITY)));
    assert_eq!(imported.cell(3, 0), Ok(ValueRef::Null));
}

#[test]
fn csv_reads_empty_nullable_strings_as_null() {
    let imported = table_from_csv(
        Schema::new([
            ColumnSpec::new("required", DataType::String),
            ColumnSpec::new("optional", DataType::String).nullable(true),
        ])
        .unwrap(),
        "required,optional\n,\n",
        true,
    )
    .unwrap();
    assert_eq!(imported.cell(0, 0), Ok(ValueRef::String("")));
    assert_eq!(imported.cell(0, 1), Ok(ValueRef::Null));
}

#[test]
fn imports_resolve_aliases_order_and_missing_nullable_fields() {
    let schema = Arc::new(
        Schema::new([
            ColumnSpec::new("id", DataType::U64),
            ColumnSpec::new("name", DataType::String).with_alias("display_name"),
            ColumnSpec::new("note", DataType::String).nullable(true),
        ])
        .unwrap(),
    );
    let json = table_from_json(
        Arc::clone(&schema),
        r#"[{"display_name":"Ada","id":1},{"note":null,"name":"Grace","id":2}]"#,
    )
    .unwrap();
    let csv = table_from_csv(
        Arc::clone(&schema),
        "display_name,id\nAda,1\nGrace,2\n",
        true,
    )
    .unwrap();
    for table in [&json, &csv] {
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell_named(0, "name"), Ok(ValueRef::String("Ada")));
        assert_eq!(table.cell(1, 0), Ok(ValueRef::U64(2)));
        assert_eq!(table.cell(0, 2), Ok(ValueRef::Null));
        assert_eq!(table.properties().len(), 0);
        assert_eq!(table.tombstone_count(), 0);
    }

    assert!(matches!(
        table_from_json(Arc::clone(&schema), r#"[{"name":"Ada"}]"#),
        Err(ImportError::Row {
            row: 0,
            source: TableError::NullNotAllowed { column: 0 }
        })
    ));
    assert!(matches!(
        table_from_csv(schema, "name\nAda\n", true),
        Err(ImportError::Row {
            row: 0,
            source: TableError::NullNotAllowed { column: 0 }
        })
    ));
}

#[test]
fn unknown_fields_follow_the_schema_policy() {
    let strict = people();
    assert!(matches!(
        table_from_json(strict.clone(), r#"[{"id":1,"name":"Ada","extra":true}]"#),
        Err(ImportError::UnknownField { row: Some(0), name }) if name == "extra"
    ));
    assert!(matches!(
        table_from_csv(strict, "id,name,extra\n1,Ada,x\n", true),
        Err(ImportError::UnknownField { row: None, name }) if name == "extra"
    ));

    let open = people().with_unknown_fields(UnknownFields::Store);
    let json = table_from_json(
        open.clone(),
        r#"[{"id":1,"name":"Ada","visits":3,"big":18446744073709551615,"ratio":0.5,"tag":"x","gone":null}]"#,
    )
    .unwrap();
    assert_eq!(json.cell_named(0, "visits"), Ok(ValueRef::I64(3)));
    assert_eq!(json.cell_named(0, "big"), Ok(ValueRef::U64(u64::MAX)));
    assert_eq!(json.cell_named(0, "ratio"), Ok(ValueRef::F64(0.5)));
    assert_eq!(json.cell_named(0, "tag"), Ok(ValueRef::String("x")));
    assert_eq!(json.cell_named(0, "gone"), Ok(ValueRef::Null));

    let csv = table_from_csv(open, "id,name,tag\n1,Ada,x\n2,Grace,\n", true).unwrap();
    assert_eq!(csv.cell_named(0, "tag"), Ok(ValueRef::String("x")));
    assert_eq!(
        csv.cell_named(1, "tag"),
        Err(TableError::ColumnNotFound("tag".into()))
    );
}

#[test]
fn repeated_fields_are_rejected() {
    let schema = Arc::new(people().with_unknown_fields(UnknownFields::Store));
    assert!(matches!(
        table_from_json(Arc::clone(&schema), r#"[{"id":1,"name":"Ada","display_name":"Ada"}]"#),
        Err(ImportError::DuplicateField { row: Some(0), name }) if name == "display_name"
    ));
    assert!(matches!(
        table_from_json(Arc::clone(&schema), r#"[{"id":1,"name":"Ada","x":1,"x":2}]"#),
        Err(ImportError::DuplicateField { row: Some(0), name }) if name == "x"
    ));
    assert!(matches!(
        table_from_csv(Arc::clone(&schema), "id,name,id\n1,Ada,1\n", true),
        Err(ImportError::DuplicateField { row: None, name }) if name == "id"
    ));
    assert!(matches!(
        table_from_csv(schema, "id,name,x,x\n1,Ada,a,b\n", true),
        Err(ImportError::DuplicateField { row: None, name }) if name == "x"
    ));
}

#[test]
fn undecodable_fields_reach_schema_validation_and_converters() {
    let schema = Schema::new([ColumnSpec::new("small", DataType::U8)]).unwrap();
    assert!(matches!(
        table_from_json(schema.clone(), r#"[{"small":300}]"#),
        Err(ImportError::Row {
            row: 0,
            source: TableError::TypeMismatch {
                column: 0,
                expected: DataType::U8,
                actual: DataType::I64
            }
        })
    ));
    assert!(matches!(
        table_from_json(schema.clone(), r#"[{"small":1},{"small":"2"}]"#),
        Err(ImportError::Row {
            row: 1,
            source: TableError::TypeMismatch {
                actual: DataType::String,
                ..
            }
        })
    ));
    assert!(matches!(
        table_from_csv(schema, "300\n", false),
        Err(ImportError::Row {
            row: 0,
            source: TableError::TypeMismatch {
                actual: DataType::String,
                ..
            }
        })
    ));

    let converted = Schema::new([ColumnSpec::new("count", DataType::U32).with_converter(
        ColumnConverter::new("parse-u32", |value| match value {
            ValueRef::String(text) => text
                .trim()
                .parse()
                .map(Value::U32)
                .map_err(|_| "not a u32".into()),
            _ => Err("expected text".into()),
        }),
    )])
    .unwrap();
    let json = table_from_json(converted.clone(), r#"[{"count":" 42 "}]"#).unwrap();
    let csv = table_from_csv(converted, "count\n\" 7 \"\n", true).unwrap();
    assert_eq!(json.cell(0, 0), Ok(ValueRef::U32(42)));
    assert_eq!(csv.cell(0, 0), Ok(ValueRef::U32(7)));
}

#[test]
fn malformed_inputs_return_errors() {
    let schema = Arc::new(people());
    for json in [
        "",
        "{}",
        "[1]",
        r#"[{"id":1,"name":"Ada"}] trailing"#,
        r#"[{"id":1,"name":"Ada"}"#,
    ] {
        assert!(
            matches!(
                table_from_json(Arc::clone(&schema), json),
                Err(ImportError::Json(_))
            ),
            "{json}"
        );
    }
    assert!(matches!(
        table_from_json(Arc::clone(&schema), r#"[{"id":1,"name":["Ada"]}]"#),
        Err(ImportError::NestedJsonValue { row: 0, name }) if name == "name"
    ));
    assert!(matches!(
        table_from_json(Arc::clone(&schema), r#"[{"id":1e400,"name":"Ada"}]"#),
        Err(ImportError::NumberOutOfRange { row: 0, name }) if name == "id"
    ));
    assert!(matches!(
        table_from_csv(Arc::clone(&schema), "1,Ada,extra\n", false),
        Err(ImportError::Row {
            row: 0,
            source: TableError::RowWidth {
                expected: 2,
                actual: 3
            }
        })
    ));
    assert!(matches!(
        table_from_csv(Arc::clone(&schema), "id,name\n1,Ada\n2\n", true),
        Err(ImportError::Csv(_))
    ));

    assert!(
        table_from_json(Arc::clone(&schema), "[]")
            .unwrap()
            .is_empty()
    );
    assert!(
        table_from_csv(Arc::clone(&schema), "", true)
            .unwrap()
            .is_empty()
    );
    assert!(
        table_from_csv(schema, "id,name\n", true)
            .unwrap()
            .is_empty()
    );
}

fn people() -> Schema {
    Schema::new([
        ColumnSpec::new("id", DataType::U64),
        ColumnSpec::new("name", DataType::String).with_alias("display_name"),
    ])
    .unwrap()
}

proptest! {
    #[test]
    fn json_and_csv_round_trip_generated_rows(
        rows in prop::collection::vec(
            (any::<i64>(), any::<u64>(), any::<f32>(), any::<f64>(), any::<String>(), any::<Option<i16>>()),
            0..64,
        )
    ) {
        let schema = Arc::new(
            Schema::new([
                ColumnSpec::new("i", DataType::I64),
                ColumnSpec::new("u", DataType::U64),
                ColumnSpec::new("f", DataType::F32),
                ColumnSpec::new("d", DataType::F64),
                ColumnSpec::new("s", DataType::String),
                ColumnSpec::new("n", DataType::I16).nullable(true),
            ])
            .unwrap(),
        );
        let mut table = Table::new(Arc::clone(&schema));
        for (i, u, f, d, s, n) in rows {
            // JSON cannot carry non-finite floats, so both formats use finite ones.
            let f = if f.is_finite() { f } else { 0.0 };
            let d = if d.is_finite() { d } else { 0.0 };
            table
                .push_row([
                    Value::I64(i),
                    Value::U64(u),
                    Value::F32(f),
                    Value::F64(d),
                    Value::from(s),
                    n.map_or(Value::Null, Value::I16),
                ])
                .unwrap();
        }

        let json = table_from_json(Arc::clone(&schema), &table_to_json(&table).unwrap()).unwrap();
        let csv = table_from_csv(schema, &table_to_csv(&table, true).unwrap(), true).unwrap();
        for imported in [&json, &csv] {
            prop_assert_eq!(imported.row_count(), table.row_count());
            for row in 0..table.row_count() {
                prop_assert_eq!(
                    imported.row(row).unwrap().iter().collect::<Vec<_>>(),
                    table.row(row).unwrap().iter().collect::<Vec<_>>()
                );
            }
        }
    }
}

//! In-memory `SQLite` integration and property tests.

#![cfg(feature = "sqlite")]

use gd::{
    Argument, Arguments, ColumnSpec, DataType, Schema, SqliteDatabase, SqliteError, Value, ValueRef,
};
use proptest::prelude::*;
use uuid::Uuid;

fn positional(values: impl IntoIterator<Item = Value>) -> Arguments {
    values
        .into_iter()
        .map(Argument::positional)
        .collect::<Arguments>()
}

#[test]
fn positional_values_round_trip_through_an_inferred_table() {
    let database = SqliteDatabase::open_in_memory().unwrap();
    database
        .execute_batch(
            "CREATE TABLE item(id INTEGER, name TEXT, payload BLOB, score REAL, note TEXT)",
        )
        .unwrap();
    database
        .execute(
            "INSERT INTO item VALUES (?1, ?2, ?3, ?4, ?5)",
            &positional([
                Value::I64(7),
                Value::from("café"),
                Value::from(vec![0_u8, 1, 255]),
                Value::F64(1.5),
                Value::Null,
            ]),
        )
        .unwrap();

    let table = database
        .query_table(
            "SELECT id, name, payload, score, note FROM item",
            &Arguments::new(),
        )
        .unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.schema().column(0).unwrap().data_type(), DataType::I64);
    assert_eq!(
        table.schema().column(4).unwrap().data_type(),
        DataType::Null
    );
    let row = table.row(0).unwrap();
    assert_eq!(row.get_named("id"), Some(ValueRef::I64(7)));
    assert_eq!(row.get_named("name"), Some(ValueRef::String("café")));
    assert_eq!(
        row.get_named("payload"),
        Some(ValueRef::Bytes(&[0, 1, 255]))
    );
    assert_eq!(row.get_named("score"), Some(ValueRef::F64(1.5)));
    assert_eq!(row.get_named("note"), Some(ValueRef::Null));
}

#[test]
fn named_parameter_policy_is_lossless_and_explicit() {
    let database = SqliteDatabase::open_in_memory().unwrap();
    database
        .execute_batch("CREATE TABLE item(id INTEGER, name TEXT)")
        .unwrap();
    let mut arguments = Arguments::new();
    arguments.push_named("id", 3_i64);
    arguments.push_named("name", "alpha");
    assert_eq!(
        database
            .execute("INSERT INTO item VALUES (:id, @name)", &arguments)
            .unwrap(),
        1
    );

    let mut duplicate = arguments.clone();
    duplicate.push_named("id", 4_i64);
    assert!(matches!(
        database.execute("INSERT INTO item VALUES (:id, :name)", &duplicate),
        Err(SqliteError::DuplicateParameter(_))
    ));

    let mut missing = Arguments::new();
    missing.push_named("id", 1_i64);
    assert!(matches!(
        database.execute("INSERT INTO item VALUES (:id, :name)", &missing),
        Err(SqliteError::MissingParameter(_))
    ));

    let mut extra = arguments;
    extra.push_named("unused", 1_i64);
    assert!(matches!(
        database.execute("INSERT INTO item VALUES (:id, :name)", &extra),
        Err(SqliteError::UnknownParameter(_))
    ));
}

#[test]
fn parameter_modes_and_integer_range_are_checked() {
    let database = SqliteDatabase::open_in_memory().unwrap();
    let mut mixed = Arguments::new();
    mixed.push_named("value", 1_i64);
    mixed.push_positional(2_i64);
    assert!(matches!(
        database.query_table("SELECT :value, ?2", &mixed),
        Err(SqliteError::MixedStatementParameters | SqliteError::MixedArguments)
    ));

    assert!(matches!(
        database.query_table("SELECT ?1", &positional([Value::U64(u64::MAX)])),
        Err(SqliteError::UnsignedOutOfRange { value: u64::MAX })
    ));
    assert!(matches!(
        database.query_table("SELECT ?1, ?2", &positional([Value::I64(1)])),
        Err(SqliteError::ParameterCount {
            expected: 2,
            actual: 1
        })
    ));
}

#[test]
fn explicit_schema_converts_bool_unsigned_float_and_uuid() {
    let database = SqliteDatabase::open_in_memory().unwrap();
    let uuid = Uuid::parse_str("12345678-1234-5678-9abc-def012345678").unwrap();
    let schema = Schema::new([
        ColumnSpec::new("enabled", DataType::Bool),
        ColumnSpec::new("count", DataType::U32),
        ColumnSpec::new("ratio", DataType::F32),
        ColumnSpec::new("id", DataType::Uuid),
    ])
    .unwrap();
    let table = database
        .query_table_with_schema(
            "SELECT ?1, ?2, ?3, ?4",
            &positional([
                Value::Bool(true),
                Value::U32(42),
                Value::F32(1.25),
                Value::Uuid(uuid),
            ]),
            schema,
        )
        .unwrap();
    let row = table.row(0).unwrap();
    assert_eq!(row.get(0), Some(ValueRef::Bool(true)));
    assert_eq!(row.get(1), Some(ValueRef::U32(42)));
    assert_eq!(row.get(2), Some(ValueRef::F32(1.25)));
    assert_eq!(row.get(3), Some(ValueRef::Uuid(uuid)));
}

#[test]
fn explicit_schema_rejects_range_type_and_nullability_errors() {
    let database = SqliteDatabase::open_in_memory().unwrap();
    let u8_schema = Schema::new([ColumnSpec::new("small", DataType::U8)]).unwrap();
    assert!(matches!(
        database.query_table_with_schema("SELECT 256", &Arguments::new(), u8_schema),
        Err(SqliteError::ValueOutOfRange { .. })
    ));

    let text_schema = Schema::new([ColumnSpec::new("text", DataType::String)]).unwrap();
    assert!(matches!(
        database.query_table_with_schema("SELECT x'00'", &Arguments::new(), text_schema),
        Err(SqliteError::ColumnType { .. })
    ));

    let non_null = Schema::new([ColumnSpec::new("required", DataType::I64)]).unwrap();
    assert!(matches!(
        database.query_table_with_schema("SELECT NULL", &Arguments::new(), non_null),
        Err(SqliteError::Table(_))
    ));
}

#[test]
fn inferred_schema_rejects_dynamic_mixed_storage_classes() {
    let database = SqliteDatabase::open_in_memory().unwrap();
    assert!(matches!(
        database.query_table(
            "SELECT value FROM (SELECT 1 AS value UNION ALL SELECT 'one')",
            &Arguments::new()
        ),
        Err(SqliteError::MixedColumnType { .. })
    ));
}

#[test]
fn caller_can_use_native_transactions() {
    let mut database = SqliteDatabase::open_in_memory().unwrap();
    database
        .execute_batch("CREATE TABLE item(value INTEGER)")
        .unwrap();
    {
        let transaction = database.connection_mut().transaction().unwrap();
        transaction
            .execute("INSERT INTO item VALUES (1)", ())
            .unwrap();
        transaction.rollback().unwrap();
    }
    let table = database
        .query_table("SELECT value FROM item", &Arguments::new())
        .unwrap();
    assert!(table.is_empty());
}

proptest! {
    #[test]
    fn integer_text_and_blob_round_trip(
        integer in any::<i64>(),
        text in ".{0,256}",
        bytes in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let database = SqliteDatabase::open_in_memory().unwrap();
        let table = database.query_table(
            "SELECT ?1 AS integer, ?2 AS text, ?3 AS bytes",
            &positional([
                Value::I64(integer),
                Value::from(text.clone()),
                Value::from(bytes.clone()),
            ]),
        ).unwrap();
        let row = table.row(0).unwrap();
        prop_assert_eq!(row.get(0), Some(ValueRef::I64(integer)));
        prop_assert_eq!(row.get(1), Some(ValueRef::String(&text)));
        prop_assert_eq!(row.get(2), Some(ValueRef::Bytes(&bytes)));
    }
}

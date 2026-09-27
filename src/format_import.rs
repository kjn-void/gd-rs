//! Reconstruction of tables from the JSON and CSV representations produced by
//! [`crate::table_to_json`] and [`crate::table_to_csv`].
//!
//! Neither format carries type information, so the reader supplies the schema.
//! Each field is decoded according to its column type. A field that does not
//! decode as that type is handed to ordinary schema validation as its natural
//! dynamic value, where a column converter may accept it or a
//! [`TableError::TypeMismatch`] reports it.

use std::fmt;
use std::sync::Arc;

use compact_str::CompactString;
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;
use thiserror::Error;
use uuid::Uuid;

use crate::{ColumnSpec, DataType, Schema, Table, TableError, UnknownFields, Value, decode_hex};

/// A failure while reconstructing a table from JSON or CSV.
///
/// Row numbers count data rows from zero: the JSON array position, or the CSV
/// record position after any header record. No table is returned on failure.
#[derive(Debug, Error)]
pub enum ImportError {
    /// The input is not valid JSON or does not have the table shape.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The input is not valid CSV, or its records have different lengths.
    #[error(transparent)]
    Csv(#[from] csv::Error),
    /// A decoded row was rejected by the schema.
    #[error("row {row}: {source}")]
    Row {
        /// Data row position.
        row: usize,
        /// The schema validation failure.
        #[source]
        source: TableError,
    },
    /// A field name is neither a column name nor an alias, and the schema rejects
    /// unknown fields.
    #[error("{}unknown field {name}", Location(*row))]
    UnknownField {
        /// Data row position, or `None` for a CSV header.
        row: Option<usize>,
        /// The unmatched field name.
        name: CompactString,
    },
    /// Two fields in one JSON object or CSV header resolve to the same column or
    /// row-local name, for example a column's name and its alias.
    #[error("{}field {name} is given more than once", Location(*row))]
    DuplicateField {
        /// Data row position, or `None` for a CSV header.
        row: Option<usize>,
        /// The repeated field name as written in the input.
        name: CompactString,
    },
    /// A JSON field holds an array or object, which no column or value can store.
    #[error("row {row}: field {name} holds a JSON array or object")]
    NestedJsonValue {
        /// Data row position.
        row: usize,
        /// The field name.
        name: CompactString,
    },
    /// A JSON number fits neither the column type nor any `i64`, `u64`, or finite
    /// `f64`.
    #[error("row {row}: field {name} holds an unrepresentable number")]
    NumberOutOfRange {
        /// Data row position.
        row: usize,
        /// The field name.
        name: CompactString,
    },
}

struct Location(Option<usize>);

impl fmt::Display for Location {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(row) => write!(formatter, "row {row}: "),
            None => formatter.write_str("header: "),
        }
    }
}

/// Reconstructs a table from a JSON array of row objects.
///
/// This reads the output of [`crate::table_to_json`] and
/// [`crate::row_order_to_json`]. Object keys may be primary column names or
/// aliases, in any order. A missing key reads as null, so it is accepted only by
/// a nullable column. Keys that match no column are stored as row-local extras
/// when the schema uses [`UnknownFields::Store`] and rejected otherwise.
///
/// Numbers are parsed from their JSON text into the column type, so integers keep
/// full 64-bit precision and floats round-trip exactly. Byte columns read
/// lowercase or uppercase hex strings and UUID columns read UUID text. The result
/// contains no tombstones and no table properties.
///
/// # Errors
///
/// Returns [`ImportError::Json`] for malformed JSON or a value that is not an
/// array of objects, [`ImportError::Row`] when a row fails schema validation,
/// and the field-level [`ImportError`] variants for unknown, repeated, nested,
/// or unrepresentable fields.
///
/// # Examples
///
/// ```
/// use gd::{ColumnSpec, DataType, Schema, ValueRef, table_from_json};
///
/// let schema = Schema::new([
///     ColumnSpec::new("id", DataType::U64),
///     ColumnSpec::new("note", DataType::String).nullable(true),
/// ])
/// .unwrap();
/// let table = table_from_json(schema, r#"[{"id":18446744073709551615},{"note":"x","id":2}]"#)
///     .unwrap();
/// assert_eq!(table.cell(0, 0), Ok(ValueRef::U64(u64::MAX)));
/// assert_eq!(table.cell(0, 1), Ok(ValueRef::Null));
/// assert_eq!(table.cell(1, 1), Ok(ValueRef::String("x")));
/// ```
pub fn table_from_json(schema: impl Into<Arc<Schema>>, json: &str) -> Result<Table, ImportError> {
    let mut table = Table::new(schema);
    let mut failure = None;
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let parsed = RowsSeed {
        table: &mut table,
        failure: &mut failure,
    }
    .deserialize(&mut deserializer);
    if let Some(failure) = failure {
        return Err(failure);
    }
    parsed?;
    deserializer.end()?;
    Ok(table)
}

/// Reconstructs a table from RFC 4180-style CSV.
///
/// This reads the output of [`crate::table_to_csv`]. With `headers`, the first
/// record names the columns by primary name or alias, in any order; a column
/// absent from the header reads as null, and a header that matches no column is
/// stored as a row-local string extra under [`UnknownFields::Store`] or rejected
/// otherwise. Without `headers`, every record must have one field per schema
/// column, in schema order.
///
/// CSV cannot distinguish null from an empty string or empty byte sequence. An
/// empty field reads as null, except in a required string or byte column, where it
/// reads as the empty value.
/// Empty extra fields are not stored. Booleans read `true` and `false`, byte
/// columns read hex, UUID columns read UUID text, and floats also accept the
/// `NaN` and `inf` spellings that [`crate::table_to_csv`] writes. The result
/// contains no tombstones and no table properties.
///
/// # Errors
///
/// Returns [`ImportError::Csv`] for malformed CSV or records of different
/// lengths, [`ImportError::Row`] when a row fails schema validation, and
/// [`ImportError::UnknownField`] or [`ImportError::DuplicateField`] for an
/// invalid header.
///
/// # Examples
///
/// ```
/// use gd::{ColumnSpec, DataType, Schema, ValueRef, table_from_csv};
///
/// let schema = Schema::new([
///     ColumnSpec::new("name", DataType::String),
///     ColumnSpec::new("score", DataType::F32).nullable(true),
/// ])
/// .unwrap();
/// let table = table_from_csv(schema, "score,name\n0.5,Ada\n,Grace\n", true).unwrap();
/// assert_eq!(table.cell(0, 1), Ok(ValueRef::F32(0.5)));
/// assert_eq!(table.cell(1, 0), Ok(ValueRef::String("Grace")));
/// assert_eq!(table.cell(1, 1), Ok(ValueRef::Null));
/// ```
pub fn table_from_csv(
    schema: impl Into<Arc<Schema>>,
    csv: &str,
    headers: bool,
) -> Result<Table, ImportError> {
    let mut table = Table::new(schema);
    let schema = table.schema_arc();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(csv.as_bytes());
    let mut records = reader.records();
    let layout = if headers {
        match records.next() {
            Some(header) => csv_header_layout(&schema, &header?)?,
            None => return Ok(table),
        }
    } else {
        (0..schema.len()).map(CsvTarget::Column).collect()
    };

    for (row, record) in records.enumerate() {
        let record = record?;
        if record.len() != layout.len() {
            return Err(ImportError::Row {
                row,
                source: TableError::RowWidth {
                    expected: layout.len(),
                    actual: record.len(),
                },
            });
        }
        let mut values: Vec<Value> = std::iter::repeat_with(|| Value::Null)
            .take(schema.len())
            .collect();
        let mut extras = Vec::new();
        for (target, field) in layout.iter().zip(record.iter()) {
            match target {
                CsvTarget::Column(column) => {
                    values[*column] = csv_cell(field, schema_column(&schema, *column));
                }
                CsvTarget::Extra(name) if !field.is_empty() => {
                    extras.push((name.clone(), Value::from(field)));
                }
                CsvTarget::Extra(_) => {}
            }
        }
        table
            .push_row_vec_with_extras(values, extras)
            .map_err(|source| ImportError::Row { row, source })?;
    }
    Ok(table)
}

fn schema_column(schema: &Schema, column: usize) -> &ColumnSpec {
    schema
        .column(column)
        .expect("column position comes from the same schema")
}

enum CsvTarget {
    Column(usize),
    Extra(CompactString),
}

fn csv_header_layout(
    schema: &Schema,
    header: &csv::StringRecord,
) -> Result<Vec<CsvTarget>, ImportError> {
    let mut seen = vec![false; schema.len()];
    let mut layout: Vec<CsvTarget> = Vec::with_capacity(header.len());
    for name in header {
        let duplicate = || ImportError::DuplicateField {
            row: None,
            name: name.into(),
        };
        if let Some(column) = schema.column_index(name) {
            if std::mem::replace(&mut seen[column], true) {
                return Err(duplicate());
            }
            layout.push(CsvTarget::Column(column));
        } else if schema.unknown_fields() == UnknownFields::Reject {
            return Err(ImportError::UnknownField {
                row: None,
                name: name.into(),
            });
        } else if layout
            .iter()
            .any(|target| matches!(target, CsvTarget::Extra(extra) if extra == name))
        {
            return Err(duplicate());
        } else {
            layout.push(CsvTarget::Extra(name.into()));
        }
    }
    Ok(layout)
}

fn csv_cell(field: &str, spec: &ColumnSpec) -> Value {
    if field.is_empty() {
        // An empty field is also how an empty string or empty byte sequence is
        // written; a required column of those types cannot mean null.
        return match spec.data_type() {
            DataType::String if !spec.is_nullable() => Value::from(""),
            DataType::Bytes if !spec.is_nullable() => Value::from(Vec::new()),
            _ => Value::Null,
        };
    }
    let decoded = match spec.data_type() {
        DataType::Bool => match field {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        DataType::F32 => field.parse().ok().map(Value::F32),
        DataType::F64 => field.parse().ok().map(Value::F64),
        DataType::Bytes => decode_hex(field).ok().map(Value::from),
        DataType::Uuid => Uuid::parse_str(field).ok().map(Value::Uuid),
        DataType::Null | DataType::String => None,
        integer => parse_integer(field, integer),
    };
    decoded.unwrap_or_else(|| Value::from(field))
}

/// Parses decimal integer text as exactly the requested integer column type.
fn parse_integer(text: &str, data_type: DataType) -> Option<Value> {
    match data_type {
        DataType::I8 => text.parse().ok().map(Value::I8),
        DataType::I16 => text.parse().ok().map(Value::I16),
        DataType::I32 => text.parse().ok().map(Value::I32),
        DataType::I64 => text.parse().ok().map(Value::I64),
        DataType::U8 => text.parse().ok().map(Value::U8),
        DataType::U16 => text.parse().ok().map(Value::U16),
        DataType::U32 => text.parse().ok().map(Value::U32),
        DataType::U64 => text.parse().ok().map(Value::U64),
        _ => None,
    }
}

struct RowsSeed<'a> {
    table: &'a mut Table,
    failure: &'a mut Option<ImportError>,
}

impl<'de> DeserializeSeed<'de> for RowsSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for RowsSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON array of row objects")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut rows: A) -> Result<(), A::Error> {
        let mut row = 0;
        while rows
            .next_element_seed(RowSeed {
                table: &mut *self.table,
                failure: &mut *self.failure,
                row,
            })?
            .is_some()
        {
            row += 1;
        }
        Ok(())
    }
}

struct RowSeed<'a> {
    table: &'a mut Table,
    failure: &'a mut Option<ImportError>,
    row: usize,
}

impl RowSeed<'_> {
    /// Records the structured failure and aborts deserialization.
    fn fail<E: de::Error>(&mut self, failure: ImportError) -> E {
        *self.failure = Some(failure);
        E::custom("table import failed")
    }
}

impl<'de> DeserializeSeed<'de> for RowSeed<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for RowSeed<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object with one row")
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut fields: A) -> Result<(), A::Error> {
        let schema = self.table.schema_arc();
        let row = self.row;
        let mut values: Vec<Option<Value>> =
            std::iter::repeat_with(|| None).take(schema.len()).collect();
        let mut extras: Vec<(CompactString, Value)> = Vec::new();
        while let Some(FieldName(name)) = fields.next_key()? {
            let raw: &'de RawValue = fields.next_value()?;
            let decoded = match schema.column_index(&name) {
                Some(column) if values[column].is_some() => Err(ImportError::DuplicateField {
                    row: Some(row),
                    name,
                }),
                Some(column) => json_cell(raw, schema_column(&schema, column), row, &name)
                    .map(|value| values[column] = Some(value)),
                None if schema.unknown_fields() == UnknownFields::Reject => {
                    Err(ImportError::UnknownField {
                        row: Some(row),
                        name,
                    })
                }
                None if extras.iter().any(|(extra, _)| *extra == name) => {
                    Err(ImportError::DuplicateField {
                        row: Some(row),
                        name,
                    })
                }
                None => json_natural(raw, row, &name).map(|value| extras.push((name, value))),
            };
            if let Err(failure) = decoded {
                return Err(self.fail(failure));
            }
        }
        let values = values
            .into_iter()
            .map(|value| value.unwrap_or(Value::Null))
            .collect();
        match self.table.push_row_vec_with_extras(values, extras) {
            Ok(_) => Ok(()),
            Err(source) => Err(self.fail(ImportError::Row { row, source })),
        }
    }
}

struct FieldName(CompactString);

impl<'de> de::Deserialize<'de> for FieldName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(FieldNameVisitor)
    }
}

struct FieldNameVisitor;

impl Visitor<'_> for FieldNameVisitor {
    type Value = FieldName;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a field name")
    }

    fn visit_str<E: de::Error>(self, name: &str) -> Result<FieldName, E> {
        Ok(FieldName(name.into()))
    }
}

/// Decodes one JSON field for a schema column.
fn json_cell(
    raw: &RawValue,
    spec: &ColumnSpec,
    row: usize,
    name: &str,
) -> Result<Value, ImportError> {
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'"') => {
            let string = json_string(text)?;
            let decoded = match spec.data_type() {
                DataType::Bytes => decode_hex(&string).ok().map(Value::from),
                DataType::Uuid => Uuid::parse_str(&string).ok().map(Value::Uuid),
                _ => None,
            };
            Ok(decoded.unwrap_or(Value::String(string)))
        }
        Some(b'-' | b'0'..=b'9') => {
            let decoded = match spec.data_type() {
                DataType::F32 => text
                    .parse()
                    .ok()
                    .filter(|v: &f32| v.is_finite())
                    .map(Value::F32),
                DataType::F64 => text
                    .parse()
                    .ok()
                    .filter(|v: &f64| v.is_finite())
                    .map(Value::F64),
                integer if is_json_integer(text) => parse_integer(text, integer),
                _ => None,
            };
            decoded.map_or_else(|| json_number(text, row, name), Ok)
        }
        _ => json_natural(raw, row, name),
    }
}

/// Decodes one JSON field as its natural dynamic value.
fn json_natural(raw: &RawValue, row: usize, name: &str) -> Result<Value, ImportError> {
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'n') => Ok(Value::Null),
        Some(b't') => Ok(Value::Bool(true)),
        Some(b'f') => Ok(Value::Bool(false)),
        Some(b'"') => Ok(Value::String(json_string(text)?)),
        Some(b'{' | b'[') => Err(ImportError::NestedJsonValue {
            row,
            name: name.into(),
        }),
        _ => json_number(text, row, name),
    }
}

fn json_string(text: &str) -> Result<CompactString, serde_json::Error> {
    // Borrow when the string has no escapes; allocate only to unescape.
    match serde_json::from_str::<&str>(text) {
        Ok(string) => Ok(string.into()),
        Err(_) => serde_json::from_str::<String>(text).map(CompactString::from),
    }
}

fn is_json_integer(text: &str) -> bool {
    !text.contains(['.', 'e', 'E'])
}

/// Decodes a JSON number as the narrowest natural type that holds it exactly.
fn json_number(text: &str, row: usize, name: &str) -> Result<Value, ImportError> {
    if is_json_integer(text) {
        if let Ok(value) = text.parse() {
            return Ok(Value::I64(value));
        }
        if let Ok(value) = text.parse() {
            return Ok(Value::U64(value));
        }
    }
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(Value::F64(value)),
        _ => Err(ImportError::NumberOutOfRange {
            row,
            name: name.into(),
        }),
    }
}

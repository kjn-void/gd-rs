//! Idiomatic Rust implementation of the core `gd` data model.
//!
//! The crate is a semantic port rather than a C++ ABI port. It uses Rust sum
//! types and lifetimes instead of the source project's manually tagged unions,
//! allocation flags, and layout-compatible borrowed views.
//!
//! The optional `sqlite` feature provides a narrow adapter between GD values,
//! arguments, typed tables, and `rusqlite`. Other database drivers and a generic
//! database abstraction are intentionally excluded.

mod arguments;
mod binary;
mod expression;
mod format;
#[cfg(feature = "sqlite")]
mod sqlite;
mod table;
mod text;
mod value;

pub use arguments::{Argument, ArgumentIndex, Arguments};
pub use binary::{
    BinaryError, BinaryReader, BinaryWriter, Endian, decode_hex, encode_hex, encode_hex_upper,
    find_bytes, rfind_bytes,
};
pub use expression::{ExpressionContext, ExpressionEngine, ExpressionError, Program, ProgramKind};
pub use format::{
    FormatError, arguments_to_json, arguments_to_uri, row_order_to_json, table_to_csv,
    table_to_json,
};
#[cfg(feature = "sqlite")]
pub use sqlite::{
    SqliteConnection, SqliteDatabase, SqliteEngineError, SqliteError, SqliteStorageClass,
};
pub use table::{
    Column, ColumnIndex, ColumnSpec, IndexKeyRef, NullOrder, Row, RowOrder, Schema, SortDirection,
    Table, TableError,
};
pub use text::{
    TextError, decode_json_string, decode_percent_component, decode_utf16, encode_json_string,
    encode_percent_component, escape_xml, prefix_chars, split_escaped, trim_ascii_control,
    validate_utf8,
};
pub use value::{DataType, Value, ValueError, ValueRef};

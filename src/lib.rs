//! Idiomatic Rust implementation of the core `gd` data model.
//!
//! The crate is a semantic port rather than a C++ ABI port. It uses Rust sum
//! types and lifetimes instead of the source project's manually tagged unions,
//! allocation flags, and layout-compatible borrowed views.
//!
//! The optional `sqlite` feature provides a narrow adapter between GD values,
//! arguments, typed tables, and `rusqlite`. Other database drivers and a generic
//! database abstraction are intentionally excluded.
//!
//! The library contains no unsafe code except three unchecked UTF-8 conversions
//! in fixed-capacity string storage. Writes accept valid strings, private lengths
//! and offsets identify their initialized prefixes, and safe string mutation
//! preserves UTF-8. The conversions avoid revalidating those bytes on every read;
//! bounds, capacity, nullability, and borrowing remain checked.

mod arguments;
mod binary;
mod expression;
mod format;
mod format_import;
// Keep the standalone usage guides executable without adding them to the public API.
#[cfg(doctest)]
mod guide_tests;
#[cfg(feature = "sqlite")]
mod sqlite;
mod string_transport;
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
    FormatError, arguments_to_json, arguments_to_uri, row_order_to_json, selection_to_csv,
    selection_to_json, shared_selection_to_csv, shared_selection_to_json, table_to_csv,
    table_to_json,
};
pub use format_import::{ImportError, table_from_csv, table_from_json};
#[cfg(feature = "sqlite")]
pub use sqlite::{
    SqliteConnection, SqliteDatabase, SqliteEngineError, SqliteError, SqliteStorageClass,
};
pub use string_transport::StringTransport;
pub use table::debug as table_debug;
pub use table::{
    Column, ColumnConversionError, ColumnConverter, ColumnElement, ColumnIndex, ColumnMapping,
    ColumnMut, ColumnSelectionError, ColumnSliceError, ColumnSpec, CompositeIndex,
    ConcurrentTableBuilder, FixedStringCellMut, FixedStringError, FixedStrings, FixedStringsMut,
    IndexKeyRef, InternedStringCellMut, InternedStringError, InternedStrings, InternedStringsMut,
    NullOrder, NullableValues, NullableValuesMut, Row, RowCompaction, RowMut, RowOrder, RowsMut,
    Schema, SelectedRow, SharedProjectionError, SharedRecordTable, SharedTableProjection,
    SharedTableSelection, SortDirection, Table, TableError, TableProjectionReadGuard,
    TableProjectionWriteGuard, TableSelection, UnknownFields,
};
pub use text::{
    TextError, decode_json_string, decode_percent_component, decode_utf16, encode_json_string,
    encode_percent_component, escape_xml, prefix_chars, split_escaped, trim_ascii_control,
    validate_utf8,
};
pub use value::{DataType, Value, ValueError, ValueRef};

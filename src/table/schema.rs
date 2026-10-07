//! Table schemas and column declarations.

use std::{fmt, sync::Arc};

use ahash::AHashMap;
use compact_str::CompactString;
use thiserror::Error;

use crate::{DataType, Value, ValueRef};

use super::TableError;

/// A failure reported by a schema column's input converter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{message}")]
pub struct ColumnConversionError {
    message: CompactString,
}

impl ColumnConversionError {
    /// Creates a conversion failure with an application-facing explanation.
    pub fn new(message: impl Into<CompactString>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the conversion failure explanation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<&str> for ColumnConversionError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<String> for ColumnConversionError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<CompactString> for ColumnConversionError {
    fn from(message: CompactString) -> Self {
        Self::new(message)
    }
}

type ConvertValue =
    dyn for<'value> Fn(ValueRef<'value>) -> Result<Value, ColumnConversionError> + Send + Sync;

/// A named, thread-safe input conversion policy reusable by schema columns.
///
/// The converter runs only for a non-null input whose logical type differs from
/// the column type. Its returned value is validated against the ordinary type and
/// nullability contract before a table mutation occurs.
///
/// The name is the converter's semantic identity when schemas are compared. Two
/// independently constructed converters with the same name must implement the
/// same conversion contract.
#[derive(Clone)]
pub struct ColumnConverter {
    name: CompactString,
    convert: Arc<ConvertValue>,
}

impl ColumnConverter {
    /// Creates a named converter from a reusable function or closure.
    pub fn new(
        name: impl Into<CompactString>,
        convert: impl for<'value> Fn(ValueRef<'value>) -> Result<Value, ColumnConversionError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            convert: Arc::new(convert),
        }
    }

    /// Returns the stable semantic name used in diagnostics and schema equality.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn convert(&self, value: ValueRef<'_>) -> Result<Value, ColumnConversionError> {
        (self.convert)(value)
    }
}

impl fmt::Debug for ColumnConverter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ColumnConverter")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ColumnConverter {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ColumnConverter {}

/// A schema column's name, optional alias, type, nullability, and input policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnSpec {
    name: CompactString,
    alias: Option<CompactString>,
    data_type: DataType,
    nullable: bool,
    converter: Option<ColumnConverter>,
    fixed_string_capacity: Option<usize>,
}

/// Policy for names that are not declared as schema columns or aliases.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UnknownFields {
    /// Reject unknown names. This is the default for a strict schema.
    #[default]
    Reject,
    /// Store unknown names as owned, row-local dynamic values.
    Store,
}

impl ColumnSpec {
    /// Creates a non-nullable column, except that a `Null` column is always nullable.
    pub fn new(name: impl Into<CompactString>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            alias: None,
            data_type,
            nullable: data_type == DataType::Null,
            converter: None,
            fixed_string_capacity: None,
        }
    }

    /// Creates a required UTF-8 column with fixed-capacity slots in one shared buffer.
    ///
    /// Each row stores a byte offset and length. Writes reject strings larger
    /// than `capacity` bytes instead of truncating them. Nullability, conversion,
    /// row views, selection, append, and compaction retain their usual semantics.
    ///
    /// ```
    /// use gd::{ColumnSpec, Schema, Table, Value, ValueRef};
    /// let mut table = Table::new(Schema::new([
    ///     ColumnSpec::fixed_string("name", 32).nullable(true),
    /// ])?);
    /// table.push_row([Value::from("Åsa")])?;
    /// table.push_row([Value::Null])?;
    /// let (_, [column]) = table.columns_io([], [0])?;
    /// let mut strings = column.fixed_strings_mut().unwrap();
    /// strings.set(1, Some("Ada"))?;
    /// assert_eq!(table.cell(1, 0)?, ValueRef::String("Ada"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Panics
    /// Panics when `capacity` is zero or exceeds `isize::MAX`.
    #[must_use]
    pub fn fixed_string(name: impl Into<CompactString>, capacity: usize) -> Self {
        assert!(
            capacity > 0 && isize::try_from(capacity).is_ok(),
            "invalid fixed string capacity"
        );
        Self {
            fixed_string_capacity: Some(capacity),
            ..Self::new(name, DataType::String)
        }
    }

    /// Returns the fixed UTF-8 slot capacity, or `None` for ordinary storage.
    #[must_use]
    pub const fn fixed_string_capacity(&self) -> Option<usize> {
        self.fixed_string_capacity
    }

    /// Sets an alternate lookup name.
    #[must_use]
    pub fn with_alias(mut self, alias: impl Into<CompactString>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Sets whether this column accepts [`crate::Value::Null`].
    ///
    /// A column whose type is [`DataType::Null`] remains nullable.
    #[must_use]
    pub const fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable || matches!(self.data_type, DataType::Null);
        self
    }

    /// Sets an explicit converter for non-null input values of another type.
    ///
    /// Exact-type values retain the existing fast path and do not call the
    /// converter. Conversion output is still checked against this column's type
    /// and nullability before the table changes.
    #[must_use]
    pub fn with_converter(mut self, converter: ColumnConverter) -> Self {
        self.converter = Some(converter);
        self
    }

    /// Returns the primary column name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the optional alternate lookup name.
    #[must_use]
    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    /// Returns the column's logical value type.
    #[must_use]
    pub const fn data_type(&self) -> DataType {
        self.data_type
    }

    /// Returns whether the column accepts null values.
    #[must_use]
    pub const fn is_nullable(&self) -> bool {
        self.nullable
    }

    /// Returns the optional input converter.
    #[must_use]
    pub fn converter(&self) -> Option<&ColumnConverter> {
        self.converter.as_ref()
    }
}

/// An immutable table schema with O(1)-expected name/alias lookup and an
/// explicit policy for row-local unknown fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schema {
    columns: Vec<ColumnSpec>,
    by_name: AHashMap<CompactString, usize>,
    unknown_fields: UnknownFields,
}

impl Schema {
    /// Validates and constructs a schema.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::DuplicateColumnName`] if a name or alias is already
    /// assigned to a different column.
    pub fn new(columns: impl IntoIterator<Item = ColumnSpec>) -> Result<Self, TableError> {
        let columns: Vec<_> = columns.into_iter().collect();
        let mut by_name = AHashMap::with_capacity(columns.len().saturating_mul(2));
        for (position, column) in columns.iter().enumerate() {
            insert_schema_name(&mut by_name, column.name(), position)?;
            if let Some(alias) = column.alias() {
                insert_schema_name(&mut by_name, alias, position)?;
            }
        }
        Ok(Self {
            columns,
            by_name,
            unknown_fields: UnknownFields::Reject,
        })
    }

    /// Sets how tables using this schema handle names absent from the schema.
    #[must_use]
    pub const fn with_unknown_fields(mut self, unknown_fields: UnknownFields) -> Self {
        self.unknown_fields = unknown_fields;
        self
    }

    /// Returns the policy for names absent from the schema.
    #[must_use]
    pub const fn unknown_fields(&self) -> UnknownFields {
        self.unknown_fields
    }

    /// Returns the number of columns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Returns whether the schema contains no columns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Returns a column definition by position.
    #[must_use]
    pub fn column(&self, position: usize) -> Option<&ColumnSpec> {
        self.columns.get(position)
    }

    /// Resolves a primary name or alias in expected O(name length).
    #[must_use]
    pub fn column_index(&self, name_or_alias: &str) -> Option<usize> {
        self.by_name.get(name_or_alias).copied()
    }

    /// Returns column definitions in positional order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ColumnSpec> + DoubleEndedIterator {
        self.columns.iter()
    }
}

fn insert_schema_name(
    names: &mut AHashMap<CompactString, usize>,
    name: &str,
    position: usize,
) -> Result<(), TableError> {
    if let Some(previous) = names.get(name) {
        if *previous != position {
            return Err(TableError::DuplicateColumnName(name.into()));
        }
        return Ok(());
    }
    names.insert(name.into(), position);
    Ok(())
}

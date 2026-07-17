//! Table schemas and column declarations.

use ahash::AHashMap;
use compact_str::CompactString;

use crate::DataType;

use super::TableError;

/// A schema column's name, optional alias, type, and nullability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColumnSpec {
    name: CompactString,
    alias: Option<CompactString>,
    data_type: DataType,
    nullable: bool,
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
        }
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

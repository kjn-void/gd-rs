//! Atomic column-wise table append with explicit source-to-destination mapping.

use std::ops::Range;

use super::{Table, TableError, collect_extras, prepare_cell};
use crate::{DataType, Value};

/// One source column copied into one destination column during table append.
///
/// A source may be reused for several destinations; each destination may appear
/// only once. Unmapped destination columns receive nulls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColumnMapping {
    /// Column position in the source table.
    pub source: usize,
    /// Column position in the destination table.
    pub destination: usize,
}

impl ColumnMapping {
    /// Creates a source-to-destination mapping.
    #[must_use]
    pub const fn new(source: usize, destination: usize) -> Self {
        Self {
            source,
            destination,
        }
    }
}

impl Table {
    /// Appends all physical source rows by column position.
    ///
    /// Widths must match. Names need not match; destination converters and
    /// nullability apply. Source tombstones and extras are preserved, and existing
    /// rows retain their positions. Destination properties remain unchanged;
    /// source properties are not merged. Returns the newly appended range.
    ///
    /// # Errors
    ///
    /// Returns a width, type, nullability, conversion, extras, or row-count error
    /// without changing the destination. Converter side effects cannot be rolled
    /// back. Allocation failure follows ordinary Rust allocation behavior.
    pub fn append(&mut self, source: &Self) -> Result<Range<usize>, TableError> {
        if self.column_count() != source.column_count() {
            return Err(TableError::RowWidth {
                expected: self.column_count(),
                actual: source.column_count(),
            });
        }
        let mapping: Vec<_> = (0..self.column_count())
            .map(|column| ColumnMapping::new(column, column))
            .collect();
        self.append_mapped(source, &mapping)
    }

    /// Appends all physical rows using destination primary names and aliases.
    ///
    /// Each destination name/alias is resolved through source schema lookup.
    /// Matching different source columns is ambiguous and rejected. Missing
    /// destinations receive nulls, so must be nullable. Unmatched fixed source
    /// columns are omitted. Extras, tombstones, properties and atomicity follow
    /// [`Self::append`].
    ///
    /// # Errors
    ///
    /// Returns an ambiguous mapping or any error from [`Self::append_mapped`].
    pub fn append_named(&mut self, source: &Self) -> Result<Range<usize>, TableError> {
        let mut mapping = Vec::with_capacity(self.column_count());
        for (destination, spec) in self.schema().iter().enumerate() {
            let primary = source.schema().column_index(spec.name());
            let alias = spec
                .alias()
                .and_then(|name| source.schema().column_index(name));
            if primary.is_some() && alias.is_some() && primary != alias {
                return Err(TableError::AmbiguousColumnMapping {
                    column: destination,
                });
            }
            if let Some(column) = primary.or(alias) {
                mapping.push(ColumnMapping::new(column, destination));
            }
        }
        self.append_mapped(source, &mapping)
    }

    /// Appends all physical source rows with an explicit column mapping.
    ///
    /// Unmapped destinations receive nulls and must be nullable, even for an empty
    /// source. Repeated sources are allowed; repeated destinations are rejected.
    /// Exact-type columns bypass converters. Other non-null values require the
    /// destination's explicit converter; there is no implicit numeric coercion.
    /// Extras are preserved and validated against the destination schema: closed
    /// schemas or collisions with destination names/aliases reject the batch.
    /// Unmapped fixed source columns are omitted rather than becoming extras.
    ///
    /// All preparation occurs in independent storage before any destination
    /// mutation. Compatible columns are cloned in bulk, converted columns are
    /// staged cell by cell, then typed vectors and sidecars are extended by moves.
    /// Source tombstones are copied. Destination properties remain unchanged.
    ///
    /// ```
    /// use gd::{ColumnMapping, ColumnSpec, DataType, Schema, Table, Value};
    /// let mut source = Table::new(Schema::new([
    ///     ColumnSpec::new("name", DataType::String),
    ///     ColumnSpec::new("id", DataType::I64),
    /// ])?);
    /// source.push_row([Value::from("Ada"), Value::I64(7)])?;
    /// let mut target = Table::new(Schema::new([
    ///     ColumnSpec::new("id", DataType::I64),
    ///     ColumnSpec::new("name", DataType::String),
    /// ])?);
    /// assert_eq!(target.append_mapped(&source, &[
    ///     ColumnMapping::new(1, 0), ColumnMapping::new(0, 1),
    /// ])?, 0..1);
    /// # Ok::<(), gd::TableError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns mapping bounds/duplication, type, nullability, conversion, extras,
    /// or row-count errors without appending any row. Converter side effects and
    /// allocation failures are outside this transactional guarantee.
    pub fn append_mapped(
        &mut self,
        source: &Self,
        mapping: &[ColumnMapping],
    ) -> Result<Range<usize>, TableError> {
        let begin = self.row_count;
        let end = begin
            .checked_add(source.row_count)
            .ok_or(TableError::RowCountOverflow)?;
        let mut layout = vec![None; self.column_count()];
        for &ColumnMapping {
            source: from,
            destination: to,
        } in mapping
        {
            if from >= source.column_count() {
                return Err(TableError::ColumnOutOfBounds {
                    column: from,
                    column_count: source.column_count(),
                });
            }
            let width = self.column_count();
            let slot = layout.get_mut(to).ok_or(TableError::ColumnOutOfBounds {
                column: to,
                column_count: width,
            })?;
            if slot.replace(from).is_some() {
                return Err(TableError::DuplicateColumnMapping { column: to });
            }
        }
        let mut staged = Self::with_capacity(self.schema_arc(), source.row_count);
        for (destination, (spec, from)) in self.schema().iter().zip(layout).enumerate() {
            if let Some(from) = from {
                let source_spec =
                    source
                        .schema()
                        .column(from)
                        .ok_or(TableError::InternalInvariant {
                            detail: "mapped source column is missing",
                        })?;
                if source_spec.data_type() == spec.data_type()
                    && source_spec.is_nullable() == spec.is_nullable()
                    && source_spec.fixed_string_capacity() == spec.fixed_string_capacity()
                {
                    staged.columns[destination] = source.columns[from].clone();
                    continue;
                }
                if source_spec.data_type() != spec.data_type()
                    && source_spec.data_type() != DataType::Null
                    && spec.converter().is_none()
                {
                    return Err(TableError::TypeMismatch {
                        column: destination,
                        expected: spec.data_type(),
                        actual: source_spec.data_type(),
                    });
                }
            } else if !spec.is_nullable() {
                return Err(TableError::NullNotAllowed {
                    column: destination,
                });
            }
            let column = &mut staged.columns[destination];
            for row in 0..source.row_count {
                let mut value = match from {
                    Some(from) => source.cell(row, from)?.to_owned(),
                    None => Value::Null,
                };
                prepare_cell(spec, &mut value, destination)?;
                column.push_validated(value)?;
            }
        }
        for row in 0..source.row_count {
            staged.extras.push_empty();
            if let Some(extras) = source.extras.get(row) {
                let extras = collect_extras(
                    self.schema(),
                    extras.iter().map(|(name, value)| (name, value.clone())),
                )?;
                if !extras.is_empty() {
                    staged.extras.set(row, extras)?;
                }
            }
        }
        // Verify every storage pair before performing any fallible internal append.
        if !self.extras.can_append(&staged.extras)
            || !self
                .columns
                .iter()
                .zip(&staged.columns)
                .all(|(left, right)| left.can_append(right))
        {
            return Err(TableError::InternalInvariant {
                detail: "append storage layouts differ",
            });
        }
        for (destination, column) in self.columns.iter_mut().zip(staged.columns) {
            destination.append(column)?;
        }
        self.extras.append(staged.extras)?;
        self.tombstones
            .append(&source.tombstones, begin, source.row_count);
        self.row_count = end;
        Ok(begin..end)
    }
}

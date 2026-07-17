//! Concurrent row collection before conversion to dense column storage.

use std::ops::Range;
use std::sync::Arc;

use compact_str::CompactString;
use orx_concurrent_vec::ConcurrentVec;
use smallvec::SmallVec;

use crate::Value;

use super::storage::RowExtras;
use super::{Schema, Table, TableError, collect_extras, validate_row};

const INLINE_ROW_VALUES: usize = 8;

#[derive(Debug)]
struct PendingRow {
    values: SmallVec<[Value; INLINE_ROW_VALUES]>,
    extras: Option<Box<RowExtras>>,
}

impl PendingRow {
    fn new(values: impl IntoIterator<Item = Value>) -> Self {
        Self {
            values: values.into_iter().collect(),
            extras: None,
        }
    }

    fn from_vec(values: Vec<Value>) -> Self {
        Self {
            values: SmallVec::from_vec(values),
            extras: None,
        }
    }

    fn with_extras(values: impl IntoIterator<Item = Value>, extras: RowExtras) -> Self {
        Self {
            values: values.into_iter().collect(),
            extras: (!extras.is_empty()).then(|| Box::new(extras)),
        }
    }
}

/// A thread-safe collector for complete rows that becomes a dense [`Table`].
///
/// The builder stores each pending row as one concurrent element. A row is
/// therefore published only after all of its fixed values and row-local extras
/// have been validated and assembled. Concurrent insertion order determines row
/// positions; callers must use the returned position rather than assuming thread
/// scheduling order.
///
/// Consuming the builder with [`ConcurrentTableBuilder::into_table`] transposes
/// the temporary row-oriented representation into the ordinary table's dense,
/// typed columns. The resulting table has no concurrent-element overhead and
/// retains the existing typed-slice and Rayon APIs.
#[derive(Debug)]
pub struct ConcurrentTableBuilder {
    schema: Arc<Schema>,
    rows: ConcurrentVec<PendingRow>,
}

impl ConcurrentTableBuilder {
    /// Creates an empty concurrent builder from an owned or shared schema.
    #[must_use]
    pub fn new(schema: impl Into<Arc<Schema>>) -> Self {
        Self {
            schema: schema.into(),
            rows: ConcurrentVec::new(),
        }
    }

    /// Returns the immutable schema used to validate pending rows.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        self.schema.as_ref()
    }

    /// Clones the shared schema handle without copying schema metadata.
    #[must_use]
    pub fn schema_arc(&self) -> Arc<Schema> {
        Arc::clone(&self.schema)
    }

    /// Returns the length of the completely published contiguous row prefix.
    ///
    /// While producers are active, a later reservation can finish before an
    /// earlier one and remain temporarily excluded from this count. Once all
    /// producers have returned, this is the exact number of rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Returns whether no complete row has been published.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Validates and concurrently appends one complete fixed-schema row.
    ///
    /// The returned position is assigned by the concurrent vector and is the
    /// row's final position after conversion with [`Self::into_table`].
    ///
    /// # Errors
    ///
    /// Returns [`TableError::RowWidth`], [`TableError::TypeMismatch`], or
    /// [`TableError::NullNotAllowed`] without publishing a row when validation
    /// fails.
    ///
    /// # Panics
    ///
    /// Panics if the concurrent vector's maximum capacity is exhausted.
    pub fn push_row<const N: usize>(&self, values: [Value; N]) -> Result<usize, TableError> {
        validate_row(self.schema(), &values)?;
        Ok(self.rows.push(PendingRow::new(values)))
    }

    /// Validates and concurrently appends one runtime-width owned row.
    ///
    /// # Errors
    ///
    /// Returns the ordinary row-validation errors without publishing a row.
    ///
    /// # Panics
    ///
    /// Panics if the concurrent vector's maximum capacity is exhausted.
    pub fn push_row_vec(&self, values: Vec<Value>) -> Result<usize, TableError> {
        validate_row(self.schema(), &values)?;
        Ok(self.rows.push(PendingRow::from_vec(values)))
    }

    /// Validates and concurrently appends a row with row-local extra fields.
    ///
    /// Repeated extra names replace earlier values. The row and all extras are
    /// validated before anything is published.
    ///
    /// # Errors
    ///
    /// Returns the ordinary row-validation errors,
    /// [`TableError::ColumnNotFound`] when the schema rejects unknown fields, or
    /// [`TableError::ExtraFieldConflictsWithColumn`] for a declared name.
    ///
    /// # Panics
    ///
    /// Panics if the concurrent vector's maximum capacity is exhausted.
    pub fn push_row_with_extras<const N: usize, I, K, V>(
        &self,
        values: [Value; N],
        extras: I,
    ) -> Result<usize, TableError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<CompactString>,
        V: Into<Value>,
    {
        validate_row(self.schema(), &values)?;
        let extras = collect_extras(self.schema(), extras)?;
        Ok(self.rows.push(PendingRow::with_extras(values, extras)))
    }

    /// Validates a batch atomically and appends its rows consecutively.
    ///
    /// Validation happens before the concurrent vector reserves positions, so
    /// one invalid row leaves the builder unchanged. The returned range contains
    /// the final positions assigned to this batch.
    ///
    /// # Errors
    ///
    /// Returns the first row-validation error without publishing any row from
    /// the batch.
    ///
    /// # Panics
    ///
    /// Panics if the complete batch exceeds the concurrent vector's maximum
    /// capacity.
    pub fn extend_rows<const N: usize>(
        &self,
        rows: impl IntoIterator<Item = [Value; N]>,
    ) -> Result<Range<usize>, TableError> {
        let mut pending = Vec::new();
        for values in rows {
            validate_row(self.schema(), &values)?;
            pending.push(PendingRow::new(values));
        }
        let count = pending.len();
        let begin = self.rows.extend(pending);
        Ok(begin..begin + count)
    }

    /// Consumes the builder and appends all pending rows to an existing table.
    ///
    /// The destination is borrowed exclusively only for this final row-to-column
    /// transpose. Existing rows retain their positions, and the returned range
    /// contains the newly appended positions.
    ///
    /// Structurally equal schemas are accepted; they do not need to share the
    /// same [`Arc`].
    ///
    /// # Errors
    ///
    /// Returns [`TableError::SchemaMismatch`] without changing `table` when the
    /// builder and destination schemas differ.
    pub fn append_to(self, table: &mut Table) -> Result<Range<usize>, TableError> {
        if self.schema.as_ref() != table.schema() {
            return Err(TableError::SchemaMismatch);
        }
        Ok(append_rows(self.rows, table))
    }

    /// Consumes the builder and transposes all rows into dense typed columns.
    #[must_use]
    pub fn into_table(self) -> Table {
        let Self { schema, rows } = self;
        let mut table = Table::with_capacity(schema, rows.len());
        let _ = append_rows(rows, &mut table);
        table
    }
}

fn append_rows(rows: ConcurrentVec<PendingRow>, table: &mut Table) -> Range<usize> {
    let begin = table.row_count();
    for pending in rows {
        let row = table.push_validated_row(pending.values);
        if let Some(extras) = pending.extras {
            table.extras.set_box(row, extras);
        }
    }
    begin..table.row_count()
}

//! Interned UTF-8 cells with immutable payloads and independently mutable handles.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

use ahash::AHashMap;
use thiserror::Error;

use super::{TableError, storage::ColumnData};

/// An invalid write through an interned string view.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InternedStringError {
    /// The requested row is outside this view.
    #[error("row {row} is out of bounds for {row_count} rows")]
    RowOutOfBounds {
        /// Requested relative row position.
        row: usize,
        /// Number of cells in this view.
        row_count: usize,
    },
    /// A null write was attempted on required storage.
    #[error("the string column does not allow nulls")]
    NullNotAllowed,
}

// Cells own entries, while the cache holds only weak entry references. Entry
// strong counts therefore track cells without hashing on clone/copy. Only the
// last entry drop removes its cache key. Each pool has its own entries even when
// an append shares the Arc<str> payload.
#[derive(Default)]
struct Interner {
    entries: Mutex<AHashMap<Arc<str>, Weak<InternedValue>>>,
}

struct InternedValue {
    text: Arc<str>,
    interner: Weak<Interner>,
}

impl InternedValue {
    fn as_str(&self) -> &str {
        &self.text
    }
}

impl fmt::Debug for InternedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.text.fmt(formatter)
    }
}

impl PartialEq for InternedValue {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Drop for InternedValue {
    fn drop(&mut self) {
        if let Some(interner) = self.interner.upgrade() {
            let mut entries = interner.entries();
            // Another thread may have found this dead weak entry and replaced
            // it while this destructor waited for the lock. Only remove ours.
            if entries
                .get(self.text.as_ref())
                .is_some_and(|entry| std::ptr::eq(entry.as_ptr(), self))
            {
                entries.remove(self.text.as_ref());
            }
        }
    }
}

impl fmt::Debug for Interner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Formatting can invoke application-provided writers. Never hold the
        // pool lock while formatting; the owning column already prints its cells.
        formatter.debug_struct("Interner").finish_non_exhaustive()
    }
}

impl Interner {
    fn entries(&self) -> MutexGuard<'_, AHashMap<Arc<str>, Weak<InternedValue>>> {
        // No user callbacks or fallible validation run while this lock is held.
        self.entries.lock().expect("interned string pool poisoned")
    }

    fn intern(self: &Arc<Self>, text: &str) -> Arc<InternedValue> {
        self.acquire(text, || Arc::from(text))
    }

    fn intern_shared(self: &Arc<Self>, text: &Arc<str>) -> Arc<InternedValue> {
        self.acquire(text, || Arc::clone(text))
    }

    fn acquire(
        self: &Arc<Self>,
        text: &str,
        create: impl FnOnce() -> Arc<str>,
    ) -> Arc<InternedValue> {
        let mut entries = self.entries();
        let text = if let Some((text, entry)) = entries.get_key_value(text) {
            if let Some(value) = entry.upgrade() {
                return value;
            }
            // Reuse the cached payload if the last entry's destructor has not
            // acquired the lock yet. That destructor must not remove this entry.
            Arc::clone(text)
        } else {
            create()
        };
        let value = Arc::new(InternedValue {
            text,
            interner: Arc::downgrade(self),
        });
        entries.insert(Arc::clone(&value.text), Arc::downgrade(&value));
        value
    }
}

#[derive(Clone, Debug)]
pub(super) struct InternedStringData {
    values: ColumnData<Arc<InternedValue>>,
    interner: Arc<Interner>,
}

impl PartialEq for InternedStringData {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl InternedStringData {
    pub(super) fn new(nullable: bool, capacity: usize) -> Self {
        Self {
            values: ColumnData::with_capacity(nullable, capacity),
            interner: Arc::new(Interner::default()),
        }
    }

    fn with_values(&self, values: ColumnData<Arc<InternedValue>>) -> Self {
        Self {
            values,
            interner: Arc::clone(&self.interner),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    pub(super) fn value(&self, row: usize) -> Result<Option<&str>, TableError> {
        self.values
            .value(row)
            .map(|value| value.map(|value| value.as_str()))
    }

    pub(super) fn view(&self) -> InternedStrings<'_> {
        let values = match &self.values {
            ColumnData::Required(values) => StringSlots::Required(values),
            ColumnData::Nullable(values) => StringSlots::Nullable(values),
        };
        InternedStrings { values }
    }

    pub(super) fn view_mut(&mut self) -> InternedStringsMut<'_> {
        let values = match &mut self.values {
            ColumnData::Required(values) => StringSlotsMut::Required(values),
            ColumnData::Nullable(values) => StringSlotsMut::Nullable(values),
        };
        InternedStringsMut {
            values,
            interner: &self.interner,
        }
    }

    pub(super) fn push(&mut self, value: Option<&str>) -> Result<(), TableError> {
        match &mut self.values {
            ColumnData::Required(values) => {
                let value = value.ok_or(TableError::InternalInvariant {
                    detail: "null was pushed into a required interned column",
                })?;
                values.push(self.interner.intern(value));
            }
            ColumnData::Nullable(values) => {
                values.push(value.map(|text| self.interner.intern(text)));
            }
        }
        Ok(())
    }

    pub(super) fn pop(&mut self) {
        match &mut self.values {
            ColumnData::Required(values) => drop(values.pop()),
            ColumnData::Nullable(values) => drop(values.pop()),
        }
    }

    pub(super) fn retain_live(&mut self, tombstoned: &[bool]) {
        let mut flags = tombstoned.iter();
        match &mut self.values {
            ColumnData::Required(values) => values.retain(|_| flags.next() != Some(&true)),
            ColumnData::Nullable(values) => values.retain(|_| flags.next() != Some(&true)),
        }
    }

    pub(super) fn copy_range(&self, range: std::ops::Range<usize>) -> Self {
        let values = match &self.values {
            ColumnData::Required(values) => ColumnData::Required(values[range].to_vec()),
            ColumnData::Nullable(values) => ColumnData::Nullable(values[range].to_vec()),
        };
        self.with_values(values)
    }

    pub(super) fn copy_rows(&self, rows: &[usize]) -> Self {
        let values = match &self.values {
            ColumnData::Required(values) => {
                ColumnData::Required(rows.iter().map(|&row| Arc::clone(&values[row])).collect())
            }
            ColumnData::Nullable(values) => {
                ColumnData::Nullable(rows.iter().map(|&row| values[row].clone()).collect())
            }
        };
        self.with_values(values)
    }

    pub(super) fn can_append(&self, other: &Self) -> bool {
        std::mem::discriminant(&self.values) == std::mem::discriminant(&other.values)
    }

    pub(super) fn append_from(&mut self, other: &Self) -> Result<(), TableError> {
        let Self { values, interner } = self;
        let same_pool = Arc::ptr_eq(interner, &other.interner);
        match (values, &other.values) {
            (ColumnData::Required(left), ColumnData::Required(right)) => {
                if same_pool {
                    left.extend_from_slice(right);
                } else {
                    left.extend(
                        right
                            .iter()
                            .map(|value| interner.intern_shared(&value.text)),
                    );
                }
            }
            (ColumnData::Nullable(left), ColumnData::Nullable(right)) => {
                if same_pool {
                    left.extend_from_slice(right);
                } else {
                    left.extend(right.iter().map(|value| {
                        value
                            .as_ref()
                            .map(|value| interner.intern_shared(&value.text))
                    }));
                }
            }
            _ => {
                return Err(TableError::InternalInvariant {
                    detail: "interned append nullability differs",
                });
            }
        }
        Ok(())
    }

    pub(super) fn append(&mut self, mut other: Self) -> Result<(), TableError> {
        let same_pool = Arc::ptr_eq(&self.interner, &other.interner);
        match (&mut self.values, &mut other.values) {
            (ColumnData::Required(left), ColumnData::Required(right)) => {
                if same_pool {
                    left.append(right);
                } else {
                    left.extend(
                        right
                            .iter()
                            .map(|value| self.interner.intern_shared(&value.text)),
                    );
                }
            }
            (ColumnData::Nullable(left), ColumnData::Nullable(right)) => {
                if same_pool {
                    left.append(right);
                } else {
                    left.extend(right.iter().map(|value| {
                        value
                            .as_ref()
                            .map(|value| self.interner.intern_shared(&value.text))
                    }));
                }
            }
            _ => {
                return Err(TableError::InternalInvariant {
                    detail: "interned append nullability differs",
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum StringSlots<'a> {
    Required(&'a [Arc<InternedValue>]),
    Nullable(&'a [Option<Arc<InternedValue>>]),
}

/// A borrowed interned string column. Nulls and empty strings are distinct.
///
/// Payloads are immutable and may be shared by other rows and table copies.
/// Reading strings never acquires the interner lock.
#[derive(Clone, Copy, Debug)]
pub struct InternedStrings<'a> {
    values: StringSlots<'a>,
}

impl<'a> InternedStrings<'a> {
    /// Returns the number of physical cells, including tombstones.
    #[must_use]
    pub fn len(self) -> usize {
        match self.values {
            StringSlots::Required(values) => values.len(),
            StringSlots::Nullable(values) => values.len(),
        }
    }

    /// Returns whether this view has no cells.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Reads a cell: outer `None` means out of bounds, inner `None` means null.
    #[must_use]
    pub fn get(self, row: usize) -> Option<Option<&'a str>> {
        match self.values {
            StringSlots::Required(values) => values.get(row).map(|value| Some(value.as_str())),
            StringSlots::Nullable(values) => values
                .get(row)
                .map(|value| value.as_ref().map(|value| value.as_str())),
        }
    }

    /// Iterates strings in physical row order without allocating or locking.
    #[must_use = "iterators are lazy and must be consumed"]
    pub fn iter(self) -> impl ExactSizeIterator<Item = Option<&'a str>> + DoubleEndedIterator {
        (0..self.len()).map(move |row| self.get(row).flatten())
    }
}

#[derive(Debug)]
enum StringSlotsMut<'a> {
    Required(&'a mut [Arc<InternedValue>]),
    Nullable(&'a mut [Option<Arc<InternedValue>>]),
}

/// An exclusive interned string view with checked replacement and disjoint splits.
///
/// Borrowed strings must be released before the view is mutated again.
///
/// ```compile_fail
/// use gd::{ColumnSpec, Schema, Table};
/// let mut table = Table::new(Schema::new([ColumnSpec::interned_string("text")]).unwrap());
/// table.push_row(["old".into()]).unwrap();
/// let (_, [column]) = table.columns_io([], [0]).unwrap();
/// let mut strings = column.interned_strings_mut().unwrap();
/// let old = strings.get(0).unwrap().unwrap();
/// strings.set(0, Some("new")).unwrap();
/// println!("{old}");
/// ```
#[derive(Debug)]
pub struct InternedStringsMut<'a> {
    values: StringSlotsMut<'a>,
    interner: &'a Arc<Interner>,
}

impl InternedStringsMut<'_> {
    /// Returns the number of physical cells in this view.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_view().len()
    }

    /// Returns whether this view has no cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows this view for lock-free reading.
    #[must_use]
    pub fn as_view(&self) -> InternedStrings<'_> {
        let values = match &self.values {
            StringSlotsMut::Required(values) => StringSlots::Required(values),
            StringSlotsMut::Nullable(values) => StringSlots::Nullable(values),
        };
        InternedStrings { values }
    }

    /// Reads a relative cell, distinguishing out-of-bounds, null and empty text.
    #[must_use]
    pub fn get(&self, row: usize) -> Option<Option<&str>> {
        self.as_view().get(row)
    }

    /// Borrows one cell for checked replacement.
    #[must_use]
    pub fn cell_mut(&mut self, row: usize) -> Option<InternedStringCellMut<'_>> {
        let value = match &mut self.values {
            StringSlotsMut::Required(values) => StringSlotMut::Required(values.get_mut(row)?),
            StringSlotsMut::Nullable(values) => StringSlotMut::Nullable(values.get_mut(row)?),
        };
        Some(InternedStringCellMut {
            value,
            interner: self.interner,
        })
    }

    /// Replaces one cell. The new text is interned; other cells keep their values.
    ///
    /// # Errors
    /// Returns a row-bounds or nullability error before changing any cell.
    pub fn set(&mut self, row: usize, value: Option<&str>) -> Result<(), InternedStringError> {
        let row_count = self.len();
        self.cell_mut(row)
            .ok_or(InternedStringError::RowOutOfBounds { row, row_count })?
            .set(value)
    }

    /// Splits into disjoint cell ranges that can be sent to scoped worker threads.
    ///
    /// # Panics
    /// Panics when `mid > self.len()`.
    #[must_use]
    pub fn split_at(self, mid: usize) -> (Self, Self) {
        let (left, right) = match self.values {
            StringSlotsMut::Required(values) => {
                let (left, right) = values.split_at_mut(mid);
                (
                    StringSlotsMut::Required(left),
                    StringSlotsMut::Required(right),
                )
            }
            StringSlotsMut::Nullable(values) => {
                let (left, right) = values.split_at_mut(mid);
                (
                    StringSlotsMut::Nullable(left),
                    StringSlotsMut::Nullable(right),
                )
            }
        };
        (
            Self {
                values: left,
                interner: self.interner,
            },
            Self {
                values: right,
                interner: self.interner,
            },
        )
    }
}

#[derive(Debug)]
enum StringSlotMut<'a> {
    Required(&'a mut Arc<InternedValue>),
    Nullable(&'a mut Option<Arc<InternedValue>>),
}

/// One exclusively borrowed interned string cell.
///
/// Replacement interns new text rather than mutating a shared payload in place.
#[derive(Debug)]
pub struct InternedStringCellMut<'a> {
    value: StringSlotMut<'a>,
    interner: &'a Arc<Interner>,
}

impl InternedStringCellMut<'_> {
    /// Reads this cell without locking the interner.
    #[must_use]
    pub fn get(&self) -> Option<&str> {
        match &self.value {
            StringSlotMut::Required(value) => Some(value.as_str()),
            StringSlotMut::Nullable(value) => value.as_ref().map(|value| value.as_str()),
        }
    }

    /// Replaces the cell and releases its old pool reference.
    ///
    /// # Errors
    /// Returns [`InternedStringError::NullNotAllowed`] before any change if a
    /// required cell receives null. Empty strings are ordinary interned values.
    pub fn set(&mut self, text: Option<&str>) -> Result<(), InternedStringError> {
        let old = match &mut self.value {
            StringSlotMut::Required(value) => {
                let text = text.ok_or(InternedStringError::NullNotAllowed)?;
                Some(std::mem::replace(*value, self.interner.intern(text)))
            }
            StringSlotMut::Nullable(value) => {
                std::mem::replace(*value, text.map(|text| self.interner.intern(text)))
            }
        };
        drop(old);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_formatting_does_not_hold_the_interner_lock() {
        use std::fmt::Write;
        struct Probe<'a> {
            pool: &'a Interner,
            locked: bool,
        }
        impl Write for Probe<'_> {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                self.locked |= self.pool.entries.try_lock().is_err();
                Ok(())
            }
        }
        let mut data = InternedStringData::new(false, 1);
        data.push(Some("value")).unwrap();
        let mut probe = Probe {
            pool: &data.interner,
            locked: false,
        };
        write!(&mut probe, "{data:?}").unwrap();
        assert!(!probe.locked);
    }

    #[test]
    fn references_are_released_after_replacement_copy_pop_and_drop() {
        let mut data = InternedStringData::new(false, 3);
        data.push(Some("a")).unwrap();
        data.push(Some("a")).unwrap();
        data.push(Some("b")).unwrap();
        let pool = Arc::clone(&data.interner);
        let clone = data.clone();
        let selected = data.copy_rows(&[1]);
        assert_eq!(pool.entries().get("a").unwrap().strong_count(), 5);
        drop(clone);
        data.pop();
        assert!(!pool.entries().contains_key("b"));
        data.view_mut().set(0, Some("c")).unwrap();
        assert_eq!(pool.entries().get("a").unwrap().strong_count(), 2);
        drop(data);
        assert_eq!(pool.entries().len(), 1);
        assert_eq!(pool.entries().get("a").unwrap().strong_count(), 1);
        drop(selected);
        assert!(pool.entries().is_empty());
    }

    #[test]
    fn independent_pool_append_and_same_pool_transfer_leave_no_unused_entries() {
        let mut left = InternedStringData::new(true, 3);
        left.push(Some("a")).unwrap();
        let left_pool = Arc::clone(&left.interner);
        let mut right = InternedStringData::new(true, 3);
        right.push(Some("a")).unwrap();
        right.push(Some("b")).unwrap();
        right.push(None).unwrap();
        let right_pool = Arc::clone(&right.interner);
        left.append(right).unwrap();
        assert!(right_pool.entries().is_empty());
        assert_eq!(left_pool.entries().get("a").unwrap().strong_count(), 2);
        let same_pool = left.copy_range(0..2);
        left.append(same_pool).unwrap();
        assert_eq!(left_pool.entries().get("a").unwrap().strong_count(), 4);
        left.retain_live(&[true, true, true, true, false, false]);
        assert_eq!(left_pool.entries().get("a").unwrap().strong_count(), 2);
        assert!(!left_pool.entries().contains_key("b"));
        drop(left);
        assert!(left_pool.entries().is_empty());
    }

    #[test]
    fn rejected_append_drops_staged_pool_references() {
        use super::super::storage::ColumnStorage;
        use super::super::{ColumnSpec, Schema, Table};
        use crate::DataType;
        let mut source = Table::new(
            Schema::new([
                ColumnSpec::interned_string("text"),
                ColumnSpec::new("value", DataType::String),
            ])
            .unwrap(),
        );
        source
            .push_row(["shared".into(), "invalid integer".into()])
            .unwrap();
        let ColumnStorage::InternedString(data) = &source.columns[0] else {
            unreachable!()
        };
        let pool = Arc::clone(&data.interner);
        let mut target = Table::new(
            Schema::new([
                ColumnSpec::interned_string("text"),
                ColumnSpec::new("value", DataType::I64),
            ])
            .unwrap(),
        );
        for _ in 0..8 {
            assert!(target.append(&source).is_err());
        }
        assert_eq!(pool.entries().get("shared").unwrap().strong_count(), 1);
        assert_eq!(target.row_count(), 0);
    }

    #[test]
    fn all_copy_paths_complete_while_the_cache_is_locked() {
        use std::{sync::mpsc, thread, time::Duration};

        for nullable in [false, true] {
            let mut data = InternedStringData::new(nullable, 4);
            let long = "long shared payload ".repeat(512);
            data.push(Some(&long)).unwrap();
            data.push(Some(&long)).unwrap();
            data.push(Some("other")).unwrap();
            if nullable {
                data.push(None).unwrap();
            }
            let pool = Arc::clone(&data.interner);
            let cache = pool.entries();
            let (send, receive) = mpsc::channel();
            let worker = thread::spawn(move || {
                let full = data.clone();
                let range = data.copy_range(0..2);
                let rows = data.copy_rows(&[2, 0, 0]);
                send.send((data, full, range, rows)).unwrap();
            });
            // Copying must neither hash strings nor acquire the lookup-cache
            // mutex. Release it even on failure so a regression cannot hang CI.
            let result = receive.recv_timeout(Duration::from_secs(5));
            drop(cache);
            worker.join().unwrap();
            let (data, full, range, rows) = result.expect("copy accessed the locked cache");
            assert_eq!(full, data);
            assert_eq!(range.value(0).unwrap(), Some(long.as_str()));
            assert_eq!(rows.value(0).unwrap(), Some("other"));
            assert_eq!(rows.value(1).unwrap(), Some(long.as_str()));
            assert_eq!(rows.value(2).unwrap(), Some(long.as_str()));
            assert_eq!(pool.entries().get(long.as_str()).unwrap().strong_count(), 8);
            drop((data, full, range, rows));
            assert!(pool.entries().is_empty());
        }
    }

    #[test]
    fn last_cell_cleanup_does_not_remove_a_concurrent_replacement() {
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let pool = Arc::new(Interner::default());
        let old = pool.intern("shared");
        let old_weak = Arc::downgrade(&old);
        let mut cache = pool.entries();
        let worker = thread::spawn(move || drop(old));
        let start = Instant::now();
        while old_weak.strong_count() != 0 {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::yield_now();
        }
        // The destructor is waiting for this lock. Model an acquire that wins
        // it first: reuse the payload and publish a new entry under the same key.
        let text = Arc::clone(cache.get_key_value("shared").unwrap().0);
        let replacement = Arc::new(InternedValue {
            text,
            interner: Arc::downgrade(&pool),
        });
        cache.insert(Arc::clone(&replacement.text), Arc::downgrade(&replacement));
        drop(cache);
        worker.join().unwrap();
        let acquired = pool.intern("shared");
        assert!(Arc::ptr_eq(&replacement, &acquired));
        assert_eq!(pool.entries().len(), 1);
        drop((acquired, replacement));
        assert!(pool.entries().is_empty());
        assert!(old_weak.upgrade().is_none());
    }

    #[test]
    fn independent_pools_release_entries_even_when_the_payload_is_shared() {
        let left = Arc::new(Interner::default());
        let right = Arc::new(Interner::default());
        let left_value = left.intern("shared");
        let payload = Arc::downgrade(&left_value.text);
        let right_value = right.intern_shared(&left_value.text);
        assert!(Arc::ptr_eq(&left_value.text, &right_value.text));
        assert!(!Arc::ptr_eq(&left_value, &right_value));
        drop(left_value);
        assert!(left.entries().is_empty());
        assert_eq!(right.entries().len(), 1);
        drop(right_value);
        assert!(right.entries().is_empty());
        assert!(payload.upgrade().is_none());
        let weak_pool = Arc::downgrade(&right);
        drop(right);
        assert!(
            weak_pool.upgrade().is_none(),
            "entry/cache ownership formed a cycle"
        );
    }
}

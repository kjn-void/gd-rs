//! Rust counterpart of `../cpp/index_sort_benchmark.cpp`: the six GD "sort table"
//! cases over a struct-of-arrays table whose string columns hold
//! `strumbra::SharedString`.

use std::cmp::Ordering;
use std::mem::ManuallyDrop;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use strumbra::SharedString;

/// Row count in the C++ benchmark.
pub const ROW_COUNT: usize = 1000;

/// Column numbers as in the C++ table: C1 `uint64`, C2 `string(8)`, C7 `string(50)`.
pub const C1: usize = 0;
pub const C2: usize = 1;
pub const C7: usize = 6;

/// Generated values for one row; C4 to C6 are never set, like the C++ test.
pub struct SourceRow {
    pub c1: u64,
    pub c2: String,
    pub c3: f64,
    pub c7: String,
}

/// Same distributions and draw order as the C++ test. The engine differs, so the
/// values do too.
pub fn generate_rows(count: usize) -> Vec<SourceRow> {
    let mut rng = StdRng::seed_from_u64(42);
    let letters = |rng: &mut StdRng, count: usize| -> String {
        (0..count)
            .map(|_| char::from(rng.random_range(b'a'..=b'z')))
            .collect()
    };

    (0..count)
        .map(|_| {
            let c2 = letters(&mut rng, 8);
            let suffix = letters(&mut rng, 25);
            let c7 = format!("01234567890123456789{suffix}");
            let c1 = rng.random_range(0..=1_000_000u64);
            let c3 = rng.random_range(0.0..1_000.0);
            SourceRow { c1, c2, c3, c7 }
        })
        .collect()
}

/// Struct of arrays: one `Vec` per column. Strings up to 12 bytes are stored
/// inline; longer ones share an `Arc` buffer, so cloning a row never copies text.
#[derive(Clone)]
pub struct Table {
    pub c1: Vec<u64>,
    pub c2: Vec<SharedString>,
    pub c3: Vec<f64>,
    pub c4: Vec<Option<f64>>,
    pub c5: Vec<Option<i32>>,
    pub c6: Vec<Option<u32>>,
    pub c7: Vec<SharedString>,
}

impl Table {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            c1: Vec::with_capacity(capacity),
            c2: Vec::with_capacity(capacity),
            c3: Vec::with_capacity(capacity),
            c4: Vec::with_capacity(capacity),
            c5: Vec::with_capacity(capacity),
            c6: Vec::with_capacity(capacity),
            c7: Vec::with_capacity(capacity),
        }
    }

    pub fn from_source(source: &[SourceRow]) -> Self {
        let string = |value: &str| SharedString::try_from(value).expect("string fits strumbra");
        let mut table = Self::with_capacity(source.len());
        for row in source {
            table.c1.push(row.c1);
            table.c2.push(string(&row.c2));
            table.c3.push(row.c3);
            table.c4.push(None);
            table.c5.push(None);
            table.c6.push(None);
            table.c7.push(string(&row.c7));
        }
        table
    }

    pub fn len(&self) -> usize {
        self.c1.len()
    }

    pub fn is_empty(&self) -> bool {
        self.c1.is_empty()
    }

    /// Replaces the rows with clones of `source`'s rows, like GD's
    /// `row_clear()` followed by `append(source)`.
    pub fn append_from(&mut self, source: &Self) {
        fn copy<T: Clone>(target: &mut Vec<T>, source: &[T]) {
            target.clear();
            target.extend_from_slice(source);
        }
        copy(&mut self.c1, &source.c1);
        copy(&mut self.c2, &source.c2);
        copy(&mut self.c3, &source.c3);
        copy(&mut self.c4, &source.c4);
        copy(&mut self.c5, &source.c5);
        copy(&mut self.c6, &source.c6);
        copy(&mut self.c7, &source.c7);
    }

    #[inline]
    pub fn swap_rows(&mut self, a: usize, b: usize) {
        self.c1.swap(a, b);
        self.c2.swap(a, b);
        self.c3.swap(a, b);
        self.c4.swap(a, b);
        self.c5.swap(a, b);
        self.c6.swap(a, b);
        self.c7.swap(a, b);
    }

    #[inline]
    pub fn compare_rows<const COLUMN: usize>(&self, a: usize, b: usize) -> Ordering {
        match COLUMN {
            C1 => self.c1[a].cmp(&self.c1[b]),
            C2 => self.c2[a].cmp(&self.c2[b]),
            C7 => self.c7[a].cmp(&self.c7[b]),
            _ => unreachable!("column {COLUMN} is not a sort key"),
        }
    }
}

/// Cases 01 to 03: physical sort. Quicksort that compares and swaps whole rows in
/// place. GD's default `sort(column, true)` is a selection sort instead.
pub fn physical_sort<const COLUMN: usize>(table: &mut Table) {
    let count = table.len();
    quicksort(&mut Rows::<COLUMN>(table), 0, count);
}

/// Cases 04 to 06: index sort, after GD's `sort(column, true, tag_sort_index{})`. The
/// same quicksort as [`physical_sort`] orders a vector of row numbers. The sorted
/// order is then applied by building a new table: each new column is filled by
/// moving the old column's values in sorted order, and each old column is freed as
/// soon as it has been moved. GD instead swaps rows in place along permutation
/// cycles; at 10,000 rows, building the new table took 5 to 34% less time.
pub fn index_sort<const COLUMN: usize>(table: &mut Table) {
    if table.len() < 2 {
        return;
    }

    let index = sorted_index::<COLUMN>(table);
    let old = std::mem::replace(table, Table::with_capacity(0));
    *table = Table {
        c1: gather(old.c1, &index),
        c2: gather(old.c2, &index),
        c3: gather(old.c3, &index),
        c4: gather(old.c4, &index),
        c5: gather(old.c5, &index),
        c6: gather(old.c6, &index),
        c7: gather(old.c7, &index),
    };
}

/// Row numbers in sorted order: `index[k]` is the row that belongs at position `k`.
fn sorted_index<const COLUMN: usize>(table: &Table) -> Vec<usize> {
    let count = table.len();
    let mut index: Vec<usize> = (0..count).collect();
    quicksort(
        &mut RowNumbers::<COLUMN> {
            table,
            index: &mut index,
        },
        0,
        count,
    );
    index
}

/// Moves `column`'s values into a new vector in `index` order. `index` must be a
/// permutation of the column's row numbers.
fn gather<T>(column: Vec<T>, index: &[usize]) -> Vec<T> {
    assert_eq!(column.len(), index.len());
    let mut column = ManuallyDrop::new(column);
    let sorted = index
        .iter()
        // SAFETY: `index` is a permutation, so each value is read exactly once.
        // `column` never drops its values, so none is dropped twice, even if
        // indexing panics.
        .map(|&row| unsafe { std::ptr::read(&column[row]) })
        .collect();
    // SAFETY: every value has been moved out; this frees only the buffer.
    unsafe {
        column.set_len(0);
        ManuallyDrop::drop(&mut column);
    }
    sorted
}

/// What the quicksort orders: positions it can compare and swap.
trait Sequence {
    fn compare(&self, a: usize, b: usize) -> Ordering;
    fn swap(&mut self, a: usize, b: usize);
}

/// The table's rows themselves.
struct Rows<'a, const COLUMN: usize>(&'a mut Table);

impl<const COLUMN: usize> Sequence for Rows<'_, COLUMN> {
    #[inline]
    fn compare(&self, a: usize, b: usize) -> Ordering {
        self.0.compare_rows::<COLUMN>(a, b)
    }

    #[inline]
    fn swap(&mut self, a: usize, b: usize) {
        self.0.swap_rows(a, b);
    }
}

/// Row numbers, compared through the rows they name.
struct RowNumbers<'a, const COLUMN: usize> {
    table: &'a Table,
    index: &'a mut [usize],
}

impl<const COLUMN: usize> Sequence for RowNumbers<'_, COLUMN> {
    #[inline]
    fn compare(&self, a: usize, b: usize) -> Ordering {
        self.table
            .compare_rows::<COLUMN>(self.index[a], self.index[b])
    }

    #[inline]
    fn swap(&mut self, a: usize, b: usize) {
        self.index.swap(a, b);
    }
}

/// Ranges this short are finished with insertion sort.
const INSERTION_SORT_LENGTH: usize = 16;

/// Sorts positions `[from, to)`. The pivot is the median of the first, middle, and
/// last positions. Partitioning stops on keys equal to the pivot, so duplicate keys
/// still split evenly. The sort recurses into the smaller part only and uses
/// insertion sort for short ranges. It is not stable.
fn quicksort(sequence: &mut impl Sequence, mut from: usize, mut to: usize) {
    while to - from > INSERTION_SORT_LENGTH {
        let pivot = partition(sequence, from, to);
        if pivot - from < to - pivot {
            quicksort(sequence, from, pivot);
            from = pivot + 1;
        } else {
            quicksort(sequence, pivot + 1, to);
            to = pivot;
        }
    }
    insertion_sort(sequence, from, to);
}

/// Partitions `[from, to)` around a median-of-three pivot and returns the pivot's
/// final position. Needs at least three positions.
fn partition(sequence: &mut impl Sequence, from: usize, to: usize) -> usize {
    let middle = from + (to - from) / 2;
    let last = to - 1;
    if sequence.compare(middle, from).is_lt() {
        sequence.swap(middle, from);
    }
    if sequence.compare(last, from).is_lt() {
        sequence.swap(last, from);
    }
    if sequence.compare(last, middle).is_lt() {
        sequence.swap(last, middle);
    }
    // The pivot moves to `from`; the smaller and larger samples bound both scans.
    sequence.swap(from, middle);

    let (mut left, mut right) = (from, to);
    loop {
        left += 1;
        while sequence.compare(left, from).is_lt() {
            left += 1;
        }
        right -= 1;
        while sequence.compare(from, right).is_lt() {
            right -= 1;
        }
        if left >= right {
            break;
        }
        sequence.swap(left, right);
    }
    sequence.swap(from, right);
    right
}

/// Sorts `[from, to)` by swapping adjacent positions.
fn insertion_sort(sequence: &mut impl Sequence, from: usize, to: usize) {
    for next in from + 1..to {
        let mut position = next;
        while position > from && sequence.compare(position, position - 1).is_lt() {
            sequence.swap(position, position - 1);
            position -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_sorted<const COLUMN: usize>(table: &Table) -> bool {
        (1..table.len()).all(|row| table.compare_rows::<COLUMN>(row - 1, row).is_le())
    }

    fn check_column<const COLUMN: usize>(source: &Table) {
        let mut physical = source.clone();
        physical_sort::<COLUMN>(&mut physical);
        assert!(is_sorted::<COLUMN>(&physical));

        let mut index = source.clone();
        index_sort::<COLUMN>(&mut index);
        assert!(is_sorted::<COLUMN>(&index));
        assert_eq!(index.len(), source.len());
    }

    #[test]
    fn both_sorts_order_every_key_column() {
        let table = Table::from_source(&generate_rows(ROW_COUNT));
        check_column::<C1>(&table);
        check_column::<C2>(&table);
        check_column::<C7>(&table);
    }

    #[test]
    fn both_sorts_handle_duplicates_and_short_tables() {
        let mut rows = generate_rows(ROW_COUNT);
        for row in &mut rows {
            row.c1 %= 3;
        }
        for count in [0, 1, 2, 3, 16, 17, 100, ROW_COUNT] {
            check_column::<C1>(&Table::from_source(&rows[..count]));
        }
    }

    #[test]
    fn both_sorts_produce_the_same_rows() {
        let source = Table::from_source(&generate_rows(ROW_COUNT));
        let rows = |table: &Table| -> Vec<(u64, String, String)> {
            (0..table.len())
                .map(|row| {
                    let (c2, c7) = (&table.c2[row], &table.c7[row]);
                    (table.c1[row], c2.to_string(), c7.to_string())
                })
                .collect()
        };

        let mut physical = source.clone();
        physical_sort::<C7>(&mut physical);
        let mut index = source.clone();
        index_sort::<C7>(&mut index);
        assert_eq!(rows(&physical), rows(&index));

        let mut original = rows(&source);
        original.sort();
        let mut sorted = rows(&index);
        sorted.sort();
        assert_eq!(sorted, original, "rows were split or lost");
    }

    #[test]
    fn strings_match_the_benchmark_distribution() {
        for row in generate_rows(ROW_COUNT) {
            assert_eq!(row.c2.len(), 8);
            assert_eq!(row.c7.len(), 45);
            assert!(row.c7.starts_with("01234567890123456789"));
            assert!(
                row.c2
                    .bytes()
                    .chain(row.c7.bytes().skip(20))
                    .all(|b| b.is_ascii_lowercase())
            );
        }
    }
}

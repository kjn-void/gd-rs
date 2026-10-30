//! Rust counterpart of `../cpp/index_sort_benchmark.cpp` and
//! `../cpp/sort_methods_benchmark.cpp`: GD's "sort table" cases over a
//! struct-of-arrays table whose string columns hold `strumbra::SharedString`.

use std::cmp::Ordering;
use std::mem::ManuallyDrop;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use strumbra::SharedString;

/// Row count in the C++ six-case benchmark.
pub const ROW_COUNT: usize = 1000;

/// Row count in the C++ three-sort-method benchmark.
pub const SORT_METHODS_ROW_COUNT: usize = 10_000;

/// Column numbers as in the C++ table: C1 `uint64`, C2 `string(8)`, C3 and C4
/// `double`, C5 `int32`, C6 `uint32`, C7 `string(50)`.
pub const C1: usize = 0;
pub const C2: usize = 1;
pub const C3: usize = 2;
pub const C4: usize = 3;
pub const C5: usize = 4;
pub const C6: usize = 5;
pub const C7: usize = 6;

/// Generated values for one row. The six-case benchmark never sets C4 to C6.
pub struct SourceRow {
    pub c1: u64,
    pub c2: String,
    pub c3: f64,
    pub c4: Option<f64>,
    pub c5: Option<i32>,
    pub c6: Option<u32>,
    pub c7: String,
}

/// Rows for the six-case benchmark, with C4 to C6 null. Same distributions and
/// draw order as the C++ test. The engine differs, so the values do too.
pub fn generate_rows(count: usize) -> Vec<SourceRow> {
    generate(count, false)
}

/// Rows for the three-sort-method benchmark, with every column set: C4 a double
/// like C3, and C5 and C6 a draw like C1 modulo 1000.
pub fn generate_all_columns(count: usize) -> Vec<SourceRow> {
    generate(count, true)
}

fn generate(count: usize, all_columns: bool) -> Vec<SourceRow> {
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
            let (mut c4, mut c5, mut c6) = (None, None, None);
            if all_columns {
                c4 = Some(rng.random_range(0.0..1_000.0));
                c5 = Some((rng.random_range(0..=1_000_000u64) % 1000) as i32);
                c6 = Some((rng.random_range(0..=1_000_000u64) % 1000) as u32);
            }
            SourceRow {
                c1,
                c2,
                c3,
                c4,
                c5,
                c6,
                c7,
            }
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
            table.c4.push(row.c4);
            table.c5.push(row.c5);
            table.c6.push(row.c6);
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
            C3 => self.c3[a].total_cmp(&self.c3[b]),
            C4 => compare_set(self.c4[a], self.c4[b], f64::total_cmp),
            C5 => compare_set(self.c5[a], self.c5[b], i32::cmp),
            C6 => compare_set(self.c6[a], self.c6[b], u32::cmp),
            C7 => self.c7[a].cmp(&self.c7[b]),
            _ => unreachable!("table has no column {COLUMN}"),
        }
    }
}

/// Compares two values of a nullable column used as a sort key. Like GD's sorts, key
/// values must not be null.
#[inline]
fn compare_set<T: Copy>(
    a: Option<T>,
    b: Option<T>,
    compare: impl Fn(&T, &T) -> Ordering,
) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => compare(&a, &b),
        _ => panic!("sort key is null"),
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
    gather_table(table, &index);
}

/// Key sort, after GD's posted `sort(column, true, tag_sort_key{})`: each row's key
/// is read once into a vector of (key, row number) pairs, the same quicksort orders
/// the pairs, and the sorted order is applied by building a new table as in
/// [`index_sort`]. Numbers are copied into the pairs; strings are referenced, as
/// GD's cached `variant_view` points into the row. All three sorts produce the
/// same row order.
pub fn key_sort<const COLUMN: usize>(table: &mut Table) {
    if table.len() < 2 {
        return;
    }

    let index = match COLUMN {
        C1 => sorted_keys(table.c1.iter().copied(), u64::cmp),
        C2 => sorted_keys(table.c2.iter(), |a, b| a.cmp(b)),
        C3 => sorted_keys(table.c3.iter().copied(), f64::total_cmp),
        C4 => sorted_keys(
            table.c4.iter().map(|v| v.expect("sort key is null")),
            f64::total_cmp,
        ),
        C5 => sorted_keys(
            table.c5.iter().map(|v| v.expect("sort key is null")),
            i32::cmp,
        ),
        C6 => sorted_keys(
            table.c6.iter().map(|v| v.expect("sort key is null")),
            u32::cmp,
        ),
        C7 => sorted_keys(table.c7.iter(), |a, b| a.cmp(b)),
        _ => unreachable!("table has no column {COLUMN}"),
    };
    gather_table(table, &index);
}

/// Sorts (key, row number) pairs by key and returns the row numbers in that order.
fn sorted_keys<K: Copy>(
    keys: impl Iterator<Item = K>,
    compare: impl Fn(&K, &K) -> Ordering,
) -> Vec<usize> {
    let mut pairs = Keys {
        pairs: keys.enumerate().map(|(row, key)| (key, row)).collect(),
        compare,
    };
    let count = pairs.pairs.len();
    quicksort(&mut pairs, 0, count);
    pairs.pairs.into_iter().map(|(_, row)| row).collect()
}

/// Replaces `table` with a new table whose row `k` is the old row `index[k]`.
fn gather_table(table: &mut Table, index: &[usize]) {
    let old = std::mem::replace(table, Table::with_capacity(0));
    *table = Table {
        c1: gather(old.c1, index),
        c2: gather(old.c2, index),
        c3: gather(old.c3, index),
        c4: gather(old.c4, index),
        c5: gather(old.c5, index),
        c6: gather(old.c6, index),
        c7: gather(old.c7, index),
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

/// (key, row number) pairs, compared by key.
struct Keys<K, F> {
    pairs: Vec<(K, usize)>,
    compare: F,
}

impl<K, F: Fn(&K, &K) -> Ordering> Sequence for Keys<K, F> {
    #[inline]
    fn compare(&self, a: usize, b: usize) -> Ordering {
        (self.compare)(&self.pairs[a].0, &self.pairs[b].0)
    }

    #[inline]
    fn swap(&mut self, a: usize, b: usize) {
        self.pairs.swap(a, b);
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

    /// Every column of an all-columns table, as comparable text, for row-by-row checks.
    fn all_columns(table: &Table) -> Vec<String> {
        (0..table.len())
            .map(|row| {
                format!(
                    "{} {} {} {:?} {:?} {:?} {}",
                    table.c1[row],
                    table.c2[row],
                    table.c3[row],
                    table.c4[row],
                    table.c5[row],
                    table.c6[row],
                    table.c7[row]
                )
            })
            .collect()
    }

    fn check_three_methods<const COLUMN: usize>(source: &Table) {
        let mut physical = source.clone();
        physical_sort::<COLUMN>(&mut physical);
        assert!(is_sorted::<COLUMN>(&physical), "physical, column {COLUMN}");

        let mut index = source.clone();
        index_sort::<COLUMN>(&mut index);
        assert!(is_sorted::<COLUMN>(&index), "index, column {COLUMN}");

        let mut key = source.clone();
        key_sort::<COLUMN>(&mut key);
        assert!(is_sorted::<COLUMN>(&key), "key, column {COLUMN}");

        let rows = all_columns(&physical);
        assert_eq!(rows, all_columns(&index), "column {COLUMN}");
        assert_eq!(rows, all_columns(&key), "column {COLUMN}");
    }

    #[test]
    fn three_methods_order_every_column_the_same_way() {
        let source = Table::from_source(&generate_all_columns(SORT_METHODS_ROW_COUNT));
        check_three_methods::<C1>(&source);
        check_three_methods::<C2>(&source);
        check_three_methods::<C3>(&source);
        check_three_methods::<C4>(&source);
        check_three_methods::<C5>(&source);
        check_three_methods::<C6>(&source);
        check_three_methods::<C7>(&source);

        let mut original = all_columns(&source);
        original.sort();
        let mut key = source.clone();
        key_sort::<C5>(&mut key);
        let mut sorted = all_columns(&key);
        sorted.sort();
        assert_eq!(sorted, original, "rows were split or lost");
    }

    #[test]
    fn key_sort_handles_duplicates_and_short_tables() {
        let rows = generate_all_columns(ROW_COUNT);
        for count in [0, 1, 2, 3, 16, 17, 100, ROW_COUNT] {
            // C5 and C6 hold only 1,000 distinct values, and C1 is reduced to three.
            let mut source = Table::from_source(&rows[..count]);
            for value in &mut source.c1 {
                *value %= 3;
            }
            check_three_methods::<C1>(&source);
            check_three_methods::<C6>(&source);
        }
    }

    #[test]
    fn all_columns_are_set() {
        for row in generate_all_columns(ROW_COUNT) {
            assert!(row.c4.is_some_and(|v| (0.0..1_000.0).contains(&v)));
            assert!(row.c5.is_some_and(|v| (0..1000).contains(&v)));
            assert!(row.c6.is_some_and(|v| v < 1000));
        }
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

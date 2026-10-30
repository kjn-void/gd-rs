//! Rust counterpart of `../cpp/sort_methods_benchmark.cpp`: sorting GD's
//! seven-column table on each column, over a struct-of-arrays table whose string
//! columns hold `strumbra::SharedString`.
//!
//! The port has one sort, [`key_sort`], the fastest of the physical, index, and key
//! sorts measured at commit 906a410.

use std::cmp::Ordering;
use std::mem::ManuallyDrop;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use strumbra::SharedString;

/// Row count in the C++ benchmark.
pub const ROW_COUNT: usize = 10_000;

/// Column numbers as in the C++ table: C1 `uint64`, C2 `string(8)`, C3 and C4
/// `double`, C5 `int32`, C6 `uint32`, C7 `string(50)`.
pub const C1: usize = 0;
pub const C2: usize = 1;
pub const C3: usize = 2;
pub const C4: usize = 3;
pub const C5: usize = 4;
pub const C6: usize = 5;
pub const C7: usize = 6;

/// Generated values for one row.
pub struct SourceRow {
    pub c1: u64,
    pub c2: String,
    pub c3: f64,
    pub c4: f64,
    pub c5: i32,
    pub c6: u32,
    pub c7: String,
}

/// Same distributions and draw order as the C++ test, with every column set: C4 a
/// double like C3, and C5 and C6 a draw like C1 modulo 1000. The engine differs, so
/// the values do too.
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
            let c4 = rng.random_range(0.0..1_000.0);
            let c5 = (rng.random_range(0..=1_000_000u64) % 1000) as i32;
            let c6 = (rng.random_range(0..=1_000_000u64) % 1000) as u32;
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

/// Struct of arrays: one `Vec` per column. C4 to C6 are nullable, like GD's.
/// Strings up to 12 bytes are stored inline; longer ones share an `Arc` buffer, so
/// cloning a row never copies text.
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
            table.c4.push(Some(row.c4));
            table.c5.push(Some(row.c5));
            table.c6.push(Some(row.c6));
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

    /// Compares two rows on one column, in the order [`key_sort`] sorts by.
    pub fn compare_rows<const COLUMN: usize>(&self, a: usize, b: usize) -> Ordering {
        match COLUMN {
            C1 => self.c1[a].cmp(&self.c1[b]),
            C2 => self.c2[a].cmp(&self.c2[b]),
            C3 => self.c3[a].total_cmp(&self.c3[b]),
            C4 => f64::total_cmp(&key(&self.c4[a]), &key(&self.c4[b])),
            C5 => key(&self.c5[a]).cmp(&key(&self.c5[b])),
            C6 => key(&self.c6[a]).cmp(&key(&self.c6[b])),
            C7 => self.c7[a].cmp(&self.c7[b]),
            _ => unreachable!("table has no column {COLUMN}"),
        }
    }
}

/// Sorts `table` on `COLUMN`, after GD's posted `sort(column, true, tag_sort_key{})`.
///
/// Each row's key is read once into a vector of (key, row number) pairs, and a
/// quicksort orders the pairs. Numbers are copied into the pairs; strings are
/// referenced, as GD's cached `variant_view` points into the row. The sorted order
/// is then applied by building a new table: each new column is filled by moving the
/// old column's values in sorted order, and each old column is freed as soon as it
/// has been moved. Like GD's sorts, key values must not be null. The sort is not
/// stable.
pub fn key_sort<const COLUMN: usize>(table: &mut Table) {
    if table.len() < 2 {
        return;
    }

    let index = match COLUMN {
        C1 => sorted_rows(table.c1.iter().copied(), u64::cmp),
        C2 => sorted_rows(table.c2.iter(), |a, b| a.cmp(b)),
        C3 => sorted_rows(table.c3.iter().copied(), f64::total_cmp),
        C4 => sorted_rows(table.c4.iter().map(key), f64::total_cmp),
        C5 => sorted_rows(table.c5.iter().map(key), i32::cmp),
        C6 => sorted_rows(table.c6.iter().map(key), u32::cmp),
        C7 => sorted_rows(table.c7.iter(), |a, b| a.cmp(b)),
        _ => unreachable!("table has no column {COLUMN}"),
    };

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

/// The value of a nullable column used as a sort key. Like GD's sorts, key values
/// must not be null.
#[inline]
fn key<T: Copy>(value: &Option<T>) -> T {
    value.expect("sort key is null")
}

/// Sorts (key, row number) pairs by key and returns the row numbers in that order:
/// `index[k]` is the row that belongs at position `k`.
fn sorted_rows<K: Copy>(
    keys: impl Iterator<Item = K>,
    compare: impl Fn(&K, &K) -> Ordering,
) -> Vec<usize> {
    let mut pairs = Keys {
        pairs: keys.enumerate().map(|(row, key)| (key, row)).collect(),
        compare,
    };
    let count = pairs.pairs.len();
    pairs.quicksort(0, count);
    pairs.pairs.into_iter().map(|(_, row)| row).collect()
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

/// (key, row number) pairs and the key order.
struct Keys<K, F> {
    pairs: Vec<(K, usize)>,
    compare: F,
}

/// Ranges this short are finished with insertion sort.
const INSERTION_SORT_LENGTH: usize = 16;

impl<K, F: Fn(&K, &K) -> Ordering> Keys<K, F> {
    #[inline]
    fn less(&self, a: usize, b: usize) -> bool {
        (self.compare)(&self.pairs[a].0, &self.pairs[b].0).is_lt()
    }

    /// Sorts positions `[from, to)`. The pivot is the median of the first, middle,
    /// and last positions. Partitioning stops on keys equal to the pivot, so
    /// duplicate keys still split evenly. The sort recurses into the smaller part
    /// only and uses insertion sort for short ranges.
    fn quicksort(&mut self, mut from: usize, mut to: usize) {
        while to - from > INSERTION_SORT_LENGTH {
            let pivot = self.partition(from, to);
            if pivot - from < to - pivot {
                self.quicksort(from, pivot);
                from = pivot + 1;
            } else {
                self.quicksort(pivot + 1, to);
                to = pivot;
            }
        }
        self.insertion_sort(from, to);
    }

    /// Partitions `[from, to)` around a median-of-three pivot and returns the
    /// pivot's final position. Needs at least three positions.
    fn partition(&mut self, from: usize, to: usize) -> usize {
        let middle = from + (to - from) / 2;
        let last = to - 1;
        if self.less(middle, from) {
            self.pairs.swap(middle, from);
        }
        if self.less(last, from) {
            self.pairs.swap(last, from);
        }
        if self.less(last, middle) {
            self.pairs.swap(last, middle);
        }
        // The pivot moves to `from`; the smaller and larger samples bound both scans.
        self.pairs.swap(from, middle);

        let (mut left, mut right) = (from, to);
        loop {
            left += 1;
            while self.less(left, from) {
                left += 1;
            }
            right -= 1;
            while self.less(from, right) {
                right -= 1;
            }
            if left >= right {
                break;
            }
            self.pairs.swap(left, right);
        }
        self.pairs.swap(from, right);
        right
    }

    /// Sorts `[from, to)` by swapping adjacent positions.
    fn insertion_sort(&mut self, from: usize, to: usize) {
        for next in from + 1..to {
            let mut position = next;
            while position > from && self.less(position, position - 1) {
                self.pairs.swap(position, position - 1);
                position -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every column of a table, as comparable text, for row-by-row checks.
    fn rows(table: &Table) -> Vec<String> {
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

    fn check_column<const COLUMN: usize>(source: &Table) {
        let mut sorted = source.clone();
        key_sort::<COLUMN>(&mut sorted);
        assert!(
            (1..sorted.len()).all(|row| sorted.compare_rows::<COLUMN>(row - 1, row).is_le()),
            "column {COLUMN} is not sorted"
        );

        let (mut before, mut after) = (rows(source), rows(&sorted));
        before.sort();
        after.sort();
        assert_eq!(after, before, "column {COLUMN} split or lost rows");
    }

    fn check_every_column(source: &Table) {
        check_column::<C1>(source);
        check_column::<C2>(source);
        check_column::<C3>(source);
        check_column::<C4>(source);
        check_column::<C5>(source);
        check_column::<C6>(source);
        check_column::<C7>(source);
    }

    #[test]
    fn key_sort_orders_every_column() {
        check_every_column(&Table::from_source(&generate_rows(ROW_COUNT)));
    }

    #[test]
    fn key_sort_handles_duplicates_and_short_tables() {
        let rows = generate_rows(1000);
        for count in [0, 1, 2, 3, 16, 17, 100, 1000] {
            // C5 and C6 hold only 1,000 distinct values, and C1 is reduced to three.
            let mut source = Table::from_source(&rows[..count]);
            for value in &mut source.c1 {
                *value %= 3;
            }
            check_every_column(&source);
        }
    }

    #[test]
    fn rows_match_the_benchmark_distribution() {
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
            assert!((0.0..1_000.0).contains(&row.c4));
            assert!((0..1000).contains(&row.c5));
            assert!(row.c6 < 1000);
        }
    }
}

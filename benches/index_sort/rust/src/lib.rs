//! Rust counterpart of `../cpp/index_sort_benchmark.cpp`: the six GD "sort table"
//! cases over a struct-of-arrays table whose string columns hold
//! `strumbra::SharedString`.

use std::cmp::Ordering;

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

/// Cases 01 to 03: selection sort, like GD's default `sort(column, true)`.
pub fn selection_sort<const COLUMN: usize>(table: &mut Table) {
    let count = table.len();
    for row in 0..count {
        let mut minimum = row;
        for candidate in row + 1..count {
            if table.compare_rows::<COLUMN>(candidate, minimum).is_lt() {
                minimum = candidate;
            }
        }
        if minimum != row {
            table.swap_rows(minimum, row);
        }
    }
}

/// Cases 04 to 06: stable sort of row numbers, then apply the permutation in place
/// by following cycles, like GD's `sort(column, true, tag_sort_index{})`.
pub fn index_sort<const COLUMN: usize>(table: &mut Table) {
    let count = table.len();
    if count < 2 {
        return;
    }

    let mut index: Vec<usize> = (0..count).collect();
    index.sort_by(|&left, &right| table.compare_rows::<COLUMN>(left, right));

    // index[k] is the original row that should end up at position k.
    for position in 0..count {
        let mut source = index[position];
        while source < position {
            source = index[source]; // follow rows that were already moved
        }
        if source != position {
            table.swap_rows(position, source);
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
        let mut selection = source.clone();
        selection_sort::<COLUMN>(&mut selection);
        assert!(is_sorted::<COLUMN>(&selection));

        let mut index = source.clone();
        index_sort::<COLUMN>(&mut index);
        assert!(is_sorted::<COLUMN>(&index));
    }

    #[test]
    fn both_sorts_order_every_key_column() {
        let table = Table::from_source(&generate_rows(ROW_COUNT));
        check_column::<C1>(&table);
        check_column::<C2>(&table);
        check_column::<C7>(&table);
    }

    #[test]
    fn index_sort_matches_stable_sort_and_keeps_rows_whole() {
        let rows = generate_rows(ROW_COUNT);
        let mut expected: Vec<(String, u64)> =
            rows.iter().map(|row| (row.c7.clone(), row.c1)).collect();
        expected.sort_by(|a, b| a.0.cmp(&b.0));

        let mut table = Table::from_source(&rows);
        index_sort::<C7>(&mut table);
        let actual: Vec<(String, u64)> = table
            .c7
            .iter()
            .zip(&table.c1)
            .map(|(c7, &c1)| (c7.to_string(), c1))
            .collect();
        assert_eq!(actual, expected);
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

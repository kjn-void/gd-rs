//! Reusable index, numeric key, and destination buffers. No allocation is needed
//! after capacity has been established. Null validation is inside the timed sort.

use super::{C1, C2, C3, C4, C5, C6, C7, Table, key};
use std::cmp::Ordering;
use strumbra::SharedString;

pub struct Workspace {
    index: Vec<usize>,
    u64_keys: Vec<(u64, usize)>,
    f64_keys: Vec<(f64, usize)>,
    i32_keys: Vec<(i32, usize)>,
    u32_keys: Vec<(u32, usize)>,
    spare: Table,
}

impl Workspace {
    pub fn new(rows: usize) -> Self {
        Self {
            index: Vec::with_capacity(rows),
            u64_keys: Vec::with_capacity(rows),
            f64_keys: Vec::with_capacity(rows),
            i32_keys: Vec::with_capacity(rows),
            u32_keys: Vec::with_capacity(rows),
            spare: Table::with_capacity(rows),
        }
    }

    pub fn sort<const COLUMN: usize>(&mut self, table: &mut Table, cache_keys: bool) {
        self.index.clear();
        if cache_keys {
            match COLUMN {
                C1 => cached(
                    table.c1.iter().copied(),
                    &mut self.u64_keys,
                    &mut self.index,
                    u64::cmp,
                ),
                C3 => cached(
                    table.c3.iter().copied(),
                    &mut self.f64_keys,
                    &mut self.index,
                    f64::total_cmp,
                ),
                C4 => cached(
                    table.c4.iter().map(key),
                    &mut self.f64_keys,
                    &mut self.index,
                    f64::total_cmp,
                ),
                C5 => cached(
                    table.c5.iter().map(key),
                    &mut self.i32_keys,
                    &mut self.index,
                    i32::cmp,
                ),
                C6 => cached(
                    table.c6.iter().map(key),
                    &mut self.u32_keys,
                    &mut self.index,
                    u32::cmp,
                ),
                C2 | C7 => self.sort_index::<COLUMN>(table),
                _ => unreachable!(),
            }
        } else {
            self.sort_index::<COLUMN>(table);
        }
        copy_column(&mut table.c1, &mut self.spare.c1, &self.index);
        move_strings(&mut table.c2, &mut self.spare.c2, &self.index);
        copy_column(&mut table.c3, &mut self.spare.c3, &self.index);
        copy_column(&mut table.c4, &mut self.spare.c4, &self.index);
        copy_column(&mut table.c5, &mut self.spare.c5, &self.index);
        copy_column(&mut table.c6, &mut self.spare.c6, &self.index);
        move_strings(&mut table.c7, &mut self.spare.c7, &self.index);
    }

    fn sort_index<const COLUMN: usize>(&mut self, table: &Table) {
        // Validate before sorting so the comparator can read nullable payloads
        // without repeating their null checks. The table is borrowed immutably.
        match COLUMN {
            C4 => assert!(table.c4.iter().all(Option::is_some), "sort key is null"),
            C5 => assert!(table.c5.iter().all(Option::is_some), "sort key is null"),
            C6 => assert!(table.c6.iter().all(Option::is_some), "sort key is null"),
            _ => (),
        }
        self.index.extend(0..table.len());
        self.index.sort_unstable_by(|&a, &b| match COLUMN {
            C4 => table.c4[a]
                .unwrap_or_default()
                .total_cmp(&table.c4[b].unwrap_or_default()),
            C5 => table.c5[a]
                .unwrap_or_default()
                .cmp(&table.c5[b].unwrap_or_default()),
            C6 => table.c6[a]
                .unwrap_or_default()
                .cmp(&table.c6[b].unwrap_or_default()),
            _ => table.compare_rows::<COLUMN>(a, b),
        });
    }
}

fn cached<K: Copy>(
    keys: impl Iterator<Item = K>,
    pairs: &mut Vec<(K, usize)>,
    index: &mut Vec<usize>,
    compare: impl Fn(&K, &K) -> Ordering,
) {
    pairs.clear();
    pairs.extend(keys.enumerate().map(|(row, key)| (key, row)));
    pairs.sort_unstable_by(|a, b| compare(&a.0, &b.0));
    index.extend(pairs.iter().map(|&(_, row)| row));
}

fn copy_column<T: Copy>(column: &mut Vec<T>, spare: &mut Vec<T>, index: &[usize]) {
    spare.clear();
    spare.extend(index.iter().map(|&row| column[row]));
    std::mem::swap(column, spare);
}

fn move_strings(column: &mut Vec<SharedString>, spare: &mut Vec<SharedString>, index: &[usize]) {
    spare.clear();
    // Taking each descriptor leaves an empty inline string behind. This moves
    // ownership safely without another Arc increment or new unsafe code.
    spare.extend(
        index
            .iter()
            .map(|&row| std::mem::replace(&mut column[row], SharedString::try_from("").unwrap())),
    );
    std::mem::swap(column, spare);
}

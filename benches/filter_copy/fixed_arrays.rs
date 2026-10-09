//! Typed `SoA` experiment: deep-copy five columns, including tagged fixed text.
//! This benchmark-local table is not the dynamic `gd::Table` API.

use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};
use std::{
    hint::black_box,
    mem::MaybeUninit,
    time::{Duration, Instant},
};

/// Inline text cell: high 8 bits are a tag, low 24 bits are the valid length.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextCell<const N: usize> {
    meta: u32,
    bytes: [u8; N],
}

impl<const N: usize> TextCell<N> {
    fn new(bytes: [u8; N], length: usize, tag: u8) -> Self {
        // Validate once when writing the cell, never in the copy loop.
        assert!(length <= N && length <= 0x00ff_ffff);
        Self {
            meta: (u32::from(tag) << 24) | u32::try_from(length).unwrap(),
            bytes,
        }
    }

    fn length(&self) -> usize {
        (self.meta & 0x00ff_ffff) as usize
    }

    fn tag(&self) -> u8 {
        (self.meta >> 24) as u8
    }
}

fn tag(row: u64, column: u8) -> u8 {
    u8::try_from((row % 256 * 17 + u64::from(column) * 101) % 256).unwrap()
}

struct ArrayTable<const N: usize> {
    id: Vec<u64>,
    selector: Vec<u64>,
    amount: Vec<u64>,
    name: Vec<TextCell<N>>,
    message: Vec<TextCell<N>>,
}

impl<const N: usize> ArrayTable<N> {
    fn with_capacity(rows: usize) -> Self {
        Self {
            id: Vec::with_capacity(rows),
            selector: Vec::with_capacity(rows),
            amount: Vec::with_capacity(rows),
            name: Vec::with_capacity(rows),
            message: Vec::with_capacity(rows),
        }
    }

    fn fixture(rows: usize) -> Self {
        let mut table = Self::with_capacity(rows);
        for row in 0..rows {
            table.id.push(row as u64);
            table
                .selector
                .push(((row % 100 * 37 + row / 100 * 17) % 100) as u64);
            table.amount.push(row as u64 * 13 + 7);
            table
                .name
                .push(TextCell::new(text(row, "name"), N, tag(row as u64, 0)));
            table
                .message
                .push(TextCell::new(text(row, "text"), N, tag(row as u64, 1)));
        }
        table
    }

    fn filter_copy(&self, percent: u64, workers: usize, pool: &ThreadPool) -> Self {
        let select = |worker| {
            let begin = self.id.len() * worker / workers;
            let end = self.id.len() * (worker + 1) / workers;
            let mut rows = Vec::with_capacity(end - begin);
            for row in begin..end {
                if self.selector[row] < percent {
                    rows.push(row);
                }
            }
            rows
        };
        let parts: Vec<Vec<usize>> = if workers == 1 {
            vec![select(0)]
        } else {
            pool.install(|| (0..workers).into_par_iter().map(select).collect())
        };
        let count = parts.iter().map(Vec::len).sum();
        let mut target = Self::with_capacity(count);
        // Like GD, allocate the final destination once, then write disjoint
        // ranges. Safe slice splitting prevents workers from aliasing writes.
        // Spare capacity avoids zeroing five columns or copying worker tables
        // into a second destination, which would add memory traffic absent in GD.
        let id = split_slots(&mut target.id, &parts, count);
        let selector = split_slots(&mut target.selector, &parts, count);
        let amount = split_slots(&mut target.amount, &parts, count);
        let name = split_slots(&mut target.name, &parts, count);
        let message = split_slots(&mut target.message, &parts, count);
        let chunks: Vec<_> = parts
            .iter()
            .zip(id)
            .zip(selector)
            .zip(amount)
            .zip(name)
            .zip(message)
            .map(
                |(((((rows, id), selector), amount), name), message)| CopyChunk {
                    rows,
                    id,
                    selector,
                    amount,
                    name,
                    message,
                },
            )
            .collect();
        if workers == 1 {
            for chunk in chunks {
                self.copy_chunk(chunk);
            }
        } else {
            pool.install(|| {
                chunks
                    .into_par_iter()
                    .for_each(|chunk| self.copy_chunk(chunk));
            });
        }
        // SAFETY: each of the five vectors has capacity >= count and length 0.
        // split_slots partitions exactly count slots into non-overlapping slices
        // with lengths equal to the corresponding row-index lists. copy_chunk
        // writes a valid Copy value to every slot; all Rayon work joins before
        // this block. A panic leaves vector lengths at zero, so uninitialized
        // memory is never read or dropped. Vec provides the correct alignment.
        // A fully safe initialized destination would add an unnecessary zeroing
        // pass; worker-local vectors would require an additional payload copy.
        #[allow(unsafe_code)]
        unsafe {
            target.id.set_len(count);
            target.selector.set_len(count);
            target.amount.set_len(count);
            target.name.set_len(count);
            target.message.set_len(count);
        }
        target
    }

    fn copy_chunk(&self, chunk: CopyChunk<'_, N>) {
        let CopyChunk {
            rows,
            id,
            selector,
            amount,
            name,
            message,
        } = chunk;
        for (destination, &row) in rows.iter().enumerate() {
            id[destination].write(self.id[row]);
            selector[destination].write(self.selector[row]);
            amount[destination].write(self.amount[row]);
            name[destination].write(self.name[row]);
            message[destination].write(self.message[row]);
        }
    }

    fn digest(&self) -> (usize, u64) {
        let mut hash = 14_695_981_039_346_656_037;
        for row in 0..self.id.len() {
            for number in [self.id[row], self.selector[row], self.amount[row]] {
                hash = hash_bytes(hash, &number.to_le_bytes());
            }
            for (column, value) in [&self.name[row], &self.message[row]]
                .into_iter()
                .enumerate()
            {
                // Outside timing, check every copied tag and length against the
                // fixture, then hash the same logical values as the GD oracle.
                assert_eq!(value.length(), N);
                assert_eq!(
                    value.tag(),
                    tag(self.id[row], u8::try_from(column).unwrap())
                );
                hash = hash_bytes(
                    hash_bytes(hash, &(value.length() as u64).to_le_bytes()),
                    &value.bytes[..value.length()],
                );
            }
        }
        (self.id.len(), hash)
    }
}

struct CopyChunk<'a, const N: usize> {
    rows: &'a [usize],
    id: &'a mut [MaybeUninit<u64>],
    selector: &'a mut [MaybeUninit<u64>],
    amount: &'a mut [MaybeUninit<u64>],
    name: &'a mut [MaybeUninit<TextCell<N>>],
    message: &'a mut [MaybeUninit<TextCell<N>>],
}

fn split_slots<'a, T>(
    column: &'a mut Vec<T>,
    parts: &[Vec<usize>],
    count: usize,
) -> Vec<&'a mut [MaybeUninit<T>]> {
    let mut remaining = &mut column.spare_capacity_mut()[..count];
    let mut chunks = Vec::with_capacity(parts.len());
    for rows in parts {
        let (chunk, tail) = remaining.split_at_mut(rows.len());
        chunks.push(chunk);
        remaining = tail;
    }
    assert!(remaining.is_empty());
    chunks
}

fn text<const N: usize>(row: usize, label: &str) -> [u8; N] {
    let prefix = format!("{:08}-{label}-", row % 100_000_000);
    let mut value = [b'x'; N];
    value[..prefix.len()].copy_from_slice(prefix.as_bytes());
    value
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211);
    }
    hash
}

fn run<const N: usize>(args: &[String]) {
    let rows: usize = args[2].parse().unwrap();
    let workers: usize = args[4].parse().unwrap();
    let percent: u64 = args[5].parse().unwrap();
    let samples: usize = args[6].parse().unwrap();
    let sample_ms: u64 = args[7].parse().unwrap();
    assert!(rows <= 100_000_000 && (1..=64).contains(&workers) && percent <= 100);
    assert!(samples > 0 && sample_ms > 0);
    let pool = ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let start = Instant::now();
    let source = ArrayTable::<N>::fixture(rows);
    let build_ns = start.elapsed().as_nanos();
    let target = source.filter_copy(percent, workers, &pool);
    let verification = target.digest();
    if args[8] == "verify" {
        drop(source);
        assert_eq!(target.digest(), verification);
        println!(
            "{}",
            serde_json::json!({"count": verification.0,
            "digest": verification.1.to_string(), "independent_target": true,
            "source_drop_checked": true, "metadata_checked": true,
            "payload_bytes_per_row": 24 + 2 * N, "metadata_bytes_per_row": 8,
            "text_cell_bytes": std::mem::size_of::<TextCell<N>>(),
            "storage_bytes_per_row": 24 + 2 * std::mem::size_of::<TextCell<N>>()})
        );
        return;
    }
    assert_eq!(args[8], "time");
    drop(target);
    let operation = || {
        black_box(black_box(&source).filter_copy(percent, workers, &pool));
    };
    let mut iterations = 1u32;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        if start.elapsed() >= Duration::from_millis(sample_ms) {
            break;
        }
        iterations *= 2;
    }
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        times.push(start.elapsed().as_secs_f64() * 1e9 / f64::from(iterations));
    }
    println!(
        "{}",
        serde_json::json!({"samples_ns": times, "iterations": iterations,
        "build_ns": build_ns.to_string(), "count": verification.0,
        "digest": verification.1.to_string(), "metadata_checked": true,
        "payload_bytes_per_row": 24 + 2 * N, "metadata_bytes_per_row": 8,
        "text_cell_bytes": std::mem::size_of::<TextCell<N>>(),
        "storage_bytes_per_row": 24 + 2 * std::mem::size_of::<TextCell<N>>()})
    );
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        9,
        "arrays rows length workers percent samples sample_ms verify|time"
    );
    assert_eq!(args[1], "arrays");
    match args[3].parse::<usize>().unwrap() {
        16 => run::<16>(&args),
        128 => run::<128>(&args),
        _ => panic!("supported string sizes: 16 and 128"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check<const N: usize>() {
        for workers in [1, 8] {
            let pool = ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            for rows in [0, 1, 7, 31, 1001] {
                for percent in [0, 10, 50, 90, 100] {
                    let mut source = ArrayTable::<N>::fixture(rows);
                    let target = source.filter_copy(percent, workers, &pool);
                    let expected: Vec<_> = (0..rows)
                        .filter(|&row| ((row % 100 * 37 + row / 100 * 17) % 100) < percent as usize)
                        .collect();
                    assert_eq!(target.id.len(), expected.len());
                    for (destination, &row) in expected.iter().enumerate() {
                        assert_eq!(target.id[destination], source.id[row]);
                        assert_eq!(target.selector[destination], source.selector[row]);
                        assert_eq!(target.amount[destination], source.amount[row]);
                        assert_eq!(target.name[destination], source.name[row]);
                        assert_eq!(target.message[destination], source.message[row]);
                    }
                    let before = target.digest();
                    source.id.fill(u64::MAX);
                    source.selector.fill(u64::MAX);
                    source.amount.fill(u64::MAX);
                    source.name.fill(TextCell::new([0; N], 0, 255));
                    source.message.fill(TextCell::new([0; N], 0, 255));
                    assert_eq!(target.digest(), before);
                    drop(source);
                    assert_eq!(target.digest(), before);
                }
            }
        }
    }

    #[test]
    fn short_arrays_are_initialized_ordered_and_independent() {
        check::<16>();
    }

    #[test]
    fn long_arrays_are_initialized_ordered_and_independent() {
        check::<128>();
    }

    #[test]
    fn metadata_layout_and_variable_lengths_are_copied() {
        assert_eq!(std::mem::size_of::<TextCell<16>>(), 20);
        assert_eq!(std::mem::size_of::<TextCell<128>>(), 132);
        assert_eq!(std::mem::align_of::<TextCell<16>>(), 4);
        let pool = ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        for workers in [1, 8] {
            let mut source = ArrayTable::<16>::fixture(31);
            for row in 0..31 {
                source.name[row] = TextCell::new(text(row, "name"), row % 17, row as u8);
                source.message[row] =
                    TextCell::new(text(row, "text"), 16 - row % 17, 255 - row as u8);
            }
            let target = source.filter_copy(50, workers, &pool);
            for (destination, &row) in target.id.iter().enumerate() {
                let row = row as usize;
                assert_eq!(target.name[destination], source.name[row]);
                assert_eq!(target.message[destination], source.message[row]);
                assert_eq!(target.name[destination].length(), row % 17);
                assert_eq!(target.message[destination].tag(), 255 - row as u8);
            }
            let saved_name = target.name.clone();
            let saved_message = target.message.clone();
            source.name.fill(TextCell::new([0; 16], 0, 0));
            source.message.fill(TextCell::new([0; 16], 0, 0));
            drop(source);
            assert_eq!(target.name, saved_name);
            assert_eq!(target.message, saved_message);
        }
    }

    #[test]
    #[should_panic(expected = "length <= N")]
    fn oversized_valid_length_is_rejected_on_write() {
        TextCell::new([0; 16], 17, 0);
    }

    #[test]
    fn disjoint_spare_capacity_ranges_cover_every_slot() {
        let pool = ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        for rows in [0, 1, 9] {
            let source = ArrayTable::<128>::fixture(rows);
            let target = source.filter_copy(100, 8, &pool);
            assert_eq!(target.id, source.id);
            assert_eq!(target.selector, source.selector);
            assert_eq!(target.amount, source.amount);
            assert_eq!(target.name, source.name);
            assert_eq!(target.message, source.message);
            drop(source);
            assert_eq!(target.id.len(), rows);
        }
    }
}

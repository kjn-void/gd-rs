//! Rust counterparts of the C++ GD `string_transport` benchmark.
//!
//! gd-rs has no `string_transport` type; these are the equivalent Rust idioms: a writer
//! over a borrowed stack buffer, a port of GD's owned storage that allocates an
//! exact-size buffer per append, and `String`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

const EXPECTED: &str = "0123456789! 0123456789! 0123456789! 0123456789! 0123456789! 0123456789!";

/// A NUL-terminated UTF-8 string written into a caller-provided buffer.
struct BorrowedTransport<'a> {
    buffer: &'a mut [u8],
    len: usize,
}

#[derive(Debug)]
struct BufferFull;

impl<'a> BorrowedTransport<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        buffer[0] = 0;
        Self { buffer, len: 0 }
    }

    fn assign(&mut self, text: &str) -> Result<(), BufferFull> {
        self.len = 0;
        self.append(text)
    }

    fn append(&mut self, text: &str) -> Result<(), BufferFull> {
        let end = self.len + text.len();
        if end >= self.buffer.len() {
            return Err(BufferFull);
        }
        self.buffer[self.len..end].copy_from_slice(text.as_bytes());
        self.buffer[end] = 0;
        self.len = end;
        Ok(())
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buffer[..self.len]).unwrap()
    }
}

/// GD's owned storage: each append allocates an exact-size, NUL-terminated buffer,
/// copies the old text into it, and frees the old buffer.
#[derive(Default)]
struct OwnedTransport {
    text: Box<[u8]>,
    len: usize,
}

impl OwnedTransport {
    fn assign(&mut self, text: &str) {
        self.len = 0;
        self.append(text);
    }

    fn append(&mut self, text: &str) {
        let end = self.len + text.len();
        let mut next = vec![0_u8; end + 1].into_boxed_slice();
        next[..self.len].copy_from_slice(&self.text[..self.len]);
        next[self.len..end].copy_from_slice(text.as_bytes());
        self.text = next;
        self.len = end;
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.text[..self.len]).unwrap()
    }
}

fn fill_borrowed(text: &mut BorrowedTransport<'_>) {
    text.assign("0123456789!").unwrap();
    for _ in 0..5 {
        text.append(" 0123456789!").unwrap();
    }
}

fn fill_owned(text: &mut OwnedTransport) {
    text.assign("0123456789!");
    for _ in 0..5 {
        text.append(" 0123456789!");
    }
}

fn fill_string(text: &mut String) {
    text.clear();
    text.push_str("0123456789!");
    for _ in 0..5 {
        text.push_str(" 0123456789!");
    }
}

fn string_transport(criterion: &mut Criterion) {
    let mut buffer = [0_u8; 150];
    let mut borrowed = BorrowedTransport::new(&mut buffer);
    fill_borrowed(&mut borrowed);
    assert_eq!(borrowed.as_str(), EXPECTED);
    let mut owned = OwnedTransport::default();
    fill_owned(&mut owned);
    assert_eq!(owned.as_str(), EXPECTED);
    let mut string = String::new();
    fill_string(&mut string);
    assert_eq!(string, EXPECTED);

    // `black_box(&mut value)` before filling hides the state, so the constant result
    // cannot be precomputed, matching `DoNotOptimize` before `Fill` in the C++ fixture.
    let mut group = criterion.benchmark_group("StringTransport");
    group.bench_function("Buffer", |bencher| {
        let mut array_buffer = [0_u8; 150];
        bencher.iter(|| {
            let mut string_text = BorrowedTransport::new(&mut array_buffer);
            fill_borrowed(black_box(&mut string_text));
            black_box(&string_text);
        });
    });
    group.bench_function("NoBuffer", |bencher| {
        bencher.iter(|| {
            let mut string_text = OwnedTransport::default();
            fill_owned(black_box(&mut string_text));
            black_box(string_text)
        });
    });
    group.bench_function("String", |bencher| {
        bencher.iter(|| {
            let mut string_text = String::new();
            fill_string(black_box(&mut string_text));
            black_box(string_text)
        });
    });
    group.finish();
}

criterion_group!(benches, string_transport);
criterion_main!(benches);

# String construction with a caller buffer

`StringTransport` builds valid UTF-8 text in a caller-provided byte slice and
automatically moves to the heap when that slice is too small. Import it from
the crate root. No Cargo feature is required.

```rust
use gd::StringTransport;

let mut buffer = [0_u8; 150];
let mut text = StringTransport::new(&mut buffer);
text.assign("12345");
text += " 12345";
text.append(" 12345");
text.append("4");
text.append("5");
text.append("6");
text.append("7");
assert_eq!(text.as_str(), "12345 12345 123454567");
assert!(text.is_borrowed());
```

The buffer's capacity includes one byte for a trailing NUL. A 150-byte buffer
therefore holds 149 text bytes without allocation. `len()` counts UTF-8 bytes,
and `capacity()` includes the terminator. Interior NUL characters are accepted.
`as_bytes()` and `as_str()` omit the final terminator; `as_bytes_with_nul()` includes
it. Empty builders without storage return a static NUL from that last accessor.

`owned()` and `Default::default()` start without a borrowed buffer or an allocation.
The first write allocates. Once owned, every append or assignment requests an
exact-size allocation of the new text length plus one; smaller assignments do
not switch back to the caller's buffer. Use standard `String` when amortized heap
growth better fits the workload. Allocation limits can panic as with standard
Rust collections; no fallible allocation API is provided.

```rust
use gd::StringTransport;

let mut buffer = [0_u8; 6];
let mut text = StringTransport::new(&mut buffer);
text.assign("12345");
assert!(text.is_borrowed());
text.append("é");
assert!(!text.is_borrowed());
assert_eq!(text.as_str(), "12345é");
assert_eq!(text.capacity(), 8); // Seven UTF-8 bytes and the terminator.
```

## Using the builder with tables

Convert `&StringTransport` into `Value` to copy its text into an independently
owned table cell. Passing the builder by value also converts into `Value`.
`Table::set_named` accepts `&StringTransport` directly through `Into<Value>`.

```rust
use gd::{ColumnSpec, DataType, Schema, StringTransport, Table, Value, ValueRef};

let schema = Schema::new([ColumnSpec::new("label", DataType::String)])?;
let mut table = Table::new(schema);
let mut buffer = [0_u8; 150];
let mut text = StringTransport::new(&mut buffer);
text.assign("first row");
table.push_row([Value::from(&text)])?;
text.assign("second row");
table.push_row([Value::from(&text)])?;
text.assign("updated first row");
table.set_named(0, "label", &text)?;
drop(text);
buffer.fill(0);
assert_eq!(table.cell(0, 0)?, ValueRef::String("updated first row"));
assert_eq!(table.cell(1, 0)?, ValueRef::String("second row"));
# Ok::<(), gd::TableError>(())
```

`ValueRef::from(&text)` borrows the initialized text without copying. Its lifetime
prevents mutation while that borrowed value remains in use:

```compile_fail
use gd::{StringTransport, ValueRef};

let mut buffer = [0_u8; 150];
let mut text = StringTransport::new(&mut buffer);
text.assign("first");
let view = ValueRef::from(&text);
text.assign("second");
println!("{view:?}");
```

The builder itself cannot outlive its caller buffer:

```compile_fail
use gd::StringTransport;

let text = {
    let mut buffer = [0_u8; 150];
    StringTransport::new(&mut buffer)
};
println!("{text}");
```

## Standard formatting

The builder implements `std::fmt::Write`, `Display`, `AsRef<str>` and `AddAssign<&str>`.
`text()` is an alias for `as_str()`.

```rust
use std::fmt::Write;
use gd::StringTransport;

let mut buffer = [0_u8; 150];
let mut text = StringTransport::new(&mut buffer);
write!(text, "item {}: {}", 7, "ready")?;
assert_eq!(text.to_string(), "item 7: ready");
# Ok::<(), std::fmt::Error>(())
```

# Types and dynamic values

This guide is the Rust counterpart of the C++ type-system, variant, and variant-view
documents. The public model consists of:

- `DataType`, a closed logical type descriptor;
- `Value`, an owned dynamic value;
- `ValueRef<'a>`, a borrowed dynamic value.

Unlike the C++ combined numeric identifiers, `DataType` is an enum. Ownership is not
encoded in numeric flags: `Value::String` and `ValueRef::String` both report
`DataType::String`, while Rust's type and lifetime describe their storage.

## Supported types

| `DataType` | Owned payload | Borrowed payload | Fixed width |
|---|---|---|---:|
| `Null` | none | none | 0 |
| `Bool` | `bool` | copied `bool` | 1 |
| `I8`, `I16`, `I32`, `I64` | matching signed integer | copied integer | 1, 2, 4, or 8 |
| `U8`, `U16`, `U32`, `U64` | matching unsigned integer | copied integer | 1, 2, 4, or 8 |
| `F32`, `F64` | matching float | copied float | 4 or 8 |
| `String` | `CompactString` | `&str` | variable |
| `Bytes` | `Box<[u8]>` | `&[u8]` | variable |
| `Uuid` | `uuid::Uuid` | copied `Uuid` | 16 |

Use `DataType::name`, `is_numeric`, `is_integer`, and `fixed_width` when schema or
diagnostic code needs type metadata.

```rust
use gd::DataType;

assert_eq!(DataType::I32.name(), "i32");
assert!(DataType::F64.is_numeric());
assert!(!DataType::F64.is_integer());
assert_eq!(DataType::Uuid.fixed_width(), Some(16));
assert_eq!(DataType::String.fixed_width(), None);
```

The C++ character-class tables, type-group bit masks, tag dispatchers, pointer types,
and platform-sized type aliases have no public `gd-rs` equivalent. Use Rust enums,
traits, `char`/`str` methods, and ordinary generics for those jobs.

## Constructing owned values

Primitive values, strings, bytes, and UUIDs implement `Into<Value>` through `From`.
The original numeric width is retained.

```rust
use gd::{DataType, Value};

let count = Value::from(42_u32);
let ratio = Value::from(1.5_f64);
let name = Value::from("Ada");
let payload = Value::from(vec![0_u8, 1, 255]);
let missing = Value::Null;

assert_eq!(count.data_type(), DataType::U32);
assert_eq!(ratio.data_type(), DataType::F64);
assert_eq!(name.as_str(), Ok("Ada"));
assert_eq!(payload.as_bytes(), Ok(&[0, 1, 255][..]));
assert_eq!(missing.data_type(), DataType::Null);
```

`Value` is a Rust sum type. A discriminant cannot disagree with its payload, and a
string or byte sequence cannot be marked owning while pointing at unrelated storage.
There is no ABI or fixed-size-layout promise.

## Borrowed values

`ValueRef` copies primitive and UUID payloads and borrows string or byte storage. Its
lifetime prevents it from outliving that storage.

```rust
use gd::{DataType, Value, ValueRef};

let source = String::from("borrowed");
let view = ValueRef::from(source.as_str());

assert_eq!(view.data_type(), DataType::String);
assert_eq!(view.as_str(), Ok("borrowed"));

let owned = view.to_owned();
assert_eq!(owned, Value::from("borrowed"));
```

Borrow an existing `Value` with `Value::as_ref`. Turning a string or byte view back
into a `Value` copies the payload. There is no layout cast between owned and borrowed
types.

## Reading and converting

Accessors return `ValueError` rather than performing broad implicit conversions:

- `as_str` accepts only `String`;
- `as_bytes` accepts only `Bytes`;
- `to_i64` accepts signed and unsigned integer variants and checks the `u64` range;
- `to_f64` accepts numeric variants and performs the ordinary Rust numeric conversion.

```rust
use gd::{Value, ValueError};

assert_eq!(Value::from(42_i16).to_i64(), Ok(42));
assert_eq!(Value::from(42_u32).to_f64(), Ok(42.0));
assert!(matches!(
    Value::from(u64::MAX).to_i64(),
    Err(ValueError::OutOfRange { .. })
));
assert!(matches!(
    Value::from("42").to_i64(),
    Err(ValueError::TypeMismatch { .. })
));
```

The C++ `convert`, arithmetic operators, ordering operators, and string-to-number
coercions are not reproduced on `Value`. Extract a checked numeric value, use normal
Rust arithmetic, or use the [expression API](expressions.md) when runtime formulas are
the requirement. Equality is variant-aware: values of different numeric widths are
not silently treated as the same type.

## Storage guidance

Short owned strings normally remain inline through `CompactString`; larger strings
allocate. Byte values own a boxed slice, and UUIDs remain inline. These choices are
implementation details rather than a serialization format. Use an explicit codec for
wire or disk compatibility and consult the [performance report](../high-level/performance.md)
before changing representation solely to reduce allocations.

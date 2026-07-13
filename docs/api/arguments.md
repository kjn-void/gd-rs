# Arguments

`Arguments` is the Rust counterpart of the C++ owning argument pairs, packed
argument buffer, and shared argument buffer. It is one insertion-ordered sequence of
owned `Argument` entries, not a map and not a serialized byte layout.

Each entry contains an optional name and one `Value`. This preserves the two C++
behaviors that ordinary maps lose:

- unnamed positional values may coexist with named values;
- duplicate names remain distinct and retain insertion order.

## Constructing a collection

```rust
use gd::{Argument, Arguments, Value};

let mut arguments = Arguments::with_capacity(4);
arguments.push_named("name", "Ada");
arguments.push_named("age", 37_i32);
arguments.push_positional(true);
arguments.push(Argument::named("score", 95.5_f64));

assert_eq!(arguments.len(), 4);
assert_eq!(
    arguments.get_named("name").unwrap().value().as_str(),
    Ok("Ada")
);
assert_eq!(arguments.get(2).unwrap().name(), None);
assert_eq!(arguments.get(2).unwrap().value(), &Value::Bool(true));
```

`push_named` and `push_positional` accept any type implementing `Into<Value>`.
`Argument::named` and `Argument::positional` are useful when constructing through
iterators:

```rust
use gd::{Argument, Arguments};

let arguments: Arguments = [10_i32, 20, 30]
    .into_iter()
    .map(|value| Argument::named("item", value))
    .collect();

assert_eq!(
    arguments
        .get_nth_named("item", 2)
        .unwrap()
        .value()
        .to_i64(),
    Ok(30)
);
```

## Access and mutation

Positional access is O(1). `get_named`, `get_nth_named`, and `contains_name` perform an
allocation-free O(n) scan.

```rust
use gd::{Arguments, Value};

let mut arguments = Arguments::new();
arguments.push_named("mode", "fast");
arguments.push_named("retry", 2_u32);

if let Some(retry) = arguments.get_mut(1) {
    *retry.value_mut() = Value::U32(3);
}

for argument in &arguments {
    match argument.name() {
        Some(name) => println!("{name}: {:?}", argument.value()),
        None => println!("positional: {:?}", argument.value()),
    }
}

let removed = arguments.remove_named("mode").unwrap();
assert_eq!(removed.value().as_str(), Ok("fast"));
```

Removing an entry is O(n), because later entries shift while preserving order.
`clear` retains the vector's allocated capacity for reuse.

## Duplicate names

The first-match methods never collapse duplicates. Use occurrence lookup when every
entry matters:

```rust
use gd::Arguments;

let mut headers = Arguments::new();
headers.push_named("accept", "application/json");
headers.push_named("accept", "text/plain");

assert_eq!(
    headers
        .get_nth_named("accept", 1)
        .unwrap()
        .value()
        .as_str(),
    Ok("text/plain")
);
```

This behavior differs from a Rust `HashMap`, where a second insertion normally
replaces the first value.

## Reusable name indexes

Build an `ArgumentIndex` when an immutable collection will receive repeated name
lookups. Construction is expected O(n); lookup is expected O(name length), followed
by O(1) occurrence selection.

```rust
use gd::Arguments;

let mut arguments = Arguments::new();
arguments.push_named("item", 10_i32);
arguments.push_positional(false);
arguments.push_named("item", 20_i32);

let index = arguments.index();
assert_eq!(index.positions("item"), &[0, 2]);
assert_eq!(
    index
        .get_nth_named("item", 1)
        .unwrap()
        .value()
        .to_i64(),
    Ok(20)
);
```

The index borrows `arguments` immutably. Rust therefore prevents pushes, removals,
and mutable element access until the index is no longer used. This replaces the C++
state in which a packed-buffer offset or borrowed name could become stale.

For short-lived collections with few entries, linear lookup is often cheaper than
building an index. The matched benchmark in the
[performance report](../high-level/performance.md#uri-shaped-arguments) documents the
current tradeoff.

## Ownership and sharing

`Arguments` owns its entry vector, names, and values. Cloning performs an ordinary
owned clone. It does not expose external-buffer storage, a packed live-memory format,
intrusive reference counts, or built-in copy-on-write.

Use normal Rust ownership when possible. When immutable sharing is required, use an
application-level `Arc<Arguments>`:

```rust
use std::sync::Arc;

use gd::Arguments;

let mut values = Arguments::new();
values.push_named("mode", "safe");

let shared = Arc::new(values);
let another_owner = Arc::clone(&shared);
assert_eq!(another_owner.get_named("mode").unwrap().value().as_str(), Ok("safe"));
```

`Arc::make_mut` can provide copy-on-write behavior when an application has measured a
need for it. The crate does not force shared ownership or synchronization on callers
that do not need them.

## Interchange and database use

- [Formatting](formatting.md) converts representable argument collections to JSON
  objects or URI query pairs.
- [SQLite](sqlite.md) binds an `Arguments` collection to named or positional statement
  parameters under a strict, lossless policy.

`Arguments` itself is not a network-packet or persistence encoding. Define a versioned
codec when packed binary compatibility is required.

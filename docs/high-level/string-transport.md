# StringTransport and table ownership

`gd::StringTransport` is a safe UTF-8 builder for short-lived text assembled from
several pieces. It writes into caller storage while the text and trailing NUL fit,
then switches automatically to an owned `Box<[u8]>`. Its representation keeps
storage and text length separate. The append fast path is explicitly inlined;
heap allocation and prefix copying are in a separate cold function.

The owned mode requests exactly the new text length plus a terminator on every
append or assignment. It remains owned after shorter assignments. This policy
matches the transport's temporary-buffer purpose; it does not provide the
amortized growth policy of standard `String`. Neither buffer capacity nor length
has C++'s packed 16-bit borrowed-size limit. Rust allocation and slice limits apply.

All writes accept UTF-8 strings, and the type exposes no mutable byte slice.
Text access validates UTF-8 safely. Interior NUL bytes are permitted, so callers
interfacing with C APIs must apply those APIs' string restrictions separately.
Empty builders can exist without an allocation; their terminated byte accessor
supplies a static NUL until writable storage is present.

Tables own their cells. `Value::from(&builder)` copies the text and lets the caller
reuse or drop the builder immediately. `ValueRef::from(&builder)` is a borrowing
view whose lifetime prevents mutation or destruction while it is used. There is
no change to the table's storage model and no borrowed backing buffer stored in
a table. The type also implements ordinary formatting and append traits.

See the [API guide and table examples](../api/string-transport.md),
[implementation](../../src/string_transport.rs), and
[ownership and growth tests](../../tests/string_transport.rs).

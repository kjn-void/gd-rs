# Unsafe code

Prefer safe Rust. Any new `unsafe` block must:

- document the invariant that makes the operation sound;
- explain why a safe design was insufficient;
- keep the unsafe region as small as practical; and
- have focused tests for the relevant memory, alignment, lifetime, or aliasing
  boundary.

Use Miri or an appropriate sanitizer when it can exercise the invariant. Sanitizer
instrumentation is for correctness checks, never ordinary performance measurements.

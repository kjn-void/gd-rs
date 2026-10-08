# Earlier four-variant cohort

These raw primary and confirmation measurements predate SharedRecordTable and
contain only GD memcpy, C++ STL strings, gd-rs CompactString and gd-rs fixed buffers.
They are retained for provenance, and are excluded from every current graph,
table and geometric mean in [the five-variant report](../../../filter-copy-results.md).

Each JSON records its source fingerprints, host, toolchain, timing contract and
complete samples. The owned-copy paths are described by the current
[Rust driver](../../../../../benches/filter_copy/driver.rs) and
[C++ driver](../../../../../benches/cpp-reference/filter_copy.cpp), but the Rust
driver subsequently gained the Arc case; the historical fingerprints therefore
differ. Use the current cohort to compare all five implementations together.

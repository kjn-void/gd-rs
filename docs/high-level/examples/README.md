# Whole-record filtering examples

These examples use one source table with three numeric fields and two text fields,
filter matching records, and construct one ordered destination.

- [M6 comparison](filter-copy-results.md): five representations, with an illustration
  of row and column layouts, small-string optimization, heap text, deep copying and
  Arc sharing.
- [Arc scaling on M6](arc-scaling-m6-results.md): chunked filtering, parallel handle
  cleanup, worker scaling and comparison with GD memcpy.
- [GD row memcpy versus gd-rs with constant size fields](filter-copy-arrays-m6-results.md):
  independent AoS and SoA copies, including a diagram of source, filter, copy and
  destination with text metadata.

Illustrations and performance graphs are in [`images`](images). Raw measurements
are in [`../measurements`](../measurements). The [benchmark guide](../../../benches/filter_copy/README.md)
documents the source code and reproduction commands.

# Initial Arc scaling screening on Apple M6

Sources: [screened Rust harness](source/benches/filter_copy/arc_scaling.rs),
[screening runner](source/benches/filter_copy/arc_scaling.py),
[original gd-rs API](source/src/table/shared_record.rs).
The [C++ comparison driver](../../../../../benches/cpp-reference/filter_copy.cpp)
has no matching shared-pointer case; this pilot measured only Arc pipelines.

The [raw data](results.json) retain the initial five-pipeline screening at one and
eight workers: original fused filter with serial destruction, fused filter with
parallel destruction, reserved chunk buffers with serial or parallel destruction,
and two-pass selected indices with parallel destruction. Each case used five
rotated rounds and three batches calibrated to at least 20 ms, under the current
M6 host load.

This experimental harness used local vector implementations to test alternatives
before adding `par_filter_chunked` and `par_drop` to gd-rs. Its source fingerprints
match every file under `source/`. It is preserved as screening evidence; none of
these samples enters the final plots or geometric means. The
[final investigation](../../../examples/arc-scaling-m6-results.md) measures the actual
public APIs with a wider core sweep and a fresh comparison against GD and STL.

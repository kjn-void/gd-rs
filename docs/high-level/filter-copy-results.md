# Whole-record filtering into one target

<!-- arc-scaling-m6:start -->

## Updated M6 comparison: parallel Arc cleanup

Sources: [Arc scaling harness](../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../src/table/shared_record.rs), [Rust comparison driver](../../benches/filter_copy/driver.rs), [C++ comparison driver](../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart; The displayed Arc variant shares payloads while the other four variants copy them.

The current M6 comparison shows five representations, with gd-rs Arc using chunked filtering and parallel cleanup. The improved Arc path is **1.158× faster at eight workers**, averaged geometrically over both string sizes and all three selection rates. All six variants were rerun together, including GD memcpy and standard STL containers. Five of six eight-worker cases improve; the large-string 90%-selection case regresses in the primary cohort and is repeated separately. These are current-load diagnostics; source allocation is excluded and completed target cleanup is included.

**Speed relative to GD** (higher is faster; equal weight per case). The first four variants deep-copy payloads; gd-rs Arc shares them.

| Group | GD memcpy | C++ STL std::string | gd-rs CompactString | gd-rs fixed buffer | gd-rs Arc |
|---|---:|---:|---:|---:|---:|
| All 12 cases | 1.000× | 0.610× | 0.573× | 0.670× | 1.676× |
| 1 worker, 16 B | 1.000× | 0.958× | 0.676× | 0.338× | 1.117× |
| 1 worker, 128 B | 1.000× | 0.604× | 0.491× | 1.137× | 3.400× |
| 8 workers, 16 B | 1.000× | 0.588× | 0.860× | 0.497× | 0.830× |
| 8 workers, 128 B | 1.000× | 0.408× | 0.377× | 1.060× | 2.504× |

![M6 performance relative to GD memcpy](measurements/filter-copy-arc-m6.png)

See the [Arc scaling investigation](arc-scaling-m6-results.md) for all absolute timings, 1/2/4/6/8/12-worker results, phase measurements, confirmation runs, and why the APIs help. Only the latest Arc implementation appears in the current tables and graphs. The original remains in the raw data and explanatory before-and-after analysis.

Earlier results are available in the [historical three-host report](filter-copy-three-host-results.md) and the [fixed-array SoA experiment](filter-copy-arrays-m6-results.md). The improved Arc path has only been measured on M6.

<!-- arc-scaling-m6:end -->

# Rust benchmarks and examples in VS Code

Open the repository root in VS Code (`code .`). Install the recommended
**rust-analyzer** and **CodeLLDB** extensions. The root
[launch configurations](../../.vscode/launch.json) and
[tasks](../../.vscode/tasks.json) are dedicated to Rust; the C++ debugging project
lives separately in [external/order-workflow](../../external/order-workflow/README.md).

Use CodeLLDB 1.12.3 or newer for Rust data-formatter support with current toolchains.
See [CodeLLDB's compatibility notes](https://github.com/vadimcn/codelldb/blob/master/CHANGELOG.md#1123).

## Launch a target

Select a `Rust benchmark: …` or `Rust example: …` entry in **Run and Debug**, set
a breakpoint, then press **F5**. The configurations cover the benchmark targets and
the `order_workflow` example in
[benches/order_workflow/driver.rs](../../benches/order_workflow/driver.rs).
Cargo also declares `text_workflow`, `filter_copy`, and `filter_copy_arrays`;
run those through their documented benchmark drivers or add a launch configuration
with the corresponding input arguments.

[CodeLLDB's Cargo integration](https://github.com/vadimcn/codelldb/blob/master/MANUAL.md#cargo-support)
builds the selected target before launch and finds its executable from Cargo's
artifact output. Cargo's dependency tracking rebuilds changed inputs and reuses
unchanged output. Every launch enables all crate features and uses the `dev`
profile for debug symbols and unoptimized stepping. No hashed executable paths
are hard-coded.

Launches run without an automatic pause; set a breakpoint to stop in the source.
Program output appears in the integrated terminal. `Process exited with code 0`
in the Debug Console indicates success, and the small order-workflow fixture
finishes quickly when no breakpoint is set.

Criterion targets default to `--test`, which runs each selected case once instead
of collecting performance samples. The filter prompt accepts part of a benchmark
name; an empty filter selects all cases. Large fixture construction still occurs.
To debug the timing path, remove `--test` from the selected configuration's `args`.
Use the repository's optimized benchmark runners for reported performance.

Three standalone benchmarks have their own defaults:

- `price_total_500k` runs its correctness/check mode with no arguments.
- `par_benchmark` uses 10,000 rows, four workers, and one pass; change its `env`
  entries to select another workload.
- `simd_comments` verifies every implementation and exits in debug builds or with
  `--test`. Release runs accept `pack-hybrid`, `soa`, `standard`, or `all`; use
  `benches/run_simd_comments.sh` for the measured comparison.

## Order workflow example

Choose `Rust example: order_workflow`. The pre-launch task creates an eleven-line
SQLite fixture at `target/vscode-rust/order-workflow-small.sqlite` using the
existing [fixture generator](../../benches/order_workflow/fixture.py). It reuses
an existing fixture without overwriting it. This launch requires Python 3 with
SQLite support (`python3` on macOS/Linux, `python` on Windows).

The launch prompts for workers, stage, and index mode; defaults are `4`, `verify`,
and `native`. Useful breakpoints are `main` in the
[driver](../../benches/order_workflow/driver.rs), and `load`, `prepare`, `variant`,
and `variants` in the [application](../../benches/order_workflow/workload.rs).
For a larger fixture, generate a new database and change the first launch argument.

## Nightly SIMD

`Rust benchmark: table_nightly_simd (nightly)` requires an installed nightly
toolchain (`rustup toolchain install nightly`). It enables `--cfg nightly_simd`
and builds under `target/vscode-rust-nightly`, keeping this build separate from
stable debug output. Without that configuration, this target's stable entry point
does no work. The `nullable_maximum` configuration exercises its stable paths.

## Build and check tasks

**Ctrl+Shift+B** builds all stable benchmarks and examples in the `dev` profile.
The stable build includes the inert fallback for the nightly-only target; use
its nightly launch to build and debug the SIMD implementation. **Terminal → Run
Task** also offers checks for all targets and library/integration tests.

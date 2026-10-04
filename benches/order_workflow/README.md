# Order workflow

From the repository root:

```sh
git submodule update --init external/gd
./benches/run_order_workflow.sh
```

The runner builds optimized Rust and C++ standalone programs, generates deterministic
SQLite fixtures under `target/order-workflow`, checks complete outputs against SQL,
and then alternates execution order across two process rounds. Default sizes are
10,000, 100,000, and 1,000,000 lines; default worker counts are 1, 2, 4, and 8, bounded
by the machine's logical CPUs. Each timed process performs one warmup and five
recorded iterations. Compilation and verification finish before that dataset is
timed. Avoid other system load while running.

```sh
./benches/run_order_workflow.sh --rows 10000 --workers 1 2 --samples 5 --rounds 2
python3 benches/order_workflow/summarize.py target/order-workflow/results.json
```

`--skip-build` is for binaries already built with the runner's documented flags.
It does not check that custom binaries use those flags. Raw results, build logs,
and stripped copies stay under `target/order-workflow`. The default GD source is
the pinned `external/gd` submodule; `--gd /path/to/gd` overrides it. CMake build
files and application code are maintained in `benches/cpp-reference`, and generated
forwarding headers stay in the build directory. The runner fingerprints the GD
source tree before and after running and records hashes of the maintained build
recipe. It builds Rust with `--locked` and rejects differing runtime SQLite
versions before timing; both shipped builds use SQLite 3.53.2.

To retain an inspectable fixture and run individual checks:

```sh
python3 benches/order_workflow/fixture.py target/order-workflow/example.sqlite --rows 10000
target/release/examples/order_workflow target/order-workflow/example.sqlite 4 verify 1 native
target/order-workflow/cpp/gd_order_workflow target/order-workflow/example.sqlite 4 verify 1 native
```

The fixture generator refuses to overwrite a file. `--small` generates a
hand-calculated eleven-line edge-case fixture. Executables accept database path,
worker count, stage (`verify`, `import`, `prepare`, `variants`, `complete`), sample
count, and index mode (`native`, `sorted`). The C++ native index is already sorted;
the sorted mode is a diagnostic switch for Rust. Always verify a dataset before
recording timings. Executables are benchmark applications for this fixture contract,
not general-purpose database import/validation tools.

See the [report](../../docs/high-level/order-workflow.md) for exact semantics,
measured results, safety findings, and limits on the conclusions.

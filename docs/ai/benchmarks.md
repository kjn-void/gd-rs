# Benchmark methodology

Do not run ordinary performance comparisons under ASan, UBSan, debug builds, or known
unrelated system load. If such a configuration is itself the subject of a measurement,
label it explicitly and do not compare it as an optimized baseline.

For checked-in benchmark results, record enough context to reproduce and interpret
them:

- host and relevant CPU configuration;
- compiler and toolchain version;
- optimization and architecture flags;
- enabled crate features;
- worker count or CPU affinity for parallel runs; and
- the command or checked-in script used.

Re-run surprising results before documenting them. Do not replace published numbers
with a single anomalous run. Every result section in the performance documents must
link directly to the checked-in Rust and C++ sources that generate it; state clearly
when no exact counterpart exists.

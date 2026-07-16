# Repository instructions

Before changing this repository, read and follow the broader guidance indexed in
[`docs/ai/index.md`](docs/ai/index.md). It defines repository boundaries, validation,
commit scope, unsafe-code requirements, benchmark methodology, and documentation
policy.

Complete this checklist before every commit.

## 1. Review documentation impact

Inspect the complete diff and decide whether it changes the public API, documented
behavior, examples, architecture, storage model, or performance claims.

- Update `docs/api` when public types, methods, errors, or API behavior change.
- Update `docs/high-level` when design, usage, examples, storage, or performance change.
- If neither directory needs an update, confirm that deliberately rather than assuming
  that a code-only change has no documentation impact.

## 2. Refresh source statistics

Regenerate and update [`docs/port/source-stats.md`](docs/port/source-stats.md) after
changes to product, test, or benchmark source. Follow the commands and file-selection
rules documented there; do not estimate the totals or retain stale figures.

## 3. Run all unit tests

Run the complete test suite with all features, then verify the supported minimal
feature configuration:

```sh
cargo test --all-targets --all-features
cargo test --lib --tests --no-default-features
```

Do not commit if either command fails.

## 4. Link performance results to their benchmarks

Before committing added or changed results in
[`docs/high-level/performance.md`](docs/high-level/performance.md), verify that every
result section links directly to the checked-in Rust and C++ benchmark sources that
produce its numbers. If one implementation has no exact counterpart, state that next
to the source links instead of implying a matched benchmark exists.

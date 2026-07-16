# AI-assisted repository work

These rules supplement the mandatory pre-commit checklist in the repository-root
[`AGENTS.md`](../../AGENTS.md). Read both that checklist and every applicable document
below before changing the repository.

- [`repository-boundaries.md`](repository-boundaries.md): permitted work in this
  repository and the sibling C++ reference.
- [`validation.md`](validation.md): Rust 1.86 compatibility and required static checks.
- [`commits.md`](commits.md): change isolation, working-tree care, commits, and pushes.
- [`unsafe-code.md`](unsafe-code.md): requirements for introducing unsafe Rust.
- [`benchmarks.md`](benchmarks.md): reproducible performance methodology and reporting.
- [`documentation.md`](documentation.md): public API and high-level documentation.

When several documents apply, follow all of them. These files do not weaken or replace
the root checklist.

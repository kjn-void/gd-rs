# Documentation

When changing a public API, update the corresponding `docs/api` document and its
examples. Update `docs/high-level` when architecture, storage, usage, or performance
changes.

Describe the current supported behavior directly. When implementation or APIs
change, replace superseded descriptions instead of retaining before/after notes,
old API counts, or explanations of how the code used to work. Git history carries
that history. Keep measurement dates, source revisions, toolchains, and workload
details as reproducibility evidence, with clearly named comparison variants.

Prefer examples that compile against the crate's actual public exports rather than
internal modules. Add a unit or integration test for an example when practical,
especially for ownership, mutation, nullability, open-schema, and concurrency
behavior.

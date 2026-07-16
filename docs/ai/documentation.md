# Documentation

When changing a public API, update the corresponding `docs/api` document and its
examples. Update `docs/high-level` when architecture, storage, usage, or performance
changes.

Prefer examples that compile against the crate's actual public exports rather than
internal modules. Add a unit or integration test for an example when practical,
especially for ownership, mutation, nullability, open-schema, and concurrency
behavior.

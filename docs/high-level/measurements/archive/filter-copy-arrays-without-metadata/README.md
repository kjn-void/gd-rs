# Fixed arrays without cell metadata: preserved cohort

This is the original M6 experiment using two bare `Vec<[u8; N]>` columns, before the two 32-bit tag/valid-length words were added. Its reported row totals are 56/280 bytes. It is excluded from the current paired results.

The raw primary and confirmation files are unchanged. Every fingerprinted project source is retained under `source/` with its original repository-relative path. GD remains pinned to the revision and source fingerprint in the raw metadata. The [saved report](report.md) and plot describe this cohort; report links have been relocated to the retained files without changing its values.

Sources: [Rust five-column table without metadata](source/benches/filter_copy/fixed_arrays.rs), [GD memcpy driver](source/benches/cpp-reference/filter_copy.cpp), [paired runner](source/benches/filter_copy/arrays.py).

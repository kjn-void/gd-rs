# Repository boundaries

The `external/gd` submodule is a read-only C++ reference unless the user explicitly
requests changes there. Never commit or push from `external/gd`; commits there belong
to the upstream repository and must be made through a separate checkout.

Store maintained C++ benchmark counterparts in `benches/cpp-reference`. Reading,
building, testing, and benchmarking `external/gd` is allowed when relevant to the
requested work, but those operations do not authorize modifying or publishing it.

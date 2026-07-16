# Repository boundaries

The sibling `../gd` repository is a read-only C++ reference unless the user explicitly
requests changes there. Never commit or push from `../gd`.

Store maintained C++ benchmark counterparts in `benches/cpp-reference`. Reading,
building, testing, and benchmarking `../gd` is allowed when relevant to the requested
work, but those operations do not authorize modifying or publishing it.

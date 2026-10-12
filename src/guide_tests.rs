//! Standalone guide examples, compiled and run by `cargo test --doc`.
//!
//! Feature-specific guides use the same gates as the APIs they demonstrate.
//! Safety comparisons and benchmark reports contain illustrative fragments and
//! are deliberately separate from these executable usage guides.

#[doc = include_str!("../docs/api/values.md")]
mod values {}

#[doc = include_str!("../docs/api/arguments.md")]
mod arguments {}

#[cfg(feature = "rayon")]
#[doc = include_str!("../docs/api/tables.md")]
mod tables {}

#[doc = include_str!("../docs/api/indexes.md")]
mod indexes {}

#[doc = include_str!("../docs/api/formatting.md")]
mod formatting {}

#[doc = include_str!("../docs/api/text.md")]
mod text {}

#[doc = include_str!("../docs/api/expressions.md")]
mod expressions {}

#[cfg(feature = "sqlite")]
#[doc = include_str!("../docs/api/sqlite.md")]
mod sqlite {}

#[doc = include_str!("../docs/high-level/binary.md")]
mod binary_overview {}

#[doc = include_str!("../docs/high-level/expression.md")]
mod expression_overview {}

#[cfg(feature = "rayon")]
#[doc = include_str!("../docs/high-level/table.md")]
mod table_overview {}

#[doc = include_str!("../docs/high-level/examples.md")]
mod examples {}

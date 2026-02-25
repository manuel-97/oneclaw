//! Layer 2: Memory — Brain
//! Tri-Search: Keyword (FTS5) + Temporal (B-tree) + Semantic (Vector)
//! Sprint 3-4: FTS5 + Temporal. Vector search in TIP-007/008.

pub mod traits;
pub mod sqlite;

pub use traits::*;
pub use sqlite::SqliteMemory;

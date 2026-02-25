//! Layer 2: Memory — Brain
//! Tri-Search: Keyword (FTS5) + Temporal (B-tree) + Semantic (Vector)
//! TIP-040: Vector search interface + SQLite extension.

pub mod traits;
pub mod sqlite;
pub mod vector;

pub use traits::*;
pub use sqlite::SqliteMemory;
pub use vector::*;

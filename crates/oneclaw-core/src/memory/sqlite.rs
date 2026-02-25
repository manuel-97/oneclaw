//! SQLite-backed Memory implementation
//! Features: Persistent storage, FTS5 full-text search, temporal indexing.
//! Vector search will be added in TIP-007/008.

use crate::error::{OneClawError, Result};
use crate::memory::traits::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::Mutex;
use std::path::Path;
use chrono::{DateTime, Utc};
use tracing::{info, debug};

/// SQLite-backed memory implementation with FTS5 full-text search.
pub struct SqliteMemory {
    conn: Mutex<Connection>,
}

impl SqliteMemory {
    /// Create new SqliteMemory with database at given path
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = db_path.as_ref().parent()
            && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| OneClawError::Memory(format!("Failed to create db directory: {}", e)))?;
        }

        let conn = Connection::open(db_path.as_ref())
            .map_err(|e| OneClawError::Memory(format!("Failed to open database: {}", e)))?;

        let memory = Self { conn: Mutex::new(conn) };
        memory.init_schema()?;
        info!(path = %db_path.as_ref().display(), "SQLite Memory initialized");
        Ok(memory)
    }

    /// Create in-memory database (for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| OneClawError::Memory(format!("Failed to open in-memory db: {}", e)))?;
        let memory = Self { conn: Mutex::new(conn) };
        memory.init_schema()?;
        info!("SQLite Memory initialized (in-memory)");
        Ok(memory)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("Memory Mutex was poisoned during schema init, recovering");
            e.into_inner()
        });

        conn.execute_batch("
            -- Main entries table
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 1,
                source TEXT NOT NULL DEFAULT 'system',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- Temporal index (B-tree on created_at for range queries)
            CREATE INDEX IF NOT EXISTS idx_memory_created_at ON memory_entries(created_at);

            -- Priority index
            CREATE INDEX IF NOT EXISTS idx_memory_priority ON memory_entries(priority);

            -- Source index
            CREATE INDEX IF NOT EXISTS idx_memory_source ON memory_entries(source);

            -- FTS5 virtual table for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                content,
                tags,
                content=memory_entries,
                content_rowid=rowid,
                tokenize='unicode61 remove_diacritics 2'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_fts(rowid, content, tags)
                VALUES (new.rowid, new.content, new.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, tags)
                VALUES ('delete', old.rowid, old.content, old.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content, tags)
                VALUES ('delete', old.rowid, old.content, old.tags);
                INSERT INTO memory_fts(rowid, content, tags)
                VALUES (new.rowid, new.content, new.tags);
            END;
        ").map_err(|e| OneClawError::Memory(format!("Schema init failed: {}", e)))?;

        // TIP-040: Vector column migration (ALTER TABLE is idempotent via column check)
        self.migrate_vector_columns(&conn)?;

        Ok(())
    }

    /// TIP-040: Add vector columns if not already present (idempotent migration).
    fn migrate_vector_columns(&self, conn: &Connection) -> Result<()> {
        // Check if embedding column already exists using PRAGMA table_info
        let has_embedding: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(memory_entries)")
                .map_err(|e| OneClawError::Memory(format!("PRAGMA table_info: {}", e)))?;
            let columns: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(1))
                .map_err(|e| OneClawError::Memory(format!("Reading columns: {}", e)))?
                .filter_map(|r| r.ok())
                .collect();
            columns.iter().any(|c| c == "embedding")
        };

        if !has_embedding {
            conn.execute_batch("
                ALTER TABLE memory_entries ADD COLUMN embedding BLOB;
                ALTER TABLE memory_entries ADD COLUMN embedding_model TEXT;
                ALTER TABLE memory_entries ADD COLUMN embedding_dim INTEGER DEFAULT 0;
            ").map_err(|e| OneClawError::Memory(format!("Vector migration failed: {}", e)))?;

            debug!("Vector columns added to memory_entries");
        }

        Ok(())
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        Ok(self.conn.lock().unwrap_or_else(|e| {
            tracing::warn!("Memory Mutex was poisoned, recovering");
            e.into_inner()
        }))
    }
}

impl Memory for SqliteMemory {
    fn store(&self, content: &str, meta: MemoryMeta) -> Result<String> {
        let conn = self.lock_conn()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(&meta.tags)
            .map_err(|e| OneClawError::Memory(format!("Tags serialize: {}", e)))?;

        conn.execute(
            "INSERT INTO memory_entries (id, content, tags, priority, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, content, tags_json, meta.priority as i32, meta.source, now, now],
        ).map_err(|e| OneClawError::Memory(format!("Insert failed: {}", e)))?;

        debug!(id = %id, tags = %tags_json, "Memory entry stored");
        Ok(id)
    }

    fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, content, tags, priority, source, created_at, updated_at
             FROM memory_entries WHERE id = ?1"
        ).map_err(|e| OneClawError::Memory(format!("Prepare failed: {}", e)))?;

        let entry: Option<RawEntry> = stmt.query_row(params![id], |row| {
            Ok(RawEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                priority: row.get(3)?,
                source: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        }).optional()
        .map_err(|e| OneClawError::Memory(format!("Query failed: {}", e)))?;

        match entry {
            Some(raw) => Ok(Some(raw.into_memory_entry()?)),
            None => Ok(None),
        }
    }

    fn search(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>> {
        let conn = self.lock_conn()?;

        let use_fts = !query.text.is_empty();
        let sql = if use_fts {
            build_fts_query(query)
        } else {
            build_filtered_query(query)
        };

        debug!(sql = %sql, use_fts = use_fts, "Memory search");

        let mut stmt = conn.prepare(&sql)
            .map_err(|e| OneClawError::Memory(format!("Prepare search: {}", e)))?;

        let mut bound_params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];

        if use_fts {
            bound_params.push(Box::new(query.text.clone()));
        }
        if let Some(after) = &query.after {
            bound_params.push(Box::new(after.to_rfc3339()));
        }
        if let Some(before) = &query.before {
            bound_params.push(Box::new(before.to_rfc3339()));
        }
        if let Some(min_pri) = &query.min_priority {
            bound_params.push(Box::new(*min_pri as i32));
        }
        bound_params.push(Box::new(query.limit as i64));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = bound_params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(rusqlite::params_from_iter(param_refs.iter()), |row| {
            Ok(RawEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                priority: row.get(3)?,
                source: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        }).map_err(|e| OneClawError::Memory(format!("Search query: {}", e)))?;

        let mut results = vec![];
        for row in rows {
            let raw = row.map_err(|e| OneClawError::Memory(format!("Row read: {}", e)))?;
            results.push(raw.into_memory_entry()?);
        }

        Ok(results)
    }

    fn delete(&self, id: &str) -> Result<bool> {
        let conn = self.lock_conn()?;
        let affected = conn.execute(
            "DELETE FROM memory_entries WHERE id = ?1",
            params![id],
        ).map_err(|e| OneClawError::Memory(format!("Delete failed: {}", e)))?;
        Ok(affected > 0)
    }

    fn count(&self) -> Result<usize> {
        let conn = self.lock_conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_entries", [],
            |row| row.get(0),
        ).map_err(|e| OneClawError::Memory(format!("Count failed: {}", e)))?;
        Ok(count as usize)
    }

    fn as_vector(&self) -> Option<&dyn crate::memory::vector::VectorMemory> {
        Some(self)
    }
}

// ==================== VectorMemory Implementation ====================

use crate::memory::vector::*;

impl VectorMemory for SqliteMemory {
    fn store_with_embedding(&self, content: &str, meta: MemoryMeta, embedding: &Embedding) -> Result<String> {
        let conn = self.lock_conn()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(&meta.tags)
            .map_err(|e| OneClawError::Memory(format!("Tags serialize: {}", e)))?;
        let emb_bytes = embedding.to_bytes();

        conn.execute(
            "INSERT INTO memory_entries (id, content, tags, priority, source, created_at, updated_at, embedding, embedding_model, embedding_dim)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id, content, tags_json, meta.priority as i32, meta.source, now, now,
                emb_bytes, embedding.model, embedding.dim() as i32,
            ],
        ).map_err(|e| OneClawError::Memory(format!("Insert with embedding failed: {}", e)))?;

        debug!(id = %id, dim = embedding.dim(), model = %embedding.model, "Memory entry stored with embedding");
        Ok(id)
    }

    fn vector_search(&self, query: &VectorQuery) -> Result<Vec<VectorSearchResult>> {
        let conn = self.lock_conn()?;

        // Brute-force cosine scan: load all rows with embeddings, compute similarity
        let mut stmt = conn.prepare(
            "SELECT id, content, tags, priority, source, created_at, updated_at, embedding, embedding_model
             FROM memory_entries
             WHERE embedding IS NOT NULL"
        ).map_err(|e| OneClawError::Memory(format!("Prepare vector search: {}", e)))?;

        let query_values = &query.embedding.values;
        let mut scored: Vec<VectorSearchResult> = Vec::new();

        let rows = stmt.query_map([], |row| {
            let emb_bytes: Vec<u8> = row.get(7)?;
            let emb_model: String = row.get(8)?;
            Ok((RawEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                priority: row.get(3)?,
                source: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            }, emb_bytes, emb_model))
        }).map_err(|e| OneClawError::Memory(format!("Vector search query: {}", e)))?;

        for row in rows {
            let (raw, emb_bytes, emb_model) = row.map_err(|e| OneClawError::Memory(format!("Row read: {}", e)))?;
            if let Some(stored_emb) = Embedding::from_bytes(&emb_bytes, emb_model) {
                let sim = cosine_similarity(query_values, &stored_emb.values);
                if sim >= query.min_similarity {
                    scored.push(VectorSearchResult {
                        entry: raw.into_memory_entry()?,
                        similarity: sim,
                    });
                }
            }
        }

        // Sort by similarity descending, take limit
        scored.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(query.limit);
        Ok(scored)
    }

    fn hybrid_search(
        &self,
        text: &str,
        query_embedding: &Embedding,
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        // List A: FTS5 keyword search (ranked by FTS5 rank)
        let fts_query = MemoryQuery::new(text).with_limit(limit * 2);
        let fts_results = self.search(&fts_query)?;
        let fts_ranked: Vec<(String, f32)> = fts_results.iter()
            .enumerate()
            .map(|(rank, entry)| (entry.id.clone(), 1.0 / (1.0 + rank as f32)))
            .collect();

        // List B: Vector similarity search
        let vec_query = VectorQuery::new(Embedding::new(
            query_embedding.values.clone(),
            query_embedding.model.clone(),
        )).with_limit(limit * 2);
        let vec_results = self.vector_search(&vec_query)?;
        let vec_ranked: Vec<(String, f32)> = vec_results.iter()
            .map(|r| (r.entry.id.clone(), r.similarity))
            .collect();

        // Merge with RRF
        let rrf_scores = reciprocal_rank_fusion(&fts_ranked, &vec_ranked);

        // Build result set — look up entries from either result set
        let mut results: Vec<VectorSearchResult> = Vec::new();
        for (id, rrf_score) in rrf_scores.iter().take(limit) {
            // Try vector results first (has similarity score)
            if let Some(vr) = vec_results.iter().find(|r| r.entry.id == *id) {
                results.push(VectorSearchResult {
                    entry: vr.entry.clone(),
                    similarity: *rrf_score,
                });
            } else if let Some(fr) = fts_results.iter().find(|e| e.id == *id) {
                results.push(VectorSearchResult {
                    entry: fr.clone(),
                    similarity: *rrf_score,
                });
            }
        }

        Ok(results)
    }

    fn has_vector_support(&self) -> bool {
        true
    }

    fn vector_stats(&self) -> Result<VectorStats> {
        let conn = self.lock_conn()?;

        let embedded_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_entries WHERE embedding IS NOT NULL", [],
            |row| row.get(0),
        ).map_err(|e| OneClawError::Memory(format!("Vector stats count: {}", e)))?;

        let unembedded_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memory_entries WHERE embedding IS NULL", [],
            |row| row.get(0),
        ).map_err(|e| OneClawError::Memory(format!("Vector stats unembedded: {}", e)))?;

        // Get dim + model from first embedded entry (if any)
        let (dim, model): (usize, String) = if embedded_count > 0 {
            let row: (i32, String) = conn.query_row(
                "SELECT embedding_dim, embedding_model FROM memory_entries WHERE embedding IS NOT NULL LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).map_err(|e| OneClawError::Memory(format!("Vector stats dim: {}", e)))?;
            (row.0 as usize, row.1)
        } else {
            (0, String::new())
        };

        Ok(VectorStats {
            embedded_count: embedded_count as usize,
            unembedded_count: unembedded_count as usize,
            dimensions: dim,
            model,
        })
    }
}

// --- Internal helpers ---

/// Raw database row before conversion
struct RawEntry {
    id: String,
    content: String,
    tags: String,
    priority: i32,
    source: String,
    created_at: String,
    updated_at: String,
}

impl RawEntry {
    fn into_memory_entry(self) -> Result<MemoryEntry> {
        let tags: Vec<String> = serde_json::from_str(&self.tags)
            .unwrap_or_default();
        let priority = match self.priority {
            0 => Priority::Low,
            2 => Priority::High,
            3 => Priority::Critical,
            _ => Priority::Medium,
        };
        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let updated_at = DateTime::parse_from_rfc3339(&self.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(MemoryEntry {
            id: self.id,
            content: self.content,
            meta: MemoryMeta { tags, priority, source: self.source },
            created_at,
            updated_at,
        })
    }
}

/// Build FTS5 search query with optional filters
fn build_fts_query(query: &MemoryQuery) -> String {
    let mut sql = String::from(
        "SELECT e.id, e.content, e.tags, e.priority, e.source, e.created_at, e.updated_at
         FROM memory_entries e
         JOIN memory_fts f ON e.rowid = f.rowid
         WHERE memory_fts MATCH ?1"
    );

    let mut param_idx = 2;
    if query.after.is_some() {
        sql.push_str(&format!(" AND e.created_at >= ?{}", param_idx));
        param_idx += 1;
    }
    if query.before.is_some() {
        sql.push_str(&format!(" AND e.created_at <= ?{}", param_idx));
        param_idx += 1;
    }
    if query.min_priority.is_some() {
        sql.push_str(&format!(" AND e.priority >= ?{}", param_idx));
        param_idx += 1;
    }

    sql.push_str(&format!(" ORDER BY rank LIMIT ?{}", param_idx));
    sql
}

/// Build regular filtered query (no FTS)
fn build_filtered_query(query: &MemoryQuery) -> String {
    let mut sql = String::from(
        "SELECT id, content, tags, priority, source, created_at, updated_at
         FROM memory_entries WHERE 1=1"
    );

    let mut param_idx = 1;
    if query.after.is_some() {
        sql.push_str(&format!(" AND created_at >= ?{}", param_idx));
        param_idx += 1;
    }
    if query.before.is_some() {
        sql.push_str(&format!(" AND created_at <= ?{}", param_idx));
        param_idx += 1;
    }
    if query.min_priority.is_some() {
        sql.push_str(&format!(" AND priority >= ?{}", param_idx));
        param_idx += 1;
    }

    sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ?{}", param_idx));
    sql
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_memory() -> SqliteMemory {
        SqliteMemory::in_memory().unwrap()
    }

    #[test]
    fn test_store_and_get() {
        let mem = test_memory();
        let id = mem.store("temperature reading 42.5C", MemoryMeta {
            tags: vec!["sensor".into(), "temperature".into()],
            priority: Priority::High,
            source: "device_01".into(),
        }).unwrap();

        let entry = mem.get(&id).unwrap().unwrap();
        assert_eq!(entry.content, "temperature reading 42.5C");
        assert_eq!(entry.meta.tags, vec!["sensor", "temperature"]);
        assert_eq!(entry.meta.priority, Priority::High);
        assert_eq!(entry.meta.source, "device_01");
    }

    #[test]
    fn test_get_nonexistent() {
        let mem = test_memory();
        assert!(mem.get("nonexistent-id").unwrap().is_none());
    }

    #[test]
    fn test_fts_search() {
        let mem = test_memory();
        mem.store("blood pressure reading 140/90", MemoryMeta::default()).unwrap();
        mem.store("temperature 37.5 degrees", MemoryMeta::default()).unwrap();
        mem.store("heart rate 72 bpm normal", MemoryMeta::default()).unwrap();

        let results = mem.search(&MemoryQuery::new("blood pressure")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("140/90"));
    }

    #[test]
    fn test_search_multiple_matches() {
        let mem = test_memory();
        mem.store("morning blood pressure 135/85", MemoryMeta::default()).unwrap();
        mem.store("evening blood pressure 140/90", MemoryMeta::default()).unwrap();
        mem.store("temperature normal", MemoryMeta::default()).unwrap();

        let results = mem.search(&MemoryQuery::new("blood pressure")).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_with_limit() {
        let mem = test_memory();
        for i in 0..20 {
            mem.store(&format!("entry number {}", i), MemoryMeta::default()).unwrap();
        }

        let results = mem.search(&MemoryQuery::new("entry").with_limit(5)).unwrap();
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_by_priority() {
        let mem = test_memory();
        mem.store("low priority note", MemoryMeta { priority: Priority::Low, ..Default::default() }).unwrap();
        mem.store("critical alert!", MemoryMeta { priority: Priority::Critical, ..Default::default() }).unwrap();
        mem.store("medium note", MemoryMeta::default()).unwrap();

        let query = MemoryQuery::new("").with_min_priority(Priority::High);
        let results = mem.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("critical"));
    }

    #[test]
    fn test_search_by_time_range() {
        let mem = test_memory();

        mem.store("first entry", MemoryMeta::default()).unwrap();
        let after_first = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        mem.store("second entry", MemoryMeta::default()).unwrap();
        mem.store("third entry", MemoryMeta::default()).unwrap();

        let query = MemoryQuery::new("")
            .with_time_range(Some(after_first), None);
        let results = mem.search(&query).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_delete() {
        let mem = test_memory();
        let id = mem.store("to delete", MemoryMeta::default()).unwrap();
        assert_eq!(mem.count().unwrap(), 1);

        assert!(mem.delete(&id).unwrap());
        assert_eq!(mem.count().unwrap(), 0);

        assert!(!mem.delete("fake-id").unwrap());
    }

    #[test]
    fn test_count() {
        let mem = test_memory();
        assert_eq!(mem.count().unwrap(), 0);
        mem.store("one", MemoryMeta::default()).unwrap();
        mem.store("two", MemoryMeta::default()).unwrap();
        mem.store("three", MemoryMeta::default()).unwrap();
        assert_eq!(mem.count().unwrap(), 3);
    }

    #[test]
    fn test_fts_unicode_vietnamese() {
        let mem = test_memory();
        mem.store("Huyet ap ba Nguyen 140/90", MemoryMeta::default()).unwrap();
        mem.store("Nhiet do ong Tran 37.5", MemoryMeta::default()).unwrap();

        let results = mem.search(&MemoryQuery::new("Nguyen")).unwrap();
        assert!(!results.is_empty(), "Should find Vietnamese name");
        assert!(results[0].content.contains("Nguyen"));
    }

    #[test]
    fn test_persistent_storage() {
        let dir = std::env::temp_dir().join("oneclaw_test_persistent");
        let _ = std::fs::remove_dir_all(&dir);

        let db_path = dir.join("test.db");

        // Write
        {
            let mem = SqliteMemory::new(&db_path).unwrap();
            mem.store("persistent data", MemoryMeta::default()).unwrap();
            assert_eq!(mem.count().unwrap(), 1);
        }

        // Read back after reopen
        {
            let mem = SqliteMemory::new(&db_path).unwrap();
            assert_eq!(mem.count().unwrap(), 1);
            let results = mem.search(&MemoryQuery::new("persistent")).unwrap();
            assert_eq!(results.len(), 1);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ==================== Vector Memory Tests ====================

    fn make_embedding(values: Vec<f32>) -> Embedding {
        Embedding::new(values, "test-model")
    }

    #[test]
    fn test_store_with_embedding() {
        let mem = test_memory();
        let emb = make_embedding(vec![1.0, 2.0, 3.0]);
        let id = mem.store_with_embedding("embedded content", MemoryMeta::default(), &emb).unwrap();

        // Should be retrievable via normal get
        let entry = mem.get(&id).unwrap().unwrap();
        assert_eq!(entry.content, "embedded content");

        // Count includes embedded entries
        assert_eq!(mem.count().unwrap(), 1);
    }

    #[test]
    fn test_vector_search_basic() {
        let mem = test_memory();
        let emb_a = make_embedding(vec![1.0, 0.0, 0.0]);
        let emb_b = make_embedding(vec![0.0, 1.0, 0.0]);
        let emb_c = make_embedding(vec![0.9, 0.1, 0.0]); // similar to A

        mem.store_with_embedding("doc A", MemoryMeta::default(), &emb_a).unwrap();
        mem.store_with_embedding("doc B", MemoryMeta::default(), &emb_b).unwrap();
        mem.store_with_embedding("doc C", MemoryMeta::default(), &emb_c).unwrap();

        // Query similar to A
        let query = VectorQuery::new(make_embedding(vec![1.0, 0.0, 0.0]));
        let results = mem.vector_search(&query).unwrap();

        assert_eq!(results.len(), 3);
        // First result should be doc A (exact match, similarity 1.0)
        assert_eq!(results[0].entry.content, "doc A");
        assert!((results[0].similarity - 1.0).abs() < 1e-5);
        // Second should be doc C (most similar to A)
        assert_eq!(results[1].entry.content, "doc C");
        assert!(results[1].similarity > 0.9);
    }

    #[test]
    fn test_vector_search_with_limit() {
        let mem = test_memory();
        for i in 0..10 {
            let emb = make_embedding(vec![i as f32, 0.0]);
            mem.store_with_embedding(&format!("doc {}", i), MemoryMeta::default(), &emb).unwrap();
        }

        let query = VectorQuery::new(make_embedding(vec![5.0, 0.0])).with_limit(3);
        let results = mem.vector_search(&query).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_vector_search_min_similarity() {
        let mem = test_memory();
        let emb_close = make_embedding(vec![1.0, 0.0]);
        let emb_far = make_embedding(vec![0.0, 1.0]);

        mem.store_with_embedding("close", MemoryMeta::default(), &emb_close).unwrap();
        mem.store_with_embedding("far", MemoryMeta::default(), &emb_far).unwrap();

        let query = VectorQuery::new(make_embedding(vec![1.0, 0.0])).with_min_similarity(0.9);
        let results = mem.vector_search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.content, "close");
    }

    #[test]
    fn test_vector_search_empty_db() {
        let mem = test_memory();
        let query = VectorQuery::new(make_embedding(vec![1.0, 0.0]));
        let results = mem.vector_search(&query).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_vector_search_skips_non_embedded() {
        let mem = test_memory();
        // Store one with embedding, one without
        mem.store_with_embedding("with vec", MemoryMeta::default(), &make_embedding(vec![1.0, 0.0])).unwrap();
        mem.store("without vec", MemoryMeta::default()).unwrap();

        let query = VectorQuery::new(make_embedding(vec![1.0, 0.0]));
        let results = mem.vector_search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.content, "with vec");
    }

    #[test]
    fn test_hybrid_search_merges_fts_and_vector() {
        let mem = test_memory();

        // Doc that matches keyword "blood pressure" + has embedding close to query
        mem.store_with_embedding(
            "blood pressure 140/90",
            MemoryMeta::default(),
            &make_embedding(vec![1.0, 0.0, 0.0]),
        ).unwrap();

        // Doc that matches keyword only (no embedding)
        mem.store("blood pressure 130/80", MemoryMeta::default()).unwrap();

        // Doc with embedding only (no keyword match)
        mem.store_with_embedding(
            "vital signs normal",
            MemoryMeta::default(),
            &make_embedding(vec![0.95, 0.1, 0.0]),
        ).unwrap();

        let results = mem.hybrid_search(
            "blood pressure",
            &make_embedding(vec![1.0, 0.0, 0.0]),
            10,
        ).unwrap();

        // Should find all 3 (two from FTS, two from vector, merged by RRF)
        assert!(results.len() >= 2, "Hybrid should merge results, got {}", results.len());
        // The "blood pressure 140/90" doc should rank highest (appears in both lists)
        assert!(
            results[0].entry.content.contains("blood pressure 140/90"),
            "Top result should be the one in both lists, got: {}",
            results[0].entry.content
        );
    }

    #[test]
    fn test_has_vector_support() {
        let mem = test_memory();
        assert!(mem.has_vector_support());
    }

    #[test]
    fn test_vector_stats_empty() {
        let mem = test_memory();
        let stats = mem.vector_stats().unwrap();
        assert_eq!(stats.embedded_count, 0);
        assert_eq!(stats.unembedded_count, 0);
        assert_eq!(stats.dimensions, 0);
        assert!(stats.model.is_empty());
    }

    #[test]
    fn test_vector_stats_with_data() {
        let mem = test_memory();
        mem.store_with_embedding("a", MemoryMeta::default(), &make_embedding(vec![1.0, 2.0, 3.0])).unwrap();
        mem.store_with_embedding("b", MemoryMeta::default(), &make_embedding(vec![4.0, 5.0, 6.0])).unwrap();
        mem.store("c", MemoryMeta::default()).unwrap();

        let stats = mem.vector_stats().unwrap();
        assert_eq!(stats.embedded_count, 2);
        assert_eq!(stats.unembedded_count, 1);
        assert_eq!(stats.dimensions, 3);
        assert_eq!(stats.model, "test-model");
    }

    #[test]
    fn test_vector_search_performance_1000() {
        let mem = test_memory();

        // Insert 1000 entries with 128-dim embeddings
        for i in 0..1000 {
            let mut values = vec![0.0_f32; 128];
            values[i % 128] = 1.0;
            values[(i + 1) % 128] = 0.5;
            let emb = Embedding::new(values, "perf-model");
            mem.store_with_embedding(&format!("perf entry {}", i), MemoryMeta::default(), &emb).unwrap();
        }

        let query_emb = {
            let mut v = vec![0.0_f32; 128];
            v[0] = 1.0;
            v[1] = 0.5;
            Embedding::new(v, "perf-model")
        };

        let start = std::time::Instant::now();
        let query = VectorQuery::new(query_emb).with_limit(10);
        let results = mem.vector_search(&query).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(results.len(), 10);
        assert!(
            elapsed.as_millis() < 100,
            "1000-entry vector search should complete in <100ms, took {}ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_schema_migration_idempotent() {
        let dir = std::env::temp_dir().join("oneclaw_test_vector_migration");
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("migrate.db");

        // First open: creates schema + vector columns
        {
            let mem = SqliteMemory::new(&db_path).unwrap();
            mem.store_with_embedding("data", MemoryMeta::default(), &make_embedding(vec![1.0])).unwrap();
        }

        // Second open: migration is idempotent (no error)
        {
            let mem = SqliteMemory::new(&db_path).unwrap();
            assert_eq!(mem.count().unwrap(), 1);

            // Vector data survives reopen
            let stats = mem.vector_stats().unwrap();
            assert_eq!(stats.embedded_count, 1);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

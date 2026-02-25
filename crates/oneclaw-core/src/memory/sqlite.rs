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
}

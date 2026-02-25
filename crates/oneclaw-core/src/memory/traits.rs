//! Layer 2: Memory Trait — Brain
//! Stores and retrieves information with metadata.
//! Supports multiple search strategies (keyword, temporal, semantic).

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Priority level for memory entries
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority.
    Low = 0,
    /// Medium priority (default).
    #[default]
    Medium = 1,
    /// High priority.
    High = 2,
    /// Critical priority.
    Critical = 3,
}

/// Metadata for a memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMeta {
    /// The tags associated with this entry.
    pub tags: Vec<String>,
    /// The priority of this entry.
    pub priority: Priority,
    /// The source that created this entry.
    pub source: String,
}

impl Default for MemoryMeta {
    fn default() -> Self {
        Self {
            tags: vec![],
            priority: Priority::Medium,
            source: "system".into(),
        }
    }
}

/// A stored memory entry with all metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The unique identifier of this entry.
    pub id: String,
    /// The text content of this entry.
    pub content: String,
    /// The metadata of this entry.
    pub meta: MemoryMeta,
    /// The creation timestamp of this entry.
    pub created_at: DateTime<Utc>,
    /// The last-updated timestamp of this entry.
    pub updated_at: DateTime<Utc>,
}

/// Search query with multiple dimensions
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// Text to search (used for keyword/FTS and semantic matching)
    pub text: String,
    /// Filter by tags (AND logic: entry must have ALL specified tags)
    pub tags: Vec<String>,
    /// The earliest creation time to include.
    pub after: Option<DateTime<Utc>>,
    /// The latest creation time to include.
    pub before: Option<DateTime<Utc>>,
    /// Filter by minimum priority
    pub min_priority: Option<Priority>,
    /// Maximum results
    pub limit: usize,
}

impl MemoryQuery {
    /// Create a new query searching for the given text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: 10,
            ..Default::default()
        }
    }

    /// Filter results to entries that contain all specified tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags; self
    }

    /// Filter results to entries within the given time range.
    pub fn with_time_range(mut self, after: Option<DateTime<Utc>>, before: Option<DateTime<Utc>>) -> Self {
        self.after = after;
        self.before = before;
        self
    }

    /// Set the maximum number of results to return.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit; self
    }

    /// Filter results to entries at or above the given priority.
    pub fn with_min_priority(mut self, priority: Priority) -> Self {
        self.min_priority = Some(priority); self
    }
}

/// Layer 2 Trait: Memory
pub trait Memory: Send + Sync {
    /// Store content with metadata. Returns entry ID.
    fn store(&self, content: &str, meta: MemoryMeta) -> Result<String>;

    /// Retrieve entry by ID
    fn get(&self, id: &str) -> Result<Option<MemoryEntry>>;

    /// Search with multi-dimensional query
    fn search(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>>;

    /// Delete entry by ID
    fn delete(&self, id: &str) -> Result<bool>;

    /// Count total entries
    fn count(&self) -> Result<usize>;
}

/// NoopMemory — in-memory stub for testing
pub struct NoopMemory {
    entries: std::sync::Mutex<Vec<MemoryEntry>>,
}

impl NoopMemory {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self { entries: std::sync::Mutex::new(vec![]) }
    }
}

impl Default for NoopMemory {
    fn default() -> Self { Self::new() }
}

impl Memory for NoopMemory {
    fn store(&self, content: &str, meta: MemoryMeta) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let entry = MemoryEntry {
            id: id.clone(),
            content: content.to_string(),
            meta,
            created_at: now,
            updated_at: now,
        };
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).push(entry);
        Ok(id)
    }

    fn get(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        Ok(entries.iter().find(|e| e.id == id).cloned())
    }

    fn search(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let results: Vec<_> = entries.iter()
            .filter(|e| {
                if !query.text.is_empty() && !e.content.to_lowercase().contains(&query.text.to_lowercase()) {
                    return false;
                }
                if !query.tags.is_empty() && !query.tags.iter().all(|t| e.meta.tags.contains(t)) {
                    return false;
                }
                if let Some(after) = query.after
                    && e.created_at < after { return false; }
                if let Some(before) = query.before
                    && e.created_at > before { return false; }
                if let Some(min) = query.min_priority
                    && (e.meta.priority as u8) < (min as u8) { return false; }
                true
            })
            .take(query.limit)
            .cloned()
            .collect();
        Ok(results)
    }

    fn delete(&self, id: &str) -> Result<bool> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let len_before = entries.len();
        entries.retain(|e| e.id != id);
        Ok(entries.len() < len_before)
    }

    fn count(&self) -> Result<usize> {
        Ok(self.entries.lock().unwrap_or_else(|e| e.into_inner()).len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_store_and_get() {
        let mem = NoopMemory::new();
        let id = mem.store("hello world", MemoryMeta::default()).unwrap();
        let entry = mem.get(&id).unwrap().unwrap();
        assert_eq!(entry.content, "hello world");
    }

    #[test]
    fn test_noop_search_by_text() {
        let mem = NoopMemory::new();
        mem.store("blood pressure 140/90", MemoryMeta::default()).unwrap();
        mem.store("temperature 37.5", MemoryMeta::default()).unwrap();

        let results = mem.search(&MemoryQuery::new("blood pressure")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("140/90"));
    }

    #[test]
    fn test_noop_search_by_tags() {
        let mem = NoopMemory::new();
        mem.store("pressure reading", MemoryMeta { tags: vec!["sensor".into(), "pressure".into()], ..Default::default() }).unwrap();
        mem.store("temp reading", MemoryMeta { tags: vec!["sensor".into(), "temp".into()], ..Default::default() }).unwrap();

        let query = MemoryQuery::new("").with_tags(vec!["pressure".into()]);
        let results = mem.search(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("pressure"));
    }

    #[test]
    fn test_noop_delete() {
        let mem = NoopMemory::new();
        let id = mem.store("to delete", MemoryMeta::default()).unwrap();
        assert_eq!(mem.count().unwrap(), 1);
        assert!(mem.delete(&id).unwrap());
        assert_eq!(mem.count().unwrap(), 0);
    }

    #[test]
    fn test_noop_count() {
        let mem = NoopMemory::new();
        assert_eq!(mem.count().unwrap(), 0);
        mem.store("one", MemoryMeta::default()).unwrap();
        mem.store("two", MemoryMeta::default()).unwrap();
        assert_eq!(mem.count().unwrap(), 2);
    }
}

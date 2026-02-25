//! Vector Search Interface for Memory Layer
//! TIP-040: Embedding storage, cosine similarity, hybrid search (FTS5 + vector with RRF).

use crate::error::Result;
use crate::memory::traits::{MemoryEntry, MemoryMeta};
use serde::{Deserialize, Serialize};

// ==================== TYPES ====================

/// A dense embedding vector (f32 for edge-device efficiency).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// The raw vector values.
    pub values: Vec<f32>,
    /// The model that produced this embedding (e.g. "nomic-embed-text").
    pub model: String,
}

impl Embedding {
    /// Create a new embedding from values and model name.
    pub fn new(values: Vec<f32>, model: impl Into<String>) -> Self {
        Self { values, model: model.into() }
    }

    /// Dimensionality of this embedding.
    pub fn dim(&self) -> usize {
        self.values.len()
    }

    /// Serialize to bytes (little-endian f32 sequence) for SQLite BLOB storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.values.len() * 4);
        for &v in &self.values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf
    }

    /// Deserialize from bytes (little-endian f32 sequence).
    pub fn from_bytes(bytes: &[u8], model: impl Into<String>) -> Option<Self> {
        if !bytes.len().is_multiple_of(4) {
            return None;
        }
        let values: Vec<f32> = bytes.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        Some(Self { values, model: model.into() })
    }

    /// L2 norm (magnitude) of the vector.
    pub fn norm(&self) -> f32 {
        self.values.iter().map(|v| v * v).sum::<f32>().sqrt()
    }
}

/// Query for vector-based search.
#[derive(Debug, Clone)]
pub struct VectorQuery {
    /// The query embedding to compare against.
    pub embedding: Embedding,
    /// Maximum number of results to return.
    pub limit: usize,
    /// Minimum cosine similarity threshold (0.0 to 1.0).
    pub min_similarity: f32,
}

impl VectorQuery {
    /// Create a new vector query.
    pub fn new(embedding: Embedding) -> Self {
        Self {
            embedding,
            limit: 10,
            min_similarity: 0.0,
        }
    }

    /// Set the maximum number of results.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit; self
    }

    /// Set minimum similarity threshold.
    pub fn with_min_similarity(mut self, threshold: f32) -> Self {
        self.min_similarity = threshold; self
    }
}

/// A search result with similarity score.
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    /// The matching memory entry.
    pub entry: MemoryEntry,
    /// Cosine similarity score (0.0 to 1.0).
    pub similarity: f32,
}

/// Statistics about vector storage.
#[derive(Debug, Clone, Default)]
pub struct VectorStats {
    /// Total entries with embeddings.
    pub embedded_count: usize,
    /// Total entries without embeddings.
    pub unembedded_count: usize,
    /// Embedding dimensionality (0 if no embeddings).
    pub dimensions: usize,
    /// The embedding model name (empty if no embeddings).
    pub model: String,
}

// ==================== TRAIT ====================

/// Extension trait for vector-based memory search.
/// Implementations store embeddings alongside memory entries and support
/// cosine similarity search and hybrid (FTS5 + vector) search with RRF.
pub trait VectorMemory: crate::memory::Memory {
    /// Store content with an embedding vector.
    fn store_with_embedding(&self, content: &str, meta: MemoryMeta, embedding: &Embedding) -> Result<String>;

    /// Pure vector similarity search (brute-force cosine scan).
    fn vector_search(&self, query: &VectorQuery) -> Result<Vec<VectorSearchResult>>;

    /// Hybrid search: merge FTS5 keyword results and vector similarity results using RRF.
    fn hybrid_search(
        &self,
        text: &str,
        query_embedding: &Embedding,
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>>;

    /// Check if vector search is supported and operational.
    fn has_vector_support(&self) -> bool;

    /// Return statistics about vector storage.
    fn vector_stats(&self) -> Result<VectorStats>;
}

// ==================== UTILITY FUNCTIONS ====================

/// Cosine similarity between two vectors.
/// Returns 0.0 if either vector is zero-length or dimensions mismatch.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut norm_a = 0.0_f32;
    let mut norm_b = 0.0_f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Reciprocal Rank Fusion (RRF) to merge two ranked lists.
///
/// Each input is a list of (entry_id, score) tuples, ranked by score descending.
/// Returns merged (entry_id, rrf_score) sorted by rrf_score descending.
///
/// RRF formula: score(d) = sum( 1 / (k + rank_i(d)) ) for each list i
/// where k = 60 (standard constant).
pub fn reciprocal_rank_fusion(
    list_a: &[(String, f32)],
    list_b: &[(String, f32)],
) -> Vec<(String, f32)> {
    const K: f32 = 60.0;

    let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    for (rank, (id, _score)) in list_a.iter().enumerate() {
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (K + rank as f32 + 1.0);
    }

    for (rank, (id, _score)) in list_b.iter().enumerate() {
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (K + rank as f32 + 1.0);
    }

    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

// ==================== NoopMemory VectorMemory ====================

impl VectorMemory for crate::memory::NoopMemory {
    fn store_with_embedding(&self, content: &str, meta: MemoryMeta, _embedding: &Embedding) -> Result<String> {
        // Delegate to normal store, ignore embedding (in-memory stub)
        crate::memory::Memory::store(self, content, meta)
    }

    fn vector_search(&self, _query: &VectorQuery) -> Result<Vec<VectorSearchResult>> {
        Ok(vec![]) // No vector support in noop
    }

    fn hybrid_search(&self, _text: &str, _query_embedding: &Embedding, _limit: usize) -> Result<Vec<VectorSearchResult>> {
        Ok(vec![]) // No vector support in noop
    }

    fn has_vector_support(&self) -> bool {
        false
    }

    fn vector_stats(&self) -> Result<VectorStats> {
        Ok(VectorStats::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Embedding tests ---

    #[test]
    fn test_embedding_new() {
        let emb = Embedding::new(vec![1.0, 2.0, 3.0], "test-model");
        assert_eq!(emb.dim(), 3);
        assert_eq!(emb.model, "test-model");
    }

    #[test]
    fn test_embedding_bytes_roundtrip() {
        let original = Embedding::new(vec![1.0, -2.5, 3.125, 0.0], "model-a");
        let bytes = original.to_bytes();
        assert_eq!(bytes.len(), 16); // 4 floats * 4 bytes
        let restored = Embedding::from_bytes(&bytes, "model-a").unwrap();
        assert_eq!(restored.values, original.values);
        assert_eq!(restored.model, "model-a");
    }

    #[test]
    fn test_embedding_from_bytes_invalid_length() {
        assert!(Embedding::from_bytes(&[1, 2, 3], "m").is_none()); // not multiple of 4
    }

    #[test]
    fn test_embedding_from_bytes_empty() {
        let emb = Embedding::from_bytes(&[], "m").unwrap();
        assert!(emb.values.is_empty());
    }

    #[test]
    fn test_embedding_norm() {
        let emb = Embedding::new(vec![3.0, 4.0], "m");
        assert!((emb.norm() - 5.0).abs() < 1e-6);
    }

    // --- Cosine similarity tests ---

    #[test]
    fn test_cosine_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-6, "Identical vectors should have similarity 1.0, got {}", sim);
    }

    #[test]
    fn test_cosine_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Orthogonal vectors should have similarity 0.0, got {}", sim);
    }

    #[test]
    fn test_cosine_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6, "Opposite vectors should have similarity -1.0, got {}", sim);
    }

    #[test]
    fn test_cosine_dimension_mismatch() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_empty_vectors() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similar_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.1, 2.1, 2.9];
        let sim = cosine_similarity(&a, &b);
        assert!(sim > 0.99, "Similar vectors should have high similarity, got {}", sim);
    }

    // --- RRF tests ---

    #[test]
    fn test_rrf_empty_lists() {
        let result = reciprocal_rank_fusion(&[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_single_list() {
        let list_a = vec![
            ("id1".into(), 0.9),
            ("id2".into(), 0.8),
        ];
        let result = reciprocal_rank_fusion(&list_a, &[]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "id1"); // rank 1 gets higher RRF score
        assert_eq!(result[1].0, "id2");
    }

    #[test]
    fn test_rrf_both_lists_same_items() {
        let list_a = vec![
            ("id1".into(), 0.9),
            ("id2".into(), 0.8),
        ];
        let list_b = vec![
            ("id1".into(), 0.95),
            ("id2".into(), 0.85),
        ];
        let result = reciprocal_rank_fusion(&list_a, &list_b);
        assert_eq!(result.len(), 2);
        // id1 is rank 1 in both lists → highest RRF
        assert_eq!(result[0].0, "id1");
    }

    #[test]
    fn test_rrf_disjoint_lists() {
        let list_a = vec![("a".into(), 0.9)];
        let list_b = vec![("b".into(), 0.9)];
        let result = reciprocal_rank_fusion(&list_a, &list_b);
        assert_eq!(result.len(), 2);
        // Both are rank 1 in their respective list → equal RRF score
        assert!((result[0].1 - result[1].1).abs() < 1e-6);
    }

    #[test]
    fn test_rrf_boosted_by_appearing_in_both() {
        let list_a = vec![
            ("a".into(), 0.9),
            ("b".into(), 0.8),
            ("c".into(), 0.7),
        ];
        let list_b = vec![
            ("c".into(), 0.95),  // c is rank 1 in list_b but rank 3 in list_a
            ("d".into(), 0.85),
        ];
        let result = reciprocal_rank_fusion(&list_a, &list_b);
        // c appears in both lists → boosted score
        let c_score = result.iter().find(|(id, _)| id == "c").unwrap().1;
        let d_score = result.iter().find(|(id, _)| id == "d").unwrap().1;
        assert!(c_score > d_score, "c (in both lists) should outscore d (only in one)");
    }

    // --- VectorQuery builder tests ---

    #[test]
    fn test_vector_query_defaults() {
        let emb = Embedding::new(vec![1.0, 2.0], "m");
        let q = VectorQuery::new(emb);
        assert_eq!(q.limit, 10);
        assert_eq!(q.min_similarity, 0.0);
    }

    #[test]
    fn test_vector_query_builder() {
        let emb = Embedding::new(vec![1.0], "m");
        let q = VectorQuery::new(emb).with_limit(5).with_min_similarity(0.7);
        assert_eq!(q.limit, 5);
        assert_eq!(q.min_similarity, 0.7);
    }
}

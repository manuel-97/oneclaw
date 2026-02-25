//! TIP-021: SQLite Memory Stress Tests
//!
//! Validates data integrity under stress:
//! - Bulk insert (1000 entries)
//! - Concurrent read/write from multiple threads
//! - Store-search-delete cycle integrity
//! - Vietnamese FTS search accuracy
//! - Priority filtering correctness
//! - Time range query accuracy
//! - Delete and re-search consistency
//! - Large content handling

use oneclaw_core::memory::{Memory, MemoryMeta, MemoryQuery, Priority};
use oneclaw_core::memory::SqliteMemory;
use std::sync::Arc;

fn test_memory() -> SqliteMemory {
    SqliteMemory::in_memory().unwrap()
}

// ==================== BULK INSERT ====================

#[test]
fn test_bulk_insert_1000_entries() {
    let mem = test_memory();

    for i in 0..1000 {
        let content = format!("Device {} | reading = {}/90 | temp = {:.1}°C",
            i, 120 + (i % 40), 20.0 + (i % 30) as f64 * 0.1);
        mem.store(&content, MemoryMeta {
            tags: vec![format!("device_{}", i), "sensor".into()],
            priority: match i % 4 {
                0 => Priority::Low,
                1 => Priority::Medium,
                2 => Priority::High,
                _ => Priority::Critical,
            },
            source: format!("sensor_{}", i % 10),
        }).unwrap();
    }

    assert_eq!(mem.count().unwrap(), 1000);

    // Search should still be fast
    let results = mem.search(&MemoryQuery::new("Device").with_limit(50)).unwrap();
    assert_eq!(results.len(), 50, "Should return exactly limit entries");
}

// ==================== CONCURRENT ACCESS ====================

#[test]
fn test_concurrent_memory_access() {
    let mem = Arc::new(test_memory());
    let mut handles = vec![];

    // 10 writer threads
    for t in 0..10 {
        let mem_clone = Arc::clone(&mem);
        handles.push(std::thread::spawn(move || {
            for i in 0..50 {
                mem_clone.store(
                    &format!("Thread {} entry {}", t, i),
                    MemoryMeta {
                        tags: vec![format!("thread_{}", t)],
                        ..Default::default()
                    },
                ).unwrap();
            }
        }));
    }

    // 5 reader threads
    for _ in 0..5 {
        let mem_clone = Arc::clone(&mem);
        handles.push(std::thread::spawn(move || {
            for _ in 0..20 {
                let _ = mem_clone.search(&MemoryQuery::new("Thread").with_limit(10));
                let _ = mem_clone.count();
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // All writes should have completed
    assert_eq!(mem.count().unwrap(), 500, "10 threads × 50 entries = 500");
}

// ==================== STORE-SEARCH-DELETE CYCLE ====================

#[test]
fn test_store_search_delete_cycle() {
    let mem = test_memory();

    // Store
    let ids: Vec<String> = (0..100).map(|i| {
        mem.store(
            &format!("Cycle entry {}", i),
            MemoryMeta {
                tags: vec!["cycle".into()],
                priority: if i % 2 == 0 { Priority::High } else { Priority::Low },
                ..Default::default()
            },
        ).unwrap()
    }).collect();

    assert_eq!(mem.count().unwrap(), 100);

    // Search
    let results = mem.search(&MemoryQuery::new("Cycle").with_limit(200)).unwrap();
    assert_eq!(results.len(), 100);

    // Delete even entries
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 0 {
            assert!(mem.delete(id).unwrap());
        }
    }

    assert_eq!(mem.count().unwrap(), 50);

    // Search again — should only find odd entries
    let results = mem.search(&MemoryQuery::new("Cycle").with_limit(200)).unwrap();
    assert_eq!(results.len(), 50);

    // Verify deleted entries are truly gone
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 0 {
            assert!(mem.get(id).unwrap().is_none(), "Deleted entry {} should be gone", i);
        } else {
            assert!(mem.get(id).unwrap().is_some(), "Kept entry {} should exist", i);
        }
    }
}

// ==================== UNICODE FTS ACCURACY ====================

#[test]
fn test_unicode_fts_search_accuracy() {
    let mem = test_memory();

    // Store entries with various content
    mem.store("Device Alpha: temperature = 22.5°C — normal", MemoryMeta {
        tags: vec!["temperature".into()],
        ..Default::default()
    }).unwrap();

    mem.store("Device Beta: humidity = 65% — normal", MemoryMeta {
        tags: vec!["humidity".into()],
        ..Default::default()
    }).unwrap();

    mem.store("Device Alpha: pressure = 1013 hPa — stable", MemoryMeta {
        tags: vec!["pressure".into()],
        ..Default::default()
    }).unwrap();

    mem.store("Device Beta: light = 450 lux — normal", MemoryMeta {
        tags: vec!["light".into()],
        ..Default::default()
    }).unwrap();

    // Search for specific device
    let results = mem.search(&MemoryQuery::new("Alpha")).unwrap();
    assert!(!results.is_empty(), "Should find entries for Alpha: got {}", results.len());
    for r in &results {
        assert!(r.content.contains("Alpha"), "Result should contain Alpha: {}", r.content);
    }

    // Search for specific measurement type
    let results = mem.search(&MemoryQuery::new("humidity")).unwrap();
    assert!(!results.is_empty(), "Should find humidity entry");
}

// ==================== PRIORITY FILTERING ====================

#[test]
fn test_priority_filtering_accuracy() {
    let mem = test_memory();

    mem.store("low note", MemoryMeta { priority: Priority::Low, ..Default::default() }).unwrap();
    mem.store("medium note", MemoryMeta { priority: Priority::Medium, ..Default::default() }).unwrap();
    mem.store("high alert", MemoryMeta { priority: Priority::High, ..Default::default() }).unwrap();
    mem.store("critical emergency", MemoryMeta { priority: Priority::Critical, ..Default::default() }).unwrap();

    // Min priority = Critical → should only find 1
    let results = mem.search(&MemoryQuery::new("").with_min_priority(Priority::Critical)).unwrap();
    assert_eq!(results.len(), 1, "Only critical: got {}", results.len());
    assert!(results[0].content.contains("critical"));

    // Min priority = High → should find 2
    let results = mem.search(&MemoryQuery::new("").with_min_priority(Priority::High)).unwrap();
    assert_eq!(results.len(), 2, "High + critical: got {}", results.len());

    // Min priority = Medium → should find 3
    let results = mem.search(&MemoryQuery::new("").with_min_priority(Priority::Medium)).unwrap();
    assert_eq!(results.len(), 3, "Medium + high + critical: got {}", results.len());

    // Min priority = Low → should find all 4
    let results = mem.search(&MemoryQuery::new("").with_min_priority(Priority::Low)).unwrap();
    assert_eq!(results.len(), 4, "All priorities: got {}", results.len());
}

// ==================== TIME RANGE QUERIES ====================

#[test]
fn test_time_range_query_accuracy() {
    let mem = test_memory();

    // Store entries with small delays to ensure time ordering
    mem.store("first entry", MemoryMeta::default()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(15));

    let midpoint = chrono::Utc::now();
    std::thread::sleep(std::time::Duration::from_millis(15));

    mem.store("second entry", MemoryMeta::default()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(15));
    mem.store("third entry", MemoryMeta::default()).unwrap();

    // After midpoint → should get entries 2 and 3
    let results = mem.search(
        &MemoryQuery::new("").with_time_range(Some(midpoint), None)
    ).unwrap();
    assert_eq!(results.len(), 2, "After midpoint should find 2: got {}", results.len());

    // Before midpoint → should get entry 1
    let results = mem.search(
        &MemoryQuery::new("").with_time_range(None, Some(midpoint))
    ).unwrap();
    assert_eq!(results.len(), 1, "Before midpoint should find 1: got {}", results.len());
}

// ==================== DELETE + RE-SEARCH CONSISTENCY ====================

#[test]
fn test_delete_removes_from_fts_index() {
    let mem = test_memory();

    let id = mem.store("unique_keyword_xyzzy content here", MemoryMeta::default()).unwrap();

    // Should find it
    let results = mem.search(&MemoryQuery::new("unique_keyword_xyzzy")).unwrap();
    assert_eq!(results.len(), 1);

    // Delete
    assert!(mem.delete(&id).unwrap());

    // Should NOT find it anymore
    let results = mem.search(&MemoryQuery::new("unique_keyword_xyzzy")).unwrap();
    assert_eq!(results.len(), 0, "Deleted entry should not appear in FTS results");
}

#[test]
fn test_double_delete() {
    let mem = test_memory();

    let id = mem.store("to delete twice", MemoryMeta::default()).unwrap();
    assert!(mem.delete(&id).unwrap());    // First delete: true
    assert!(!mem.delete(&id).unwrap());   // Second delete: false (already gone)
}

// ==================== LARGE CONTENT ====================

#[test]
fn test_large_content_store_and_retrieve() {
    let mem = test_memory();

    // 50KB content
    let large = "Sensor data entry record. ".repeat(2000);
    assert!(large.len() > 50_000);

    let id = mem.store(&large, MemoryMeta::default()).unwrap();
    let entry = mem.get(&id).unwrap().unwrap();
    assert_eq!(entry.content.len(), large.len());
}

#[test]
fn test_many_tags() {
    let mem = test_memory();

    // 100 tags per entry
    let tags: Vec<String> = (0..100).map(|i| format!("tag_{}", i)).collect();
    let id = mem.store("entry with many tags", MemoryMeta {
        tags: tags.clone(),
        ..Default::default()
    }).unwrap();

    let entry = mem.get(&id).unwrap().unwrap();
    assert_eq!(entry.meta.tags.len(), 100);
}

// ==================== EDGE CASES ====================

#[test]
fn test_special_characters_in_content() {
    let mem = test_memory();

    let tricky = vec![
        "Content with 'single quotes' and \"double quotes\"",
        "SQL injection: '; DROP TABLE memory_entries; --",
        "Backslash: C:\\Users\\test\\file.txt",
        "Newlines:\nLine2\nLine3\n",
        "Tabs:\tCol1\tCol2\tCol3",
        "Unicode escapes: \\u0041 \\n \\t",
        "Percent: 100% completion",
        "Ampersand & angle brackets < > {} []",
    ];

    for content in &tricky {
        let id = mem.store(content, MemoryMeta::default()).unwrap();
        let entry = mem.get(&id).unwrap().unwrap();
        assert_eq!(&entry.content, content, "Content mismatch for: {}", content);
    }
}

#[test]
fn test_empty_tags_and_source() {
    let mem = test_memory();

    let id = mem.store("entry with empty meta", MemoryMeta {
        tags: vec![],
        priority: Priority::Medium,
        source: String::new(),
    }).unwrap();

    let entry = mem.get(&id).unwrap().unwrap();
    assert!(entry.meta.tags.is_empty());
}

#[test]
fn test_rapid_store_count_consistency() {
    let mem = test_memory();

    for i in 0..500 {
        mem.store(&format!("rapid entry {}", i), MemoryMeta::default()).unwrap();
    }

    // Count should match exactly
    assert_eq!(mem.count().unwrap(), 500);
}

//! SqliteSecurityStore — Persistent paired device storage
//!
//! Replaces the flat-file registry with SQLite for richer device records
//! that survive restarts: device_id, paired_at, label, last_seen.

use crate::error::{OneClawError, Result};
use crate::security::traits::PairedDevice;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// SQLite-backed security persistence store
pub struct SqliteSecurityStore {
    conn: Mutex<Connection>,
}

impl SqliteSecurityStore {
    /// Create a new SqliteSecurityStore (file-backed)
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)
            .map_err(|e| OneClawError::Security(format!("Failed to open security DB: {}", e)))?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_tables()?;
        Ok(store)
    }

    /// Create an in-memory store (for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| OneClawError::Security(format!("Failed to open in-memory DB: {}", e)))?;
        let store = Self { conn: Mutex::new(conn) };
        store.init_tables()?;
        Ok(store)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS paired_devices (
                device_id TEXT PRIMARY KEY,
                paired_at TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                last_seen TEXT NOT NULL
            );"
        ).map_err(|e| OneClawError::Security(format!("Failed to create tables: {}", e)))?;
        Ok(())
    }

    /// Store a paired device record
    pub fn store_device(&self, device: &PairedDevice) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO paired_devices (device_id, paired_at, label, last_seen)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                device.device_id,
                device.paired_at.to_rfc3339(),
                device.label,
                device.last_seen.to_rfc3339(),
            ],
        ).map_err(|e| OneClawError::Security(format!("Failed to store device: {}", e)))?;
        Ok(())
    }

    /// List all paired devices
    pub fn list_devices(&self) -> Result<Vec<PairedDevice>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT device_id, paired_at, label, last_seen FROM paired_devices ORDER BY paired_at DESC"
        ).map_err(|e| OneClawError::Security(format!("Failed to prepare query: {}", e)))?;

        let devices = stmt.query_map([], |row| {
            let device_id: String = row.get(0)?;
            let paired_at_str: String = row.get(1)?;
            let label: String = row.get(2)?;
            let last_seen_str: String = row.get(3)?;

            let paired_at = chrono::DateTime::parse_from_rfc3339(&paired_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let last_seen = chrono::DateTime::parse_from_rfc3339(&last_seen_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(PairedDevice {
                device_id,
                paired_at,
                label,
                last_seen,
            })
        }).map_err(|e| OneClawError::Security(format!("Failed to query devices: {}", e)))?;

        let mut result = Vec::new();
        for d in devices.flatten() {
            result.push(d);
        }
        Ok(result)
    }

    /// Remove a device by exact device_id
    pub fn remove_device(&self, device_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let rows = conn.execute(
            "DELETE FROM paired_devices WHERE device_id = ?1",
            rusqlite::params![device_id],
        ).map_err(|e| OneClawError::Security(format!("Failed to remove device: {}", e)))?;
        Ok(rows > 0)
    }

    /// Get a device by exact device_id
    pub fn get_device(&self, device_id: &str) -> Result<Option<PairedDevice>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT device_id, paired_at, label, last_seen FROM paired_devices WHERE device_id = ?1"
        ).map_err(|e| OneClawError::Security(format!("Failed to prepare query: {}", e)))?;

        let mut rows = stmt.query_map(rusqlite::params![device_id], |row| {
            let device_id: String = row.get(0)?;
            let paired_at_str: String = row.get(1)?;
            let label: String = row.get(2)?;
            let last_seen_str: String = row.get(3)?;

            let paired_at = chrono::DateTime::parse_from_rfc3339(&paired_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let last_seen = chrono::DateTime::parse_from_rfc3339(&last_seen_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(PairedDevice {
                device_id,
                paired_at,
                label,
                last_seen,
            })
        }).map_err(|e| OneClawError::Security(format!("Failed to query device: {}", e)))?;

        match rows.next() {
            Some(Ok(device)) => Ok(Some(device)),
            _ => Ok(None),
        }
    }

    /// Find a device by prefix match (for unpair command)
    pub fn find_by_prefix(&self, prefix: &str) -> Result<Vec<PairedDevice>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let pattern = format!("{}%", prefix);
        let mut stmt = conn.prepare(
            "SELECT device_id, paired_at, label, last_seen FROM paired_devices WHERE device_id LIKE ?1"
        ).map_err(|e| OneClawError::Security(format!("Failed to prepare query: {}", e)))?;

        let devices = stmt.query_map(rusqlite::params![pattern], |row| {
            let device_id: String = row.get(0)?;
            let paired_at_str: String = row.get(1)?;
            let label: String = row.get(2)?;
            let last_seen_str: String = row.get(3)?;

            let paired_at = chrono::DateTime::parse_from_rfc3339(&paired_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let last_seen = chrono::DateTime::parse_from_rfc3339(&last_seen_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            Ok(PairedDevice {
                device_id,
                paired_at,
                label,
                last_seen,
            })
        }).map_err(|e| OneClawError::Security(format!("Failed to query devices: {}", e)))?;

        let mut result = Vec::new();
        for d in devices.flatten() {
            result.push(d);
        }
        Ok(result)
    }

    /// Update the last_seen timestamp for a device
    pub fn update_last_seen(&self, device_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE paired_devices SET last_seen = ?1 WHERE device_id = ?2",
            rusqlite::params![now, device_id],
        ).map_err(|e| OneClawError::Security(format!("Failed to update last_seen: {}", e)))?;
        Ok(())
    }

    /// Count paired devices
    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM paired_devices",
            [],
            |row| row.get(0),
        ).map_err(|e| OneClawError::Security(format!("Failed to count devices: {}", e)))?;
        Ok(count as usize)
    }

    /// Load all device IDs into a HashSet (for DefaultSecurity compatibility)
    pub fn load_device_ids(&self) -> Result<std::collections::HashSet<String>> {
        let devices = self.list_devices()?;
        Ok(devices.into_iter().map(|d| d.device_id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::traits::Identity;

    #[test]
    fn test_store_and_list_devices() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        assert_eq!(store.count().unwrap(), 0);

        let device = PairedDevice::from_identity(&Identity {
            device_id: "test-device-001".into(),
            paired_at: chrono::Utc::now(),
        });
        store.store_device(&device).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        let devices = store.list_devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "test-device-001");
    }

    #[test]
    fn test_remove_device() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        let device = PairedDevice::from_identity(&Identity {
            device_id: "remove-me".into(),
            paired_at: chrono::Utc::now(),
        });
        store.store_device(&device).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        assert!(store.remove_device("remove-me").unwrap());
        assert_eq!(store.count().unwrap(), 0);

        // Removing non-existent returns false
        assert!(!store.remove_device("ghost").unwrap());
    }

    #[test]
    fn test_get_device() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        let device = PairedDevice::from_identity(&Identity {
            device_id: "get-me".into(),
            paired_at: chrono::Utc::now(),
        });
        store.store_device(&device).unwrap();

        let found = store.get_device("get-me").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().device_id, "get-me");

        let not_found = store.get_device("ghost").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_by_prefix() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        for id in ["abc-111", "abc-222", "def-333"] {
            let device = PairedDevice::from_identity(&Identity {
                device_id: id.into(),
                paired_at: chrono::Utc::now(),
            });
            store.store_device(&device).unwrap();
        }

        let matches = store.find_by_prefix("abc").unwrap();
        assert_eq!(matches.len(), 2);

        let matches = store.find_by_prefix("def").unwrap();
        assert_eq!(matches.len(), 1);

        let matches = store.find_by_prefix("xyz").unwrap();
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_update_last_seen() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        let device = PairedDevice::from_identity(&Identity {
            device_id: "seen-device".into(),
            paired_at: chrono::Utc::now(),
        });
        let original_seen = device.last_seen;
        store.store_device(&device).unwrap();

        // Small sleep to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.update_last_seen("seen-device").unwrap();

        let updated = store.get_device("seen-device").unwrap().unwrap();
        assert!(updated.last_seen >= original_seen);
    }

    #[test]
    fn test_load_device_ids() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        for id in ["d1", "d2", "d3"] {
            let device = PairedDevice::from_identity(&Identity {
                device_id: id.into(),
                paired_at: chrono::Utc::now(),
            });
            store.store_device(&device).unwrap();
        }

        let ids = store.load_device_ids().unwrap();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains("d1"));
        assert!(ids.contains("d2"));
        assert!(ids.contains("d3"));
    }

    #[test]
    fn test_store_device_upsert() {
        let store = SqliteSecurityStore::in_memory().unwrap();
        let mut device = PairedDevice::from_identity(&Identity {
            device_id: "upsert-me".into(),
            paired_at: chrono::Utc::now(),
        });
        store.store_device(&device).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        // Update with label
        device.label = "Pi living room".into();
        store.store_device(&device).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        let found = store.get_device("upsert-me").unwrap().unwrap();
        assert_eq!(found.label, "Pi living room");
    }

    #[test]
    fn test_file_backed_persistence() {
        let tmp = std::env::temp_dir().join("oneclaw_test_security.db");
        let _ = std::fs::remove_file(&tmp);

        // Store a device
        {
            let store = SqliteSecurityStore::new(&tmp).unwrap();
            let device = PairedDevice::from_identity(&Identity {
                device_id: "persist-test".into(),
                paired_at: chrono::Utc::now(),
            });
            store.store_device(&device).unwrap();
        }

        // Reopen and verify device survives
        {
            let store = SqliteSecurityStore::new(&tmp).unwrap();
            assert_eq!(store.count().unwrap(), 1);
            let device = store.get_device("persist-test").unwrap().unwrap();
            assert_eq!(device.device_id, "persist-test");
        }

        let _ = std::fs::remove_file(&tmp);
    }
}

//! DefaultSecurity — Production security implementation
//! Deny-by-default. Filesystem scoping. Pairing codes.

use crate::error::{OneClawError, Result};
use crate::security::traits::{SecurityCore, Action, ActionKind, Permit, Identity, PairedDevice};
use crate::security::path_guard::PathGuard;
use crate::security::pairing::PairingManager;
use crate::security::persistence::SqliteSecurityStore;
use std::collections::HashSet;
use std::sync::Mutex;
use std::path::PathBuf;

/// Production-ready Security Core implementation.
///
/// Enforces deny-by-default: unpaired devices are rejected.
/// All filesystem access is validated through PathGuard.
/// Pairing codes are one-time, TTL-bound, and cryptographically random.
/// Paired devices optionally persist to SQLite (survives restarts).
pub struct DefaultSecurity {
    path_guard: PathGuard,
    pairing: PairingManager,
    /// Set of paired device IDs (in-memory cache)
    paired_devices: Mutex<HashSet<String>>,
    /// Whether pairing is required for actions
    pairing_required: bool,
    /// Legacy flat-file registry path (deprecated, kept for backward compat)
    registry_path: Option<PathBuf>,
    /// SQLite persistence store (replaces flat-file registry)
    persistence: Option<SqliteSecurityStore>,
}

impl DefaultSecurity {
    /// Create a new DefaultSecurity with explicit configuration.
    pub fn new(
        workspace: impl Into<PathBuf>,
        workspace_only: bool,
        pairing_required: bool,
        pairing_code_ttl_seconds: i64,
    ) -> Self {
        Self {
            path_guard: PathGuard::new(workspace, workspace_only),
            pairing: PairingManager::new(pairing_code_ttl_seconds),
            paired_devices: Mutex::new(HashSet::new()),
            pairing_required,
            registry_path: None,
            persistence: None,
        }
    }

    /// Create with sensible defaults for production:
    /// workspace_only=true, pairing_required=true, 5 min TTL.
    pub fn production(workspace: impl Into<PathBuf>) -> Self {
        Self::new(workspace, true, true, 300)
    }

    /// Create with relaxed settings for development:
    /// workspace_only=true, pairing_required=false, 1 hour TTL.
    pub fn development(workspace: impl Into<PathBuf>) -> Self {
        Self::new(workspace, true, false, 3600)
    }

    /// Set a persistent registry file path. Loads existing paired devices on set.
    pub fn with_registry_path(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        // Load existing paired devices from file
        if let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(mut devices) = self.paired_devices.lock()
        {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    devices.insert(trimmed.to_string());
                }
            }
            if !devices.is_empty() {
                tracing::info!(count = devices.len(), "Loaded paired devices from registry");
            }
        }
        self.registry_path = Some(path);
        self
    }

    /// Set a SQLite persistence store. Loads existing paired devices into memory.
    pub fn with_persistence(mut self, store: SqliteSecurityStore) -> Self {
        // Load existing paired devices from SQLite into in-memory cache
        if let Ok(ids) = store.load_device_ids()
            && let Ok(mut devices) = self.paired_devices.lock()
        {
            for id in &ids {
                devices.insert(id.clone());
            }
            if !ids.is_empty() {
                tracing::info!(count = ids.len(), "Loaded paired devices from SQLite");
            }
        }
        self.persistence = Some(store);
        self
    }

    /// Persist current paired devices to registry file
    fn persist_registry(&self) {
        if let Some(ref path) = self.registry_path
            && let Ok(devices) = self.paired_devices.lock()
        {
            let content: Vec<&str> = devices.iter().map(|s| s.as_str()).collect();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(path, content.join("\n")) {
                tracing::warn!("Failed to persist device registry: {}", e);
            }
        }
    }
}

impl DefaultSecurity {
    /// Verify pairing code and grant access atomically.
    /// PairingManager.verify() atomically marks the code as used within its lock.
    /// Then we insert into paired_devices with poison recovery to ensure grant succeeds.
    fn verify_and_grant(&self, code: &str) -> Result<Identity> {
        // Step 1: Atomic verify (within PairingManager lock — marks code as used)
        let identity = self.pairing.verify(code)?;

        // Step 2: Grant access (with poison recovery — must not fail after code consumed)
        {
            let mut devices = self.paired_devices.lock()
                .unwrap_or_else(|e| e.into_inner());
            devices.insert(identity.device_id.clone());
        }

        // Step 3: Persist to SQLite if configured
        if let Some(ref store) = self.persistence {
            let device = PairedDevice::from_identity(&identity);
            if let Err(e) = store.store_device(&device) {
                tracing::warn!("Failed to persist device to SQLite: {}", e);
            }
        }

        // Step 3b: Legacy flat-file persistence (backward compat)
        self.persist_registry();

        Ok(identity)
    }
}

impl SecurityCore for DefaultSecurity {
    fn authorize(&self, action: &Action) -> Result<Permit> {
        // Deny-by-default: check pairing first (skip for PairDevice actions)
        if self.pairing_required && !matches!(action.kind, ActionKind::PairDevice) {
            let devices = self.paired_devices.lock()
                .unwrap_or_else(|e| e.into_inner());
            if !devices.contains(&action.actor) && action.actor != "system" {
                return Ok(Permit {
                    granted: false,
                    reason: format!("Device '{}' not paired. Pair first.", action.actor),
                });
            }
        }

        // Action-specific checks
        match &action.kind {
            ActionKind::PairDevice => {
                // Pairing is always allowed (it's how you get authorized)
                Ok(Permit { granted: true, reason: "Pairing action allowed".into() })
            }
            ActionKind::Read | ActionKind::Write => {
                // Check filesystem path
                let path = std::path::Path::new(&action.resource);
                match self.path_guard.check(path) {
                    Ok(()) => Ok(Permit { granted: true, reason: "Path check passed".into() }),
                    Err(e) => Ok(Permit { granted: false, reason: format!("{}", e) }),
                }
            }
            ActionKind::Execute => {
                Ok(Permit { granted: true, reason: "Execution allowed for paired device".into() })
            }
            ActionKind::Network => {
                Ok(Permit { granted: true, reason: "Network allowed for paired device".into() })
            }
        }
    }

    fn check_path(&self, path: &std::path::Path) -> Result<()> {
        self.path_guard.check(path)
    }

    fn generate_pairing_code(&self) -> Result<String> {
        self.pairing.generate()
    }

    fn verify_pairing_code(&self, code: &str) -> Result<Identity> {
        self.verify_and_grant(code)
    }

    fn list_devices(&self) -> Result<Vec<PairedDevice>> {
        if let Some(ref store) = self.persistence {
            store.list_devices()
        } else {
            // Fallback: build PairedDevice list from in-memory HashSet
            let devices = self.paired_devices.lock()
                .unwrap_or_else(|e| e.into_inner());
            Ok(devices.iter().map(|id| PairedDevice {
                device_id: id.clone(),
                paired_at: chrono::Utc::now(),
                label: String::new(),
                last_seen: chrono::Utc::now(),
            }).collect())
        }
    }

    fn remove_device(&self, device_id_prefix: &str) -> Result<PairedDevice> {
        // Find matching device(s)
        if let Some(ref store) = self.persistence {
            let matches = store.find_by_prefix(device_id_prefix)?;
            match matches.len() {
                0 => Err(OneClawError::Security(format!("No device matching '{}'", device_id_prefix))),
                1 => {
                    // Safe: we just checked len() == 1
                    let device = matches.into_iter().next()
                        .ok_or_else(|| OneClawError::Security("unexpected empty match".into()))?;
                    store.remove_device(&device.device_id)?;
                    // Also remove from in-memory cache
                    if let Ok(mut devices) = self.paired_devices.lock() {
                        devices.remove(&device.device_id);
                    }
                    Ok(device)
                }
                n => Err(OneClawError::Security(format!(
                    "Ambiguous: '{}' matches {} devices. Use longer prefix.", device_id_prefix, n
                ))),
            }
        } else {
            // No persistence — search in-memory
            let mut devices = self.paired_devices.lock()
                .unwrap_or_else(|e| e.into_inner());
            let matches: Vec<String> = devices.iter()
                .filter(|id| id.starts_with(device_id_prefix))
                .cloned()
                .collect();
            match matches.len() {
                0 => Err(OneClawError::Security(format!("No device matching '{}'", device_id_prefix))),
                1 => {
                    // Safe: we just checked len() == 1
                    let id = matches.into_iter().next()
                        .ok_or_else(|| OneClawError::Security("unexpected empty match".into()))?;
                    devices.remove(&id);
                    Ok(PairedDevice {
                        device_id: id,
                        paired_at: chrono::Utc::now(),
                        label: String::new(),
                        last_seen: chrono::Utc::now(),
                    })
                }
                n => Err(OneClawError::Security(format!(
                    "Ambiguous: '{}' matches {} devices. Use longer prefix.", device_id_prefix, n
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn test_security() -> DefaultSecurity {
        DefaultSecurity::production(env::current_dir().unwrap())
    }

    #[test]
    fn test_unpaired_device_denied() {
        let sec = test_security();
        let action = Action {
            kind: ActionKind::Read,
            resource: ".".into(),
            actor: "unknown-device".into(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(!permit.granted);
        assert!(permit.reason.contains("not paired"));
    }

    #[test]
    fn test_pairing_action_always_allowed() {
        let sec = test_security();
        let action = Action {
            kind: ActionKind::PairDevice,
            resource: "".into(),
            actor: "new-device".into(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_full_pairing_flow() {
        let sec = test_security();

        // 1. Generate code
        let code = sec.generate_pairing_code().unwrap();
        assert_eq!(code.len(), 6);

        // 2. Verify code -> get identity
        let identity = sec.verify_pairing_code(&code).unwrap();

        // 3. Now device can perform actions
        let action = Action {
            kind: ActionKind::Read,
            resource: env::current_dir().unwrap().join("Cargo.toml").to_string_lossy().into(),
            actor: identity.device_id.clone(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_system_actor_bypasses_pairing() {
        let sec = test_security();
        let action = Action {
            kind: ActionKind::Execute,
            resource: "internal".into(),
            actor: "system".into(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_blocked_path_denied_even_for_paired() {
        let sec = test_security();

        // Pair a device
        let code = sec.generate_pairing_code().unwrap();
        let identity = sec.verify_pairing_code(&code).unwrap();

        // Try to access /etc/passwd
        let action = Action {
            kind: ActionKind::Read,
            resource: "/etc/passwd".into(),
            actor: identity.device_id,
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(!permit.granted);
    }

    #[test]
    fn test_development_mode_no_pairing_required() {
        let sec = DefaultSecurity::development(env::current_dir().unwrap());
        let action = Action {
            kind: ActionKind::Read,
            resource: env::current_dir().unwrap().join("Cargo.toml").to_string_lossy().into(),
            actor: "any-device".into(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_persistent_device_registry() {
        let tmp = env::temp_dir().join("oneclaw_test_registry.txt");
        let _ = std::fs::remove_file(&tmp);

        // Pair a device and verify it is persisted
        let sec = DefaultSecurity::production(env::current_dir().unwrap())
            .with_registry_path(&tmp);
        let code = sec.generate_pairing_code().unwrap();
        let identity = sec.verify_pairing_code(&code).unwrap();

        // Registry file should exist and contain the device ID
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains(&identity.device_id));

        // Create a new DefaultSecurity and load from file — device should still be paired
        let sec2 = DefaultSecurity::production(env::current_dir().unwrap())
            .with_registry_path(&tmp);
        let action = Action {
            kind: ActionKind::Read,
            resource: env::current_dir().unwrap().join("Cargo.toml").to_string_lossy().into(),
            actor: identity.device_id,
        };
        let permit = sec2.authorize(&action).unwrap();
        assert!(permit.granted, "Device should be authorized after loading from registry");

        let _ = std::fs::remove_file(&tmp);
    }
}

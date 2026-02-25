//! Security trait definitions — SecurityCore trait, Action, Permit, Identity, NoopSecurity

use crate::error::Result;

/// Action that requires authorization
#[derive(Debug, Clone)]
pub struct Action {
    /// The kind of action being requested.
    pub kind: ActionKind,
    /// The resource this action targets.
    pub resource: String,
    /// The actor requesting this action.
    pub actor: String,
}

/// Kind of action being requested
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Read access to a resource.
    Read,
    /// Write access to a resource.
    Write,
    /// Execute a command or tool.
    Execute,
    /// Network access.
    Network,
    /// Pair a new device.
    PairDevice,
}

/// Authorization permit
#[derive(Debug, Clone)]
pub struct Permit {
    /// Whether the action was authorized.
    pub granted: bool,
    /// The reason for the authorization decision.
    pub reason: String,
}

/// Device identity after pairing
#[derive(Debug, Clone)]
pub struct Identity {
    /// The unique identifier of the paired device.
    pub device_id: String,
    /// The timestamp when the device was paired.
    pub paired_at: chrono::DateTime<chrono::Utc>,
}

/// Persistent paired device record (survives restarts)
#[derive(Debug, Clone)]
pub struct PairedDevice {
    /// The unique identifier of the paired device.
    pub device_id: String,
    /// The timestamp when the device was paired.
    pub paired_at: chrono::DateTime<chrono::Utc>,
    /// Human-readable label (e.g., "Raspberry Pi phòng khách")
    pub label: String,
    /// Last time this device was seen (activity timestamp)
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

impl PairedDevice {
    /// Create a new PairedDevice from an Identity
    pub fn from_identity(identity: &Identity) -> Self {
        Self {
            device_id: identity.device_id.clone(),
            paired_at: identity.paired_at,
            label: String::new(),
            last_seen: identity.paired_at,
        }
    }
}

/// Layer 0 Trait: Security Core
pub trait SecurityCore: Send + Sync {
    /// Authorize an action. Deny-by-default.
    fn authorize(&self, action: &Action) -> Result<Permit>;

    /// Check if a filesystem path is allowed
    fn check_path(&self, path: &std::path::Path) -> Result<()>;

    /// Generate a one-time pairing code
    fn generate_pairing_code(&self) -> Result<String>;

    /// Verify a pairing code and return device identity
    fn verify_pairing_code(&self, code: &str) -> Result<Identity>;

    /// List all paired devices
    fn list_devices(&self) -> Result<Vec<PairedDevice>>;

    /// Remove a paired device by device_id (exact or prefix match)
    fn remove_device(&self, device_id_prefix: &str) -> Result<PairedDevice>;
}

/// NoopSecurity: Allows everything. FOR TESTING ONLY.
pub struct NoopSecurity;

impl SecurityCore for NoopSecurity {
    fn authorize(&self, _action: &Action) -> Result<Permit> {
        Ok(Permit { granted: true, reason: "noop: all allowed".into() })
    }

    fn check_path(&self, _path: &std::path::Path) -> Result<()> {
        Ok(())
    }

    fn generate_pairing_code(&self) -> Result<String> {
        Ok("000000".to_string())
    }

    fn verify_pairing_code(&self, _code: &str) -> Result<Identity> {
        Ok(Identity {
            device_id: "noop-device".into(),
            paired_at: chrono::Utc::now(),
        })
    }

    fn list_devices(&self) -> Result<Vec<PairedDevice>> {
        Ok(vec![])
    }

    fn remove_device(&self, _device_id_prefix: &str) -> Result<PairedDevice> {
        Err(crate::error::OneClawError::Security("Noop: no devices to remove".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_security_allows_all() {
        let sec = NoopSecurity;
        let action = Action {
            kind: ActionKind::Read,
            resource: "/some/path".into(),
            actor: "test".into(),
        };
        let permit = sec.authorize(&action).unwrap();
        assert!(permit.granted);
    }

    #[test]
    fn test_noop_pairing() {
        let sec = NoopSecurity;
        let code = sec.generate_pairing_code().unwrap();
        let identity = sec.verify_pairing_code(&code).unwrap();
        assert_eq!(identity.device_id, "noop-device");
    }
}

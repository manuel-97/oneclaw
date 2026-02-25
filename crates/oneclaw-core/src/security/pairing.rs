//! Device pairing — one-time 6-digit codes with TTL expiry

use crate::error::{OneClawError, Result};
use crate::security::traits::Identity;
use std::collections::HashMap;
use std::sync::Mutex;

/// Generate a cryptographically random 6-digit code using `ring`.
fn generate_code() -> Result<String> {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 4];
    rng.fill(&mut bytes)
        .map_err(|_| OneClawError::Security("RNG failed".into()))?;
    let num = u32::from_be_bytes(bytes) % 1_000_000;
    Ok(format!("{:06}", num))
}

/// Manages one-time pairing codes with TTL expiry.
pub struct PairingManager {
    /// Active pairing codes: code -> (created_at, used)
    codes: Mutex<HashMap<String, (chrono::DateTime<chrono::Utc>, bool)>>,
    /// Code validity duration in seconds
    code_ttl_seconds: i64,
}

impl PairingManager {
    /// Create a new PairingManager with the given code TTL in seconds.
    pub fn new(code_ttl_seconds: i64) -> Self {
        Self {
            codes: Mutex::new(HashMap::new()),
            code_ttl_seconds,
        }
    }

    /// Generate a new pairing code. Expired codes are cleaned up automatically.
    pub fn generate(&self) -> Result<String> {
        let code = generate_code()?;
        let mut codes = self.codes.lock()
            .map_err(|_| OneClawError::Security("Lock poisoned".into()))?;

        // Clean expired codes
        let now = chrono::Utc::now();
        codes.retain(|_, (created, used)| {
            !*used && (now - *created).num_seconds() < self.code_ttl_seconds
        });

        codes.insert(code.clone(), (now, false));
        Ok(code)
    }

    /// Verify a pairing code. Returns Identity if valid. Code is consumed (one-time use).
    pub fn verify(&self, code: &str) -> Result<Identity> {
        let mut codes = self.codes.lock()
            .map_err(|_| OneClawError::Security("Lock poisoned".into()))?;

        match codes.get_mut(code) {
            None => Err(OneClawError::Security("Invalid pairing code".into())),
            Some((created, used)) => {
                if *used {
                    return Err(OneClawError::Security("Pairing code already used".into()));
                }
                let now = chrono::Utc::now();
                if (now - *created).num_seconds() >= self.code_ttl_seconds {
                    codes.remove(code);
                    return Err(OneClawError::Security("Pairing code expired".into()));
                }
                *used = true;
                Ok(Identity {
                    device_id: uuid::Uuid::new_v4().to_string(),
                    paired_at: now,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_format() {
        let mgr = PairingManager::new(300);
        let code = mgr.generate().unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_verify_valid_code() {
        let mgr = PairingManager::new(300);
        let code = mgr.generate().unwrap();
        let identity = mgr.verify(&code).unwrap();
        assert!(!identity.device_id.is_empty());
    }

    #[test]
    fn test_code_single_use() {
        let mgr = PairingManager::new(300);
        let code = mgr.generate().unwrap();
        assert!(mgr.verify(&code).is_ok());
        assert!(mgr.verify(&code).is_err()); // second use fails
    }

    #[test]
    fn test_invalid_code_rejected() {
        let mgr = PairingManager::new(300);
        assert!(mgr.verify("999999").is_err());
    }

    #[test]
    fn test_expired_code_rejected() {
        let mgr = PairingManager::new(0); // 0 second TTL = instant expire
        let code = mgr.generate().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(mgr.verify(&code).is_err());
    }
}

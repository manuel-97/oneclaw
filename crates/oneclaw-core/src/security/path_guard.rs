//! Filesystem path validation — prevent escape from workspace

use std::path::{Path, PathBuf};
use crate::error::{OneClawError, Result};

/// Blocked system directories (deny access even if not workspace_only)
const BLOCKED_DIRS: &[&str] = &[
    "/etc", "/usr", "/bin", "/sbin", "/boot", "/dev", "/proc", "/sys",
    "/var/run", "/var/log", "/root", "/lib", "/lib64", "/snap", "/mnt",
];

/// Blocked sensitive dotfiles
const BLOCKED_DOTFILES: &[&str] = &[
    ".ssh", ".gnupg", ".config/gcloud", ".aws",
];

/// Guards filesystem access within allowed boundaries.
pub struct PathGuard {
    workspace: PathBuf,
    workspace_only: bool,
}

impl PathGuard {
    /// Create a new PathGuard scoped to the given workspace directory.
    pub fn new(workspace: impl Into<PathBuf>, workspace_only: bool) -> Self {
        Self {
            workspace: workspace.into(),
            workspace_only,
        }
    }

    /// Validate that a path is safe to access.
    pub fn check(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();

        // 1. Null byte injection check
        if path_str.contains('\0') {
            return Err(OneClawError::Security(
                "Null byte detected in path".into()
            ));
        }

        // 2. Resolve to canonical path (catches symlink escapes)
        //    If path doesn't exist yet, check the parent
        let canonical = if path.exists() {
            path.canonicalize()
                .map_err(|e| OneClawError::Security(format!("Cannot resolve path: {}", e)))?
        } else if let Some(parent) = path.parent() {
            if parent.exists() {
                let mut resolved = parent.canonicalize()
                    .map_err(|e| OneClawError::Security(format!("Cannot resolve parent: {}", e)))?;
                if let Some(filename) = path.file_name() {
                    resolved.push(filename);
                }
                resolved
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        let canonical_str = canonical.to_string_lossy();
        let original_str = path.to_string_lossy();

        // 3. Check blocked system directories (check both original and canonical
        //    to handle symlinks like /etc -> /private/etc on macOS)
        for blocked in BLOCKED_DIRS {
            if canonical_str.starts_with(blocked) || original_str.starts_with(blocked) {
                return Err(OneClawError::Security(
                    format!("Access to system directory blocked: {}", blocked)
                ));
            }
        }

        // 4. Check blocked dotfiles (check both original and canonical)
        for dotfile in BLOCKED_DOTFILES {
            if canonical_str.contains(dotfile) || original_str.contains(dotfile) {
                return Err(OneClawError::Security(
                    format!("Access to sensitive dotfile blocked: {}", dotfile)
                ));
            }
        }

        // 5. Workspace scoping
        if self.workspace_only {
            let workspace_canonical = self.workspace.canonicalize()
                .unwrap_or_else(|_| self.workspace.clone());
            if !canonical.starts_with(&workspace_canonical) {
                return Err(OneClawError::Security(
                    format!("Path outside workspace: {} (workspace: {})",
                        canonical_str, workspace_canonical.display())
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn test_guard() -> PathGuard {
        PathGuard::new(env::current_dir().unwrap(), true)
    }

    #[test]
    fn test_null_byte_rejected() {
        let guard = test_guard();
        let path = Path::new("/tmp/evil\0.txt");
        assert!(guard.check(path).is_err());
    }

    #[test]
    fn test_blocked_system_dir() {
        let guard = PathGuard::new("/some/workspace", false);
        let path = Path::new("/etc/passwd");
        assert!(guard.check(path).is_err());
    }

    #[test]
    fn test_blocked_dotfile() {
        let guard = PathGuard::new("/some/workspace", false);
        let path = Path::new("/home/user/.ssh/id_rsa");
        assert!(guard.check(path).is_err());
    }

    #[test]
    fn test_workspace_scoping() {
        let workspace = env::current_dir().unwrap();
        let guard = PathGuard::new(&workspace, true);

        // File within workspace should pass
        let in_workspace = workspace.join("Cargo.toml");
        if in_workspace.exists() {
            assert!(guard.check(&in_workspace).is_ok());
        }

        // File outside workspace should fail
        let outside = Path::new("/tmp/outside.txt");
        if outside.parent().unwrap().exists() && !workspace.starts_with("/tmp") {
            assert!(guard.check(outside).is_err());
        }
    }
}

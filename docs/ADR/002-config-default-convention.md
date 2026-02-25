# ADR-002: Config Default Convention

**Status:** Accepted
**Date:** 2026-02-22
**Decided by:** Chu thau (TIP-003 Verify)

## Context

Rust's `#[derive(Default)]` produces zero-values (`false` for bool, `""` for String, `0` for numbers). However, OneClaw's config structs need non-zero defaults matching serde's `#[serde(default = "...")]` attributes — for example, `deny_by_default = true` and `providers.default = "noop"`.

This mismatch was discovered independently in TIP-001 (SecurityConfig), TIP-002 (boundary edge), and TIP-003 (ProvidersConfig), confirming it as a recurring pattern rather than a one-off issue.

## Decision

**Config structs whose serde defaults differ from Rust zero-values MUST implement `Default` manually (not `#[derive(Default)]`).**

The manual `Default` impl MUST produce values identical to the serde defaults, so that:
- `OneClawConfig::default()` == parsing an empty TOML string
- Code that constructs config programmatically gets the same safe defaults as file-based config

## Consequences

- Every new config struct must be checked: if any field's serde default differs from its type's zero-value, write a manual `Default` impl.
- `#[derive(Default)]` is still fine for structs where zero-values ARE the correct defaults (e.g., non-config data structs).
- This convention is enforced by review, not by tooling. Tests like `test_config_defaults_when_sections_missing` serve as regression guards.

## Examples

```rust
// WRONG — derive gives deny_by_default = false
#[derive(Default, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub deny_by_default: bool,
}

// CORRECT — manual impl matches serde defaults
impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            deny_by_default: true,
            pairing_required: true,
            workspace_only: true,
        }
    }
}
```

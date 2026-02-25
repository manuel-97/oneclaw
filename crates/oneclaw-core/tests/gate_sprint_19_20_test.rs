//! Sprint 19-20 Gate Verification Tests — TIP-031
//!
//! These tests enforce project-level invariants that must hold
//! before tagging v1.2 Field Ready.

use oneclaw_core::config::OneClawConfig;

// ═══════════════════════════════════════════════════
// Gate: Security persistence defaults
// ═══════════════════════════════════════════════════

#[test]
fn gate_security_persistence_enabled_by_default() {
    let config = OneClawConfig::default_config();
    assert!(
        config.security.persist_pairing,
        "persist_pairing MUST default true — caregivers should not re-pair after reboot"
    );
    assert!(
        !config.security.persist_path.is_empty(),
        "persist_path MUST have a default value"
    );
    assert_eq!(config.security.persist_path, "data/security.db");
}

// ═══════════════════════════════════════════════════
// Gate: Provider trait is object-safe
// ═══════════════════════════════════════════════════

#[test]
fn gate_provider_trait_is_object_safe() {
    use oneclaw_core::provider::{Provider, NoopTestProvider, FallbackChain, ReliableProvider};

    // Box<dyn Provider> compiles — proves object safety
    let p: Box<dyn Provider> = Box::new(NoopTestProvider::available());
    assert!(p.is_available());

    // ReliableProvider wraps any Provider
    let reliable = ReliableProvider::new(NoopTestProvider::available(), 3);
    let resp = reliable.chat("system", "test").unwrap();
    assert!(!resp.content.is_empty());

    // FallbackChain compiles with Vec<Box<dyn Provider>>
    let chain = FallbackChain::new(vec![
        Box::new(NoopTestProvider::available()),
    ]);
    assert!(chain.is_available());
}

// ═══════════════════════════════════════════════════
// Gate: Provider config defaults to anthropic/claude
// ═══════════════════════════════════════════════════

#[test]
fn gate_provider_config_defaults() {
    let config = OneClawConfig::default_config();
    assert_eq!(config.provider.primary, "anthropic");
    assert!(config.provider.model.contains("claude"),
        "Default model should be Claude: {}", config.provider.model);
    assert_eq!(config.provider.max_tokens, 1024);
    assert!((config.provider.temperature - 0.3).abs() < f32::EPSILON);
}

// ═══════════════════════════════════════════════════
// Gate: No bare .unwrap() in production code
// ═══════════════════════════════════════════════════

#[test]
fn gate_no_unwrap_in_production() {
    // Scan production source files for bare .unwrap() calls
    // Safe patterns excluded: unwrap_or, unwrap_or_else, unwrap_or_default, unwrap_err
    // Test code excluded: anything inside #[cfg(test)] blocks
    //
    // Implementation: read each source file, track whether we're inside a test block,
    // and flag any bare .unwrap() outside test code.

    let src_dirs = [
        "src/security/",
        "src/memory/",
        "src/orchestrator/",
        "src/provider/",
        "src/event_bus/",
        "src/tool/",
        "src/channel/",
        "src/config.rs",
        "src/error.rs",
        "src/runtime.rs",
        "src/registry.rs",
        "src/metrics.rs",
        "src/lib.rs",
    ];

    let base = env!("CARGO_MANIFEST_DIR");
    let mut violations = Vec::new();

    for dir_or_file in &src_dirs {
        let path = std::path::Path::new(base).join(dir_or_file);
        if path.is_dir() {
            scan_directory(&path, &mut violations);
        } else if path.is_file() {
            scan_file(&path, &mut violations);
        }
    }

    assert!(
        violations.is_empty(),
        "Found {} bare .unwrap() in production code:\n{}",
        violations.len(),
        violations.join("\n")
    );
}

fn scan_directory(dir: &std::path::Path, violations: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory(&path, violations);
            } else if path.extension().is_some_and(|e| e == "rs") {
                scan_file(&path, violations);
            }
        }
    }
}

fn scan_file(path: &std::path::Path, violations: &mut Vec<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut in_test_block = false;
    let mut brace_depth_at_test_start: Option<usize> = None;
    let mut brace_depth: usize = 0;

    for (line_num, line) in content.lines().enumerate() {
        // Track brace depth
        for ch in line.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                _ => {}
            }
        }

        // Detect test module start
        if line.contains("#[cfg(test)]") {
            in_test_block = true;
            brace_depth_at_test_start = Some(brace_depth);
            continue;
        }

        // Detect test module end (when we return to the brace depth before the test block)
        if in_test_block {
            if let Some(start_depth) = brace_depth_at_test_start
                && brace_depth <= start_depth && line.trim() == "}"
            {
                in_test_block = false;
                brace_depth_at_test_start = None;
            }
            continue; // Skip all test code
        }

        // Skip comments
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }

        // Check for bare .unwrap()
        if line.contains(".unwrap()") {
            // Exclude safe patterns
            if line.contains("unwrap_or")
                || line.contains("unwrap_or_else")
                || line.contains("unwrap_or_default")
                || line.contains("unwrap_err")
            {
                continue;
            }

            violations.push(format!(
                "  {}:{}: {}",
                path.display(),
                line_num + 1,
                trimmed,
            ));
        }
    }
}

// ═══════════════════════════════════════════════════
// Gate: All core traits have Noop implementations
// ═══════════════════════════════════════════════════

#[test]
fn gate_all_traits_have_noop() {
    use oneclaw_core::security::NoopSecurity;
    use oneclaw_core::memory::NoopMemory;
    use oneclaw_core::event_bus::NoopEventBus;
    use oneclaw_core::orchestrator::router::NoopRouter;
    use oneclaw_core::orchestrator::context::NoopContextManager;
    use oneclaw_core::orchestrator::chain::NoopChainExecutor;

    // All noops should instantiate without panic
    let _sec = NoopSecurity;
    let _mem = NoopMemory::new();
    let _bus = NoopEventBus::new();
    let _router = NoopRouter;
    let _ctx = NoopContextManager;
    let _chain = NoopChainExecutor::new();
}

// ═══════════════════════════════════════════════════
// Gate: Runtime boots from default config
// ═══════════════════════════════════════════════════

#[test]
fn gate_runtime_boots_from_defaults() {
    let config = OneClawConfig::default_config();
    let workspace = std::env::current_dir().unwrap();
    let runtime = oneclaw_core::runtime::Runtime::from_config(config, workspace).unwrap();
    assert!(runtime.boot().is_ok());
}

//! Layer 4: Tool — Hands
//! Sandboxed tool execution with security gating.

use crate::error::Result;
use std::collections::HashMap;

/// Tool parameter definition (for LLM function calling)
#[derive(Debug, Clone)]
pub struct ToolParam {
    /// The name of the parameter.
    pub name: String,
    /// The description of the parameter.
    pub description: String,
    /// Whether this parameter is required.
    pub required: bool,
}

/// Tool definition — what the tool does
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// The name of the tool.
    pub name: String,
    /// The description of the tool.
    pub description: String,
    /// The parameters accepted by the tool.
    pub params: Vec<ToolParam>,
    /// Category for grouping: "io", "network", "system", "notify"
    pub category: String,
}

/// Result of a tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Whether the tool execution succeeded.
    pub success: bool,
    /// The output or error message from the tool.
    pub output: String,
    /// Additional metadata from the tool execution.
    pub metadata: HashMap<String, String>,
}

impl ToolResult {
    /// Create a successful tool result with the given output.
    pub fn ok(output: impl Into<String>) -> Self {
        Self { success: true, output: output.into(), metadata: HashMap::new() }
    }
    /// Create a failed tool result with the given error message.
    pub fn err(message: impl Into<String>) -> Self {
        Self { success: false, output: message.into(), metadata: HashMap::new() }
    }
    /// Attach a metadata key-value pair to this result.
    pub fn with_meta(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), val.into());
        self
    }
}

/// Layer 4 Trait: Tool — Execute actions in the world
pub trait Tool: Send + Sync {
    /// Tool info for discovery/LLM function calling
    fn info(&self) -> ToolInfo;

    /// Execute the tool with given parameters
    /// Security check happens BEFORE this is called (by ToolRegistry)
    fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult>;
}

/// Noop tool — always succeeds with echo
pub struct NoopTool;

impl NoopTool {
    /// Create a new no-op tool.
    pub fn new() -> Self { Self }
}

impl Default for NoopTool {
    fn default() -> Self { Self::new() }
}

impl Tool for NoopTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "noop".into(),
            description: "Does nothing (test tool)".into(),
            params: vec![],
            category: "system".into(),
        }
    }

    fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult> {
        Ok(ToolResult::ok(format!("noop executed with {} params", params.len())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_tool() {
        let tool = NoopTool::new();
        let info = tool.info();
        assert_eq!(info.name, "noop");

        let result = tool.execute(&HashMap::new()).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_tool_result_builders() {
        let ok = ToolResult::ok("done").with_meta("time", "10ms");
        assert!(ok.success);
        assert_eq!(ok.metadata.get("time"), Some(&"10ms".to_string()));

        let err = ToolResult::err("failed");
        assert!(!err.success);
    }
}

//! Tool Registry — Manages available tools with security gating
//!
//! Before executing any tool, the registry checks:
//! 1. Tool exists
//! 2. Required parameters present
//! 3. Emits event after execution

use crate::error::{OneClawError, Result};
use crate::tool::traits::*;
use crate::event_bus::{Event, EventBus, EventPriority};
use std::collections::HashMap;
use tracing::{info, warn};

/// Registry of available tools with parameter validation and event emission.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty tool registry.
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    /// Register a tool
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.info().name.clone();
        info!(tool = %name, "Tool registered");
        self.tools.insert(name, tool);
    }

    /// List all available tools (for LLM function calling)
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.values().map(|t| t.info()).collect()
    }

    /// Get tool info by name
    pub fn get_tool_info(&self, name: &str) -> Option<ToolInfo> {
        self.tools.get(name).map(|t| t.info())
    }

    /// Execute a tool with parameter validation
    pub fn execute(
        &self,
        tool_name: &str,
        params: &HashMap<String, String>,
        event_bus: Option<&dyn EventBus>,
    ) -> Result<ToolResult> {
        // 1. Find tool
        let tool = self.tools.get(tool_name)
            .ok_or_else(|| OneClawError::Tool(format!("Tool '{}' not found", tool_name)))?;

        let info = tool.info();

        // 2. Validate required params
        for param in &info.params {
            if param.required && !params.contains_key(&param.name) {
                return Err(OneClawError::Tool(format!(
                    "Tool '{}' requires parameter '{}'", tool_name, param.name
                )));
            }
        }

        // 3. Execute
        info!(tool = %tool_name, "Executing tool");
        let result = match tool.execute(params) {
            Ok(r) => r,
            Err(e) => {
                warn!(tool = %tool_name, error = %e, "Tool execution failed");
                return Err(e);
            }
        };

        // 4. Emit event
        if let Some(bus) = event_bus {
            let mut event = Event::new(
                format!("tool.{}", tool_name),
                "tool-registry",
            );
            event = event
                .with_data("tool", tool_name)
                .with_data("success", result.success.to_string())
                .with_data("output_len", result.output.len().to_string());

            if !result.success {
                event = event.with_priority(EventPriority::High);
            }

            let _ = bus.publish(event);
        }

        Ok(result)
    }

    /// Get tool count
    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_list() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(NoopTool::new()));
        assert_eq!(reg.count(), 1);
        let tools = reg.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "noop");
    }

    #[test]
    fn test_execute_existing_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(NoopTool::new()));
        let result = reg.execute("noop", &HashMap::new(), None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_nonexistent_tool() {
        let reg = ToolRegistry::new();
        let result = reg.execute("nonexistent", &HashMap::new(), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_required_param_validation() {
        struct ParamTool;
        impl Tool for ParamTool {
            fn info(&self) -> ToolInfo {
                ToolInfo {
                    name: "param-tool".into(),
                    description: "test".into(),
                    params: vec![
                        ToolParam { name: "url".into(), description: "URL".into(), required: true },
                        ToolParam { name: "timeout".into(), description: "Timeout".into(), required: false },
                    ],
                    category: "test".into(),
                }
            }
            fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult> {
                Ok(ToolResult::ok(format!("fetched {}", params.get("url").unwrap())))
            }
        }

        let mut reg = ToolRegistry::new();
        reg.register(Box::new(ParamTool));

        // Missing required param
        let result = reg.execute("param-tool", &HashMap::new(), None);
        assert!(result.is_err());

        // With required param
        let mut params = HashMap::new();
        params.insert("url".into(), "http://example.com".into());
        let result = reg.execute("param-tool", &params, None).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_emits_event() {
        use crate::event_bus::DefaultEventBus;

        let mut reg = ToolRegistry::new();
        reg.register(Box::new(NoopTool::new()));

        let bus = DefaultEventBus::new();
        reg.execute("noop", &HashMap::new(), Some(&bus)).unwrap();

        assert_eq!(bus.pending_count(), 1);
        bus.drain().unwrap();

        let events = bus.recent_events(1).unwrap();
        assert_eq!(events[0].topic, "tool.noop");
    }
}

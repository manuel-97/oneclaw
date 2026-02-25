//! System Info Tool — Report system status

use oneclaw_core::tool::{Tool, ToolInfo, ToolParam, ToolResult};
use oneclaw_core::error::Result;
use std::collections::HashMap;

/// Tool that reports system information such as OS, memory, and uptime.
pub struct SystemInfoTool;

impl SystemInfoTool {
    /// Create a new `SystemInfoTool` instance.
    pub fn new() -> Self { Self }
}

impl Default for SystemInfoTool {
    fn default() -> Self { Self::new() }
}

impl Tool for SystemInfoTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "system_info".into(),
            description: "Get system information (OS, memory, uptime)".into(),
            params: vec![
                ToolParam {
                    name: "section".into(),
                    description: "What to report: all, os, memory, uptime".into(),
                    required: false,
                },
            ],
            category: "system".into(),
        }
    }

    fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult> {
        let section = params.get("section").map(|s| s.as_str()).unwrap_or("all");

        let mut info = String::new();

        if section == "all" || section == "os" {
            info.push_str(&format!("OS: {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
        }

        if section == "all" || section == "uptime" {
            let uptime = std::fs::read_to_string("/proc/uptime")
                .ok()
                .and_then(|s| s.split_whitespace().next().map(String::from))
                .and_then(|s| s.parse::<f64>().ok())
                .map(|secs| {
                    let hours = (secs / 3600.0) as u64;
                    let mins = ((secs % 3600.0) / 60.0) as u64;
                    format!("{}h {}m", hours, mins)
                })
                .unwrap_or_else(|| "unknown".into());
            info.push_str(&format!("Uptime: {}\n", uptime));
        }

        if section == "all" || section == "memory" {
            let mem_info = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .map(|content| {
                    let mut total = 0u64;
                    let mut available = 0u64;
                    for line in content.lines() {
                        if line.starts_with("MemTotal:") {
                            total = parse_meminfo_kb(line);
                        } else if line.starts_with("MemAvailable:") {
                            available = parse_meminfo_kb(line);
                        }
                    }
                    let used = total.saturating_sub(available);
                    format!("Memory: {} MB used / {} MB total", used / 1024, total / 1024)
                })
                .unwrap_or_else(|| "Memory: info not available".into());
            info.push_str(&format!("{}\n", mem_info));
        }

        Ok(ToolResult::ok(info.trim_end().to_string())
            .with_meta("section", section))
    }
}

fn parse_meminfo_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info_all() {
        let tool = SystemInfoTool::new();
        let result = tool.execute(&HashMap::new()).unwrap();
        assert!(result.success);
        assert!(result.output.contains("OS:"));
    }

    #[test]
    fn test_system_info_os_only() {
        let tool = SystemInfoTool::new();
        let mut params = HashMap::new();
        params.insert("section".into(), "os".into());
        let result = tool.execute(&params).unwrap();
        assert!(result.output.contains("OS:"));
    }

    #[test]
    fn test_system_info_tool_info() {
        let tool = SystemInfoTool::new();
        let info = tool.info();
        assert_eq!(info.name, "system_info");
        assert_eq!(info.category, "system");
    }
}

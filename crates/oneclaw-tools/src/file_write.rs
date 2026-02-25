//! File Write Tool — Write content to files within workspace
//! Respects Security PathGuard (only writes within allowed workspace)

use oneclaw_core::tool::{Tool, ToolInfo, ToolParam, ToolResult};
use oneclaw_core::error::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// Tool that writes content to files within a sandboxed workspace.
pub struct FileWriteTool {
    workspace: PathBuf,
}

impl FileWriteTool {
    /// Create a new `FileWriteTool` scoped to the given workspace directory.
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self { workspace: workspace.into() }
    }
}

impl Tool for FileWriteTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "file_write".into(),
            description: "Write content to a file within the agent workspace".into(),
            params: vec![
                ToolParam { name: "path".into(), description: "Relative file path".into(), required: true },
                ToolParam { name: "content".into(), description: "Content to write".into(), required: true },
                ToolParam { name: "mode".into(), description: "Write mode: overwrite (default) or append".into(), required: false },
            ],
            category: "io".into(),
        }
    }

    fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult> {
        let rel_path = params.get("path")
            .ok_or_else(|| oneclaw_core::error::OneClawError::Tool("Missing 'path' param".into()))?;
        let content = params.get("content")
            .ok_or_else(|| oneclaw_core::error::OneClawError::Tool("Missing 'content' param".into()))?;
        let mode = params.get("mode").map(|s| s.as_str()).unwrap_or("overwrite");

        // Security: resolve within workspace only
        let full_path = self.workspace.join(rel_path);

        // Check that resolved path is still within workspace
        let canonical_workspace = self.workspace.canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());

        // Check parent directory (if it exists, canonicalize to resolve ..)
        if let Some(parent) = full_path.parent()
            && parent.exists()
            && let Ok(canonical_parent) = parent.canonicalize()
            && !canonical_parent.starts_with(&canonical_workspace)
        {
            return Ok(ToolResult::err(format!(
                "Path escape detected: '{}' is outside workspace", rel_path
            )));
        }

        // Also check for obvious traversal patterns
        if rel_path.contains("..") {
            return Ok(ToolResult::err(format!(
                "Path escape detected: '{}' contains '..'", rel_path
            )));
        }

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| oneclaw_core::error::OneClawError::Tool(
                    format!("Failed to create directory: {}", e)
                ))?;
        }

        // Write
        let result = match mode {
            "append" => {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true).append(true).open(&full_path)
                    .map_err(|e| oneclaw_core::error::OneClawError::Tool(format!("Open failed: {}", e)))?;
                file.write_all(content.as_bytes())
                    .map_err(|e| oneclaw_core::error::OneClawError::Tool(format!("Write failed: {}", e)))?;
                format!("Appended {} bytes to {}", content.len(), rel_path)
            }
            _ => {
                std::fs::write(&full_path, content)
                    .map_err(|e| oneclaw_core::error::OneClawError::Tool(format!("Write failed: {}", e)))?;
                format!("Wrote {} bytes to {}", content.len(), rel_path)
            }
        };

        Ok(ToolResult::ok(result)
            .with_meta("path", rel_path)
            .with_meta("bytes", content.len().to_string())
            .with_meta("mode", mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_write_overwrite() {
        let dir = std::env::temp_dir().join("oneclaw_tool_test_write");
        let _ = std::fs::create_dir_all(&dir);

        let tool = FileWriteTool::new(&dir);
        let mut params = HashMap::new();
        params.insert("path".into(), "test.txt".into());
        params.insert("content".into(), "hello world".into());

        let result = tool.execute(&params).unwrap();
        assert!(result.success);
        assert!(dir.join("test.txt").exists());
        assert_eq!(std::fs::read_to_string(dir.join("test.txt")).unwrap(), "hello world");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_write_append() {
        let dir = std::env::temp_dir().join("oneclaw_tool_test_append");
        let _ = std::fs::create_dir_all(&dir);

        let tool = FileWriteTool::new(&dir);
        let mut params = HashMap::new();
        params.insert("path".into(), "log.txt".into());
        params.insert("content".into(), "line 1\n".into());
        tool.execute(&params).unwrap();

        params.insert("content".into(), "line 2\n".into());
        params.insert("mode".into(), "append".into());
        tool.execute(&params).unwrap();

        let content = std::fs::read_to_string(dir.join("log.txt")).unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("line 2"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_write_path_escape_blocked() {
        let dir = std::env::temp_dir().join("oneclaw_tool_test_escape");
        let _ = std::fs::create_dir_all(&dir);

        let tool = FileWriteTool::new(&dir);
        let mut params = HashMap::new();
        params.insert("path".into(), "../../etc/passwd".into());
        params.insert("content".into(), "hacked".into());

        let result = tool.execute(&params).unwrap();
        assert!(!result.success);
        assert!(result.output.contains("escape"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

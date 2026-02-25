# oneclaw-tools

Built-in tool implementations for OneClaw.

Implements the `Tool` trait from `oneclaw-core` for common operations.

## Tools

- **SystemInfoTool** — Reports OS, hostname, uptime, memory usage. Category: `system`.
- **FileWriteTool** — Workspace-scoped file writing (append/overwrite). Path-guarded to workspace directory. Category: `io`.
- **NotifyTool** — Caregiver notification output (currently prints to stdout, designed for SMS/push integration). Category: `notify`.

## Custom Tools

Implement the `Tool` trait to add your own:

```rust
use oneclaw_core::tool::{Tool, ToolInfo, ToolResult};

struct MyTool;
impl Tool for MyTool {
    fn info(&self) -> ToolInfo { /* ... */ }
    fn execute(&self, params: &HashMap<String, String>) -> Result<ToolResult> { /* ... */ }
}
```

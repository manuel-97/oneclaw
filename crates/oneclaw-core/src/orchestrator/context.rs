//! Context Manager — Assemble prompts with memory context
//!
//! Combines: system prompt + relevant memories + user message
//! Budget-aware: trims context if over token limit

use crate::error::Result;

/// Trait for assembling and compressing prompt context
pub trait ContextManager: Send + Sync {
    /// Assemble full context from task + available context
    fn assemble(&self, task: &str, budget_tokens: usize) -> Result<String>;
    /// Compress context to fit within token budget
    fn compress(&self, context: &str, target_tokens: usize) -> Result<String>;
}

/// No-op context manager that passes through input unchanged
pub struct NoopContextManager;
impl ContextManager for NoopContextManager {
    fn assemble(&self, task: &str, _budget: usize) -> Result<String> {
        Ok(task.to_string())
    }
    fn compress(&self, context: &str, _target: usize) -> Result<String> {
        Ok(context.to_string())
    }
}

/// Default context manager with system prompt and memory integration
pub struct DefaultContextManager {
    system_prompt: String,
    /// Rough chars-per-token ratio for budget estimation
    chars_per_token: usize,
}

impl DefaultContextManager {
    /// Create a new context manager with the given system prompt
    pub fn new(system_prompt: impl Into<String>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            chars_per_token: 4, // Rough average for English/Vietnamese mixed text
        }
    }

    /// Get system prompt
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    /// Build full context string from memories + user query
    /// Format: memory_context + user_message
    pub fn build_context(&self, memories: &[String], user_message: &str, budget_tokens: usize) -> String {
        let budget_chars = budget_tokens * self.chars_per_token;

        let mut context_parts: Vec<String> = vec![];

        // Add memory context if available
        if !memories.is_empty() {
            let mut memory_section = String::from("Dữ liệu liên quan từ bộ nhớ:\n");
            for mem in memories {
                let line = format!("- {}\n", mem);
                if memory_section.len() + line.len() + user_message.len() > budget_chars {
                    memory_section.push_str("(... còn nữa nhưng đã cắt bớt)\n");
                    break;
                }
                memory_section.push_str(&line);
            }
            context_parts.push(memory_section);
        }

        // Add user message
        context_parts.push(format!("Câu hỏi/yêu cầu: {}", user_message));

        context_parts.join("\n")
    }
}

impl ContextManager for DefaultContextManager {
    fn assemble(&self, task: &str, budget_tokens: usize) -> Result<String> {
        let budget_chars = budget_tokens * self.chars_per_token;
        if task.len() > budget_chars {
            Ok(task[..budget_chars].to_string())
        } else {
            Ok(task.to_string())
        }
    }

    fn compress(&self, context: &str, target_tokens: usize) -> Result<String> {
        let target_chars = target_tokens * self.chars_per_token;
        if context.len() <= target_chars {
            Ok(context.to_string())
        } else {
            // Simple truncation with marker
            let truncated = &context[..target_chars.saturating_sub(20)];
            Ok(format!("{}... (đã rút gọn)", truncated))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_context_manager_build() {
        let cm = DefaultContextManager::new("You are OneClaw, a helpful AI assistant.");
        let memories = vec![
            "sensor_01 | temperature | value = 22.5".to_string(),
            "sensor_01 | temperature | value = 23.1".to_string(),
        ];
        let context = cm.build_context(&memories, "Analyze sensor readings", 1000);
        assert!(context.contains("Dữ liệu liên quan"));
        assert!(context.contains("22.5"));
        assert!(context.contains("Analyze"));
    }

    #[test]
    fn test_context_budget_trimming() {
        let cm = DefaultContextManager::new("test");
        let long_memories: Vec<String> = (0..100)
            .map(|i| format!("Memory entry {} with a lot of content to fill up space", i))
            .collect();
        let context = cm.build_context(&long_memories, "query", 50); // Very tight budget
        // Should be trimmed, not include all 100 entries
        assert!(context.len() < 1000);
    }

    #[test]
    fn test_compress() {
        let cm = DefaultContextManager::new("test");
        let long_text = "a".repeat(1000);
        let compressed = cm.compress(&long_text, 50).unwrap();
        assert!(compressed.len() < 250);
        assert!(compressed.contains("đã rút gọn"));
    }

    #[test]
    fn test_empty_memories() {
        let cm = DefaultContextManager::new("You are OneClaw.");
        let context = cm.build_context(&[], "hello", 1000);
        assert!(!context.contains("Dữ liệu liên quan"));
        assert!(context.contains("hello"));
    }

    #[test]
    fn test_system_prompt() {
        let cm = DefaultContextManager::new("You are OneClaw, an edge AI assistant.");
        assert!(cm.system_prompt().contains("OneClaw"));
        assert!(cm.system_prompt().contains("edge"));
    }
}

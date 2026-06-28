//! Hermes 引擎的可插拔介质契约。
//!
//! Hermes 引擎（Coordinator / Supervisor / Planner）只通过 [`AgentRuntime`] trait
//! 与底层 agent 通信。具体实现（SdkRuntime / CliRuntime 等）实现此 trait 后即可
//! 被引擎调度，引擎本身不依赖任何具体介质。

use async_trait::async_trait;
use tokio::sync::mpsc;

/// 介质可插拔契约：SDK / CLI / 任意 agent loop 都实现它。Hermes 引擎只认这个。
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    fn capabilities(&self) -> RuntimeCapabilities;
    async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError>;
    async fn send(
        &self,
        handle: &AgentHandle,
        prompt: String,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError>;
    async fn abort(&self, handle: &AgentHandle) -> Result<(), RuntimeError>;
    async fn liveness(&self, handle: &AgentHandle) -> Liveness;
    async fn stop(&self, handle: &AgentHandle) -> Result<(), RuntimeError>;
}

#[derive(Debug, Clone)]
pub struct RuntimeStartSpec {
    pub agent_id: String, // = AgentId（复用 chat::AgentId 语义）
    pub cwd: std::path::PathBuf,
    pub model: String,
    pub provider: String, // claude / codex / gemini / ...
}

#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub agent_id: String,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeCapabilities {
    /// true=有结构化 tool_use/tool_result（判活精准）；false=仅文本+进程存活（降级判活）。
    pub structured_events: bool,
    pub supports_resume: bool,
    pub supports_permission_prompt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Alive,
    Dead,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta(String),
    Thinking(String),
    ToolUse { id: String, name: String },
    ToolResult { tool_use_id: String, is_error: bool },
    NeedsInput, // 工具权限 / ask-user（等待态）
    Done {
        success: bool,
        files_modified: Vec<String>,
    },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct RuntimeError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_event_done_constructs_and_matches() {
        let ev = AgentEvent::Done {
            success: true,
            files_modified: vec!["src/lib.rs".to_string(), "README.md".to_string()],
        };
        match ev {
            AgentEvent::Done {
                success,
                files_modified,
            } => {
                assert!(success);
                assert_eq!(files_modified, vec!["src/lib.rs", "README.md"]);
            }
            _ => panic!("expected Done variant"),
        }
    }

    #[test]
    fn agent_event_failed_constructs_and_matches() {
        let ev = AgentEvent::Failed {
            error: "boom".to_string(),
        };
        match ev {
            AgentEvent::Failed { error } => assert_eq!(error, "boom"),
            _ => panic!("expected Failed variant"),
        }
    }

    #[test]
    fn runtime_capabilities_has_three_bool_fields() {
        let caps = RuntimeCapabilities {
            structured_events: true,
            supports_resume: false,
            supports_permission_prompt: true,
        };
        assert!(caps.structured_events);
        assert!(!caps.supports_resume);
        assert!(caps.supports_permission_prompt);
    }

    #[test]
    fn liveness_variants_compare_equal() {
        assert_eq!(Liveness::Alive, Liveness::Alive);
        assert_eq!(Liveness::Dead, Liveness::Dead);
        assert_eq!(Liveness::Unknown, Liveness::Unknown);
        assert_ne!(Liveness::Alive, Liveness::Dead);
        assert_ne!(Liveness::Dead, Liveness::Unknown);
        assert_ne!(Liveness::Unknown, Liveness::Alive);
    }
}

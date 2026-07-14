//! Scripted Hermes runtime used by `hermes_run_mock`.
//!
//! It is intentionally deterministic: planner agents receive a fixed JSON plan,
//! workers receive a short structured event sequence. This lets the cockpit run
//! the real Coordinator/event path without consuming LLM tokens.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::hermes::runtime::{
    AgentEvent, AgentHandle, AgentRuntime, Liveness, RuntimeCapabilities, RuntimeError,
    RuntimeStartSpec,
};

#[derive(Debug, Clone)]
pub struct ScriptedRuntime {
    plan_json: String,
}

impl Default for ScriptedRuntime {
    fn default() -> Self {
        Self {
            plan_json: r#"{"tasks":[{"id":"mock-task-1","spec":"梳理当前工作区并确认 Helm 驾驶舱数据链路","deps":[],"assignment":{"runtime":"sdk","model":"sonnet"}},{"id":"mock-task-2","spec":"实现驾驶舱演示运行的 UI 验收闭环","deps":["mock-task-1"],"assignment":{"runtime":"sdk","model":"sonnet"}}]}"#.to_string(),
        }
    }
}

#[async_trait]
impl AgentRuntime for ScriptedRuntime {
    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            structured_events: true,
            supports_resume: false,
            supports_permission_prompt: true,
        }
    }

    async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
        Ok(AgentHandle {
            agent_id: spec.agent_id,
        })
    }

    async fn send(
        &self,
        handle: &AgentHandle,
        _prompt: String,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
        let (tx, rx) = mpsc::unbounded_channel();

        if handle.agent_id.starts_with("planner-plan-") {
            let _ = tx.send(AgentEvent::TextDelta(self.plan_json.clone()));
            let _ = tx.send(AgentEvent::Done {
                success: true,
                files_modified: vec![],
            });
            drop(tx);
            return Ok(rx);
        }

        if handle.agent_id.starts_with("planner-judge-") {
            let _ = tx.send(AgentEvent::TextDelta(
                r#"{"winnerIndex":0,"scores":[0.93],"reason":"scripted mock run"}"#.to_string(),
            ));
            let _ = tx.send(AgentEvent::Done {
                success: true,
                files_modified: vec![],
            });
            drop(tx);
            return Ok(rx);
        }

        let _ = tx.send(AgentEvent::Thinking("分析任务上下文".to_string()));
        let _ = tx.send(AgentEvent::ToolUse {
            id: format!("tool-{}", handle.agent_id),
            name: "Read".to_string(),
        });
        let _ = tx.send(AgentEvent::ToolResult {
            tool_use_id: format!("tool-{}", handle.agent_id),
            is_error: false,
        });
        let _ = tx.send(AgentEvent::TextDelta("完成脚本化 worker 回放".to_string()));
        let _ = tx.send(AgentEvent::Done {
            success: true,
            files_modified: vec!["src/components/helm/HelmCockpit.tsx".to_string()],
        });
        drop(tx);

        Ok(rx)
    }

    async fn abort(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn liveness(&self, _handle: &AgentHandle) -> Liveness {
        Liveness::Alive
    }

    async fn stop(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
        Ok(())
    }
}

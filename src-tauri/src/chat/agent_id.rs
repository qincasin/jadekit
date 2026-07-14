//! Agent 标识与归一化。
//! agent_id 同时用作 daemon 的 CLAUDE_SESSION_ID 与 permission 子目录名，
//! 因此必须可安全用于文件路径。集中定义，避免魔法字符串。

/// 一个可部署 Agent 实例的标识。当前用字符串，未来可换 newtype。
pub type AgentId = String;

/// 单聊默认 Agent（兼容旧的单 daemon 行为）。
pub const DEFAULT_AGENT_ID: &str = "default";

/// 归一化前端传入的 agent_id：去空白、去路径分隔符；空则回退默认。
/// 保证结果可安全用作 CLAUDE_SESSION_ID 与 permission 子目录名。
pub fn sanitize_agent_id(raw: &str) -> AgentId {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !matches!(c, '/' | '\\') && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        DEFAULT_AGENT_ID.to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_agent_id, DEFAULT_AGENT_ID};

    #[test]
    fn empty_falls_back_to_default() {
        assert_eq!(sanitize_agent_id("   "), DEFAULT_AGENT_ID);
        assert_eq!(sanitize_agent_id(""), DEFAULT_AGENT_ID);
    }

    #[test]
    fn strips_path_separators_and_trims() {
        assert_eq!(sanitize_agent_id("  a/b\\c  "), "abc");
    }

    #[test]
    fn keeps_plain_id() {
        assert_eq!(sanitize_agent_id("agent-7"), "agent-7");
    }
}

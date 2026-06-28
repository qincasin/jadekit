#![allow(dead_code)]
//! Hermes Planner —— 唯一的 LLM 钩子层（Task 13：纯函数）。
//!
//! 注意：本模块的公开 API（提示构造 / 解析 / Roster / ReplanAction）当前尚无调用方——
//! Task 14 才会把它们接到真实 LLM 会话上。为避免 dead_code 噪音污染构建输出，
//! 在此显式 `allow(dead_code)`；Task 14 接入后可移除。
//!
//! Planner 是 Hermes 中唯一让 LLM 介入编排的地方，负责两个决策点：
//!   * `plan(goal, roster)`：把用户目标拆解成任务 DAG，并为每个任务选兵
//!     （指定 [`RuntimeKind`] + tool + model，见 §9 路由）。
//!   * `replan(run, failed_task, result)`：某任务失败/完成后，决定下一步动作
//!     （重试 / 换兵 / 升级 / 收敛）。
//!
//! 本文件**只实现 Task 13**——纯函数：prompt 构造 + 容错 JSON 解析 + 不变量校验。
//! **不调用任何 LLM / AgentRuntime**（网络/IO 留给 Task 14），便于离线单测。
//!
//! ## 提示 / 响应契约（Prompt / Response Contract）
//!
//! ### plan
//! 输入 `(goal, roster)` → 输出一段结构化提示，指示模型**只**回 JSON：
//! ```json
//! { "tasks": [ { "id": "t1", "spec": "...", "deps": ["t0"],
//!                "assignment": { "runtime": "sdk"|"cli", "model": "<roster model>" } } ] }
//! ```
//! `parse_plan_response` 把模型文本（可能裹在 prose / ```json 围栏里）解析回
//! `Vec<Task>`，并执行两条铁律：
//!   1. **选兵不变量**：每个 `assignment.model`（+ runtime）必须在 roster 内。
//!   2. **DAG 不变量**：每个 `dep` 必须引用响应内已声明的 task id。
//!
//! ### replan
//! 输入 `(run, failed_task, result)` → 输出提示模型从
//! `{retry, reassign, escalate, converge}` 中选一个，回 JSON：
//! ```json
//! { "decision": "retry", "reason": "...",
//!   "assignment": { "runtime": "sdk", "model": "<roster model>" } }   // 仅 reassign 必需
//! ```
//! `parse_replan_response` 同样容错解析；`reassign` 必须带在 roster 内的 assignment。
//!
//! ## 容错策略（Tolerance Strategy）
//!
//! 模型输出常常不是干净的 JSON——会被自然语言包裹、被 markdown 围栏包裹，
//! 甚至夹带前后闲聊。统一策略见 [`extract_json_object`]：
//!   1. 若出现 ```json ... ``` / ``` ... ``` 围栏，优先取围栏内容。
//!   2. 否则从第一个 `{` 扫到与之配对的最后一个 `}`（用括号深度计数，跳过字符串字面量）。
//!   3. 解析失败 → `Err`。
//!
//! 这样能吞下「Sure, here's the plan:\n```json\n{...}\n```\nLet me know!」这类输出。
//!
//! ## 不变量校验（Validation Invariants）
//!
//! 解析阶段对**可选字段宽容、对不变量严格**：
//!   * task 缺 `id` → 按 `t{i}` 自动补；缺 `deps` → 当作 `[]`。
//!   * 但以下任一失败 → 返回 `Err`，整个响应作废（不部分接受）：
//!     - 任务列表为空。
//!     - 某任务的 assignment.model（或 runtime）不在 roster 内。
//!     - 某任务的 dep 引用了响应中不存在的 task id。
//!     - JSON 根本无法解析。
//!   * replan 还多一条：`decision = reassign` 时必须带 assignment 且在 roster 内。

use serde::{Deserialize, Serialize};

use crate::hermes::types::{
    AgentAssignment, CoordinatorRun, RuntimeKind, Task, TaskStatus,
};

// =============================================================================
// 常量：JSON 键 / 决策 token —— 消除魔法串
// =============================================================================

/// plan 响应根对象的 `tasks` 键。
const KEY_TASKS: &str = "tasks";
/// task 对象的 `id` 键。
const KEY_ID: &str = "id";
/// task 对象的 `spec` 键。
const KEY_SPEC: &str = "spec";
/// task 对象的 `deps` 键。
const KEY_DEPS: &str = "deps";
/// task / replan 对象的 `assignment` 键。
const KEY_ASSIGNMENT: &str = "assignment";
/// assignment 对象的 `runtime` 键。
const KEY_RUNTIME: &str = "runtime";
/// assignment 对象的 `model` 键。
const KEY_MODEL: &str = "model";

/// replan 响应的 `decision` 键。
const KEY_DECISION: &str = "decision";
/// replan 响应的 `reason` 键。
const KEY_REASON: &str = "reason";

/// RuntimeKind::Sdk 对应的默认 tool 标识（Planner 按 runtime 自动选 tool，不让模型指定）。
const DEFAULT_SDK_TOOL: &str = "claude-sdk";
/// RuntimeKind::Cli 对应的默认 tool 标识。
const DEFAULT_CLI_TOOL: &str = "claude-cli";

// =============================================================================
// Roster —— 可选介质清单（最小可用集）
// =============================================================================

/// 一项可选介质：runtime × model × 人类可读标签 + 可选成本提示。
///
/// 设计来源 §9：Roster = 可用 CLI×模型 + 成本/上下文窗口/能力标签。
/// 本结构是 Planner 需要的最小子集——足够构造提示、足够校验选兵。
/// 更丰富的字段（context window、capability tags）留给后续子相位按需扩。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub runtime: RuntimeKind,
    pub model: String,
    pub label: String,
    /// 形如 `"low"`/`"mid"`/`"high"` 或任意自由文本；提示里展示给模型参考。
    pub cost_hint: Option<String>,
}

/// Roster：一组 [`RosterEntry`]。以 newtype 包装，便于挂 helper。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Roster(pub Vec<RosterEntry>);

impl Roster {
    /// 判定 `(runtime, model)` 是否在本 roster 内（Planner 选兵校验用）。
    pub fn contains(&self, runtime: RuntimeKind, model: &str) -> bool {
        self.0
            .iter()
            .any(|e| e.runtime == runtime && e.model == model)
    }

    /// 空切片视图，便于测试与默认构造。
    pub fn empty() -> Self {
        Self(vec![])
    }
}

// =============================================================================
// Replan 决策类型
// =============================================================================

/// Planner 在任务失败后可选的动作。对齐 §6.5「重试/换兵/上报/收敛」四态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplanAction {
    /// 同任务同 assignment 重跑一次。
    Retry,
    /// 换 runtime/model 再试（必须带新的 [`AgentAssignment`]）。
    Reassign,
    /// 升级到人工 / 上层（放弃自动恢复）。
    Escalate,
    /// 视为已收敛，停止后续重试（即使部分失败也接受当前产出）。
    Converge,
}

impl ReplanAction {
    /// 稳定 token，与提示里告诉模型的字符串严格一致。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Reassign => "reassign",
            Self::Escalate => "escalate",
            Self::Converge => "converge",
        }
    }

    /// 反序列化；非四个 token 之一 → `Err`。
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "retry" => Ok(Self::Retry),
            "reassign" => Ok(Self::Reassign),
            "escalate" => Ok(Self::Escalate),
            "converge" => Ok(Self::Converge),
            other => Err(format!("unknown ReplanAction: {other}")),
        }
    }
}

/// replan 的解析结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplanDecision {
    pub decision: ReplanAction,
    pub reason: String,
    /// 仅 `decision = Reassign` 时必需；其它动作可缺省。
    pub assignment: Option<AgentAssignment>,
}

// =============================================================================
// 提示构造
// =============================================================================

/// 构造 plan 提示：给定目标 + roster，输出一段确定性提示，要求模型**只**回结构化 JSON。
///
/// 提示内容：
///   1. 陈述目标。
///   2. 逐条列出 roster 全部条目（label + runtime + model + cost hint）。
///   3. 用自然语言 + JSON schema 示例，约束模型只能回 `{"tasks":[...]}`。
///
/// 同样的 `(goal, roster)` 输入 → 字节级一致的输出（无随机性）。
pub fn build_plan_prompt(goal: &str, roster: &Roster) -> String {
    // roster 条目逐行渲染，每行都对齐 schema 字段名。
    let mut roster_lines = String::new();
    if roster.0.is_empty() {
        roster_lines.push_str("  (roster 为空——这种情形通常意味着上游配置缺失)\n");
    }
    for entry in &roster.0 {
        let cost = entry.cost_hint.as_deref().unwrap_or("n/a");
        roster_lines.push_str(&format!(
            "  - label={label} | runtime={rt} | model={model} | cost_hint={cost}\n",
            label = entry.label,
            rt = entry.runtime.as_str(),
            model = entry.model,
            cost = cost,
        ));
    }

    format!(
        r#"你是 Hermes 编排引擎的 Planner。请把下面的目标拆解为任务 DAG，并为每个任务选兵。

# 目标
{goal}

# 可选介质（Roster）
只能从以下 runtime×model 组合里挑 assignment.model；用 roster 之外的 model 视为非法。
{roster_lines}
# 输出契约（严格）
- 只输出一个 JSON 对象，不要任何解释文字、不要 markdown 围栏。
- 顶层结构：{{ "{tasks}": [ {{ ...task... }} ] }}
- 每个 task 对象字段：
    - "{id}": 字符串，唯一（缺省则按出现顺序补 t1/t2/...）
    - "{spec}": 字符串，对该任务的明确指令（自包含，可被独立 agent 执行）
    - "{deps}": 字符串数组，本任务依赖的其它 task id（无依赖则空数组或缺省）
    - "{assignment}": {{ "{runtime}": "sdk"|"cli", "{model}": "<必须来自上方 roster>" }}
- 任务图必须是 DAG（禁止环）；dep 必须引用本响应内已声明的 id。

# 示例
{{ "{tasks}": [
  {{ "{id}": "t1", "{spec}": "调研依赖", "{deps}": [], "{assignment}": {{ "{runtime}": "sdk", "{model}": "sonnet" }} }},
  {{ "{id}": "t2", "{spec}": "实现核心", "{deps}": ["t1"], "{assignment}": {{ "{runtime}": "cli", "{model}": "glm-5.2" }} }}
] }}
"#,
        goal = goal,
        roster_lines = roster_lines.trim_end(),
        tasks = KEY_TASKS,
        id = KEY_ID,
        spec = KEY_SPEC,
        deps = KEY_DEPS,
        assignment = KEY_ASSIGNMENT,
        runtime = KEY_RUNTIME,
        model = KEY_MODEL,
    )
}

/// 构造 replan 提示：给定失败的 run / task / 结果，请模型从四个动作里选一个。
///
/// `assignment` 字段仅在 `decision = reassign` 时必需，提示里会显式说明。
pub fn build_replan_prompt(run: &CoordinatorRun, failed_task: &Task, result: &str) -> String {
    format!(
        r#"你是 Hermes 编排引擎的 Planner。某个任务失败了，请决定下一步动作。

# 编排运行
- run_id: {run_id}
- goal: {goal}

# 失败任务
- task_id: {task_id}
- spec: {spec}

# 失败结果 / 产出
{result}

# 输出契约（严格）
- 只输出一个 JSON 对象，不要任何解释文字、不要 markdown 围栏。
- 顶层结构：
    {{ "{decision}": "retry"|"reassign"|"escalate"|"converge",
       "{reason}": "<简短解释>",
       "{assignment}": {{ "{runtime}": "sdk"|"cli", "{model}": "<roster 内的 model>" }} }}
- {assignment} 字段**仅当** decision = "reassign" 时必需；其它动作可省略。
- 动作含义：
    - retry: 同任务同 assignment 重跑。
    - reassign: 换 runtime/model 再试（必须给 assignment）。
    - escalate: 升级到人工 / 上层（放弃自动恢复）。
    - converge: 视为已收敛，停止后续重试。

# 示例
{{ "{decision}": "retry", "{reason}": "瞬时错误，重跑一次", "{assignment}": {{ "{runtime}": "sdk", "{model}": "sonnet" }} }}
"#,
        run_id = run.id,
        goal = run.goal,
        task_id = failed_task.id,
        spec = failed_task.spec,
        result = result,
        decision = KEY_DECISION,
        reason = KEY_REASON,
        assignment = KEY_ASSIGNMENT,
        runtime = KEY_RUNTIME,
        model = KEY_MODEL,
    )
}

// =============================================================================
// 容错 JSON 提取
// =============================================================================

/// 从可能被 prose / markdown 围栏包裹的模型输出里，抠出最外层 JSON 对象文本。
///
/// 策略（按优先级）：
///   1. **围栏优先**：找 ```json ... ``` 或 ``` ... ```，取围栏内的内容。
///      模型常用围栏包 JSON，即便我们要求「不要围栏」也常常出现，必须容忍。
///   2. **括号配对**：否则从首个 `{` 起，按 `{`/`}` 深度计数扫描至深度归 0，
///      跳过字符串字面量内的括号（避免 `"{"` 干扰），返回那段子串。
///   3. 找不到配对 → `None`。
///
/// 返回的是原始子串（含可能的尾随逗号等小问题也尽量交给 serde 容错）。
fn extract_json_object(text: &str) -> Option<&str> {
    // —— 策略 1：markdown 围栏 ——
    // 匹配 ```json\n...\n``` 或 ```\n...\n```；取第一段围栏内容。
    if let Some(fenced) = extract_fenced_block(text) {
        return Some(fenced);
    }

    // —— 策略 2：括号深度配对，跳过字符串字面量 ——
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// 抠 ```json ... ``` 或 ``` ... ``` 围栏内的内容（不含围栏本身）。
/// 仅取第一段围栏——Planner 的契约要求单一 JSON 对象。
fn extract_fenced_block(text: &str) -> Option<&str> {
    // 直接按字符索引：定位首个 ``` → 跳到行尾（含可选语言标识）→ 找配对的闭合 ```。
    let open = text.find("```")?;
    // 跳过 ``` 与可选的语言标识（如 ```json），到行尾。
    let after_fence = &text[open + 3..];
    let lang_newline = after_fence.find('\n')?;
    let content_start = open + 3 + lang_newline + 1;

    // 在剩余文本里找配对的闭合 ```。
    let rest = &text[content_start..];
    let close = rest.find("```")?;
    Some(&text[content_start..content_start + close])
}

// =============================================================================
// 解析：plan
// =============================================================================

/// plan 响应的 JSON 反序列化中间结构（字段全可选，便于宽容解析）。
#[derive(Debug, Deserialize)]
struct PlanResponseDto {
    tasks: Vec<TaskDto>,
}

#[derive(Debug, Deserialize)]
struct TaskDto {
    id: Option<String>,
    spec: Option<String>,
    deps: Option<Vec<String>>,
    assignment: Option<AssignmentDto>,
}

#[derive(Debug, Deserialize)]
struct AssignmentDto {
    runtime: Option<String>,
    model: Option<String>,
    /// tool 字段是 AgentAssignment 的必需项；缺省时按 runtime 推一个默认。
    tool: Option<String>,
}

/// 解析 plan 响应文本 → `Vec<Task>`。
///
/// 步骤：容错提取 JSON → 反序列化 → 逐条构造 `Task`（补 id/状态/默认 tool）
/// → 校验 roster 选兵 + DAG 依赖。
pub fn parse_plan_response(text: &str, roster: &Roster) -> Result<Vec<Task>, String> {
    let json_str = extract_json_object(text).ok_or_else(|| {
        "parse_plan_response: 响应中找不到可识别的 JSON 对象".to_string()
    })?;

    let dto: PlanResponseDto = serde_json::from_str(json_str).map_err(|e| {
        format!("parse_plan_response: JSON 反序列化失败: {e}")
    })?;

    if dto.tasks.is_empty() {
        return Err("parse_plan_response: 任务列表为空".to_string());
    }

    // —— 第一遍：构造 Task，并收齐所有已声明的 id（供后续 dep 校验）——
    let now = now_iso();
    let mut tasks = Vec::with_capacity(dto.tasks.len());
    let mut known_ids: Vec<String> = Vec::with_capacity(dto.tasks.len());

    for (idx, t) in dto.tasks.into_iter().enumerate() {
        // 缺 id → 按 t1/t2/... 自动补（从 1 开始，对齐示例）。
        let id = t.id.unwrap_or_else(|| format!("t{}", idx + 1));
        // 缺 spec → 视为非法（无法执行空指令）。
        let spec = t.spec.ok_or_else(|| {
            format!("parse_plan_response: task {id} 缺少 spec 字段")
        })?;
        let deps = t.deps.unwrap_or_default();

        let assignment = t
            .assignment
            .map(|a| build_assignment(&id, a, roster))
            .transpose()?;

        known_ids.push(id.clone());
        tasks.push(Task {
            id,
            parent_id: None,
            spec,
            status: TaskStatus::Pending, // 状态先统一 Pending，下面按 deps 修正
            deps,
            result: None,
            assignment,
            created_at: now.clone(),
            completed_at: None,
        });
    }

    // —— 第二遍：状态修正（无 deps → Ready）+ DAG 依赖校验 ——
    let known_set: std::collections::HashSet<&str> =
        known_ids.iter().map(|s| s.as_str()).collect();

    for task in &mut tasks {
        if task.deps.is_empty() {
            task.status = TaskStatus::Ready;
        }
        for dep in &task.deps {
            if !known_set.contains(dep.as_str()) {
                return Err(format!(
                    "parse_plan_response: task {} 的 dep '{}' 未在响应内声明",
                    task.id, dep
                ));
            }
        }
    }

    Ok(tasks)
}

/// 把 [`AssignmentDto`] 转成 [`AgentAssignment`]，并校验 (runtime, model) 在 roster 内。
fn build_assignment(
    task_id: &str,
    dto: AssignmentDto,
    roster: &Roster,
) -> Result<AgentAssignment, String> {
    let runtime_str = dto.runtime.ok_or_else(|| {
        format!("parse_plan_response: task {task_id} 的 assignment 缺少 runtime")
    })?;
    let runtime = RuntimeKind::from_str(&runtime_str).map_err(|e| {
        format!("parse_plan_response: task {task_id} 的 runtime 非法: {e}")
    })?;
    let model = dto.model.ok_or_else(|| {
        format!("parse_plan_response: task {task_id} 的 assignment 缺少 model")
    })?;

    if !roster.contains(runtime, &model) {
        return Err(format!(
            "parse_plan_response: task {task_id} 的 assignment (runtime={}, model={}) 不在 roster 内",
            runtime.as_str(),
            model
        ));
    }

    // tool 缺省：按 runtime 给一个稳定的默认（Sdk→claude-sdk, Cli→claude-cli）。
    // YAGNI：更复杂的 tool 映射留给后续子相位按 roster 的 label 决定。
    let tool = dto.tool.unwrap_or_else(|| default_tool(runtime).to_string());
    Ok(AgentAssignment {
        runtime,
        tool,
        model,
    })
}

fn default_tool(runtime: RuntimeKind) -> &'static str {
    match runtime {
        RuntimeKind::Sdk => DEFAULT_SDK_TOOL,
        RuntimeKind::Cli => DEFAULT_CLI_TOOL,
    }
}

// =============================================================================
// 解析：replan
// =============================================================================

#[derive(Debug, Deserialize)]
struct ReplanResponseDto {
    decision: Option<String>,
    reason: Option<String>,
    assignment: Option<AssignmentDto>,
}

/// 解析 replan 响应文本 → [`ReplanDecision`]。
/// `decision = reassign` 时必须带 assignment 且 (runtime, model) 在 roster 内。
pub fn parse_replan_response(text: &str, roster: &Roster) -> Result<ReplanDecision, String> {
    let json_str = extract_json_object(text).ok_or_else(|| {
        "parse_replan_response: 响应中找不到可识别的 JSON 对象".to_string()
    })?;

    let dto: ReplanResponseDto = serde_json::from_str(json_str).map_err(|e| {
        format!("parse_replan_response: JSON 反序列化失败: {e}")
    })?;

    let decision_str = dto.decision.ok_or_else(|| {
        "parse_replan_response: 缺少 decision 字段".to_string()
    })?;
    let decision = ReplanAction::from_str(&decision_str).map_err(|e| {
        format!("parse_replan_response: {e}")
    })?;
    let reason = dto.reason.unwrap_or_default();

    let assignment = match (decision, dto.assignment) {
        (ReplanAction::Reassign, Some(a)) => Some(build_assignment("replan", a, roster)?),
        (ReplanAction::Reassign, None) => {
            return Err(
                "parse_replan_response: decision=reassign 必须带 assignment".to_string(),
            );
        }
        // 非 reassign 动作即使模型带了 assignment 也忽略，避免误用。
        (_, maybe_a) => maybe_a
            .map(|a| build_assignment("replan", a, roster))
            .transpose()?,
    };

    Ok(ReplanDecision {
        decision,
        reason,
        assignment,
    })
}

// =============================================================================
// 小工具
// =============================================================================

/// 当前时间的 ISO-8601 字符串（UTC）。单测里也用——纯函数不依赖外部时钟，
/// 这里只是为了给 Task.created_at 一个合理非空值；测试不依赖具体时间。
fn now_iso() -> String {
    use chrono::Utc;
    Utc::now().to_rfc3339()
}

// =============================================================================
// 测试 —— 先写失败测试（RED），再实现到 GREEN
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::types::{CoordinatorRun, RunStatus};

    // —— 测试用 roster 工厂 ——
    fn sample_roster() -> Roster {
        Roster(vec![
            RosterEntry {
                runtime: RuntimeKind::Sdk,
                model: "sonnet".to_string(),
                label: "Claude Sonnet (SDK)".to_string(),
                cost_hint: Some("mid".to_string()),
            },
            RosterEntry {
                runtime: RuntimeKind::Cli,
                model: "glm-5.2".to_string(),
                label: "GLM 5.2 (CLI)".to_string(),
                cost_hint: Some("low".to_string()),
            },
        ])
    }

    fn sample_run() -> CoordinatorRun {
        CoordinatorRun {
            id: "run-1".to_string(),
            goal: "发布 v1.0".to_string(),
            status: RunStatus::Running,
            coordinator_handle: "coord-1".to_string(),
            poll_interval_ms: 1000,
            created_at: "2026-06-28T00:00:00Z".to_string(),
            completed_at: None,
        }
    }

    fn failed_task() -> Task {
        Task {
            id: "t2".to_string(),
            parent_id: None,
            spec: "实现核心功能".to_string(),
            status: TaskStatus::Failed,
            deps: vec!["t1".to_string()],
            result: None,
            assignment: None,
            created_at: "2026-06-28T00:00:00Z".to_string(),
            completed_at: None,
        }
    }

    // ===== Case 1: build_plan_prompt 含 goal + 每条 roster 条目 =====
    #[test]
    fn build_plan_prompt_contains_goal_and_roster() {
        let roster = sample_roster();
        let prompt = build_plan_prompt("发布 v1.0", &roster);

        assert!(prompt.contains("发布 v1.0"), "prompt 必须包含 goal 原文");
        for entry in &roster.0 {
            assert!(
                prompt.contains(&entry.label),
                "prompt 必须包含 roster label: {}",
                entry.label
            );
            assert!(
                prompt.contains(&entry.model),
                "prompt 必须包含 roster model: {}",
                entry.model
            );
        }
    }

    // ===== Case 2: 干净 JSON → 正确 N 个 Task，deps/assignment 正确 =====
    #[test]
    fn parse_plan_response_clean_json() {
        let roster = sample_roster();
        let json = r#"{"tasks":[
            {"id":"t1","spec":"调研依赖","deps":[],"assignment":{"runtime":"sdk","model":"sonnet"}},
            {"id":"t2","spec":"实现核心","deps":["t1"],"assignment":{"runtime":"cli","model":"glm-5.2"}}
        ]}"#;
        let tasks = parse_plan_response(json, &roster).expect("干净 JSON 必须解析成功");
        assert_eq!(tasks.len(), 2);

        let t1 = &tasks[0];
        assert_eq!(t1.id, "t1");
        assert_eq!(t1.spec, "调研依赖");
        assert!(t1.deps.is_empty());
        assert_eq!(t1.status, TaskStatus::Ready); // 无 deps → Ready
        let a1 = t1.assignment.as_ref().expect("t1 必须有 assignment");
        assert_eq!(a1.runtime, RuntimeKind::Sdk);
        assert_eq!(a1.model, "sonnet");

        let t2 = &tasks[1];
        assert_eq!(t2.id, "t2");
        assert_eq!(t2.deps, vec!["t1"]);
        assert_eq!(t2.status, TaskStatus::Pending); // 有 deps → Pending
        let a2 = t2.assignment.as_ref().expect("t2 必须有 assignment");
        assert_eq!(a2.runtime, RuntimeKind::Cli);
        assert_eq!(a2.model, "glm-5.2");
    }

    // ===== Case 3: assignment model 不在 roster → Err =====
    #[test]
    fn parse_plan_response_rejects_out_of_roster_model() {
        let roster = sample_roster();
        let json = r#"{"tasks":[
            {"id":"t1","spec":"x","deps":[],"assignment":{"runtime":"sdk","model":"opus-pro"}}
        ]}"#;
        let err = parse_plan_response(json, &roster).expect_err("opus-pro 不在 roster，必须 Err");
        assert!(
            err.contains("不在 roster 内"),
            "错误信息应说明 roster 校验失败，got: {err}"
        );
    }

    // ===== Case 4: dep 引用未知 task id → Err =====
    #[test]
    fn parse_plan_response_rejects_unknown_dep() {
        let roster = sample_roster();
        let json = r#"{"tasks":[
            {"id":"t1","spec":"x","deps":["tGhost"],"assignment":{"runtime":"sdk","model":"sonnet"}}
        ]}"#;
        let err = parse_plan_response(json, &roster).expect_err("dep tGhost 不存在，必须 Err");
        assert!(
            err.contains("tGhost"),
            "错误信息应指出未知 dep id，got: {err}"
        );
    }

    // ===== Case 5: JSON 被 prose + ```json 围栏包裹 → 仍能解析 =====
    #[test]
    fn parse_plan_response_tolerates_fences_and_prose() {
        let roster = sample_roster();
        let text = "Sure, here's the plan:\n```json\n{\"tasks\":[\n  {\"id\":\"t1\",\"spec\":\"调研\",\"deps\":[],\"assignment\":{\"runtime\":\"sdk\",\"model\":\"sonnet\"}}\n]}\n```\nLet me know if you need anything else!";
        let tasks = parse_plan_response(text, &roster).expect("围栏 + prose 必须容忍");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(
            tasks[0].assignment.as_ref().unwrap().model,
            "sonnet"
        );
    }

    // ===== Case 6: 畸形 JSON → Err =====
    #[test]
    fn parse_plan_response_rejects_malformed_json() {
        let roster = sample_roster();
        let malformed = "这里是自然语言，没有完整 JSON 对象";
        assert!(parse_plan_response(malformed, &roster).is_err());

        let broken = r#"{"tasks":[{"id":"t1","#; // 截断
        assert!(parse_plan_response(broken, &roster).is_err());
    }

    // ===== Case 7: build_replan_prompt + parse_replan_response =====
    #[test]
    fn build_replan_prompt_contains_context() {
        let run = sample_run();
        let task = failed_task();
        let prompt = build_replan_prompt(&run, &task, "TypeError: x is undefined");

        assert!(prompt.contains(&run.id));
        assert!(prompt.contains(&run.goal));
        assert!(prompt.contains(&task.id));
        assert!(prompt.contains(&task.spec));
        assert!(prompt.contains("TypeError: x is undefined"));
    }

    #[test]
    fn parse_replan_response_valid_retry() {
        let roster = sample_roster();
        let text = r#"{"decision":"retry","reason":"瞬时错误"}"#;
        let d = parse_replan_response(text, &roster).expect("合法 retry 必须解析");
        assert_eq!(d.decision, ReplanAction::Retry);
        assert_eq!(d.reason, "瞬时错误");
        assert!(d.assignment.is_none(), "retry 不应有 assignment");
    }

    #[test]
    fn parse_replan_response_reassign_in_roster() {
        let roster = sample_roster();
        let text = r#"{"decision":"reassign","reason":"换更强的","assignment":{"runtime":"sdk","model":"sonnet"}}"#;
        let d = parse_replan_response(text, &roster).expect("合法 reassign 必须解析");
        assert_eq!(d.decision, ReplanAction::Reassign);
        let a = d.assignment.expect("reassign 必须带 assignment");
        assert_eq!(a.runtime, RuntimeKind::Sdk);
        assert_eq!(a.model, "sonnet");
    }

    #[test]
    fn parse_replan_response_reassign_out_of_roster_err() {
        let roster = sample_roster();
        let text = r#"{"decision":"reassign","reason":"x","assignment":{"runtime":"sdk","model":"gpt-5"}}"#;
        let err = parse_replan_response(text, &roster).expect_err("gpt-5 不在 roster 必须报错");
        assert!(err.contains("不在 roster 内"), "got: {err}");
    }

    #[test]
    fn parse_replan_response_reassign_missing_assignment_err() {
        let roster = sample_roster();
        let text = r#"{"decision":"reassign","reason":"x"}"#;
        let err = parse_replan_response(text, &roster).expect_err("reassign 无 assignment 必须报错");
        assert!(err.contains("reassign"), "got: {err}");
    }

    // ===== 辅助：roster.contains / extract_json_object 边界 =====
    #[test]
    fn roster_contains_matches_runtime_and_model() {
        let r = sample_roster();
        assert!(r.contains(RuntimeKind::Sdk, "sonnet"));
        assert!(r.contains(RuntimeKind::Cli, "glm-5.2"));
        // runtime 不匹配
        assert!(!r.contains(RuntimeKind::Cli, "sonnet"));
        // model 不匹配
        assert!(!r.contains(RuntimeKind::Sdk, "glm-5.2"));
    }

    #[test]
    fn extract_json_object_handles_plain_object() {
        let s = r#"{"a":1}"#;
        assert_eq!(extract_json_object(s), Some(s));
    }

    #[test]
    fn extract_json_object_handles_nested_and_strings() {
        // 字符串里的 `{` 不应干扰深度计数
        let s = r#"{"a":"{ not real","b":{"c":1}}"#;
        let extracted = extract_json_object(s).expect("应提取完整外层对象");
        assert!(extracted.ends_with("}}"));
    }

    #[test]
    fn extract_json_object_returns_none_for_no_brace() {
        assert_eq!(extract_json_object("no json here"), None);
    }

    #[test]
    fn replan_action_roundtrip() {
        for v in [
            ReplanAction::Retry,
            ReplanAction::Reassign,
            ReplanAction::Escalate,
            ReplanAction::Converge,
        ] {
            assert_eq!(ReplanAction::from_str(v.as_str()).unwrap(), v);
        }
        assert!(ReplanAction::from_str("bogus").is_err());
    }
}

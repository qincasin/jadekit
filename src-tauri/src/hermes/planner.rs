#![allow(dead_code)]
//! Hermes Planner —— 唯一的 LLM 钩子层。
//!
//! Task 13 落地了**纯函数层**（提示构造 / 容错 JSON 解析 / Roster / ReplanDecision）。
//! Task 14（本提交）把纯函数层接到真实 LLM 会话上：[`Planner::plan`] /
//! [`Planner::replan`] 经 [`AgentRuntime`] 起一个无 worktree 的临时 planner agent，
//! 发提示 → 收敛 TextDelta → Done 后解析。Coordinator 在开局调 `plan`、失败后调 `replan`。
//!
//! Planner 是 Hermes 中唯一让 LLM 介入编排的地方，负责两个决策点：
//!   * `plan(goal, roster)`：把用户目标拆解成任务 DAG，并为每个任务选兵
//!     （指定 [`RuntimeKind`] + tool + model，见 §9 路由）。
//!   * `replan(run, failed_task, result)`：某任务失败/完成后，决定下一步动作
//!     （重试 / 换兵 / 升级 / 收敛）。
//!
//! LLM 仅在这两个点介入——其余 Coordinator 循环 / 派发 / 熔断 / 收敛判定全部确定性。
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
use std::path::Path;
use std::sync::Arc;

use crate::hermes::runtime::{AgentEvent, AgentRuntime, RuntimeStartSpec};
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
///   1. **围栏优先（双围栏兜底）**：依次枚举每段 ```json ... ``` / ``` ... ``` 围栏内容。
///      模型常常先吐一段 ```rust``` / ```text``` 闲聊围栏，再吐真正的 ```json``` 围栏——
///      所以**不能只看第一段围栏**：若第一段围栏不是合法 JSON，要继续尝试后续围栏。
///   2. **括号配对**：所有围栏都不行时，从首个 `{` 起，按 `{`/`}` 深度计数扫描至深度归 0，
///      跳过字符串字面量内的括号（避免 `"{"` 干扰），返回那段子串。
///   3. 找不到配对 → `None`。
///
/// 返回的是原始子串（含可能的尾随逗号等小问题也尽量交给 serde 容错）。
fn extract_json_object(text: &str) -> Option<&str> {
    // —— 策略 1：逐段围栏尝试 ——
    // 若第一段围栏不是合法 JSON（serde 反序列化失败），换下一段围栏再试。
    // 用 serde 是否能解析出 PlanResponseDto/ReplanResponseDto 的最小骨架（`{` 顶层对象）
    // 来判断合法性——这里统一以「serde_json::from_str::<serde_json::Value> 是否 Ok」为准，
    // 它要求顶层是合法 JSON 值，能挡住 ```rust``` 这种代码块。
    for fenced in iter_fenced_blocks(text) {
        if serde_json::from_str::<serde_json::Value>(fenced).is_ok() {
            return Some(fenced);
        }
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

/// 枚举 ```json ... ``` / ``` ... ``` 围栏内的内容（不含围栏本身）。
///
/// 模型常先吐一段 ```rust```/```text``` 闲聊围栏，再吐真正的 ```json```——
/// 单看第一段会误中闲聊围栏。这里枚举**所有**围栏内容，让
/// [`extract_json_object`] 逐段试，挑出第一段能解析为合法 JSON 的。
///
/// 实现细节：以 byte 索引推进扫描，每段围栏 = 一对 ```...``` 之间的内容
/// （跳过可选的语言标识如 ```json 到行尾）。
fn iter_fenced_blocks(text: &str) -> impl Iterator<Item = &str> {
    FencedBlockIter { text, cursor: 0 }
}

/// 围栏迭代器：每次 `next` 返回下一段 ``` ... ``` 内容（不含围栏围栏本身）。
struct FencedBlockIter<'a> {
    text: &'a str,
    cursor: usize,
}

impl<'a> Iterator for FencedBlockIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let text = self.text;
        // 从 cursor 起找下一个开围栏 ```。
        let open = text[self.cursor..].find("```")?;
        let open_abs = self.cursor + open;
        // 跳过 ``` 与可选的语言标识（如 ```json），到行尾。
        let after_fence = &text[open_abs + 3..];
        let lang_newline = after_fence.find('\n')?;
        let content_start = open_abs + 3 + lang_newline + 1;

        // 在剩余文本里找配对的闭合 ```。
        let rest = &text[content_start..];
        let close = rest.find("```")?;
        let content_end = content_start + close;

        // 推进游标到闭合围栏之后，下次 next() 从这里继续找下一段。
        self.cursor = content_end + 3;

        Some(&text[content_start..content_end])
    }
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
// Planner —— LLM 驱动层（Task 14）
// =============================================================================
//
// 设计：Planner 是一个**轻量的 agent wrapper**——它经 AgentRuntime 起一个临时
// planner agent，发提示，然后**纯文本**地收敛 TextDelta 到一段文本。planner 不
// 编辑代码、不需要 worktree（cwd 直接 = repo_root）；不消费 ToolUse/ToolResult/
// NeedsInput（那是 worker agent 的事）。这是 §6.5「Planner 也是一个 Agent」的实现。
//
// LLM 介入的两个点严格限定为：
//   * [`Planner::plan`]：开局拆解。
//   * [`Planner::replan`]：失败后决策。
// 其余 Coordinator 循环（派发 / 收敛 / 熔断 / 心跳）全部确定性——Planner 不碰。

/// 默认 planner provider（对齐 Coordinator 的 [`DEFAULT_PROVIDER`](crate::hermes::coordinator::DEFAULT_PROVIDER)）。
/// 注：Phase 2 只有 Claude SDK 一种介质；Phase 3 才有 Codex/Gemini 等多 provider。
const DEFAULT_PROVIDER: &str = "claude";

/// 默认 planner 模型（对齐 Coordinator 的 DEFAULT_MODEL）。
const DEFAULT_MODEL: &str = "sonnet";

/// Planner 驱动器：经 [`AgentRuntime`] 起临时 planner agent，发提示并解析响应。
///
/// 不持有状态——每次 `plan`/`replan` 都新起一个 agent，跑完即丢。
/// runtime 由调用方（Coordinator）注入，便于测试用 MockRuntime 回放固定事件流。
pub struct Planner {
    runtime: Arc<dyn AgentRuntime>,
}

impl Planner {
    pub fn new(runtime: Arc<dyn AgentRuntime>) -> Self {
        Self { runtime }
    }

    /// 开局拆解：起 planner agent → 发 `build_plan_prompt` → 收敛 TextDelta → 解析。
    ///
    /// 流程：
    ///   1. `start` 一个无 worktree 的临时 planner agent（agent_id = `planner-<短 id>`，
    ///      cwd = repo_root）。
    ///   2. `send` 提示，拿到事件流 receiver。
    ///   3. 排空事件流：`TextDelta(text)` 累积到 buffer；`Done{success:true}` 终止；
    ///      `Failed` / `Done{success:false}` → `Err`。
    ///      ToolUse/ToolResult/NeedsInput 等 worker 事件忽略（planner 只做文本补全）。
    ///   4. `parse_plan_response(&buffer, roster)` → `Vec<Task>`。
    pub async fn plan(
        &self,
        goal: &str,
        roster: &Roster,
        repo_root: &Path,
    ) -> Result<Vec<Task>, String> {
        let prompt = build_plan_prompt(goal, roster);
        let agent_id = format!("planner-{}", short_id());

        // 起临时 planner agent：无 worktree，cwd = repo_root（planner 不改代码）。
        let handle = self
            .runtime
            .start(RuntimeStartSpec {
                agent_id: agent_id.clone(),
                cwd: repo_root.to_path_buf(),
                model: DEFAULT_MODEL.to_string(),
                provider: DEFAULT_PROVIDER.to_string(),
            })
            .await
            .map_err(|e| format!("planner start failed: {:?}", e))?;

        // 发提示 → 收敛到一段文本。
        let mut rx = self
            .runtime
            .send(&handle, prompt)
            .await
            .map_err(|e| format!("planner send failed: {:?}", e))?;
        let buffer = collect_text(&mut rx).await?;

        // 解析 → Vec<Task>（含 roster 选兵校验 + DAG 依赖校验）。
        parse_plan_response(&buffer, roster)
    }

    /// 失败后决策：起 planner agent → 发 `build_replan_prompt` → 收敛 → 解析。
    ///
    /// 与 `plan` 同构；解析结果为 [`ReplanDecision`]，Coordinator 据此决定
    /// 重试 / 换兵 / 升级 / 收敛。
    pub async fn replan(
        &self,
        run: &CoordinatorRun,
        failed_task: &Task,
        result: &str,
        roster: &Roster,
        repo_root: &Path,
    ) -> Result<ReplanDecision, String> {
        let prompt = build_replan_prompt(run, failed_task, result);
        let agent_id = format!("planner-replan-{}", short_id());

        let handle = self
            .runtime
            .start(RuntimeStartSpec {
                agent_id: agent_id.clone(),
                cwd: repo_root.to_path_buf(),
                model: DEFAULT_MODEL.to_string(),
                provider: DEFAULT_PROVIDER.to_string(),
            })
            .await
            .map_err(|e| format!("planner(replan) start failed: {:?}", e))?;

        let mut rx = self
            .runtime
            .send(&handle, prompt)
            .await
            .map_err(|e| format!("planner(replan) send failed: {:?}", e))?;
        let buffer = collect_text(&mut rx).await?;

        parse_replan_response(&buffer, roster)
    }
}

/// 排空 AgentEvent 流，累积 TextDelta 到一段文本。
///
/// 终止条件：
///   * `Done{success:true}` → 返回累积的文本。
///   * `Done{success:false}` → `Err`（agent 自报失败）。
///   * `Failed{error}` → `Err`。
///   * channel 关闭（recv 返回 None）→ `Err`（未 Done 就断流，视为异常）。
///
/// 其它事件（ToolUse / ToolResult / Thinking / NeedsInput）忽略——planner
/// 是纯文本补全，不应触发工具调用；若模型自作主张调工具，事件被忽略，
/// 最终没有文本可解析时 parse 阶段自然会报错。
async fn collect_text(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
) -> Result<String, String> {
    let mut buffer = String::new();
    let mut done_success: Option<bool> = None;
    let mut fail_error: Option<String> = None;

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::TextDelta(text) => buffer.push_str(&text),
            AgentEvent::Done { success, .. } => {
                done_success = Some(success);
                break;
            }
            AgentEvent::Failed { error } => {
                fail_error = Some(error);
                break;
            }
            // worker 侧的事件，planner 忽略。
            _ => {}
        }
    }

    if let Some(err) = fail_error {
        return Err(format!("planner agent failed: {err}"));
    }
    match done_success {
        Some(true) => Ok(buffer),
        Some(false) => Err("planner agent Done{success:false}".to_string()),
        None => Err("planner agent stream closed without Done".to_string()),
    }
}

/// 生成短随机 id（用于 planner agent_id 去重）。与 coordinator::nanos_hex 同源。
fn short_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // 取低 32 位的 hex，足够 agent_id 去重用。
    format!("{:x}", (nanos & 0xffff_ffff))
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

    // =========================================================================
    // Task 14 测试：Planner LLM 驱动 + Fix A 双围栏解析
    // =========================================================================
    //
    // 用 MockRuntime 回放固定的 TextDelta 流 + Done{success:true}，
    // 模拟 LLM 返回已知的 plan/replan JSON。所有断言离线、确定性。

    use crate::hermes::runtime::{
        AgentEvent, AgentHandle, Liveness, RuntimeCapabilities, RuntimeError,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    /// 与 coordinator::tests::MockRuntime 同构：每个 agent_id 编程一段事件流，
    /// send() 返回 receiver 之前压入全部事件并 drop sender。
    struct PlannerMockRuntime {
        events: std::sync::Mutex<HashMap<String, Vec<AgentEvent>>>,
    }

    impl PlannerMockRuntime {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(HashMap::new()),
            }
        }

        /// 给某个 agent_id（含通配 `*` 表示「任意 agent_id 都匹配」）编程事件流。
        fn program(&self, agent_id: &str, events: Vec<AgentEvent>) {
            self.events
                .lock()
                .unwrap()
                .insert(agent_id.to_string(), events);
        }
    }

    #[async_trait]
    impl AgentRuntime for PlannerMockRuntime {
        fn capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities {
                structured_events: true,
                supports_resume: false,
                supports_permission_prompt: false,
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
            let mut map = self.events.lock().unwrap();
            // 优先精确匹配 agent_id；否则用 `*` 通配。
            let key = if map.contains_key(&handle.agent_id) {
                handle.agent_id.clone()
            } else {
                "*".to_string()
            };
            if let Some(ev_list) = map.remove(&key) {
                for ev in ev_list {
                    let _ = tx.send(ev);
                }
            }
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

    /// 把一段文本切成 TextDelta 事件 + Done{success:true} 终止。
    fn text_deltas_then_done(text: &str) -> Vec<AgentEvent> {
        vec![
            AgentEvent::TextDelta(text.to_string()),
            AgentEvent::Done {
                success: true,
                files_modified: vec![],
            },
        ]
    }

    /// 构造一个 Planner，绑定 MockRuntime。返回 (Planner, Arc<MockRuntime>) 以便测试侧 program。
    fn planner_with_mock() -> (
        Planner,
        Arc<PlannerMockRuntime>,
        std::path::PathBuf,
    ) {
        let runtime: Arc<PlannerMockRuntime> = Arc::new(PlannerMockRuntime::new());
        let planner = Planner::new(runtime.clone() as Arc<dyn AgentRuntime>);
        let repo_root = std::path::PathBuf::from("/tmp/repo");
        (planner, runtime, repo_root)
    }

    // ===== Case 1: Planner::plan with mock → 期望的 Vec<Task> =====
    #[tokio::test]
    async fn planner_plan_with_mock_returns_expected_tasks() {
        let (planner, runtime, repo_root) = planner_with_mock();
        let roster = sample_roster();
        // 用通配 `*`——planner agent_id 含纳秒随机后缀，测试侧无法预知。
        let plan_json = r#"{"tasks":[
            {"id":"t1","spec":"调研","deps":[],"assignment":{"runtime":"sdk","model":"sonnet"}},
            {"id":"t2","spec":"实现","deps":["t1"],"assignment":{"runtime":"cli","model":"glm-5.2"}}
        ]}"#;
        runtime.program("*", text_deltas_then_done(plan_json));

        let tasks = planner
            .plan("发布 v1.0", &roster, &repo_root)
            .await
            .expect("plan 应成功解析");
        assert_eq!(tasks.len(), 2);

        // 选兵 + deps 都正确。
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(tasks[0].status, TaskStatus::Ready);
        let a0 = tasks[0].assignment.as_ref().unwrap();
        assert_eq!(a0.runtime, RuntimeKind::Sdk);
        assert_eq!(a0.model, "sonnet");

        assert_eq!(tasks[1].id, "t2");
        assert_eq!(tasks[1].deps, vec!["t1"]);
        assert_eq!(tasks[1].status, TaskStatus::Pending);
        let a1 = tasks[1].assignment.as_ref().unwrap();
        assert_eq!(a1.runtime, RuntimeKind::Cli);
        assert_eq!(a1.model, "glm-5.2");
    }

    // ===== Case 2: Planner::replan with mock → 期望的 ReplanDecision =====
    #[tokio::test]
    async fn planner_replan_with_mock_returns_expected_decision() {
        let (planner, runtime, repo_root) = planner_with_mock();
        let roster = sample_roster();
        let replan_json =
            r#"{"decision":"retry","reason":"瞬时错误，重跑一次"}"#;
        runtime.program("*", text_deltas_then_done(replan_json));

        let run = sample_run();
        let task = failed_task();
        let decision = planner
            .replan(&run, &task, "TypeError: x is undefined", &roster, &repo_root)
            .await
            .expect("replan 应成功解析");

        assert_eq!(decision.decision, ReplanAction::Retry);
        assert_eq!(decision.reason, "瞬时错误，重跑一次");
        assert!(decision.assignment.is_none());
    }

    // ===== Case 3: Planner::plan 当 mock emit Failed → Err =====
    #[tokio::test]
    async fn planner_plan_when_mock_failed_returns_err() {
        let (planner, runtime, repo_root) = planner_with_mock();
        let roster = sample_roster();
        runtime.program(
            "*",
            vec![AgentEvent::Failed {
                error: "model overloaded".to_string(),
            }],
        );

        let err = planner
            .plan("发布 v1.0", &roster, &repo_root)
            .await
            .expect_err("Failed 事件 → Err");
        assert!(
            err.contains("model overloaded"),
            "错误信息应传播原始 error，got: {err}"
        );
    }

    // ===== Case 3b: Planner::plan 当 mock emit Done{success:false} → Err =====
    #[tokio::test]
    async fn planner_plan_when_done_false_returns_err() {
        let (planner, runtime, repo_root) = planner_with_mock();
        let roster = sample_roster();
        runtime.program(
            "*",
            vec![AgentEvent::Done {
                success: false,
                files_modified: vec![],
            }],
        );

        planner
            .plan("发布 v1.0", &roster, &repo_root)
            .await
            .expect_err("Done{success:false} → Err");
    }

    // ===== Case 3c: Planner::plan 当 mock 直接关流（无 Done）→ Err =====
    #[tokio::test]
    async fn planner_plan_when_stream_closed_without_done_returns_err() {
        let (planner, runtime, repo_root) = planner_with_mock();
        let roster = sample_roster();
        // 不编程任何事件——send 返回空流立刻 None。
        runtime.program("*", vec![]);

        let err = planner
            .plan("发布 v1.0", &roster, &repo_root)
            .await
            .expect_err("无 Done 关流 → Err");
        assert!(
            err.contains("without Done"),
            "应报 stream closed without Done，got: {err}"
        );
    }

    // ===== Case 7 (Fix A): 双围栏响应——先 ```rust``` 再 ```json``` → 正确解析 =====
    #[test]
    fn parse_plan_response_handles_dual_fence_rust_then_json() {
        let roster = sample_roster();
        // 模型先吐了一段 ```rust``` 闲聊代码块，再吐真正的 ```json``` 围栏。
        // 单围栏（只看第一段）会把 rust 代码当成 JSON 解析失败；双围栏枚举应跳到 json 段。
        let text = "Sure, let me think...\n```rust\nfn main() { println!(\"hi\"); }\n```\n\nHere's the plan:\n```json\n{\"tasks\":[\n  {\"id\":\"t1\",\"spec\":\"调研\",\"deps\":[],\"assignment\":{\"runtime\":\"sdk\",\"model\":\"sonnet\"}}\n]}\n```\n";
        let tasks = parse_plan_response(text, &roster)
            .expect("双围栏：应跳过 rust 段、解析 json 段");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(tasks[0].assignment.as_ref().unwrap().model, "sonnet");
    }

    // ===== Case 7b (Fix A): 三围栏 + prose 都能找到合法 JSON 段 =====
    #[test]
    fn parse_replan_response_handles_multi_fence() {
        let roster = sample_roster();
        let text = "Thinking...\n```text\nsome notes\n```\n```python\nprint(1)\n```\n```json\n{\"decision\":\"escalate\",\"reason\":\"太难了\"}\n```\n";
        let d = parse_replan_response(text, &roster)
            .expect("多围栏：应跳过非 JSON 段、解析 json 段");
        assert_eq!(d.decision, ReplanAction::Escalate);
    }

    // ===== Case 7c (Fix A): 单围栏但内容是合法 JSON —— 仍然工作（回归） =====
    #[test]
    fn parse_plan_response_single_fence_still_works() {
        let roster = sample_roster();
        let text = "```json\n{\"tasks\":[{\"id\":\"t1\",\"spec\":\"x\",\"deps\":[],\"assignment\":{\"runtime\":\"sdk\",\"model\":\"sonnet\"}}]}\n```";
        let tasks = parse_plan_response(text, &roster).expect("单围栏合法 JSON 仍工作");
        assert_eq!(tasks.len(), 1);
    }

    // ===== Case 7d (Fix A): 没有围栏的裸 JSON 仍然工作（回归） =====
    #[test]
    fn parse_plan_response_no_fence_still_works() {
        let roster = sample_roster();
        let text = r#"前缀文本 {"tasks":[{"id":"t1","spec":"x","deps":[],"assignment":{"runtime":"sdk","model":"sonnet"}}]} 后缀"#;
        let tasks = parse_plan_response(text, &roster).expect("无围栏裸 JSON 走括号扫描");
        assert_eq!(tasks.len(), 1);
    }
}

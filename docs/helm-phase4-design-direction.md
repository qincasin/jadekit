# Helm Phase 4 驾驶舱设计规范与视觉指南 (Helm Phase 4 Cockpit Design Direction)

**状态 (Status)**: 已对齐设计稿评审版 (Approved Draft)
**参考契约 (Reference Contract)**: [docs/helm-hermes-ui-contract.md](file:///Users/jiaxing/code/github/jadekit/docs/helm-hermes-ui-contract.md)
**质量基线 (Quality Baseline)**: [docs/superpowers/specs/2026-06-27-helm-hermes-design.md#section-10](file:///Users/jiaxing/code/github/jadekit/docs/superpowers/specs/2026-06-27-helm-hermes-design.md)

---

## 1. 设计宗旨：异构 AI 舰队指挥舰桥 (Design Thesis)
Helm (舵轮) 不是通用的数据大盘或报表看板，而是**异构 AI 舰队的指挥舰桥** (Command Bridge for heterogeneous AI fleet)。其核心交互理念是**两段式编排与 DAG 分工**：

```
① 规划聊 (Lead Planning Chat) ———> 产出 Plan (任务 DAG)
      │
      ▼ (派发给 Hermes 执行)
② 驾驶舱监控 (Cockpit Fleet Monitoring) ———> worker 各自独立 worktree 并行工作
      │
      ▼ (干预与收敛)
③ 检查与合并 (Review & Merge) ———> 预检/二次确认合并 retained 结果
```

### 核心设计原则
1. **形态 = 两段式 (Two-Phase Workflow)**:
   - **规划聊 (Planning Stage)**: 用户 ↔ Lead 模型（支持挑模型 + superpowers 头脑风暴 + 写 Plan）进行完整多轮对话，确定任务 Plan (DAG 依赖)。在产出的 Plan DAG 框下方，直接附带「派发此 plan / Dispatch this plan」按钮，实现一键从规划派发至 Hermes 运行。
   - **派发与执行 (Dispatch Stage)**: 确定 Plan 后，一键派发给 Hermes，Hermes 将不同子任务拆分给不同 agent，在各自独立的 `worktree` 中并行执行。
   - **驾驶舱 (Cockpit Stage)**: 舰队并行运行，用户在驾驶舱监控、干预 (Needs-Attention) 并审查/合并最终产物。
   - **分工核心**: 核心在于多 agent 的 **DAG 并行分工**（每个 agent 干不同的活）。多模型打分 (Fanout + Judge) 降级为次要的可选功能，不作为主推卖点。
2. **两种会话分清 (Separation of Sessions)**:
   - **规划聊 (Lead Chat)**: 完整的交互式聊天，支持所有高级功能（模型选择器、自动补全、Superpowers 等）。Lead 发言自带模型徽章（如 "LLM Planner (Lead) · claude-opus"）及 Superpowers 使用标记。
   - **Worker 执行会话 (Worker Transcript)**: 只读为主的执行日志，展现 agent 在其 worktree 里的执行轨迹（Thinking, Read/Edit/Bash/Search 工具调用及 Diff 预览），外加干预响应槽。支持面包屑路径追溯（如 "来自 plan · task_02"）。
3. **冷启动首屏与空态隔离 (Cold Start & Empty State isolation)**:
   - 在尚未启动任何 run 时，别展现空白的三栏。使用 Composer-forward 引导卡（“下达一个目标/计划，启动舰队 / Dispatch a goal to Hermes to launch your fleet”），左右面板展示虚线/灰化的幽灵占位。
   - 检查器 (Inspector) 的空态必须与有数据 diff 预览严格隔离，不得并存。

---

## 2. 两种会话的界面表达 (Dual Session Interface)

| 维度 | 规划聊 (Lead Planner Chat) | Worker 执行会话 (Worker Transcript) |
|---|---|---|
| **性质** | 全功能交互会话 (Interactive Chat) | 只读日志流 + 介入槽 (ReadOnly Log + Intervention) |
| **功能集** | 挑模型/Roster + 自动补全 + Superpowers (标注) + 完整历史 | 执行轨迹 (Tool_use, Thinking) + 单 agent 启停重试 + 干预响应 + 面包屑 |
| **渲染模式** | 复用 Jadekit 现有 Chat (MessageList) | 干净会话流 (Clean Message Stream) |

### Worker 执行会话目标态 (Target State)
在 Phase 3.5 桥接通后，Worker 执行会话必须呈现为“干净、无噪音”的会话流：
- **首个 Turn 聚合**: 移除重复的回退态解说大卡，将派发上下文信息（`assignee`、`failureCount`、派发时间、关联任务等）折叠隐藏到第一个 Turn（"Hermes 派发 / Hermes Dispatch"）的下拉详情中，避免首屏噪音。
- **丰富多轮展示**:
  - `ThinkingBlock`: 带有耗时与 Token 统计。
  - `ToolBlock` 簇: 包含 `Read` (文件读取参数)、`Edit` (带 `EditDiffPreview` 代码改动预览，确保删除行和新增行的对比正确且逻辑对应)、`Bash` (命令行执行输出)、`Search` (搜索匹配)。
  - `Subagent` 侧链 (reviewer sidechain): 展示子代理历史面板。
  - `needs-attention` 内联卡: 浮现权限询问 (ToolPermission)、用户问答 (AskUserQuestion) 等待干预。
- **顶部桥接小字说明**:
  > 完整会话由 Phase 3.5 worker-transcript 桥提供；实现阶段桥未接通前显示活动流回退态，不伪造 transcript。
  > Full transcripts are provided by Phase 3.5 worker-transcript bridge. Falling back to simple activity stream during early implementation.
- **单-Agent 动作**:
  - “跳到 worktree” (Jump to Worktree): Grounded/激活。
  - “停止该 agent” (Abort) 与 “重试” (Retry): 禁用占位，Tooltip 提示 "需引擎 Phase 3.5"。
  - 增加面包屑提示：在控制栏或会话标题区以 `来自 plan · <taskId>` 形式呈现，保持来源明确。

---

## 3. 状态标识 AgentStateDot (Liveness States)
严格对齐 [docs/helm-hermes-ui-contract.md §5.1](file:///Users/jiaxing/code/github/jadekit/docs/helm-hermes-ui-contract.md) 契约词表。

| 状态 Token | CSS 映射角色 | 白底无障碍设计 (Contrast & Indicator) | 语义与用户行为 |
|---|---|---|---|
| `working` | 黄/橙转圈 | 选用偏深暗金黄色 (如 `#b45309`/`#92400e`)；加 1px 细暗色边框以防在白底上飘。 | Stream 活动中 (thinking/text/tool_use)；用户可监控或停止。 |
| `needs-attention` | 琥珀色 Solid | 强对比度琥珀色 (如 `#d97706` + `#78350f` 描边)，内部配以白色感叹号 `!` 图标。 | 等待用户输入或工具授权。在此状态下不会因超时被强杀。 |
| `done` | 翠绿色 | 翠绿色 (如 `#10b981`) 环，内部带对勾 `✓` 图标。 | Agent 正常退出。失败语义由任务 DTO 的 `failed` 表达，防止双重标红。 |
| `interrupted` | 红色 | 红色 (如 `#ef4444`) 方框，内部带方形阻断块 `▢` 标识。 | 被 supervisor 强杀、超时或异常中止，需人工审查并重试/废弃。 |

> [!IMPORTANT]
> 琥珀色 (`needs-attention`) 与 红色 (`interrupted`) 必须严格区分：前者表示“安全等待，请干预”，后者表示“异常中断，已终止”。颜色不能是唯一信号，必须搭配形状、文字和 ARIA 描述。

---

## 4. 顶部与底部交互配置 (Composer, Roster & Commands)

### 4.1 顶部 Run Bar
- **Orchestration Pulse**: 单一的全局脉冲线（进度条带有 `.pulse-rail` 与动效光标），集中表达整条编排的推进度，避免多个分散的进度环。
- 包含 `Cmd+K 跳转 (Jump)` 快捷键提示、Roster 配置面板入口、及全局停止 Run 按钮。

### 4.2 底部 Composer 与 Roster
- **口吻调整 (A Tone)**:
  - Composer placeholder: "输入目标或计划，Hermes 将拆分并派发给舰队并行执行... / Enter a goal or plan, Hermes will decompose and dispatch to the fleet in parallel..."。
- **模型配置 (Roster Selection)**:
  - 接入 Roster 下拉菜单，选定“派哪些 CLI × 模型”。
  - Fanout 降为不显眼的小型勾选项（"并行评判 (Fanout & Judge)"），默认不启用。
- **`@` 菜单分类 (Categorized Mentions)**:
  - **文件** (`文件 / Files`): Grounded，可点击联想输入。
  - **技能** (`技能 / Skills`): 走 `/` 触发，Grounded，可点击选择。
  - **插件 / MCP工具 / agent能力** (`插件/MCP/能力 / Plugins/MCP/Capabilities`): 置灰禁用占位，Tooltip 提示 "Phase 5/6 规划中"。
- **演示运行按钮 (Demo Run Button)**:
  - 位于 Composer 右侧/下方，用于通过 `hermes_run_mock` 脚本化回放测试，不消耗 token。
  - 当前状态为**禁用占位**，Tooltip 提示 "需引擎 Phase 3.5"。

---

## 5. 三栏布局与面板交互 (Pane Management)
- **左 = 舰队看板 (Fleet Kanban)**:
  - 按任务状态 `Pending`(待派) -> `Running`(执行中) -> `Review`(待评审/awaiting-merge) -> `Done/Failed`(完成/中断) 分为四列泳道。
  - 点击 Worker 卡片，中栏状态投影切换到该 Worker 的执行会话。
- **中 = 动态工作区 (Active Workspace)**:
  - 无选中 worker 时：展示 **规划聊 (Lead Planner Chat)** 全功能面板。
  - 选中 worker 时：展示 **Worker 执行日志 (Worker Transcript)**。顶部提供 “返回主规划 / Back to Planner” 按钮。
- **右 = 检查器 (Inspector)**:
  - **空态隔离**: 默认显示 `尚未选择待评审 worker / No worker selected` 空卡，右下角“合并”/“丢弃”按钮禁用。
  - **评审态**: 只有在左侧选中处于 `awaiting-merge` / `Review` 状态的 retained worktree 后，才渲染 diff 视图及“合并保留结果”（带预检+二次确认）和“预检后丢弃”操作。

---

## 6. 响应式与无障碍基线 (Responsive & A11y)
- **窄屏适配**: 宽度低于 1100px 时，检查器 (Inspector) 折叠为底部滑块；低于 760px 时，三栏收缩为单栏，通过顶部 Pane Switcher 切换。
- **键盘优先**: 所有交互卡片可获得焦点 (`focus-visible`)，支持 `Cmd+Enter` 派发。
- **减弱动画 (Reduced Motion)**: 响应 `prefers-reduced-motion`，屏蔽旋转环动画，退化为静止状态符号。

# 交接：Gemini 重做 Helm Phase 4 驾驶舱 UX（仅设计稿，不写代码）

> 给 Gemini 的冷启动交接。**当前处于 UI/UX 设计阶段——只出设计稿（设计文档 + 静态 mock），不写生产代码、不进 Phase 4 代码任务。** 出稿后停下交人工确认基调。

## 启动话术（整段可直接粘）

```
你来重做 Helm 驾驶舱(Phase 4)的一版 UX —— 只出设计稿（设计文档 + 静态 mock），不写生产代码、不进 Phase 4 代码任务。我们还在 UI/UX 设计阶段。

唯一依据先读这份汇总简报（多轮讨论 converge 的结论都在里面）：
docs/helm-phase4-ux-brief.md
配合参考：docs/helm-hermes-ui-contract.md（契约/事实源）、docs/superpowers/specs/2026-06-27-helm-hermes-design.md §10（质量基线）、既有稿 docs/helm-phase4-design-direction.md + docs/helm-phase4-cockpit-mock.html（可在其上刷新）。

产出：刷新 docs/helm-phase4-design-direction.md + docs/helm-phase4-cockpit-mock.html 两个文件，出一版可视稿；出完停下来交人工确认基调，不要继续写代码。

死守这几条（简报里有完整版）：
1. 形态=两段式：规划聊（全功能 chat：挑模型+superpowers+补全+完整 transcript）→ 派发给 Hermes（把不同子任务分到不同 agent、各自独立 worktree 并行干）→ 收敛 → 驾驶舱看/干预/合并。核心是 DAG 分工，不是"大家想一遍打分"（fanout+judge 降为不起眼的可选）。
2. 两种会话分清：规划聊=全功能交互 chat；worker 执行会话=只读为主的执行日志+干预槽。
3. 中栏 worker 会话画成"丰富多轮目标态"（假数据：多轮 turn + ThinkingBlock + Read/Edit(diff)/Bash/Search tool block + 子代理 + MessageMeta），顶部一句"目标态 vs 桥未通回退态"说明；清掉重复的回退卡和单独的派发信息大卡，做成一条干净会话流。
4. 默认 light/白 + 文案中文（同步 zh/en）；AgentStateDot 4 态对齐契约、白底验对比度（琥珀=等待 vs 红=中断严格分）。
5. composer：A 口吻（下达目标/计划→拆分到 worktree 并行）；模型选择=Roster；@ 菜单画成分类【文件/技能 可点，插件/MCP工具/agent能力 禁用占位+tooltip"Phase 5/6"】；加一个「演示运行」按钮（禁用占位，tooltip"需引擎 Phase 3.5"，将来用脚本化 mock 在 UI 里测、不烧 token）。
6. 复用优先（现有 chat 渲染/toolBlocks/对话框/选择器，重组不重写）；契约即事实源，禁自创字段；grounded 的画可点、后补的画占位禁用（见简报 §11 一览表）。
7. 冷启动首屏（无 run）：composer-forward"下达一个目标/计划，启动舰队"，别空白三栏；右栏检查器空态别和有数据预览并存。
8. 代码探索用 codegraph（jadekit+orca 已 sync）。出稿即停，等人工确认。
```

## 两段重点文字（已在简报，顺手版）

**① LLM mock 测试（简报 §7）：** 引擎测试里已有 MockRuntime（脚本化回放 LLM 事件），Phase 3.5 提升为 `hermes_run_mock`，复用与真实 run 完全相同的事件/命令路径、只换介质 → 发真 `hermes://run|task|agent` 事件，cockpit 正常渲染，零真 LLM。UI 加「演示运行」按钮（当前禁用占位），将来点它在 cockpit 里跑脚本化编排看全流程，不烧 token——日常验收主通路。剧本覆盖：多 worker 并行、working/tool_use、needs-attention、done、interrupted、awaiting-merge。

**② @ 菜单（简报 §6）：** composer 的 `@` 菜单画成分类：`文件`（grounded 可点）、`技能`（走 `/` grounded 可点）、`插件`/`MCP 工具`/`agent 能力`（禁用占位 + tooltip"Phase 5/6"）。另加 plugin/skill 选择器 picker（禁用占位）。设计画全形态，Phase 4 只实现可点的，禁用项实现时略过。

## 环境 / 接力说明

- 工作分支：`feat/helm-phase4a-shell`（Codex 已提交 Task 1 设计稿在 `89af8ba`，Gemini 在其上刷新那两个文件）。
- superpowers / frontend-design 技能若 Gemini 环境不可用：忽略，按简报 + §10 手动做设计 pass，同样出可视稿。
- **只做设计稿，出完即停，等人工确认基调**——确认后才进 Phase 4 代码（届时另起执行话术）。

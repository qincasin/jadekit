# Helm Phase 4 驾驶舱 — UX 对齐简报（Gemini 重做 UX 的依据）

> 用途：把多轮讨论converge 出来的 UX 决定汇总成**单一依据**。Gemini 据此**重新出一版 UX**（设计文档 + 静态 mock），不要从旧理解重新推。配合：契约 `docs/helm-hermes-ui-contract.md`、质量基线 `docs/superpowers/specs/2026-06-27-helm-hermes-design.md §10`、实现计划 `docs/superpowers/plans/2026-06-28-helm-hermes-phase4-cockpit.md`、既有设计稿 `docs/helm-phase4-design-direction.md`（可刷新）。
>
> **原则：设计稿用真实感假数据画"目标态"是对的；只有上线代码不许给用户看假数据。grounded 的现在可点，后补的画占位禁用 + tooltip。**

## 1. Helm 是什么（形态定义，最重要）—— 两段式，别画偏

```
① 规划聊：你 ↔ 某个模型正常聊天（挑模型 + 可挂 superpowers 头脑风暴/写 plan）→ 产出 plan(任务 DAG)
② 派发给 Hermes：plan 定了 → Hermes 把【不同子任务】分给【不同 agent】、各自【独立 worktree】并行干 → 收敛合并
③ 驾驶舱：看舰队执行 / 干预 / 合并
```

- **核心 = DAG 分工**（A）：一个 plan 拆成多个**不同**子任务，各 agent 在**各自 worktree** 干**不同**的活，产物合并。**这是 Helm 的本体。**
- **不是**（B）：把同一件事丢给 N 个模型各想一遍再打分——那是 fanout+judge，**与 superpowers/openspec 重叠，降为次要可选，别当卖点**。
- **plan 来源**：主路径=你+聊+superpowers 自己定 plan 喂给 Hermes；便利路径=给个目标让 LLM Planner 自动拆。两者都留，主用前者。
- **Helm 比 superpowers/openspec 多的**：进程级真并行 + 每 agent 独立 worktree + 持久化编排 + 崩溃恢复 + 驾驶舱。互补不重复。

## 2. 两种会话（关键区分，决定每栏多丰富）

| | **规划聊**（你 ↔ lead 模型） | **worker 执行会话**（舰队里每个 agent） |
|---|---|---|
| 性质 | **完整交互式 chat** | **只读为主的执行日志** |
| 要什么 | 挑模型 + superpowers + 补全 + 完整 transcript（**全功能，复用现有 jadekit chat**） | 活动流 / transcript(桥后) + 干预槽（不需要逐条对话） |

## 3. 三栏 cockpit（面板可开合、单入口、非路由）

- **左 = 舰队看板**：Kanban 泳道（待派/执行中/待评审/完成）+ AgentStateDot + 每卡 CLI 图标 + 模型徽章 + 分支 + diff +/-；跨泳道拖拽=干预。
- **中 = 选中 agent 的执行会话**（worker，只读为主）+ 干预槽。
- **右 = 检查器**：worktree diff + 合并/丢弃（预检+二次确认）+（judge 次要，可不画）。
- 顶部 run bar：plan/run 状态 + 进度 pulse + Cmd+K + Roster + 停止/取消。
- 底部 composer：派发入口（见 §6）。

## 4. 中栏 worker 会话 —— 丰富目标态（mock 用假数据画足）

画成**一段有血有肉的多轮 agent 工作会话**（这是 Phase 3.5 transcript 桥接通后的目标态）：
- 多轮 message-turn：派发 → thinking → tool_use(Read)+结果 → thinking → tool_use(Edit)+**diff 预览** → text → tool_use(Bash)+输出 → 子代理 sidechain 可展开 → 更多轮 → done；每轮带 token/耗时。
- 复用现有视觉语言：MessageList 轮次 + toolBlocks（Read/Edit+EditDiffPreview/Bash/Search，可展开看命令·参数·结果·diff）+ ThinkingBlock + SubagentHistoryPanel + MessageMeta。
- 中间夹一个 needs-attention 内联干预卡。
- **顶部一句小字**："完整会话由 Phase 3.5 worker-transcript 桥提供；实现阶段桥未接通前显示活动流回退态，不伪造 transcript。"
- **清理**：不要保留重复的"回退态"解说卡 + 单独的"派发信息"大卡——回退说明只留顶部一句；派发信息（assignee/failureCount/时间）折进会话头一个可展开小条。**一条干净的会话流。**
- 会话头单-agent 动作：跳到 worktree（grounded）；停止该 agent / 重试（禁用占位，tooltip"Phase 3.5"）。

## 5. AgentStateDot 状态词表（逐字对齐契约 §5.1，白底验对比度）

`working`(黄/转圈) · `needs-attention`(琥珀，权限/等待输入) · `done`(翠绿勾) · `interrupted`(红，reap/abort)。
- **严格区分**：琥珀=等待（永不被超时杀）、红=中断/卡死。
- **白底默认**：amber/working 黄在纯白上易发飘，用略深色或加描边环，确保 4 态一眼可辨。色彩不能是唯一信号（配图标/形状/aria）。

## 6. composer / 派发（A 口吻 + 能力入口）

- **口吻 = A**："下达目标/计划 → Hermes 拆成子任务 → 分发到各 worktree 并行执行"。**不是**"让大家并行想"。
- **模型选择 = Roster**：复用 SelectorDropdown(model/mode/reasoning) + ModelIcon + provider 管理，选"派哪些 CLI×模型"。
- **fanout 降级**：作为一个不起眼的可选模式，不当头牌。
- **`@` 菜单分类（设计画全，grounded 可点 / 其余禁用占位 Phase 5/6）**：
  - `文件`（grounded，可点）、`技能`（走 `/` grounded，可点）；
  - `插件` / `MCP 工具` / `agent 能力`（**禁用占位 + tooltip"Phase 5/6"**）。
  - 另：plugin/skill 选择器 picker——禁用占位。
- 键盘优先（⌘Enter 派发）。

## 7. 测试通路 —— 「演示运行」按钮（脚本化 mock LLM，不烧 token）

> 命令行 devtools 测编排太麻烦。引擎测试里已有 MockRuntime（脚本化回放 LLM 事件），Phase 3.5 会提升为 `hermes_run_mock`，复用与真实 run **完全相同的事件/命令路径**，只换介质 → 发真 `hermes://run|task|agent` 事件，cockpit 正常渲染，零真 LLM。

- composer 旁加一个 **「演示运行」按钮** → 将来调 `hermes_run_mock` → 在 UI 里点一下就驱动整条编排看效果（舰队状态点实时刷、点卡进会话、diff/合并、干预浮现、Cmd+K 跳）。
- 当前禁用态 + tooltip"需引擎 Phase 3.5"。这是**日常验收主通路**，真 LLM run 作补充。
- 内置演示剧本应覆盖：并行多 worker、working/tool_use、needs-attention、done、interrupted、awaiting-merge——让所有状态点/泳道/干预点都能被"点出来"。

## 8. 干预点（全部友好）

- 复用现有 `ToolPermissionDialog` / `AskUserQuestionDialog` / `PlanApprovalDialog`，适配多 agent（标明"哪个 agent / worktree 在请求"）。
- DecisionGate 应答 UI。卡住/失败友好浮现（说清"发生什么+怎么办"+ 跳转/重试入口）。
- 破坏性操作（删 worktree/丢弃/合并冲突）：预检 + 二次确认 + 回滚提示。

## 9. 设计基调（command bridge，沿用 `helm-phase4-design-direction.md`，可刷新）

- thesis = 异构 AI 舰队的指挥舰桥（非通用 dashboard）；signature = AgentStateDot 舰队 + 三栏舰桥 + 单一 orchestration pulse（收成一个表达，别散）。
- 在 DaisyUI 主题 token 上扩展，不另起配色；数据密度区 mono/tabular。
- **冷启动首屏**（还没任何 run）：composer-forward，"下达一个目标/计划，启动舰队"，别空白三栏。

## 10. 质量基线 + 通则（§10 硬标准）

- **默认 light/白**（dark 跟随主题）；**文案默认中文**、同步 zh/en（现在 mock 别全英文）。
- 键盘优先 + 可见焦点；实时不卡 + 长列表虚拟化；reduced-motion；明暗 + 移动窄屏。
- 每面板 empty/loading/error 三态文案。
- **复用优先**：现有 chat 渲染/toolBlocks/对话框/fanout 对比/composer 选择器，重组进 cockpit，不重写。
- **契约即事实源**：命令/事件/字段/状态 token 全部对齐 `docs/helm-hermes-ui-contract.md`，禁自创。
- 右栏检查器：空态不要和有数据 diff 预览并存。

## 11. grounded vs 后补 一览（设计画哪些可点 / 哪些禁用占位）

| 元素 | 现在 | 处理 |
|---|---|---|
| 三栏/Kanban/AgentStateDot/⌘K/Roster/composer 派发/diff·合并·丢弃/干预对话框 | grounded | 正常做 |
| 规划聊全功能（挑模型 + `/`技能 + `@`文件 + superpowers） | grounded | 正常做 |
| worker 完整 transcript | 待 Phase 3.5 桥 | 目标态 mock 假数据画；实现走活动流回退 + transcript-ready 容器 |
| 单-agent 停止/重试 | 待 Phase 3.5 | 禁用占位 |
| 演示运行（mock LLM 测试） | 待 Phase 3.5 | 禁用占位（按钮画上） |
| `@`插件/MCP工具/agent能力、plugin·skill picker、per-task 能力配置 | 待 Phase 5/6 | 禁用占位 |
| judge 评判结果 | 次要、休眠 | 可不画或极次要 |

---

**Gemini 任务**：基于本简报 + 既有 mock，**重出一版 UX**（刷新 `docs/helm-phase4-design-direction.md` + `docs/helm-phase4-cockpit-mock.html`），把上面 grounded 的画足、后补的画占位禁用，中栏 worker 会话画成丰富目标态、规划聊体现全功能 chat、形态体现"规划→派发→各 worktree 并行→收敛"。出稿后交人工确认基调，再进 Phase 4 代码。

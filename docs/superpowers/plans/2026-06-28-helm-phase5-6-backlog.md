# Helm Phase 5/6 — Backlog（后补项,设计稿可先画占位,实现略过)

> 这些是已对齐"方向正确、但当前后端给不出 / 非当前阶段核心"的能力。**Phase 4 设计稿可以把入口画出来(禁用态 + tooltip"Phase 5/6"),但 Phase 4 不实现、实现时跳过这些禁用项。** 回头按本 backlog 派 agent 补。

## 能力引用 / @ 菜单扩展(规划聊)

- **`@` 引用技能 / 插件 / MCP 工具**(当前只 grounded 了 `@`文件 + `/`斜杠技能):把 `@` 菜单扩成分类 `文件 / 技能 / 插件 / MCP 工具 / agent 能力`。
- **per-agent 作用域**:`@` 到**某个 worker agent 名下的 MCP / skill**(把能力绑定到具体 worker)。
- **plugin / skill 选择器**(composer 里的 picker)。

## per-task agent 能力配置(引擎侧)

- 派发某 worker 时**指定它拥有哪些 MCP / skill / plugin**(能力随任务下发)。需要扩 `AgentAssignment` + 派发路径,属引擎活。

## fanout + judge(降为次要可选)

- fanout(同一任务 N 模型并行 + judge 打分)与 superpowers/openspec 重叠,**不是 Helm 核心**(核心是 DAG 分工 + worktree 并行)。
- judge 引擎(Phase 3f)已建好但**不优先接 Coordinator**;保持休眠,**仅当 fanout 择优真有需求时**再接 `hermes_judge_show` + UI。
- UI 把 fanout 降成不起眼的可选模式,不当卖点。

## 自带 plan 执行(已挪到 Phase 3.5,列此备忘)

- `hermes_run_with_plan(tasks[])`:接收现成 DAG(规划聊 / superpowers / openspec 产出)直接执行,不经 LLM Planner。**这条优先级高,已放 Phase 3.5**(不是 5/6);此处仅交叉引用。

## 单-agent abort / retry(已挪到 Phase 3.5,列此备忘)

- 会话头"停止该 agent / 重试"——Phase 3.5 `hermes_agent_abort`;UI 当前禁用态占位。

---

**优先级提示**:`hermes_run_with_plan` 与单-agent abort 在 Phase 3.5(近期、小);`@`能力引用 / per-agent 能力作用域 / per-task 能力配置 / fanout-judge 在 Phase 5/6(远期)。设计稿对所有这些画占位即可,实现按阶段来。

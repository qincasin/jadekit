# 代理热更新设计:接通代理闭环(热更新 + 熔断 + 故障转移 + 用量)

> 日期:2026-06-27
> 作用域:`src-tauri/src/proxy/*`、`src-tauri/src/services/provider_service.rs`、proxy/takeover 相关命令与前端开关
> 对齐基准:cc-switch(`/Users/jiaxing/code/github/cc-switch`)的 proxy takeover + live config 机制

## 背景与问题

切换 Provider/模型后,**运行中的终端 Claude Code 会话拿不到新模型**,需 `/model` 或重启。

**根因(上游硬限制,已用本机 cc-switch 源码 + Claude Code 2.1.195 binary 双重验证):**
- Claude Code 的模型别名 env(`ANTHROPIC_DEFAULT_*_MODEL`)**只在会话启动时读一次**,运行中改文件不重读。
- cc-switch 不开代理时,切换也只是 `sync_current_to_live` 写文件,**同样只能让新会话生效**;它的运行中热更新**完全依赖代理 takeover**(源码注释 `During proxy takeover...`)。
- 结论:**运行中会话热更新 = 必须开代理**,无第二条路。

## jadekit 代理现状(已逐一核实)

jadekit 已有**照 cc-switch 设计搭好的代理骨架**,但请求链没接通:

| 模块 | 存在 | 接入请求链 | 状态 |
|---|---|---|---|
| `server.rs` | ✅ | — | 活:监听端口,挂 /health + fallback proxy_handler |
| `handlers.rs::proxy_handler` | ✅ | ✅ | 活,但**只纯转发** |
| `provider_router.rs::resolve_upstream` | ✅ | ✅ | 半残:内部调 `list_providers` 返回空 `vec![]` |
| `failover_switch.rs` | ✅ | ❌ | 已写未接(`get_available_provider` 已含熔断检查) |
| `circuit_breaker.rs` | ✅ | ❌ | 已写,仅被 failover 引用 |
| `model_mapper.rs` | ✅ | ❌ | dead_code(`apply_model_mapping` 现成可用) |
| `usage/{parser,calculator,logger}` | ✅ | ❌ | 已写未接 |
| `types.rs` `LiveBackup`/`ProxyTakeoverStatus` | ✅ 类型 | ❌ | 类型在,无写入/还原逻辑 |

本设计 = **接线 + takeover + 修残桩**,不是从零造。

## 已确认的关键设计决策

1. **必须开代理**才支持运行中热更新(上游限制)。
2. **takeover 方式**:开代理时改写 `~/.claude/settings.json` 的 `ANTHROPIC_BASE_URL` → `http://127.0.0.1:<port>`,原值备份进 `LiveBackup`;关代理时还原。
3. **模型映射真相源**:开代理时由代理在**每个请求**动态改写(`model_mapper`,读 active provider 的模型+1M 声明);不靠 env。
4. **env 模型字段处理**:takeover 时把 `ANTHROPIC_DEFAULT_*_MODEL` 连同 BASE_URL 一起从 settings.json 移除并备份(避免与代理改写双重 `[1M]`/冲突),关代理时还原。对齐 cc-switch "live 归代理所有"。
5. **范围:全接** —— 热更新 + 熔断 + 故障转移 + 用量,四块都接进 proxy_handler。

## 架构

```
CLI ──(ANTHROPIC_BASE_URL=127.0.0.1:port)──> proxy_handler
   1. provider_router : 实时选 active provider(修残桩:list_providers→list_providers_from_db)
   2. failover_switch : get_available_provider(app) → 熔断检查后选可用 provider
   3. model_mapper    : apply_model_mapping(body, &provider) 改写请求体模型名(+[1M])
   4. forward         : http_client::forward_request 到上游(base_url + auth header)
   5. usage           : parser::extract_usage/extract_model + calculator::calculate_cost + logger::log_event
   6. circuit_breaker : 成功 failover_switch::on_success / 失败 on_failure 回写熔断状态
   7. 流式转发响应给 CLI(Body::from_stream,沿用现有)
```

外加 **takeover** 子系统:开/关代理时改写 + 备份/还原 settings.json。

## 组件设计

### A. 修残桩:provider_router 实时选 provider
`resolve_upstream` 当前调 `provider_service::list_providers`(空残桩)。改为调真实 DB 查询。但因为要接故障转移,实际选 provider 的职责交给 **failover_switch::get_available_provider(app)**(它内部已做熔断检查 + 选 active 或队列可用 provider)。`resolve_upstream` 退化为"根据选定的 provider 构建 target_url + auth header"。

### B. 接线:proxy_handler 串起完整链
重写 `proxy_handler`:
1. 读请求体(已有)
2. `let provider = failover_switch::get_available_provider(AppType::Claude)?;`(含熔断)
3. `let (mapped_body, original_model, outbound_model) = model_mapper::apply_model_mapping(json_body, &provider);`
4. 构建 target_url + auth header(从 provider.url / api_key,逻辑同现有 resolve_upstream)
5. `forward_request(method, &target_url, headers, mapped_body_bytes)`
6. 成功:`failover_switch::on_success(&provider.id, &cfg)`;失败:`on_failure(&provider.id, &cfg, ...)`
7. 用量:从响应(非流式)或流式累积中 `extract_usage` → `calculate_cost` → `logger::log_event`
8. 流式转发响应(沿用 Body::from_stream)

### C. 故障转移与流式的边界(重要约束)
故障转移只在**请求建立阶段**生效:若 forward 在**拿到响应头之前**失败(连接失败/5xx 立即返回),可切下一个 provider 重试。**一旦开始流式输出 SSE,不再切换**(已输出的内容无法回滚)。`get_available_provider` 在熔断打开时跳过该 provider。

### D. takeover:开/关代理改写 settings.json
- **开代理**(start_proxy 成功后,对启用接管的 app):
  1. 读 `~/.claude/settings.json`,把 `env.ANTHROPIC_BASE_URL` 和 `env.ANTHROPIC_DEFAULT_*_MODEL`(4个)的原值序列化存入 `LiveBackup`(写 DB,沿用 live_backup 表/存储,若无则新增)。
  2. 改写 `env.ANTHROPIC_BASE_URL = "http://127.0.0.1:<actual_port>"`,移除 4 个模型 env,写回。
  3. 置 `ProxyTakeoverStatus.claude = true`。
- **关代理**(stop_proxy):从 `LiveBackup` 还原原始 env(BASE_URL + 模型字段),清 takeover 标志,删备份。
- **幂等 + 崩溃恢复**:启动时若发现 takeover 标志为真但代理没跑,应还原(避免 settings.json 永久指向死代理)。沿用 `restore_proxy_state_on_startup` 思路(cc-switch lib.rs:1051 有同名逻辑)。

### E. 用量记录
代理转发成功后,用 `usage::parser::extract_usage(body)` 取 input/output tokens、`extract_model` 取模型(优先 outbound_model 真值),`calculator::calculate_cost` 算成本,`logger::log_event(RequestLogEvent{...})` 落盘。流式响应需累积 SSE 末尾的 usage 事件。logger 需在代理启动时 `init_logger(log_dir)`。

## 错误处理
- forward 在响应头前失败 → 故障转移重试下一个(若开启);全部失败 → 502 给 CLI,记 on_failure。
- 流式中断 → 透传错误给 CLI,不重试。
- takeover 改写 settings.json 失败 → start_proxy 报错并回滚(不能让代理跑起来但 CLI 没指过来,或反之)。
- settings.json 不存在/非法 JSON → 明确错误,不静默。

## 测试
- 单测:`model_mapper::apply_model_mapping` 已有逻辑;`circuit_breaker` 状态机;takeover 改写+还原(给定 settings JSON,断言 BASE_URL 被改、模型 env 被移、LiveBackup 存原值;还原后完全复原;幂等)。
- 单测:故障转移选 provider(熔断打开跳过)。
- 手动 e2e:开代理 → 运行中 claude 会话 → jadekit 切换 provider/模型 → 会话下一个请求即走新模型(`/context` 看 1M、响应头看模型);关代理 → settings.json 完全还原。

## 非目标(YAGNI)
- 不做 API 格式转换(Claude↔OpenAI↔Gemini),只代理 Claude。Codex/Gemini 的 takeover 本轮不做(仅 Claude)。
- 不改前端代理面板的大改版,只需保证有启停代理 + 接管开关的入口(若已有则复用)。
- 流式中的故障转移不做。

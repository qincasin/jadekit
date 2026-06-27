# 代理热更新 Implementation Plan(接通热更新+熔断+故障转移+用量)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 jadekit 开启代理后,运行中的 Claude Code 会话切换 Provider/模型即时生效(热更新),并把已写好的熔断/故障转移/用量接进代理请求链。

**Architecture:** 接通 jadekit 现有代理骨架成完整闭环:修 `list_providers` 残桩 → proxy_handler 串起 failover/model_mapper/forward/usage/circuit_breaker → 新增 takeover(开代理改写 settings.json 的 BASE_URL 并移除模型 env、备份还原)。运行中会话因走本地代理,下一个请求即走新配置。

**Tech Stack:** Rust (Tauri 2, axum, reqwest, tokio, serde_json), 前端 React/TS(仅代理开关入口)。

## Global Constraints

- 新增代码补中文注释(尤其状态流转、配置写入、安全边界)。
- 不写魔法字符串:端口/BASE_URL 模板/要移除的 env key 清单/备份文件名都用集中常量。复用已有常量:`CLAUDE_PROVIDER_ENV_KEYS`(provider_service.rs,Task3 已加)可用于模型 env 移除参考。
- 不动 `src/components/chat/StatusPanel.tsx` 及任何 antigravity 在途文件。
- 后端门禁 `cd src-tauri && cargo test` 全绿;前端门禁 `npx tsc --noEmit`(本项目无 yarn lint)。
- 用 TDD:先写失败测试→看失败→实现→看通过→提交。每个 task 单独 commit,末尾加 `Co-Authored-By: Codex <noreply@openai.com>`。
- 设计依据:`docs/superpowers/specs/2026-06-27-proxy-hotreload-design.md`。对齐基准 cc-switch:`/Users/jiaxing/code/github/cc-switch`。
- 关键约束:故障转移只在请求建立阶段(拿到响应头前)生效;**流式开始后不切换 provider**。
- takeover 本轮**仅 Claude**(Codex/Gemini 不做)。

## 文件结构

- `src-tauri/src/services/provider_service.rs` — 修 `list_providers` 残桩(Task 1)
- `src-tauri/src/proxy/handlers.rs` — 重写 proxy_handler 串起完整链(Task 4)
- `src-tauri/src/proxy/takeover.rs`(新建)— takeover 改写/备份/还原(Task 2)
- `src-tauri/src/services/proxy_service.rs` — start/stop 调 takeover + init_logger(Task 3、Task 5)
- `src-tauri/src/lib.rs` — 启动时崩溃恢复还原 takeover(Task 6)

---

### Task 1: 修复 list_providers 残桩(整个链路根基)

**Files:**
- Modify: `src-tauri/src/services/provider_service.rs`(`list_providers` ~259 返回空 vec 的残桩)
- Test: 同文件 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub fn list_providers(app: AppType) -> Result<Vec<Provider>, String>` 返回该 app 真实 providers(供 `failover_switch`/`provider_router` 用)。

背景:`failover_switch::get_available_provider` 和 `on_failure` 都调 `provider_service::list_providers`,但该函数当前是 `Ok(vec![])` 残桩(注释 `TODO: 此函数保留兼容`)。这是代理选不到 provider 的根因。但该函数签名不带 `db`,而 DB 在 `AppState` 里。

- [ ] **Step 1: 确认 DB 获取方式**

读 `provider_service.rs` 看其它函数(如 `list_providers_from_db`)如何拿 `Arc<Database>`。代理上下文没有 `State` 注入,需要一个全局 DB 句柄。检查 `src-tauri/src/store.rs` / `lib.rs` 是否已有全局 `AppState`/DB 单例(grep `OnceLock<.*Database>` / `static.*DB` / `global_db`)。

- [ ] **Step 2: 写失败测试**

在 mod tests 加(若已有全局 DB 测试夹具则复用;否则测 list_providers_from_db 的过滤逻辑即可):
```rust
#[test]
fn test_list_providers_returns_app_filtered() {
    // 用 in-memory Database 插入 2 个 claude + 1 个 codex provider,
    // 断言 list_providers(Claude) 返回 2 个且都是 claude。
    // (参考现有 dao/provider_service 测试如何建 in-memory db)
}
```

- [ ] **Step 3: 实现**

若存在全局 DB 单例:让 `list_providers` 从单例取 DB 再 `list_providers_from_db(&db, app)`。
若不存在:新增 `pub fn set_global_db(db: Arc<Database>)` + `static GLOBAL_DB: OnceLock<Arc<Database>>`,在 `lib.rs` setup 时注入;`list_providers` 从中读取。中文注释说明这是为代理(无 State 上下文)提供 DB 访问。

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test test_list_providers_returns_app_filtered`
Expected: PASS。再 `cargo test` 全绿。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/provider_service.rs src-tauri/src/lib.rs
git commit -m "fix(proxy): 修复 list_providers 残桩,代理可实时读 active provider

Co-Authored-By: Codex <noreply@openai.com>"
```

---

### Task 2: takeover 改写/备份/还原(纯函数 + 文件持久化)

**Files:**
- Create: `src-tauri/src/proxy/takeover.rs`
- Modify: `src-tauri/src/proxy/mod.rs`(加 `pub mod takeover;`)
- Test: `takeover.rs` 内 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces:
  - `const PROXY_TAKEOVER_ENV_KEYS: &[&str]` = `["ANTHROPIC_BASE_URL","ANTHROPIC_DEFAULT_SONNET_MODEL","ANTHROPIC_DEFAULT_OPUS_MODEL","ANTHROPIC_DEFAULT_HAIKU_MODEL","ANTHROPIC_REASONING_MODEL"]`
  - `pub fn apply_takeover_to_settings(settings: &mut serde_json::Value, proxy_base_url: &str) -> serde_json::Value` — 返回被移除/改写字段的备份(原 env 子集),并就地把 settings 的 env.ANTHROPIC_BASE_URL 设为 proxy_base_url、移除 4 个模型 env。
  - `pub fn restore_takeover_to_settings(settings: &mut serde_json::Value, backup: &serde_json::Value)` — 用备份还原 env。
  - `pub fn save_backup(backup: &serde_json::Value) -> Result<(), std::io::Error>` / `pub fn load_backup() -> Option<serde_json::Value>` / `pub fn clear_backup()` — 备份持久化到独立文件 `{app_data}/proxy_live_backup.json`(崩溃可恢复)。备份文件路径用常量。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn test_apply_and_restore_takeover_roundtrip() {
    use serde_json::json;
    let mut settings = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.zhipu.com",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5.2[1M]",
            "ANTHROPIC_AUTH_TOKEN": "sk-x",
            "KEEP_ME": "yes"
        }
    });
    let backup = apply_takeover_to_settings(&mut settings, "http://127.0.0.1:8080");
    // BASE_URL 改写、模型 env 移除、无关字段保留
    assert_eq!(settings["env"]["ANTHROPIC_BASE_URL"], json!("http://127.0.0.1:8080"));
    assert!(settings["env"].get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none());
    assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-x"));
    assert_eq!(settings["env"]["KEEP_ME"], json!("yes"));
    // 还原后完全复原
    restore_takeover_to_settings(&mut settings, &backup);
    assert_eq!(settings["env"]["ANTHROPIC_BASE_URL"], json!("https://api.zhipu.com"));
    assert_eq!(settings["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], json!("glm-5.2[1M]"));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cd src-tauri && cargo test test_apply_and_restore_takeover_roundtrip`
Expected: 编译失败(takeover 模块/函数不存在)。

- [ ] **Step 3: 实现 takeover.rs**

实现上述函数。`apply` 时:遍历 `PROXY_TAKEOVER_ENV_KEYS`,把 env 中存在的原值收集进 backup 对象;然后 env.ANTHROPIC_BASE_URL = proxy_base_url,其余模型 key remove。`restore` 时:先把 takeover 写入的 BASE_URL 移除,再把 backup 里的原值写回(backup 里没有的 key 保持移除状态)。中文注释标注安全边界。备份文件路径用 `app_paths`(参考项目其它模块如何取 app data 目录)。

- [ ] **Step 4: 运行测试**

Run: `cd src-tauri && cargo test test_apply_and_restore_takeover_roundtrip` → PASS;`cargo test` 全绿。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/proxy/takeover.rs src-tauri/src/proxy/mod.rs
git commit -m "feat(proxy): takeover 改写/备份/还原 settings.json(纯函数+持久化)

Co-Authored-By: Codex <noreply@openai.com>"
```

---

### Task 3: start/stop 代理时执行 takeover + 启动日志

**Files:**
- Modify: `src-tauri/src/services/proxy_service.rs`(`start_proxy` ~67、`stop_proxy` ~72)
- Modify: `src-tauri/src/proxy/server.rs`(若需暴露 actual_port)

**Interfaces:**
- Consumes: Task 2 的 takeover 函数;`server::start` 返回的实际端口。
- Produces: start_proxy 成功后对 Claude 执行 takeover + `usage::logger::init_logger`;stop_proxy 还原 takeover。

- [ ] **Step 1: 读现状**

读 `proxy_service::start_proxy`/`stop_proxy` 全文,确认 `server::start` 返回结构里有实际端口(server.rs:118 `actual_port`)。确认如何拿 `~/.claude/settings.json` 路径(provider_service 有 `get_claude_settings_path`)。

- [ ] **Step 2: 实现 start 侧 takeover**

start_proxy 在 server 启动成功、拿到 actual_port 后:
1. `usage::logger::init_logger(log_dir)`(log_dir 用 app data 下的 usage 目录,参考 calculator/logger 注释路径约定)。
2. 读 settings.json → `let backup = takeover::apply_takeover_to_settings(&mut settings, &format!("http://127.0.0.1:{actual_port}"))` → 写回 settings.json → `takeover::save_backup(&backup)`。
3. 若 takeover 写文件失败:停掉刚启动的 server 并返回错误(不能 server 起了但 CLI 没指过来)。中文注释。

- [ ] **Step 3: 实现 stop 侧还原**

stop_proxy:`if let Some(backup) = takeover::load_backup()` → 读 settings.json → `restore_takeover_to_settings(&mut settings, &backup)` → 写回 → `clear_backup()`;然后停 server。中文注释。

- [ ] **Step 4: 验证**

Run: `cd src-tauri && cargo test`(全绿;此 task 主要是集成,单测覆盖在 Task 2;此处确保编译+不回归)。
手动推演:start 后 settings.json 的 BASE_URL 指向 127.0.0.1:port 且模型 env 消失;stop 后完全还原。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/proxy_service.rs src-tauri/src/proxy/server.rs
git commit -m "feat(proxy): start/stop 代理时执行 takeover 并初始化用量日志

Co-Authored-By: Codex <noreply@openai.com>"
```

---

### Task 4: proxy_handler 串起完整链(failover+model_mapper+forward+circuit)

**Files:**
- Modify: `src-tauri/src/proxy/handlers.rs`(重写 `proxy_handler`)
- Modify: `src-tauri/src/proxy/provider_router.rs`(`resolve_upstream` 改为基于传入 provider 构建 url+headers,不再自己选 provider)

**Interfaces:**
- Consumes: `failover_switch::get_available_provider(app)`、`model_mapper::apply_model_mapping(body, &provider)`、`circuit_breaker::default_config`、`failover_switch::on_success/on_failure`、`http_client::forward_request`。
- Produces: 完整请求处理链。

- [ ] **Step 1: 重构 provider_router::resolve_upstream**

改签名为 `pub fn build_route(provider: &Provider, request_path: &str) -> Result<RouteResult, ProxyError>`:用传入 provider 的 url/api_key 构建 target_url + auth header(逻辑同现有 resolve_upstream 的 24-47 行,但 provider 由参数传入而非内部 find)。保留 RouteResult 结构。

- [ ] **Step 2: 重写 proxy_handler**

在读完 body、normalize thinking 之后:
```rust
let config = circuit_breaker::default_config();
// 实时选可用 provider(含熔断检查),热更新核心:每个请求都重选
let provider = failover_switch::get_available_provider(AppType::Claude)?;
// 解析请求体为 JSON 做模型映射(失败则原样转发)
let json_body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
let (mapped_json, _orig, outbound_model) = if json_body.is_object() {
    crate::proxy::model_mapper::apply_model_mapping(json_body, &provider)
} else { (json_body, None, None) };
let forward_body = if mapped_json.is_null() { body_bytes.clone() }
    else { serde_json::to_vec(&mapped_json).map(Into::into).unwrap_or(body_bytes.clone()) };
let route = provider_router::build_route(&provider, &request_path)?;
// ... 合并 headers(同现有)...
let started = std::time::Instant::now();
match http_client::forward_request(method, &route.target_url, forward_headers, forward_body).await {
    Ok(resp) => {
        failover_switch::on_success(&provider.id, &config);
        // ... 用量记录见 Task 5,流式转发响应(同现有 73-87)...
    }
    Err(e) => {
        failover_switch::on_failure(AppType::Claude, &provider.id, &config);
        return Err(ProxyError::ForwardFailed(e.to_string()));
    }
}
```
中文注释说明:每个请求重选 provider = 热更新生效点;forward 失败才记 on_failure;流式开始后不切换(本 task 暂不做多 provider 重试循环,失败直接返回——重试留给后续,符合"流式不切"约束)。

- [ ] **Step 3: 编译 + 不回归**

Run: `cd src-tauri && cargo test`(全绿)。代理无法纯单测请求链(需真实上游),此 task 靠编译 + 手动 e2e。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/proxy/handlers.rs src-tauri/src/proxy/provider_router.rs
git commit -m "feat(proxy): proxy_handler 串起 failover+模型映射+熔断完整链

Co-Authored-By: Codex <noreply@openai.com>"
```

---

### Task 5: 接入用量记录

**Files:**
- Modify: `src-tauri/src/proxy/handlers.rs`(在 forward 成功分支记录用量)

**Interfaces:**
- Consumes: `usage::parser::extract_usage(body)`、`extract_model(body, fallback)`、`usage::calculator::calculate_cost(model, in, out)`、`usage::logger::log_event(RequestLogEvent)`。
- `RequestLogEvent` 字段:`id, timestamp(DateTime<Utc>), provider_id, model, input_tokens, output_tokens, total_tokens, cost_usd, latency_ms, status, error`。

- [ ] **Step 1: 实现用量记录**

forward 成功、流式转发响应前/中,累积响应字节(流式需在 stream 结束时取 usage SSE)。最小实现:对**非流式**响应先 buffer 整个 body 取 usage;流式响应在转发完成后从累积 buffer 提取(若实现成本高,流式可仅记 latency+model+status,tokens 置 0 并注释 TODO——但不可静默丢整条记录)。构建 `RequestLogEvent`:
```rust
let (input, output) = usage::parser::extract_usage(&resp_bytes);
let model = usage::parser::extract_model(&resp_bytes, outbound_model.as_deref().unwrap_or("unknown"));
let cost = usage::calculator::calculate_cost(&model, input, output);
usage::logger::log_event(RequestLogEvent {
    id: uuid/timestamp 生成, timestamp: Utc::now(), provider_id: provider.id.clone(),
    model, input_tokens: input, output_tokens: output, total_tokens: input+output,
    cost_usd: cost, latency_ms: started.elapsed().as_millis() as u64, status: status_u16, error: None,
});
```
中文注释。id 生成用项目已有方式(grep 现有 id 生成,如 `provider-{Date}` 或 uuid crate)。

- [ ] **Step 2: 编译 + 不回归**

Run: `cd src-tauri && cargo test` 全绿。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/proxy/handlers.rs
git commit -m "feat(proxy): 代理转发记录用量到日志

Co-Authored-By: Codex <noreply@openai.com>"
```

---

### Task 6: 启动崩溃恢复(takeover 残留还原)

**Files:**
- Modify: `src-tauri/src/lib.rs`(setup 阶段,参考是否已有 restore_proxy_state 钩子)

**Interfaces:**
- Consumes: `takeover::load_backup`/`restore_takeover_to_settings`/`clear_backup`;proxy_config 表的 enabled 状态。

- [ ] **Step 1: 实现恢复逻辑**

app 启动 setup 时:若 `takeover::load_backup()` 有备份 **但代理实际未运行/未配置为启动**,说明上次异常退出留下 settings.json 指向死代理 → 读 settings.json、`restore_takeover_to_settings`、写回、`clear_backup`。中文注释说明这是防止 CLI 永久指向死代理。

- [ ] **Step 2: 编译验证**

Run: `cd src-tauri && cargo test` 全绿。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(proxy): 启动时还原残留 takeover,避免 CLI 指向死代理

Co-Authored-By: Codex <noreply@openai.com>"
```

---

## Self-Review

**Spec coverage:**
- 修残桩 → Task 1 ✓
- takeover 改写/备份/还原 → Task 2 ✓;start/stop 接入 → Task 3 ✓;崩溃恢复 → Task 6 ✓
- proxy_handler 串 failover+model_mapper+circuit → Task 4 ✓
- 用量 → Task 5 ✓
- 故障转移流式边界(流式不切)→ Task 4 注明(本轮失败直接返回,不做流式重试,符合约束)✓
- 仅 Claude takeover → 全程限定 AppType::Claude ✓

**Placeholder scan:** Task 1 Step 1 与 Task 5 id 生成让实现者"读现状确认"——属定位指引(给了 grep 目标),非占位。Task 5 流式 tokens 的 fallback 已明确"不可静默丢整条",非模糊。

**Type consistency:** `apply_takeover_to_settings`/`restore_takeover_to_settings`/`save_backup`/`load_backup`/`clear_backup`、`build_route`、`get_available_provider`/`on_success`/`on_failure`、`apply_model_mapping`、`RequestLogEvent` 字段——前后一致,均与已核实的现有签名匹配。

**已知风险(交终审/手动验证):** 代理请求链无法纯单测(需真实上游),Task 4/5 靠编译 + 手动 e2e;流式 usage 提取是已知难点,允许降级但不丢记录。

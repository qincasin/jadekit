#![allow(dead_code)]
use crate::database::Database;
use crate::models::app_type::AppType;
use crate::models::provider::Provider;
use crate::proxy::takeover;
use crate::services::app_paths;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// 1M 上下文模型后缀（Claude Code 官方机制，匹配前会被剥离）。
pub const ONE_M_CONTEXT_SUFFIX: &str = "[1M]";

static GLOBAL_DB: OnceLock<Arc<Database>> = OnceLock::new();

/// 注册生产数据库句柄，供没有 Tauri `State` 的代理请求链读取 Provider。
pub fn set_global_db(db: Arc<Database>) {
    let _ = GLOBAL_DB.set(db);
}

fn global_db() -> Result<&'static Arc<Database>, String> {
    GLOBAL_DB
        .get()
        .ok_or_else(|| "Database is not initialized".to_string())
}

// ── 官方订阅特殊 Provider ────────────────────────────────────
//
// 中文注释（安全边界 / 状态流转）：
// 官方订阅是一类「不写入 apikey/base_url」的特殊 Provider。激活它时，我们要
// 反向地把此前供应商写进 CLI 配置的字段「清除」掉，让 CLI 找不到第三方 apikey
// / base_url，从而回落到它自带的 OAuth 订阅登录态（对齐 cc-switch 的
// is_official_provider / restore-official 行为）。这些 id 与字段清单都集中为
// 常量，避免在业务代码里散落魔法字符串。

/// 官方订阅特殊 Provider 的固定 id（对齐 cc-switch is_official_provider 思路）。
/// pub：前端内置官方 Provider 时需要复用同一 id，保证前后端一致。
pub const CLAUDE_OFFICIAL_PROVIDER_ID: &str = "__claude_official__";
pub const CODEX_OFFICIAL_PROVIDER_ID: &str = "__codex_official__";

/// 切到 Claude 官方订阅时需从 ~/.claude/settings.json 的 env 移除的供应商字段。
/// 中文注释：与 `sync_to_claude_settings` / `merge_provider_to_env` 写入的键复用
/// 同一份语义（含 Task-1 的 `[1M]` 后缀模型，它们用的就是这几个键），移除后
/// CLI 找不到 apikey/base_url，回落自带 OAuth 订阅登录态。
const CLAUDE_PROVIDER_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_REASONING_MODEL",
];

/// 切到 Codex 官方订阅时需从 ~/.codex/auth.json 移除的供应商字段。
/// 中文注释：`sync_to_codex_config` 往 auth.json 写的就是 OPENAI_API_KEY；
/// 移除它即让 Codex CLI 回落 OAuth 登录态。
const CODEX_AUTH_PROVIDER_KEYS: &[&str] = &["OPENAI_API_KEY"];

/// 切到 Codex 官方订阅时需从 ~/.codex/config.toml 顶层移除的供应商字段。
/// 中文注释：`write_codex_toml_config` 往 config.toml 顶层写 model_provider 与
/// model，并新增 `[model_providers.newapi]` 表（base_url 等）。移除这三项即抹掉
/// 第三方供应商指向，让 Codex 回落官方默认。
const CODEX_CONFIG_PROVIDER_KEYS: &[&str] = &["model_provider", "model", "model_providers"];

/// 判断给定 Provider id 是否为官方订阅特殊 Provider。
pub fn is_official_provider(id: &str) -> bool {
    id == CLAUDE_OFFICIAL_PROVIDER_ID || id == CODEX_OFFICIAL_PROVIDER_ID
}

/// 构造仅用于触发「官方订阅清除分支」的合成 Provider。
/// 中文注释（安全边界）：该 Provider 不入库、不携带 apikey/base_url，只作为
/// switch 路径里的状态载体交给 sync 层，让 sync 层清除第三方供应商字段。
fn build_official_provider(id: &str, app: AppType) -> Provider {
    Provider {
        id: id.to_string(),
        name: match app {
            AppType::Claude => "Claude 官方订阅".to_string(),
            AppType::Codex => "Codex 官方订阅".to_string(),
            _ => "官方订阅".to_string(),
        },
        app_type: app,
        api_key: String::new(),
        url: None,
        default_sonnet_model: None,
        default_opus_model: None,
        default_haiku_model: None,
        default_reasoning_model: None,
        custom_params: None,
        settings_config: None,
        meta: None,
        icon: None,
        in_failover_queue: false,
        description: None,
        tags: None,
        is_active: true,
        created_at: chrono::Utc::now(),
        last_used: Some(chrono::Utc::now()),
        proxy_config: None,
        one_m_context: None,
    }
}

/// 从 Claude env 对象移除所有供应商字段（幂等：移除不存在的键是 no-op，
/// 不触碰其他无关键）。
fn clear_claude_provider_env(env: &mut serde_json::Map<String, serde_json::Value>) {
    for key in CLAUDE_PROVIDER_ENV_KEYS {
        env.remove(*key);
    }
}

// ── 路径函数 ──────────────────────────────────────────────

fn get_data_dir() -> Result<PathBuf, io::Error> {
    app_paths::data_dir()
}

pub(crate) fn get_claude_settings_path() -> Result<PathBuf, io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    Ok(home.join(".claude").join("settings.json"))
}

// ── 数据库读写（v3+）──────────────────────────────────────────────

/// 获取指定应用的物理配置文件原始内容（读取供前端展示）
pub fn get_provider_config_files(app: AppType) -> Result<Vec<(String, String)>, io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;

    // 配置表：每个应用对应的文件列表
    const CLAUDE_FILES: &[(&str, &str)] = &[(".claude/settings.json", "{}")];
    const CODEX_FILES: &[(&str, &str)] = &[(".codex/auth.json", "{}"), (".codex/config.toml", "")];
    const GEMINI_FILES: &[(&str, &str)] = &[(".gemini/.env", "")];

    let file_configs = match app {
        AppType::Claude => CLAUDE_FILES,
        AppType::Codex => CODEX_FILES,
        AppType::Gemini => GEMINI_FILES,
        _ => &[],
    };

    Ok(file_configs
        .iter()
        .map(|(path, default)| {
            let full_path = home.join(path.trim_start_matches('.'));
            let content = read_file_content(&full_path, default);
            (path.to_string(), content)
        })
        .collect())
}

/// 列出指定应用的 providers（从数据库读取）
pub fn list_providers_from_db(db: &Arc<Database>, app: AppType) -> Result<Vec<Provider>, String> {
    let all = db.list_providers()?;
    let mut providers: Vec<Provider> = all.into_iter().filter(|p| p.app_type == app).collect();
    if matches!(app, AppType::Claude | AppType::Codex) && !providers.iter().any(|p| p.is_active) {
        let official_id = match app {
            AppType::Claude => CLAUDE_OFFICIAL_PROVIDER_ID,
            AppType::Codex => CODEX_OFFICIAL_PROVIDER_ID,
            _ => unreachable!(),
        };
        // 官方订阅不入库；DB 中没有活跃自定义 Provider 时，合成 active 官方项供代理路由识别。
        providers.push(build_official_provider(official_id, app));
    }
    Ok(providers)
}

/// 列出所有应用的 providers（从数据库读取）
pub fn list_all_providers_from_db(db: &Arc<Database>) -> Result<Vec<Provider>, String> {
    db.list_providers()
}

/// 获取单个 provider（从数据库读取）
pub fn get_provider_from_db(db: &Arc<Database>, id: &str) -> Result<Provider, String> {
    db.get_provider(id)?
        .ok_or_else(|| format!("Provider {} not found", id))
}

/// 添加 provider（写入数据库）
pub fn add_provider_to_db(db: &Arc<Database>, provider: Provider) -> Result<(), String> {
    db.upsert_provider(&provider)
}

/// 更新 provider（更新数据库并同步 active provider 到应用配置）
pub fn update_provider_in_db(
    db: &Arc<Database>,
    id: &str,
    updated: Provider,
) -> Result<(), String> {
    // 先获取原有记录，保留 is_active 和 created_at
    let existing = db
        .get_provider(id)?
        .ok_or_else(|| format!("Provider {} not found", id))?;

    let mut provider = updated;
    provider.is_active = existing.is_active;
    provider.created_at = existing.created_at;

    db.upsert_provider(&provider)?;

    // 如果是 active provider，立即同步到应用配置
    if provider.is_active {
        sync_provider_to_app_config(&provider).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 删除 provider（从数据库删除）
pub fn delete_provider_from_db(db: &Arc<Database>, id: &str) -> Result<(), String> {
    db.delete_provider(id)?;
    Ok(())
}

/// 切换 provider（更新数据库并同步到应用配置）
pub fn switch_provider_in_db(
    db: &Arc<Database>,
    app: AppType,
    provider_id: &str,
) -> Result<(), String> {
    use chrono::Utc;

    // 1. 获取所有 providers
    let mut all = db.list_providers()?;

    // 2. 同一应用内只有一个活跃
    for p in all.iter_mut() {
        if p.app_type == app {
            p.is_active = p.id == provider_id;
            if p.is_active {
                p.last_used = Some(Utc::now());
            }
        }
    }

    // 3. 批量更新数据库
    for p in all {
        db.upsert_provider(&p)?;
    }

    // 4. 同步到应用配置
    // 官方订阅是合成 Provider（不在 DB），特判：构造空认证的官方 Provider，
    // 走 sync 触发清除分支，让 CLI 回落自带 OAuth 订阅登录态。
    let active = if is_official_provider(provider_id) {
        build_official_provider(provider_id, app)
    } else {
        db.get_provider(provider_id)?
            .ok_or_else(|| format!("Provider {} not found", provider_id))?
    };
    let takeover_active = takeover::load_backup().is_some();
    if should_sync_provider_to_app_config(app, takeover_active) {
        sync_provider_to_app_config(&active).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 移动 provider 位置（更新数据库顺序）
pub fn move_provider_in_db(
    db: &Arc<Database>,
    provider_id: &str,
    target_index: usize,
) -> Result<(), String> {
    let mut all = db.list_providers()?;
    let current = all
        .iter()
        .position(|p| p.id == provider_id)
        .ok_or_else(|| format!("Provider {} not found", provider_id))?;
    if current == target_index {
        return Ok(());
    }
    let provider = all.remove(current);
    let insert_at = target_index.min(all.len());
    all.insert(insert_at, provider);

    // 重新写入所有 providers
    for p in all {
        db.upsert_provider(&p)?;
    }

    Ok(())
}

/// 列出指定应用的 providers（兼容旧版，使用 DB）
pub fn list_providers(app: AppType) -> Result<Vec<Provider>, String> {
    // 代理转发运行在 axum handler 内，没有 Tauri State 注入；这里通过启动时注册的
    // 全局 DB 句柄读取最新 Provider，确保每个请求都能看到前端刚切换的配置。
    list_providers_from_db(global_db()?, app)
}

/// 列出所有应用的 providers（兼容旧版，使用 DB）
pub fn list_all_providers() -> Result<Vec<Provider>, String> {
    // TODO: 此函数保留兼容，新代码请使用 list_all_providers_from_db
    Ok(vec![])
}

/// 获取单个 provider（兼容旧版，使用 DB）
pub fn get_provider(_id: &str) -> Result<Provider, String> {
    // TODO: 此函数保留兼容
    Err(format!("Provider not found"))
}

/// 添加 provider（兼容旧版，使用 DB）
pub fn add_provider(_provider: Provider) -> Result<(), String> {
    // TODO: 此函数保留兼容
    Ok(())
}

/// 更新 provider（兼容旧版，使用 DB）
pub fn update_provider(_id: &str, _updated: Provider) -> Result<(), String> {
    // TODO: 此函数保留兼容
    Ok(())
}

/// 删除 provider（兼容旧版，使用 DB）
pub fn delete_provider(_id: &str) -> Result<(), String> {
    // TODO: 此函数保留兼容
    Ok(())
}

/// 切换 provider（兼容旧版，使用 DB）
pub fn switch_provider(_app: AppType, _provider_id: &str) -> Result<(), String> {
    // TODO: 此函数保留兼容
    Ok(())
}

/// 移动 provider 位置（兼容旧版，使用 DB）
pub fn move_provider(_provider_id: &str, _target_index: usize) -> Result<(), String> {
    // TODO: 此函数保留兼容
    Ok(())
}

// ── 配置预览和同步函数（保持不变）─────────────────────────────────

// ── settingsConfig 中需要映射为 env 变量的字段 ─────────────

/// settingsConfig 中某些字段不是 settings.json 顶层配置，
/// 而是需要映射为 env 环境变量。
/// 本函数从 settings 中提取这些字段，写入 env 并从顶层移除。
fn remap_settings_to_env(settings: &mut serde_json::Value) {
    // 提取所有需要映射到 env 的布尔字段
    let teammates_enabled = settings
        .get("teammatesMode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_traffic = settings
        .get("disableNonessentialTraffic")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_attribution = settings
        .get("disableAttributionHeader")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_installation_checks = settings
        .get("disableInstallationChecks")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let enable_tool_search = settings
        .get("enableToolSearch")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let enable_powershell_tool = settings
        .get("enablePowerShellTool")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_telemetry = settings
        .get("disableTelemetry")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_bug_command = settings
        .get("disableBugCommand")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_autoupdater = settings
        .get("disableAutoupdater")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let disable_error_reporting = settings
        .get("disableErrorReporting")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_output = settings
        .get("maxOutputTokens")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let effort_level = settings
        .get("effortLevel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let ripgrep_mode = settings
        .get("ripgrepMode")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let api_timeout_ms = settings
        .get("apiTimeoutMs")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let bash_default_timeout_ms = settings
        .get("bashDefaultTimeoutMs")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mcp_timeout_ms = settings
        .get("mcpTimeoutMs")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mcp_tool_timeout_ms = settings
        .get("mcpToolTimeoutMs")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let task_max_output_length = settings
        .get("taskMaxOutputLength")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let anthropic_betas = settings
        .get("anthropicBetas")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let anthropic_custom_headers = settings
        .get("anthropicCustomHeaders")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let custom_model_option = settings
        .get("customModelOption")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let custom_model_option_name = settings
        .get("customModelOptionName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let custom_model_option_description = settings
        .get("customModelOptionDescription")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let custom_model_option_capabilities = settings
        .get("customModelOptionCapabilities")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    // 从顶层移除（不属于 settings.json 原生字段）
    if let Some(obj) = settings.as_object_mut() {
        obj.remove("teammatesMode");
        obj.remove("disableNonessentialTraffic");
        obj.remove("disableAttributionHeader");
        obj.remove("disableInstallationChecks");
        obj.remove("enableToolSearch");
        obj.remove("enablePowerShellTool");
        obj.remove("disableTelemetry");
        obj.remove("disableBugCommand");
        obj.remove("disableAutoupdater");
        obj.remove("disableErrorReporting");
        obj.remove("maxOutputTokens");
        obj.remove("effortLevel");
        obj.remove("ripgrepMode");
        obj.remove("apiTimeoutMs");
        obj.remove("bashDefaultTimeoutMs");
        obj.remove("mcpTimeoutMs");
        obj.remove("mcpToolTimeoutMs");
        obj.remove("taskMaxOutputLength");
        obj.remove("anthropicBetas");
        obj.remove("anthropicCustomHeaders");
        obj.remove("customModelOption");
        obj.remove("customModelOptionName");
        obj.remove("customModelOptionDescription");
        obj.remove("customModelOptionCapabilities");
    }

    // 写入 env
    if let Some(env) = settings.get_mut("env").and_then(|e| e.as_object_mut()) {
        // teammatesMode → CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS
        if teammates_enabled {
            env.insert(
                "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS");
        }
        // disableNonessentialTraffic → CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC
        if disable_traffic {
            env.insert(
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC");
        }
        // disableAttributionHeader → CLAUDE_CODE_ATTRIBUTION_HEADER
        if disable_attribution {
            env.insert(
                "CLAUDE_CODE_ATTRIBUTION_HEADER".to_string(),
                serde_json::Value::String("0".to_string()),
            );
        } else {
            env.remove("CLAUDE_CODE_ATTRIBUTION_HEADER");
        }
        // disableInstallationChecks → DISABLE_INSTALLATION_CHECKS
        if disable_installation_checks {
            env.insert(
                "DISABLE_INSTALLATION_CHECKS".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("DISABLE_INSTALLATION_CHECKS");
        }
        // enableToolSearch → ENABLE_TOOL_SEARCH
        if enable_tool_search {
            env.insert(
                "ENABLE_TOOL_SEARCH".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("ENABLE_TOOL_SEARCH");
        }
        // ripgrepMode → USE_BUILTIN_RIPGREP
        match ripgrep_mode.as_str() {
            "builtin" => {
                env.insert(
                    "USE_BUILTIN_RIPGREP".to_string(),
                    serde_json::Value::String("1".to_string()),
                );
            }
            "system" => {
                env.insert(
                    "USE_BUILTIN_RIPGREP".to_string(),
                    serde_json::Value::String("0".to_string()),
                );
            }
            _ => {
                env.remove("USE_BUILTIN_RIPGREP");
            }
        }
        // enablePowerShellTool → CLAUDE_CODE_USE_POWERSHELL_TOOL
        if enable_powershell_tool {
            env.insert(
                "CLAUDE_CODE_USE_POWERSHELL_TOOL".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("CLAUDE_CODE_USE_POWERSHELL_TOOL");
        }
        // disableTelemetry → DISABLE_TELEMETRY
        if disable_telemetry {
            env.insert(
                "DISABLE_TELEMETRY".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("DISABLE_TELEMETRY");
        }
        // disableBugCommand → DISABLE_BUG_COMMAND
        if disable_bug_command {
            env.insert(
                "DISABLE_BUG_COMMAND".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("DISABLE_BUG_COMMAND");
        }
        // disableAutoupdater → DISABLE_AUTOUPDATER
        if disable_autoupdater {
            env.insert(
                "DISABLE_AUTOUPDATER".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("DISABLE_AUTOUPDATER");
        }
        // disableErrorReporting → DISABLE_ERROR_REPORTING
        if disable_error_reporting {
            env.insert(
                "DISABLE_ERROR_REPORTING".to_string(),
                serde_json::Value::String("1".to_string()),
            );
        } else {
            env.remove("DISABLE_ERROR_REPORTING");
        }
        // maxOutputTokens → CLAUDE_CODE_MAX_OUTPUT_TOKENS（用户自定义值）
        if !max_output.is_empty() {
            env.insert(
                "CLAUDE_CODE_MAX_OUTPUT_TOKENS".to_string(),
                serde_json::Value::String(max_output.clone()),
            );
        } else {
            env.remove("CLAUDE_CODE_MAX_OUTPUT_TOKENS");
        }
        // effortLevel → CLAUDE_CODE_EFFORT_LEVEL
        if !effort_level.is_empty() {
            env.insert(
                "CLAUDE_CODE_EFFORT_LEVEL".to_string(),
                serde_json::Value::String(effort_level.clone()),
            );
        } else {
            env.remove("CLAUDE_CODE_EFFORT_LEVEL");
        }
        // apiTimeoutMs → API_TIMEOUT_MS
        if !api_timeout_ms.is_empty() {
            env.insert(
                "API_TIMEOUT_MS".to_string(),
                serde_json::Value::String(api_timeout_ms.clone()),
            );
        } else {
            env.remove("API_TIMEOUT_MS");
        }
        // bashDefaultTimeoutMs → BASH_DEFAULT_TIMEOUT_MS
        if !bash_default_timeout_ms.is_empty() {
            env.insert(
                "BASH_DEFAULT_TIMEOUT_MS".to_string(),
                serde_json::Value::String(bash_default_timeout_ms.clone()),
            );
        } else {
            env.remove("BASH_DEFAULT_TIMEOUT_MS");
        }
        // mcpTimeoutMs → MCP_TIMEOUT
        if !mcp_timeout_ms.is_empty() {
            env.insert(
                "MCP_TIMEOUT".to_string(),
                serde_json::Value::String(mcp_timeout_ms.clone()),
            );
        } else {
            env.remove("MCP_TIMEOUT");
        }
        // mcpToolTimeoutMs → MCP_TOOL_TIMEOUT
        if !mcp_tool_timeout_ms.is_empty() {
            env.insert(
                "MCP_TOOL_TIMEOUT".to_string(),
                serde_json::Value::String(mcp_tool_timeout_ms.clone()),
            );
        } else {
            env.remove("MCP_TOOL_TIMEOUT");
        }
        // taskMaxOutputLength → TASK_MAX_OUTPUT_LENGTH
        if !task_max_output_length.is_empty() {
            env.insert(
                "TASK_MAX_OUTPUT_LENGTH".to_string(),
                serde_json::Value::String(task_max_output_length.clone()),
            );
        } else {
            env.remove("TASK_MAX_OUTPUT_LENGTH");
        }
        // anthropicBetas → ANTHROPIC_BETAS
        if !anthropic_betas.is_empty() {
            env.insert(
                "ANTHROPIC_BETAS".to_string(),
                serde_json::Value::String(anthropic_betas.clone()),
            );
        } else {
            env.remove("ANTHROPIC_BETAS");
        }
        // anthropicCustomHeaders → ANTHROPIC_CUSTOM_HEADERS
        if !anthropic_custom_headers.is_empty() {
            env.insert(
                "ANTHROPIC_CUSTOM_HEADERS".to_string(),
                serde_json::Value::String(anthropic_custom_headers.clone()),
            );
        } else {
            env.remove("ANTHROPIC_CUSTOM_HEADERS");
        }
        // customModelOption* → ANTHROPIC_CUSTOM_MODEL_OPTION*
        if !custom_model_option.is_empty() {
            env.insert(
                "ANTHROPIC_CUSTOM_MODEL_OPTION".to_string(),
                serde_json::Value::String(custom_model_option.clone()),
            );
        } else {
            env.remove("ANTHROPIC_CUSTOM_MODEL_OPTION");
        }
        if !custom_model_option_name.is_empty() {
            env.insert(
                "ANTHROPIC_CUSTOM_MODEL_OPTION_NAME".to_string(),
                serde_json::Value::String(custom_model_option_name.clone()),
            );
        } else {
            env.remove("ANTHROPIC_CUSTOM_MODEL_OPTION_NAME");
        }
        if !custom_model_option_description.is_empty() {
            env.insert(
                "ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION".to_string(),
                serde_json::Value::String(custom_model_option_description.clone()),
            );
        } else {
            env.remove("ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION");
        }
        if !custom_model_option_capabilities.is_empty() {
            env.insert(
                "ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES".to_string(),
                serde_json::Value::String(custom_model_option_capabilities.clone()),
            );
        } else {
            env.remove("ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES");
        }
    }
}

// ── 读取当前 Claude settings.json 中的 checkbox 状态 ──────────

/// 从当前 settings.json 中读取所有 checkbox 对应的配置状态，
/// 用于编辑 Provider 时正确初始化复选框。
pub fn get_claude_settings_state() -> Result<serde_json::Value, io::Error> {
    let settings_path = get_claude_settings_path()?;
    let settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
    } else {
        serde_json::json!({})
    };

    let env = settings.get("env").and_then(|e| e.as_object());

    let mut deprecated_fields = Vec::new();
    if settings.get("hideSignature").is_some() {
        deprecated_fields.push("hideSignature");
    }
    if settings.get("enabledPlugins").is_some() {
        deprecated_fields.push("enabledPlugins");
    }
    if env.and_then(|e| e.get("ANTHROPIC_REASONING_MODEL")).is_some() {
        deprecated_fields.push("env.ANTHROPIC_REASONING_MODEL");
    }

    Ok(serde_json::json!({
        "alwaysThinkingEnabled": settings.get("alwaysThinkingEnabled")
            .and_then(|v| v.as_bool()).unwrap_or(false),
        "teammatesMode": env.and_then(|e| e.get("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"))
            .and_then(|v| v.as_str()) == Some("1"),
        "disableNonessentialTraffic": env.and_then(|e| e.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"))
            .and_then(|v| v.as_str()) == Some("1"),
        "disableAttributionHeader": env.and_then(|e| e.get("CLAUDE_CODE_ATTRIBUTION_HEADER"))
            .and_then(|v| v.as_str()) == Some("0"),
        "disableInstallationChecks": env.and_then(|e| e.get("DISABLE_INSTALLATION_CHECKS"))
            .and_then(|v| v.as_str()) == Some("1"),
        "enableToolSearch": env.and_then(|e| e.get("ENABLE_TOOL_SEARCH"))
            .and_then(|v| v.as_str()) == Some("1"),
        "ripgrepMode": match env.and_then(|e| e.get("USE_BUILTIN_RIPGREP")).and_then(|v| v.as_str()) {
            Some("0") => "system",
            Some(_) => "builtin",
            None => "",
        },
        "enablePowerShellTool": env.and_then(|e| e.get("CLAUDE_CODE_USE_POWERSHELL_TOOL"))
            .and_then(|v| v.as_str()) == Some("1"),
        "disableTelemetry": env.and_then(|e| e.get("DISABLE_TELEMETRY"))
            .and_then(|v| v.as_str()) == Some("1"),
        "disableBugCommand": env.and_then(|e| e.get("DISABLE_BUG_COMMAND"))
            .and_then(|v| v.as_str()) == Some("1"),
        "disableAutoupdater": env.and_then(|e| e.get("DISABLE_AUTOUPDATER"))
            .and_then(|v| v.as_str()) == Some("1"),
        "disableErrorReporting": env.and_then(|e| e.get("DISABLE_ERROR_REPORTING"))
            .and_then(|v| v.as_str()) == Some("1"),
        "maxOutputTokens": env.and_then(|e| e.get("CLAUDE_CODE_MAX_OUTPUT_TOKENS"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "effortLevel": env.and_then(|e| e.get("CLAUDE_CODE_EFFORT_LEVEL"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "apiTimeoutMs": env.and_then(|e| e.get("API_TIMEOUT_MS"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "bashDefaultTimeoutMs": env.and_then(|e| e.get("BASH_DEFAULT_TIMEOUT_MS"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "mcpTimeoutMs": env.and_then(|e| e.get("MCP_TIMEOUT"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "mcpToolTimeoutMs": env.and_then(|e| e.get("MCP_TOOL_TIMEOUT"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "taskMaxOutputLength": env.and_then(|e| e.get("TASK_MAX_OUTPUT_LENGTH"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "anthropicBetas": env.and_then(|e| e.get("ANTHROPIC_BETAS"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "anthropicCustomHeaders": env.and_then(|e| e.get("ANTHROPIC_CUSTOM_HEADERS"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "customModelOption": env.and_then(|e| e.get("ANTHROPIC_CUSTOM_MODEL_OPTION"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "customModelOptionName": env.and_then(|e| e.get("ANTHROPIC_CUSTOM_MODEL_OPTION_NAME"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "customModelOptionDescription": env.and_then(|e| e.get("ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "customModelOptionCapabilities": env.and_then(|e| e.get("ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES"))
            .and_then(|v| v.as_str()).unwrap_or(""),
        "deprecatedFields": deprecated_fields,
    }))
}

// ── 配置预览辅助函数 ──────────────────────────────────────────────

/// 读取文件内容，失败时返回默认值
fn read_file_content(path: &std::path::Path, default: &str) -> String {
    if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| default.to_string())
    } else {
        default.to_string()
    }
}

/// 构建 JSON 文件预览（带基线）
fn build_json_preview(
    path: &std::path::Path,
    preview: &serde_json::Value,
) -> Result<(String, String, String), io::Error> {
    let baseline: serde_json::Value = if path.exists() {
        let content = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_string());
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    Ok((
        path.to_string_lossy().to_string(),
        serde_json::to_string_pretty(preview)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?,
        serde_json::to_string_pretty(&baseline)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?,
    ))
}

/// 构建 TOML 配置预览（带基线）
fn build_toml_preview(
    path: &std::path::Path,
    build_doc: impl FnOnce(&toml::Value) -> toml::Value,
) -> Result<(String, String, String), io::Error> {
    let existing = read_file_content(path, "");

    let baseline: toml::Value = if existing.is_empty() {
        toml::Value::Table(toml::Table::new())
    } else {
        toml::from_str(&existing)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
    };

    let baseline_str = if existing.is_empty() {
        String::new()
    } else {
        toml::to_string_pretty(&baseline)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?
    };

    let doc = build_doc(&baseline);
    let toml_str = toml::to_string_pretty(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    Ok((path.to_string_lossy().to_string(), toml_str, baseline_str))
}

/// 构建 env 文件预览（带基线）
fn build_env_preview(
    path: &std::path::Path,
    build_lines: impl FnOnce() -> Vec<String>,
) -> Result<(String, String, String), io::Error> {
    let baseline = read_file_content(path, "");
    let content = build_lines().join("\n");

    Ok((path.to_string_lossy().to_string(), content, baseline))
}

// ── 配置同步辅助函数 ──────────────────────────────────────────────

/// 合并 provider 的 API Key 和可选字段到 env 对象
/// 启用 1M 且模型非空时给模型值拼 `[1M]` 后缀，否则原样返回。
/// 中文注释：空模型不拼，避免写出孤立的 `[1M]` 触发上游报错。
fn model_with_1m(model: &Option<String>, enabled: bool) -> Option<String> {
    match model {
        Some(m) if enabled && !m.trim().is_empty() => Some(format!("{m}{ONE_M_CONTEXT_SUFFIX}")),
        other => other.clone(),
    }
}

fn merge_provider_to_env(
    env: &mut serde_json::Map<String, serde_json::Value>,
    api_key: &str,
    optional_fields: &[(&str, &Option<String>)],
) {
    env.insert(
        "ANTHROPIC_AUTH_TOKEN".to_string(),
        serde_json::Value::String(api_key.to_string()),
    );

    for (key, value) in optional_fields {
        match value {
            Some(v) => env.insert(key.to_string(), serde_json::Value::String(v.clone())),
            None => env.remove(*key),
        };
    }
}

/// 写入 Codex 的 TOML 配置
fn write_codex_toml_config(
    config_path: &std::path::Path,
    base_url: &str,
    model: &str,
) -> Result<(), io::Error> {
    let existing = read_file_content(config_path, "");

    let mut doc: toml::Value = if existing.is_empty() {
        toml::Value::Table(toml::Table::new())
    } else {
        toml::from_str(&existing)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?
    };

    if let toml::Value::Table(ref mut t) = doc {
        t.insert(
            "model_provider".into(),
            toml::Value::String("newapi".into()),
        );
        t.insert("model".into(), toml::Value::String(model.into()));

        let mp = t
            .entry("model_providers")
            .or_insert(toml::Value::Table(toml::Table::new()));
        if let toml::Value::Table(ref mut mp_table) = mp {
            let newapi = mp_table
                .entry("newapi")
                .or_insert(toml::Value::Table(toml::Table::new()));
            if let toml::Value::Table(ref mut newapi_table) = newapi {
                newapi_table.insert("base_url".into(), toml::Value::String(base_url.to_string()));
                newapi_table
                    .entry("name")
                    .or_insert(toml::Value::String("Custom".into()));
                newapi_table
                    .entry("wire_api")
                    .or_insert(toml::Value::String("responses".into()));
                newapi_table
                    .entry("requires_openai_auth")
                    .or_insert(toml::Value::Boolean(true));
            }
        }
    }

    let toml_str = toml::to_string_pretty(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    fs::write(config_path, toml_str.as_bytes())
}

/// 写入 Gemini 的 env 配置
fn write_gemini_env(
    env_path: &std::path::Path,
    url: Option<&str>,
    api_key: &str,
    model: Option<&str>,
) -> Result<(), io::Error> {
    let mut env_lines = Vec::new();

    if let Some(u) = url {
        let trimmed = u.trim();
        if !trimmed.is_empty() {
            env_lines.push(format!("GOOGLE_GEMINI_BASE_URL={}", trimmed));
        }
    }
    if !api_key.is_empty() {
        env_lines.push(format!("GEMINI_API_KEY={}", api_key.trim()));
    }
    if let Some(m) = model {
        let trimmed = m.trim();
        if !trimmed.is_empty() {
            env_lines.push(format!("GEMINI_MODEL={}", trimmed));
        }
    }

    fs::write(env_path, env_lines.join("\n").as_bytes())
}

/// 预览 provider 切换后的完整配置文件内容（不写入磁盘）
/// 返回 Vec<(文件标题，预览内容，基线内容)>，基线是同一序列化器处理的原始文件，确保 diff 只反映真实差异
pub fn preview_provider_sync(
    provider: &Provider,
) -> Result<Vec<(String, String, String)>, io::Error> {
    match provider.app_type {
        AppType::Claude => preview_claude_settings(provider),
        AppType::Codex => preview_codex_config(provider),
        AppType::Gemini => preview_gemini_config(provider),
        _ => preview_generic_settings(provider),
    }
}

/// 预览 Claude settings.json 合并结果
fn preview_claude_settings(
    provider: &Provider,
) -> Result<Vec<(String, String, String)>, io::Error> {
    let settings_path = get_claude_settings_path()?;
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
    } else {
        serde_json::json!({})
    };

    let baseline = serde_json::to_string_pretty(&settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    // 合并 settingsConfig 顶层字段
    if let Some(ref sc) = provider.settings_config {
        if let Some(obj) = sc.as_object() {
            for (k, v) in obj {
                settings[k] = v.clone();
            }
        }
    }

    // 确保 env 对象存在
    if settings.get("env").is_none() {
        settings["env"] = serde_json::json!({});
    }
    let env = settings["env"]
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "env is not an object"))?;

    // 合并 provider 配置：按 1M 声明给各模型角色拼 `[1M]` 后缀
    let one_m = provider.one_m_context.clone().unwrap_or_default();
    let sonnet = model_with_1m(&provider.default_sonnet_model, one_m.sonnet);
    let opus = model_with_1m(&provider.default_opus_model, one_m.opus);
    let haiku = model_with_1m(&provider.default_haiku_model, one_m.haiku);
    let reasoning = model_with_1m(&provider.default_reasoning_model, one_m.reasoning);
    let optional_fields = [
        ("ANTHROPIC_BASE_URL", &provider.url),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", &sonnet),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", &opus),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", &haiku),
        ("ANTHROPIC_REASONING_MODEL", &reasoning),
    ];
    merge_provider_to_env(env, &provider.api_key, &optional_fields);

    // 合并自定义参数
    if let Some(ref params) = provider.custom_params {
        for (key, value) in params {
            env.insert(key.clone(), value.clone());
        }
    }

    // 映射特殊字段到 env
    remap_settings_to_env(&mut settings);

    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(vec![(
        ".claude/settings.json".to_string(),
        content,
        baseline,
    )])
}

/// 预览 Codex 配置合并结果
fn preview_codex_config(provider: &Provider) -> Result<Vec<(String, String, String)>, io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let codex_dir = home.join(".codex");

    // auth.json
    let auth_path = codex_dir.join("auth.json");
    let auth_preview = serde_json::json!({ "OPENAI_API_KEY": provider.api_key });
    let (auth_title, auth_content, auth_baseline) = build_json_preview(&auth_path, &auth_preview)?;

    let mut files = vec![(auth_title, auth_content, auth_baseline)];

    // config.toml
    if let Some(ref url) = provider.url {
        let base_url = normalize_codex_base_url(url);
        let model = provider
            .default_sonnet_model
            .as_deref()
            .unwrap_or("o4-mini");
        let config_path = codex_dir.join("config.toml");

        let (config_title, config_content, config_baseline) =
            build_toml_preview(&config_path, |baseline_doc| {
                let mut doc = baseline_doc.clone();
                if let toml::Value::Table(ref mut t) = doc {
                    t.insert(
                        "model_provider".into(),
                        toml::Value::String("newapi".into()),
                    );
                    t.insert("model".into(), toml::Value::String(model.into()));

                    let mp = t
                        .entry("model_providers")
                        .or_insert(toml::Value::Table(toml::Table::new()));
                    if let toml::Value::Table(ref mut mp_table) = mp {
                        let newapi = mp_table
                            .entry("newapi")
                            .or_insert(toml::Value::Table(toml::Table::new()));
                        if let toml::Value::Table(ref mut newapi_table) = newapi {
                            newapi_table.insert("base_url".into(), toml::Value::String(base_url));
                            newapi_table
                                .entry("name")
                                .or_insert(toml::Value::String("Custom".into()));
                            newapi_table
                                .entry("wire_api")
                                .or_insert(toml::Value::String("responses".into()));
                            newapi_table
                                .entry("requires_openai_auth")
                                .or_insert(toml::Value::Boolean(true));
                        }
                    }
                }
                doc
            })?;
        files.push((config_title, config_content, config_baseline));
    }

    Ok(files)
}

/// 预览 Gemini 配置合并结果
fn preview_gemini_config(provider: &Provider) -> Result<Vec<(String, String, String)>, io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let env_path = home.join(".gemini").join(".env");

    let url = provider.url.as_ref().map(|s| s.as_str());
    let model = provider.default_sonnet_model.as_ref().map(|s| s.as_str());

    build_env_preview(&env_path, || {
        let mut lines = Vec::new();
        if let Some(u) = url {
            if !u.trim().is_empty() {
                lines.push(format!("GOOGLE_GEMINI_BASE_URL={}", u.trim()));
            }
        }
        if !provider.api_key.is_empty() {
            lines.push(format!("GEMINI_API_KEY={}", provider.api_key.trim()));
        }
        if let Some(m) = model {
            if !m.trim().is_empty() {
                lines.push(format!("GEMINI_MODEL={}", m.trim()));
            }
        }
        lines
    })
    .map(|(title, content, baseline)| vec![(title, content, baseline)])
}

/// 预览通用应用配置合并结果
fn preview_generic_settings(
    provider: &Provider,
) -> Result<Vec<(String, String, String)>, io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let config_dir = home.join(format!(".{}", provider.app_type.as_str()));
    let config_path = config_dir.join(provider.app_type.config_file_name());

    let mut settings: serde_json::Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let baseline = serde_json::to_string_pretty(&settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    if settings.get("env").is_none() {
        settings["env"] = serde_json::json!({});
    }

    let prefix = provider.app_type.env_prefix();
    let env = settings["env"]
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "env is not an object"))?;

    env.insert(
        format!("{}_AUTH_TOKEN", prefix),
        serde_json::Value::String(provider.api_key.clone()),
    );

    if let Some(ref url) = provider.url {
        env.insert(
            format!("{}_BASE_URL", prefix),
            serde_json::Value::String(url.clone()),
        );
    } else {
        env.remove(&format!("{}_BASE_URL", prefix));
    }

    let title = format!(
        ".{}/{}",
        provider.app_type.as_str(),
        provider.app_type.config_file_name()
    );
    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    Ok(vec![(title, content, baseline)])
}

// ── 配置同步 ──────────────────────────────────────────────

/// 将 provider 配置同步到对应应用的配置文件
fn sync_provider_to_app_config(provider: &Provider) -> Result<(), io::Error> {
    match provider.app_type {
        AppType::Claude => sync_to_claude_settings(provider),
        AppType::Codex => sync_to_codex_config(provider),
        AppType::Gemini => sync_to_gemini_config(provider),
        _ => sync_to_generic_settings(provider),
    }
}

fn should_sync_provider_to_app_config(app: AppType, takeover_active: bool) -> bool {
    // 代理接管 Claude 时，settings.json 必须保持指向本地代理；Provider 热更新由
    // proxy handler 每请求读取 DB 生效，不能再让切换 Provider 覆盖 BASE_URL。
    !(app == AppType::Claude && takeover_active)
}

/// 同步到 Claude ~/.claude/settings.json
fn sync_to_claude_settings(provider: &Provider) -> Result<(), io::Error> {
    let settings_path = get_claude_settings_path()?;
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
    } else {
        serde_json::json!({})
    };

    // 官方订阅：清除供应商 env 字段，让 CLI 回落 OAuth 订阅登录态后直接写回。
    // 中文注释（状态流转）：在合并任何 provider 配置之前提前分流，避免又把
    // apikey/base_url 写回去。
    if is_official_provider(&provider.id) {
        if settings.get("env").is_none() {
            settings["env"] = serde_json::json!({});
        }
        if let Some(env) = settings["env"].as_object_mut() {
            clear_claude_provider_env(env);
        }
        return crate::services::storage::json_store::write_json(&settings_path, &settings);
    }

    // 合并 settingsConfig 顶层字段
    if let Some(ref sc) = provider.settings_config {
        if let Some(obj) = sc.as_object() {
            for (k, v) in obj {
                settings[k] = v.clone();
            }
        }
    }

    // 确保 env 对象存在
    if settings.get("env").is_none() {
        settings["env"] = serde_json::json!({});
    }
    let env = settings["env"]
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "env is not an object"))?;

    // 合并 provider 配置：按 1M 声明给各模型角色拼 `[1M]` 后缀
    let one_m = provider.one_m_context.clone().unwrap_or_default();
    let sonnet = model_with_1m(&provider.default_sonnet_model, one_m.sonnet);
    let opus = model_with_1m(&provider.default_opus_model, one_m.opus);
    let haiku = model_with_1m(&provider.default_haiku_model, one_m.haiku);
    let reasoning = model_with_1m(&provider.default_reasoning_model, one_m.reasoning);
    let optional_fields = [
        ("ANTHROPIC_BASE_URL", &provider.url),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", &sonnet),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", &opus),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", &haiku),
        ("ANTHROPIC_REASONING_MODEL", &reasoning),
    ];
    merge_provider_to_env(env, &provider.api_key, &optional_fields);

    // 合并自定义参数
    if let Some(ref params) = provider.custom_params {
        for (key, value) in params {
            env.insert(key.clone(), value.clone());
        }
    }

    // 映射特殊字段到 env
    remap_settings_to_env(&mut settings);

    crate::services::storage::json_store::write_json(&settings_path, &settings)
}

/// 通用应用配置同步（非 Claude 应用）
fn sync_to_generic_settings(provider: &Provider) -> Result<(), io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let config_dir = home.join(format!(".{}", provider.app_type.as_str()));
    fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join(provider.app_type.config_file_name());

    let mut settings: serde_json::Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if settings.get("env").is_none() {
        settings["env"] = serde_json::json!({});
    }

    let prefix = provider.app_type.env_prefix();
    let env = settings["env"]
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "env is not an object"))?;

    env.insert(
        format!("{}_AUTH_TOKEN", prefix),
        serde_json::Value::String(provider.api_key.clone()),
    );

    if let Some(ref url) = provider.url {
        env.insert(
            format!("{}_BASE_URL", prefix),
            serde_json::Value::String(url.clone()),
        );
    } else {
        env.remove(&format!("{}_BASE_URL", prefix));
    }

    crate::services::storage::json_store::write_json(&config_path, &settings)
}

fn normalize_codex_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// 从 Codex config.toml 顶层移除供应商字段（幂等，不触碰其他键）。
/// 中文注释：保留 doc 中用户其他自定义配置，只抹掉指向第三方供应商的字段。
fn clear_codex_provider_config(config_path: &std::path::Path) -> Result<(), io::Error> {
    if !config_path.exists() {
        return Ok(());
    }
    let existing = read_file_content(config_path, "");
    if existing.is_empty() {
        return Ok(());
    }
    let mut doc: toml::Value = toml::from_str(&existing)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    if let toml::Value::Table(ref mut t) = doc {
        for key in CODEX_CONFIG_PROVIDER_KEYS {
            t.remove(*key);
        }
    }
    let toml_str = toml::to_string_pretty(&doc)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    fs::write(config_path, toml_str.as_bytes())
}

fn sync_to_codex_config(provider: &Provider) -> Result<(), io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir)?;

    // 官方订阅：清除供应商写入的 apikey/base_url，让 Codex CLI 回落 OAuth 登录态。
    // 中文注释（状态流转）：与 Claude 分支同构，在写入任何 provider 配置前分流。
    if is_official_provider(&provider.id) {
        // 1) 清 auth.json 里的 OPENAI_API_KEY（保留文件中其他键）。
        let auth_path = codex_dir.join("auth.json");
        if auth_path.exists() {
            let content = fs::read_to_string(&auth_path)?;
            let mut auth: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            if let Some(obj) = auth.as_object_mut() {
                for key in CODEX_AUTH_PROVIDER_KEYS {
                    obj.remove(*key);
                }
            }
            crate::services::storage::json_store::write_json(&auth_path, &auth)?;
        }
        // 2) 清 config.toml 顶层的供应商字段。
        clear_codex_provider_config(&codex_dir.join("config.toml"))?;
        return Ok(());
    }

    // auth.json
    let auth_path = codex_dir.join("auth.json");
    let auth = serde_json::json!({ "OPENAI_API_KEY": provider.api_key });
    crate::services::storage::json_store::write_json(&auth_path, &auth)?;

    // config.toml
    if let Some(ref url) = provider.url {
        let base_url = normalize_codex_base_url(url);
        let model = provider
            .default_sonnet_model
            .as_deref()
            .unwrap_or("o4-mini");
        let config_path = codex_dir.join("config.toml");
        write_codex_toml_config(&config_path, &base_url, model)?;
    }

    Ok(())
}

fn sync_to_gemini_config(provider: &Provider) -> Result<(), io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir)?;

    // .env
    let env_path = gemini_dir.join(".env");
    let url = provider.url.as_ref().map(|s| s.as_str());
    let model = provider.default_sonnet_model.as_ref().map(|s| s.as_str());
    write_gemini_env(&env_path, url, &provider.api_key, model)?;

    // settings.json
    let settings_path = gemini_dir.join("settings.json");
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    settings["security"]["auth"]["selectedType"] = serde_json::json!("gemini-api-key");
    crate::services::storage::json_store::write_json(&settings_path, &settings)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_provider(settings_config: serde_json::Value) -> Provider {
        Provider {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            app_type: AppType::Claude,
            api_key: "test-api-key".to_string(),
            url: Some("https://api.example.com".to_string()),
            default_sonnet_model: Some("claude-sonnet-test".to_string()),
            default_opus_model: None,
            default_haiku_model: None,
            default_reasoning_model: None,
            custom_params: None,
            settings_config: Some(settings_config),
            meta: None,
            icon: None,
            in_failover_queue: false,
            description: None,
            tags: None,
            is_active: false,
            created_at: Utc::now(),
            last_used: None,
            proxy_config: None,
            one_m_context: None,
        }
    }

    fn provider_for_app(id: &str, app_type: AppType) -> Provider {
        Provider {
            id: id.to_string(),
            name: id.to_string(),
            app_type,
            api_key: format!("sk-{id}"),
            url: Some("https://api.example.com".to_string()),
            default_sonnet_model: Some("claude-sonnet-test".to_string()),
            default_opus_model: None,
            default_haiku_model: None,
            default_reasoning_model: None,
            custom_params: None,
            settings_config: None,
            meta: None,
            icon: None,
            in_failover_queue: false,
            description: None,
            tags: None,
            is_active: app_type == AppType::Claude,
            created_at: Utc::now(),
            last_used: None,
            proxy_config: None,
            one_m_context: None,
        }
    }

    #[test]
    fn test_list_providers_returns_app_filtered() {
        let db = Arc::new(Database::in_memory().expect("init in-memory db"));
        db.upsert_provider(&provider_for_app("claude-a", AppType::Claude))
            .expect("insert claude-a");
        db.upsert_provider(&provider_for_app("claude-b", AppType::Claude))
            .expect("insert claude-b");
        db.upsert_provider(&provider_for_app("codex-a", AppType::Codex))
            .expect("insert codex-a");

        set_global_db(db);

        let providers = list_providers(AppType::Claude).expect("list claude providers");

        assert_eq!(providers.len(), 2);
        assert!(providers.iter().all(|p| p.app_type == AppType::Claude));
    }

    #[test]
    fn test_list_providers_includes_official_when_no_custom_active() {
        let db = Arc::new(Database::in_memory().expect("init in-memory db"));
        let mut provider = provider_for_app("claude-a", AppType::Claude);
        provider.is_active = false;
        db.upsert_provider(&provider)
            .expect("insert claude-a");

        let providers = list_providers_from_db(&db, AppType::Claude).expect("list providers");

        assert!(providers
            .iter()
            .any(|p| p.id == CLAUDE_OFFICIAL_PROVIDER_ID && p.is_active));
    }

    #[test]
    fn test_official_provider_clears_claude_env() {
        use serde_json::json;
        let mut env = json!({
            "ANTHROPIC_AUTH_TOKEN": "sk-x",
            "ANTHROPIC_BASE_URL": "https://x",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-4.6[1M]",
            "KEEP_ME": "yes"
        });
        clear_claude_provider_env(env.as_object_mut().unwrap());
        assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none());
        assert!(env.get("ANTHROPIC_BASE_URL").is_none());
        assert!(env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none());
        // 无关字段保留
        assert_eq!(env.get("KEEP_ME").and_then(|v| v.as_str()), Some("yes"));
        // 幂等：再清一次不报错
        clear_claude_provider_env(env.as_object_mut().unwrap());
    }

    #[test]
    fn test_is_official_provider() {
        assert!(is_official_provider(CLAUDE_OFFICIAL_PROVIDER_ID));
        assert!(is_official_provider(CODEX_OFFICIAL_PROVIDER_ID));
        assert!(!is_official_provider("user-123"));
    }

    #[test]
    fn test_build_official_provider_uses_empty_credentials() {
        let provider = build_official_provider(CLAUDE_OFFICIAL_PROVIDER_ID, AppType::Claude);

        assert!(is_official_provider(&provider.id));
        assert_eq!(provider.app_type, AppType::Claude);
        assert!(provider.api_key.is_empty());
        assert!(provider.url.is_none());
        assert!(provider.is_active);
    }

    #[test]
    fn test_model_with_1m_appends_suffix() {
        // 启用且模型非空 → 拼后缀
        assert_eq!(
            model_with_1m(&Some("glm-4.6".to_string()), true),
            Some("glm-4.6[1M]".to_string())
        );
        // 启用但模型为空 → 不拼（避免孤立 [1M]）
        assert_eq!(model_with_1m(&None, true), None);
        assert_eq!(model_with_1m(&Some("".to_string()), true), Some("".to_string()));
        // 未启用 → 原样
        assert_eq!(
            model_with_1m(&Some("glm-4.6".to_string()), false),
            Some("glm-4.6".to_string())
        );
    }

    #[test]
    fn takeover_active_skips_claude_settings_sync() {
        assert!(!should_sync_provider_to_app_config(AppType::Claude, true));
        assert!(should_sync_provider_to_app_config(AppType::Claude, false));
        assert!(should_sync_provider_to_app_config(AppType::Codex, true));
    }

    #[test]
    fn remap_claude_advanced_controls_to_env() {
        let mut settings = serde_json::json!({
            "env": {
                "ENABLE_TOOL_SEARCH": "1",
                "USE_BUILTIN_RIPGREP": "stale"
            },
            "enableToolSearch": true,
            "enablePowerShellTool": true,
            "ripgrepMode": "builtin",
            "effortLevel": "xhigh",
            "maxOutputTokens": "100000",
            "apiTimeoutMs": "1200000",
            "bashDefaultTimeoutMs": "300000",
            "mcpTimeoutMs": "30000",
            "mcpToolTimeoutMs": "100000",
            "taskMaxOutputLength": "32000",
            "anthropicBetas": "beta-a,beta-b",
            "anthropicCustomHeaders": "X-Test: 1",
            "customModelOption": "provider/model",
            "customModelOptionName": "Provider Model",
            "customModelOptionDescription": "Manual model",
            "customModelOptionCapabilities": "thinking,vision"
        });

        remap_settings_to_env(&mut settings);
        let env = settings.get("env").and_then(|v| v.as_object()).unwrap();

        assert_eq!(env.get("ENABLE_TOOL_SEARCH").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(env.get("CLAUDE_CODE_USE_POWERSHELL_TOOL").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(env.get("USE_BUILTIN_RIPGREP").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(env.get("CLAUDE_CODE_EFFORT_LEVEL").and_then(|v| v.as_str()), Some("xhigh"));
        assert_eq!(env.get("CLAUDE_CODE_MAX_OUTPUT_TOKENS").and_then(|v| v.as_str()), Some("100000"));
        assert_eq!(env.get("API_TIMEOUT_MS").and_then(|v| v.as_str()), Some("1200000"));
        assert_eq!(env.get("BASH_DEFAULT_TIMEOUT_MS").and_then(|v| v.as_str()), Some("300000"));
        assert_eq!(env.get("MCP_TIMEOUT").and_then(|v| v.as_str()), Some("30000"));
        assert_eq!(env.get("MCP_TOOL_TIMEOUT").and_then(|v| v.as_str()), Some("100000"));
        assert_eq!(env.get("TASK_MAX_OUTPUT_LENGTH").and_then(|v| v.as_str()), Some("32000"));
        assert_eq!(env.get("ANTHROPIC_BETAS").and_then(|v| v.as_str()), Some("beta-a,beta-b"));
        assert_eq!(env.get("ANTHROPIC_CUSTOM_HEADERS").and_then(|v| v.as_str()), Some("X-Test: 1"));
        assert_eq!(env.get("ANTHROPIC_CUSTOM_MODEL_OPTION").and_then(|v| v.as_str()), Some("provider/model"));
        assert_eq!(env.get("ANTHROPIC_CUSTOM_MODEL_OPTION_NAME").and_then(|v| v.as_str()), Some("Provider Model"));
        assert_eq!(env.get("ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION").and_then(|v| v.as_str()), Some("Manual model"));
        assert_eq!(env.get("ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES").and_then(|v| v.as_str()), Some("thinking,vision"));
        assert!(settings.get("ripgrepMode").is_none());
        assert!(settings.get("effortLevel").is_none());
    }

    #[test]
    fn local_claude_settings_preview_preserves_existing_env_and_adds_new_controls() {
        let settings_path = get_claude_settings_path().expect("home directory should exist");
        if !settings_path.exists() {
            return;
        }

        let original = fs::read_to_string(&settings_path).expect("local Claude settings should be readable");
        let original_json: serde_json::Value =
            serde_json::from_str(&original).expect("local Claude settings should be valid JSON");
        let original_env = original_json
            .get("env")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut settings_config = get_claude_settings_state()
            .expect("local Claude settings state should be readable");
        if let Some(obj) = settings_config.as_object_mut() {
            obj.insert("enableToolSearch".to_string(), serde_json::json!(true));
            obj.insert("ripgrepMode".to_string(), serde_json::json!("system"));
            obj.insert("effortLevel".to_string(), serde_json::json!("max"));
            obj.insert("apiTimeoutMs".to_string(), serde_json::json!("1200000"));
        }

        let files = preview_provider_sync(&test_provider(settings_config))
        .expect("preview should read and merge local Claude settings");

        let (_, content, _) = files
            .iter()
            .find(|(title, _, _)| title == ".claude/settings.json")
            .expect("Claude preview should include settings.json");
        let preview: serde_json::Value =
            serde_json::from_str(content).expect("preview should be valid JSON");
        let env = preview.get("env").and_then(|v| v.as_object()).unwrap();

        for key in original_env.keys() {
            if !matches!(
                key.as_str(),
                "ANTHROPIC_AUTH_TOKEN"
                    | "ANTHROPIC_BASE_URL"
                    | "ANTHROPIC_DEFAULT_SONNET_MODEL"
                    | "ANTHROPIC_DEFAULT_OPUS_MODEL"
                    | "ANTHROPIC_DEFAULT_HAIKU_MODEL"
                    | "ANTHROPIC_REASONING_MODEL"
                    | "ENABLE_TOOL_SEARCH"
                    | "USE_BUILTIN_RIPGREP"
                    | "CLAUDE_CODE_EFFORT_LEVEL"
                    | "API_TIMEOUT_MS"
            ) {
                assert!(env.contains_key(key), "existing env key {key} should be preserved");
            }
        }

        assert_eq!(env.get("ENABLE_TOOL_SEARCH").and_then(|v| v.as_str()), Some("1"));
        assert_eq!(env.get("USE_BUILTIN_RIPGREP").and_then(|v| v.as_str()), Some("0"));
        assert_eq!(env.get("CLAUDE_CODE_EFFORT_LEVEL").and_then(|v| v.as_str()), Some("max"));
        assert_eq!(env.get("API_TIMEOUT_MS").and_then(|v| v.as_str()), Some("1200000"));
    }
}

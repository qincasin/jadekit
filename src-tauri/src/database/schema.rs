use rusqlite::Connection;

/// providers 表新增列的幂等迁移语句（对齐已有 settings_config/proxy_config 的 JSON TEXT 存储方式）。
/// 旧库通过 `CREATE TABLE IF NOT EXISTS` 不会自动补列，需用 ALTER TABLE 补齐；
/// 重复执行会触发 "duplicate column" 错误，按幂等处理忽略。
const PROVIDERS_ADD_COLUMN_MIGRATIONS: &[&str] =
    &["ALTER TABLE providers ADD COLUMN one_m_context TEXT"];

pub fn create_tables(conn: &Connection) -> Result<(), String> {
    create_base_tables(conn)?;
    migrate_provider_columns(conn)?;
    Ok(())
}

/// 对旧库补齐 providers 表后加的列；忽略 "duplicate column" 表示列已存在。
fn migrate_provider_columns(conn: &Connection) -> Result<(), String> {
    for sql in PROVIDERS_ADD_COLUMN_MIGRATIONS {
        if let Err(e) = conn.execute(sql, []) {
            let msg = e.to_string();
            // 列已存在属于正常的幂等情况，其余错误才向上抛
            if !msg.contains("duplicate column") {
                return Err(format!("Failed to migrate provider columns: {msg}"));
            }
        }
    }
    Ok(())
}

fn create_base_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS mcp_servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            server_config TEXT NOT NULL,
            description TEXT,
            tags TEXT NOT NULL DEFAULT '[]',
            enabled_claude BOOLEAN NOT NULL DEFAULT 0,
            enabled_codex BOOLEAN NOT NULL DEFAULT 0,
            enabled_gemini BOOLEAN NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS skills (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            directory TEXT NOT NULL,
            repo_owner TEXT,
            repo_name TEXT,
            repo_branch TEXT DEFAULT 'main',
            readme_url TEXT,
            enabled_claude BOOLEAN NOT NULL DEFAULT 0,
            enabled_codex BOOLEAN NOT NULL DEFAULT 0,
            enabled_gemini BOOLEAN NOT NULL DEFAULT 0,
            installed_at INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS skill_repos (
            owner TEXT NOT NULL,
            name TEXT NOT NULL,
            branch TEXT NOT NULL DEFAULT 'main',
            enabled BOOLEAN NOT NULL DEFAULT 1,
            PRIMARY KEY (owner, name)
        );

        CREATE TABLE IF NOT EXISTS prompts (
            id TEXT NOT NULL,
            app_type TEXT NOT NULL,
            name TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            description TEXT,
            enabled INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (id, app_type)
        );

        -- 应用配置表（key-value 存储）
        CREATE TABLE IF NOT EXISTS app_configs (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Provider 表
        CREATE TABLE IF NOT EXISTS providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            app_type TEXT NOT NULL,
            api_key TEXT NOT NULL,
            url TEXT,
            default_sonnet_model TEXT,
            default_opus_model TEXT,
            default_haiku_model TEXT,
            default_reasoning_model TEXT,
            custom_params TEXT,
            settings_config TEXT,
            meta TEXT,
            icon TEXT,
            in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
            description TEXT,
            tags TEXT,
            is_active BOOLEAN NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            last_used INTEGER,
            proxy_config TEXT,
            one_m_context TEXT
        );

        -- 全局代理配置表（单行表）
        CREATE TABLE IF NOT EXISTS global_proxies (
            id TEXT PRIMARY KEY,
            enabled BOOLEAN NOT NULL DEFAULT 0,
            http_proxy TEXT,
            https_proxy TEXT,
            socks5_proxy TEXT,
            no_proxy TEXT,
            updated_at INTEGER NOT NULL
        );

        -- 代理配置表（每个应用独立配置）
        CREATE TABLE IF NOT EXISTS proxy_config (
            app_type TEXT PRIMARY KEY,
            enabled BOOLEAN NOT NULL DEFAULT 0,
            auto_failover_enabled BOOLEAN NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 3,
            streaming_first_byte_timeout INTEGER NOT NULL DEFAULT 60,
            streaming_idle_timeout INTEGER NOT NULL DEFAULT 120,
            non_streaming_timeout INTEGER NOT NULL DEFAULT 600,
            circuit_failure_threshold INTEGER NOT NULL DEFAULT 5,
            circuit_success_threshold INTEGER NOT NULL DEFAULT 2,
            circuit_timeout_seconds INTEGER NOT NULL DEFAULT 60,
            circuit_error_rate_threshold REAL NOT NULL DEFAULT 0.6,
            circuit_min_requests INTEGER NOT NULL DEFAULT 10
        );

        -- 故障转移队列表
        CREATE TABLE IF NOT EXISTS failover_queue (
            app_type TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (app_type, provider_id)
        );

        -- Provider 健康状态表
        CREATE TABLE IF NOT EXISTS provider_health (
            provider_id TEXT NOT NULL,
            app_type TEXT NOT NULL,
            is_healthy BOOLEAN NOT NULL DEFAULT 1,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            last_success_at TEXT,
            last_failure_at TEXT,
            last_error TEXT,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (provider_id, app_type)
        );

        -- Antigravity 账号表
        CREATE TABLE IF NOT EXISTS antigravity_accounts (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            name TEXT,
            access_token TEXT NOT NULL,
            refresh_token TEXT NOT NULL,
            expires_in INTEGER DEFAULT 0,
            expiry_timestamp INTEGER DEFAULT 0,
            oauth_client_key TEXT,
            project_id TEXT,
            subscription_tier TEXT,
            custom_label TEXT,
            is_active INTEGER DEFAULT 0,
            disabled INTEGER DEFAULT 0,
            disabled_reason TEXT,
            quota_json TEXT,
            device_profile_json TEXT,
            created_at INTEGER NOT NULL,
            last_used INTEGER NOT NULL,
            order_index INTEGER DEFAULT 0
        );

        -- Antigravity 操作日志表
        CREATE TABLE IF NOT EXISTS ag_operation_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL,
            account_email TEXT NOT NULL,
            operation TEXT NOT NULL,
            detail TEXT,
            created_at INTEGER NOT NULL
        );
        ",
    )
    .map_err(|e| format!("Failed to create tables: {e}"))
}

#![allow(dead_code)]
use crate::database::{lock_conn, Database};
use crate::models::app_type::AppType;
use crate::models::provider::Provider;
use chrono::Utc;
use indexmap::IndexMap;
use rusqlite::OptionalExtension;
use std::str::FromStr;

impl Database {
    /// 获取所有 Provider
    pub fn list_providers(&self) -> Result<Vec<Provider>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, name, app_type, api_key, url, default_sonnet_model, default_opus_model, default_haiku_model, default_reasoning_model, custom_params, settings_config, meta, icon, in_failover_queue, description, tags, is_active, created_at, last_used, proxy_config, one_m_context FROM providers ORDER BY name ASC")
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let custom_params_str: Option<String> = row.get(9)?;
                let settings_config_str: Option<String> = row.get(10)?;
                let meta_str: Option<String> = row.get(11)?;
                let tags_str: Option<String> = row.get(15)?;
                let proxy_config_str: Option<String> = row.get(19)?;
                let one_m_context_str: Option<String> = row.get(20)?;

                Ok(Provider {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    app_type: AppType::from_str(&row.get::<_, String>(2)?)
                        .unwrap_or(AppType::Claude),
                    api_key: row.get(3)?,
                    url: row.get(4)?,
                    default_sonnet_model: row.get(5)?,
                    default_opus_model: row.get(6)?,
                    default_haiku_model: row.get(7)?,
                    default_reasoning_model: row.get(8)?,
                    custom_params: custom_params_str.and_then(|s| serde_json::from_str(&s).ok()),
                    settings_config: settings_config_str
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    meta: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                    icon: row.get(12)?,
                    in_failover_queue: row.get(13)?,
                    description: row.get(14)?,
                    tags: tags_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default(),
                    is_active: row.get(16)?,
                    created_at: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(17)?, 0)
                        .unwrap_or_default(),
                    last_used: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(18)?, 0)
                        .or_else(|| chrono::DateTime::<Utc>::from_timestamp(0, 0)),
                    proxy_config: proxy_config_str.and_then(|s| serde_json::from_str(&s).ok()),
                    // 解析持久化的 1M 上下文声明（与 settings_config 同方式）
                    one_m_context: one_m_context_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            })
            .map_err(|e| format!("Failed to query providers: {e}"))?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(row.map_err(|e| format!("Failed to read row: {e}"))?);
        }
        Ok(providers)
    }

    /// 获取单个 Provider
    pub fn get_provider(&self, id: &str) -> Result<Option<Provider>, String> {
        let conn = lock_conn!(self.conn);
        let provider = conn
            .query_row(
                "SELECT id, name, app_type, api_key, url, default_sonnet_model, default_opus_model, default_haiku_model, default_reasoning_model, custom_params, settings_config, meta, icon, in_failover_queue, description, tags, is_active, created_at, last_used, proxy_config, one_m_context FROM providers WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    let custom_params_str: Option<String> = row.get(9)?;
                    let settings_config_str: Option<String> = row.get(10)?;
                    let meta_str: Option<String> = row.get(11)?;
                    let tags_str: Option<String> = row.get(15)?;
                    let proxy_config_str: Option<String> = row.get(19)?;
                    let one_m_context_str: Option<String> = row.get(20)?;

                    Ok(Provider {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        app_type: AppType::from_str(&row.get::<_, String>(2)?).unwrap_or(AppType::Claude),
                        api_key: row.get(3)?,
                        url: row.get(4)?,
                        default_sonnet_model: row.get(5)?,
                        default_opus_model: row.get(6)?,
                        default_haiku_model: row.get(7)?,
                        default_reasoning_model: row.get(8)?,
                        custom_params: custom_params_str.and_then(|s| serde_json::from_str(&s).ok()),
                        settings_config: settings_config_str.and_then(|s| serde_json::from_str(&s).ok()),
                        meta: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                        icon: row.get(12)?,
                        in_failover_queue: row.get(13)?,
                        description: row.get(14)?,
                        tags: tags_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                        is_active: row.get(16)?,
                        created_at: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(17)?, 0).unwrap_or_default(),
                        last_used: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(18)?, 0).or_else(|| chrono::DateTime::<Utc>::from_timestamp(0, 0)),
                        proxy_config: proxy_config_str.and_then(|s| serde_json::from_str(&s).ok()),
                        // 解析持久化的 1M 上下文声明（与 settings_config 同方式）
                        one_m_context: one_m_context_str.and_then(|s| serde_json::from_str(&s).ok()),
                    })
                },
            )
            .optional()
            .map_err(|e| format!("Failed to get provider: {e}"))?;
        Ok(provider)
    }

    /// 保存 Provider（INSERT OR REPLACE）
    pub fn upsert_provider(&self, provider: &Provider) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        let custom_params_str = provider
            .custom_params
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        let settings_config_str = provider
            .settings_config
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        let meta_str = provider
            .meta
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        let tags_str = serde_json::to_string(&provider.tags).ok();
        let proxy_config_str = provider
            .proxy_config
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok());
        // 1M 上下文声明序列化为 JSON TEXT 落库（与 settings_config 同方式）
        let one_m_context_str = serde_json::to_string(&provider.one_m_context).ok();

        conn.execute(
            "INSERT OR REPLACE INTO providers (id, name, app_type, api_key, url, default_sonnet_model, default_opus_model, default_haiku_model, default_reasoning_model, custom_params, settings_config, meta, icon, in_failover_queue, description, tags, is_active, created_at, last_used, proxy_config, one_m_context) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            rusqlite::params![
                provider.id,
                provider.name,
                provider.app_type.as_str(),
                provider.api_key,
                provider.url,
                provider.default_sonnet_model,
                provider.default_opus_model,
                provider.default_haiku_model,
                provider.default_reasoning_model,
                custom_params_str,
                settings_config_str,
                meta_str,
                provider.icon,
                provider.in_failover_queue,
                provider.description,
                tags_str,
                provider.is_active,
                provider.created_at.timestamp(),
                provider.last_used.map(|dt| dt.timestamp()).unwrap_or(0),
                proxy_config_str,
                one_m_context_str,
            ],
        )
        .map_err(|e| format!("Failed to upsert provider: {e}"))?;
        Ok(())
    }

    /// 删除 Provider
    pub fn delete_provider(&self, id: &str) -> Result<bool, String> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute("DELETE FROM providers WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| format!("Failed to delete provider: {e}"))?;
        Ok(affected > 0)
    }

    /// 设置 Provider 激活状态（保留兼容）
    #[allow(dead_code)]
    pub fn set_active_provider(&self, id: &str, is_active: bool) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE providers SET is_active = ?1 WHERE id = ?2",
            rusqlite::params![is_active, id],
        )
        .map_err(|e| format!("Failed to set provider active: {e}"))?;
        Ok(())
    }

    /// 设置 Provider 最后使用时间（保留兼容）
    #[allow(dead_code)]
    pub fn set_provider_last_used(&self, id: &str, last_used: i64) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "UPDATE providers SET last_used = ?1 WHERE id = ?2",
            rusqlite::params![last_used, id],
        )
        .map_err(|e| format!("Failed to set provider last_used: {e}"))?;
        Ok(())
    }

    /// 获取指定应用类型的所有 Provider
    pub fn list_providers_by_app(&self, app_type: &str) -> Result<Vec<Provider>, String> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT id, name, app_type, api_key, url, default_sonnet_model, default_opus_model, default_haiku_model, default_reasoning_model, custom_params, settings_config, meta, icon, in_failover_queue, description, tags, is_active, created_at, last_used, proxy_config, one_m_context FROM providers WHERE app_type = ?1 ORDER BY name ASC")
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![app_type], |row| {
                let custom_params_str: Option<String> = row.get(9)?;
                let settings_config_str: Option<String> = row.get(10)?;
                let meta_str: Option<String> = row.get(11)?;
                let tags_str: Option<String> = row.get(15)?;
                let proxy_config_str: Option<String> = row.get(19)?;
                let one_m_context_str: Option<String> = row.get(20)?;

                Ok(Provider {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    app_type: AppType::from_str(&row.get::<_, String>(2)?)
                        .unwrap_or(AppType::Claude),
                    api_key: row.get(3)?,
                    url: row.get(4)?,
                    default_sonnet_model: row.get(5)?,
                    default_opus_model: row.get(6)?,
                    default_haiku_model: row.get(7)?,
                    default_reasoning_model: row.get(8)?,
                    custom_params: custom_params_str.and_then(|s| serde_json::from_str(&s).ok()),
                    settings_config: settings_config_str
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    meta: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                    icon: row.get(12)?,
                    in_failover_queue: row.get(13)?,
                    description: row.get(14)?,
                    tags: tags_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default(),
                    is_active: row.get(16)?,
                    created_at: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(17)?, 0)
                        .unwrap_or_default(),
                    last_used: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(18)?, 0)
                        .or_else(|| chrono::DateTime::<Utc>::from_timestamp(0, 0)),
                    proxy_config: proxy_config_str.and_then(|s| serde_json::from_str(&s).ok()),
                    // 解析持久化的 1M 上下文声明（与 settings_config 同方式）
                    one_m_context: one_m_context_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            })
            .map_err(|e| format!("Failed to query providers by app: {e}"))?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(row.map_err(|e| format!("Failed to read row: {e}"))?);
        }
        Ok(providers)
    }

    /// 获取指定应用类型的单个 Provider
    pub fn get_provider_by_app(
        &self,
        id: &str,
        app_type: &str,
    ) -> Result<Option<Provider>, String> {
        let conn = lock_conn!(self.conn);
        let provider = conn
            .query_row(
                "SELECT id, name, app_type, api_key, url, default_sonnet_model, default_opus_model, default_haiku_model, default_reasoning_model, custom_params, settings_config, meta, icon, in_failover_queue, description, tags, is_active, created_at, last_used, proxy_config, one_m_context FROM providers WHERE id = ?1 AND app_type = ?2",
                rusqlite::params![id, app_type],
                |row| {
                    let custom_params_str: Option<String> = row.get(9)?;
                    let settings_config_str: Option<String> = row.get(10)?;
                    let meta_str: Option<String> = row.get(11)?;
                    let tags_str: Option<String> = row.get(15)?;
                    let proxy_config_str: Option<String> = row.get(19)?;
                    let one_m_context_str: Option<String> = row.get(20)?;

                    Ok(Provider {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        app_type: AppType::from_str(&row.get::<_, String>(2)?).unwrap_or(AppType::Claude),
                        api_key: row.get(3)?,
                        url: row.get(4)?,
                        default_sonnet_model: row.get(5)?,
                        default_opus_model: row.get(6)?,
                        default_haiku_model: row.get(7)?,
                        default_reasoning_model: row.get(8)?,
                        custom_params: custom_params_str.and_then(|s| serde_json::from_str(&s).ok()),
                        settings_config: settings_config_str.and_then(|s| serde_json::from_str(&s).ok()),
                        meta: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                        icon: row.get(12)?,
                        in_failover_queue: row.get(13)?,
                        description: row.get(14)?,
                        tags: tags_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
                        is_active: row.get(16)?,
                        created_at: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(17)?, 0).unwrap_or_default(),
                        last_used: chrono::DateTime::<Utc>::from_timestamp(row.get::<_, i64>(18)?, 0).or_else(|| chrono::DateTime::<Utc>::from_timestamp(0, 0)),
                        proxy_config: proxy_config_str.and_then(|s| serde_json::from_str(&s).ok()),
                        // 解析持久化的 1M 上下文声明（与 settings_config 同方式）
                        one_m_context: one_m_context_str.and_then(|s| serde_json::from_str(&s).ok()),
                    })
                },
            )
            .optional()
            .map_err(|e| format!("Failed to get provider by app: {e}"))?;
        Ok(provider)
    }

    /// 获取指定应用类型的当前激活 Provider ID
    pub fn get_current_provider_id(&self, app_type: &str) -> Result<Option<String>, String> {
        let conn = lock_conn!(self.conn);
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM providers WHERE app_type = ?1 AND is_active = 1 LIMIT 1",
                rusqlite::params![app_type],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Failed to get current provider id: {e}"))?;
        Ok(id)
    }

    /// 设置指定应用类型的当前激活 Provider
    ///
    /// 先将同 app_type 的所有 Provider 设为非激活，再将目标 Provider 设为激活
    pub fn set_current_provider_by_app(&self, app_type: &str, id: &str) -> Result<(), String> {
        let conn = lock_conn!(self.conn);
        // 先将同 app_type 的所有 Provider 设为非激活
        conn.execute(
            "UPDATE providers SET is_active = 0 WHERE app_type = ?1",
            rusqlite::params![app_type],
        )
        .map_err(|e| format!("Failed to deactivate providers: {e}"))?;

        // 再将目标 Provider 设为激活
        conn.execute(
            "UPDATE providers SET is_active = 1 WHERE id = ?1 AND app_type = ?2",
            rusqlite::params![id, app_type],
        )
        .map_err(|e| format!("Failed to activate provider: {e}"))?;
        Ok(())
    }

    /// 获取指定应用类型的所有 Provider，以 IndexMap 返回（按 id 索引）
    pub fn get_all_providers_map(
        &self,
        app_type: &str,
    ) -> Result<IndexMap<String, Provider>, String> {
        let providers = self.list_providers_by_app(app_type)?;
        let mut map = IndexMap::new();
        for provider in providers {
            map.insert(provider.id.clone(), provider);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::OneMContext;

    /// 构造一个最小可用的 Provider，用于持久化往返测试
    fn sample_provider(one_m_context: Option<OneMContext>) -> Provider {
        Provider {
            id: "p-1m-roundtrip".to_string(),
            name: "RoundTrip Provider".to_string(),
            app_type: AppType::Claude,
            api_key: "sk-test".to_string(),
            url: Some("https://api.example.com".to_string()),
            default_sonnet_model: Some("claude-sonnet".to_string()),
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
            is_active: false,
            created_at: Utc::now(),
            last_used: None,
            proxy_config: None,
            one_m_context,
        }
    }

    #[test]
    fn one_m_context_survives_upsert_get_roundtrip() {
        let db = Database::in_memory().expect("init in-memory db");
        // 仅 sonnet 声明 1M 上下文
        let provider = sample_provider(Some(OneMContext {
            sonnet: true,
            opus: false,
            haiku: false,
            reasoning: false,
        }));
        db.upsert_provider(&provider).expect("upsert provider");

        let loaded = db
            .get_provider(&provider.id)
            .expect("get provider")
            .expect("provider exists");

        // 关键断言：sonnet 的 1M 声明在落库后仍然存活
        let one_m = loaded
            .one_m_context
            .expect("one_m_context should be persisted");
        assert!(one_m.sonnet);
        assert!(!one_m.opus);
        assert!(!one_m.haiku);
        assert!(!one_m.reasoning);
    }
}

use crate::models::app_type::AppType;
use crate::models::provider::Provider;
use crate::proxy::error::ProxyError;
use crate::services::provider_service;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

/// 路由结果：目标 URL、认证头、额外头
pub struct RouteResult {
    pub target_url: String,
    pub headers: HeaderMap,
}

/// 基于已选中的 provider 构建上游目标和认证头。
pub fn build_route(provider: &Provider, request_path: &str) -> Result<RouteResult, ProxyError> {
    let base_url = provider
        .url
        .as_deref()
        .unwrap_or("https://api.anthropic.com");
    let base_url = base_url.trim_end_matches('/');
    let target_url = format!("{}{}", base_url, request_path);

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if base_url.contains("anthropic.com") {
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&provider.api_key)
                .map_err(|e| ProxyError::ConfigError(e.to_string()))?,
        );
    } else {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", provider.api_key))
                .map_err(|e| ProxyError::ConfigError(e.to_string()))?,
        );
    }

    Ok(RouteResult {
        target_url,
        headers,
    })
}

/// 根据当前活跃 provider 解析上游目标
pub fn resolve_upstream(request_path: &str) -> Result<RouteResult, ProxyError> {
    // 获取 Claude 应用的活跃 provider
    let providers = provider_service::list_providers(AppType::Claude)
        .map_err(|e| ProxyError::Internal(e.to_string()))?;

    let active = providers
        .into_iter()
        .find(|p| p.is_active)
        .ok_or(ProxyError::NoAvailableProvider)?;

    build_route(&active, request_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::Provider;
    use chrono::Utc;

    fn provider(url: Option<String>) -> Provider {
        Provider {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            app_type: AppType::Claude,
            api_key: "sk-test".to_string(),
            url,
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
            created_at: Utc::now(),
            last_used: None,
            proxy_config: None,
            one_m_context: None,
        }
    }

    #[test]
    fn build_route_uses_passed_provider() {
        let route = build_route(
            &provider(Some("https://proxy.example.com/".to_string())),
            "/v1/messages?x=1",
        )
        .expect("route");

        assert_eq!(route.target_url, "https://proxy.example.com/v1/messages?x=1");
        assert_eq!(
            route.headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()),
            Some("Bearer sk-test")
        );
    }
}

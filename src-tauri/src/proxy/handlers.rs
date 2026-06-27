use crate::models::app_type::AppType;
use crate::proxy::circuit_breaker;
use crate::proxy::error::ProxyError;
use crate::proxy::failover_switch;
use crate::proxy::http_client;
use crate::proxy::model_mapper;
use crate::proxy::provider_router;
use crate::proxy::server;
use crate::proxy::thinking_rectifier;
use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// 代理请求处理器：转发所有请求到上游 provider
pub async fn proxy_handler(req: Request<Body>) -> Result<Response, ProxyError> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let request_path = format!("{}{}", path, query);

    // 收集原始请求头（排除 host 等 hop-by-hop 头）
    let original_headers = req.headers().clone();

    // 读取请求体
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?;

    // 预处理：将 thinking.type "adaptive" 转为 "enabled"（兼容第三方反代）
    let body_bytes = match thinking_rectifier::normalize_thinking_type(&body_bytes) {
        Ok(Some(fixed)) => fixed.into(),
        _ => body_bytes,
    };

    let config = circuit_breaker::default_config();
    // 热更新核心：每个请求建立前都重新读取当前可用 provider，运行中的 CLI 会话无需重启。
    let provider = failover_switch::get_available_provider(AppType::Claude)?;

    let json_body: serde_json::Value =
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
    let (mapped_json, _original_model, _outbound_model) = if json_body.is_object() {
        model_mapper::apply_model_mapping(json_body, &provider)
    } else {
        (serde_json::Value::Null, None, None)
    };
    let forward_body = if mapped_json.is_null() {
        body_bytes.clone()
    } else {
        serde_json::to_vec(&mapped_json)
            .map(bytes::Bytes::from)
            .unwrap_or_else(|_| body_bytes.clone())
    };

    let route = provider_router::build_route(&provider, &request_path)?;

    // 合并请求头：路由头优先，保留原始请求中的非冲突头
    let mut forward_headers = route.headers;
    for (key, value) in original_headers.iter() {
        let name = key.as_str().to_lowercase();
        // 跳过 hop-by-hop 头和已设置的头
        if matches!(
            name.as_str(),
            "host" | "connection" | "transfer-encoding" | "content-length"
        ) {
            continue;
        }
        if !forward_headers.contains_key(key) {
            forward_headers.insert(key.clone(), value.clone());
        }
    }

    // 转发请求
    let method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| ProxyError::InvalidRequest(e.to_string()))?;

    let upstream_resp = match http_client::forward_request(
        method,
        &route.target_url,
        forward_headers,
        forward_body,
    )
    .await
    {
        Ok(resp) => {
            failover_switch::on_success(&provider.id, &config);
            resp
        }
        Err(e) => {
            // 只在请求建立阶段记录失败；一旦拿到响应体并开始流式转发，就不再切换 provider。
            let _ = failover_switch::on_failure(AppType::Claude, &provider.id, &config);
            return Err(ProxyError::ForwardFailed(e.to_string()));
        }
    };

    // 递增请求计数
    server::increment_request_count();

    // 构建响应
    let status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = upstream_resp.headers().clone();

    // 流式转发响应体
    let body_stream = upstream_resp.bytes_stream();
    let body = Body::from_stream(body_stream);

    let mut response = Response::builder().status(status);
    for (key, value) in resp_headers.iter() {
        let name = key.as_str().to_lowercase();
        if matches!(name.as_str(), "transfer-encoding" | "connection") {
            continue;
        }
        response = response.header(key, value);
    }

    response
        .body(body)
        .map_err(|e| ProxyError::Internal(e.to_string()))
}

/// 健康检查端点
pub async fn health_handler() -> impl IntoResponse {
    let state = server::get_state();
    (StatusCode::OK, axum::Json(state))
}

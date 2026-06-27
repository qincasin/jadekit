//! NDJSON protocol types for daemon communication.
//!
//! These mirror the message shapes produced/consumed by
//! `resources/ai-bridge/daemon.js`. Keep them in sync with that file.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request written to the daemon's stdin (one JSON object per line).
///
/// `method` is `"<provider>.<command>"`, e.g. `"claude.send"`, or a daemon
/// control verb: `"heartbeat"`, `"status"`, `"shutdown"`, `"abort"`.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonRequest {
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl DaemonRequest {
    pub fn new(id: impl Into<String>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            id: id.into(),
            method: method.into(),
            params,
        }
    }

    /// A control request that carries no params (heartbeat/status/shutdown/abort).
    pub fn control(id: impl Into<String>, method: impl Into<String>) -> Self {
        Self::new(id, method, None)
    }
}

/// A line read from the daemon's stdout, parsed from NDJSON.
///
/// The daemon multiplexes several message kinds on one stream; we discriminate
/// by which fields are present rather than a single tag, matching daemon.js.
#[derive(Debug, Clone, Deserialize)]
pub struct RawLine {
    /// Request id this line belongs to (absent for broadcast lifecycle events).
    #[serde(default)]
    pub id: Option<String>,
    /// Lifecycle/broadcast type, e.g. "daemon", "heartbeat", "status".
    #[serde(default)]
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Lifecycle event name when kind == "daemon" (ready/starting/sdk_loaded...).
    #[serde(default)]
    pub event: Option<String>,
    /// A streamed output line for an active request.
    #[serde(default)]
    pub line: Option<String>,
    /// Stderr text captured for an active request.
    #[serde(default)]
    pub stderr: Option<String>,
    /// Present (true) on the terminal line of a request.
    #[serde(default)]
    pub done: Option<bool>,
    /// Whether the request succeeded (only meaningful when done == true).
    #[serde(default)]
    pub success: Option<bool>,
    /// Error message on failure.
    #[serde(default)]
    pub error: Option<String>,
    /// Heartbeat timestamp / misc numeric payloads.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// A high-level streamed item emitted toward a request's listener.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamLine {
    /// A streamed output line from the SDK (already shaped by ai-bridge).
    Line { text: String },
    /// Captured stderr text.
    Stderr { text: String },
    /// Terminal signal for the request.
    Done {
        success: bool,
        error: Option<String>,
    },
}

/// A daemon lifecycle event (broadcast, not tied to a request id).
#[derive(Debug, Clone, Serialize)]
pub struct DaemonEvent {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

//! The Rust port of jetbrains-cc-gui's `DaemonBridge.java`.
//!
//! Owns a long-running Node.js daemon process and provides request/response +
//! streaming over NDJSON. Responses are demultiplexed by request id; streamed
//! lines are delivered through a per-request channel while the request is in
//! flight.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, Mutex, RwLock};

use super::protocol::{DaemonEvent, DaemonRequest, RawLine, StreamLine};
use super::resources;

/// Session ID for permission file IPC. Hardcoded for now; can be randomized later.
pub const SESSION_ID: &str = "default";

/// Callback invoked for daemon lifecycle/broadcast events (ready, sdk_loaded…).
pub type EventSink = Arc<dyn Fn(DaemonEvent) + Send + Sync>;

/// Per-request sender for streamed lines. Dropped when the request completes.
type StreamSender = mpsc::UnboundedSender<StreamLine>;

struct Inner {
    stdin: Option<ChildStdin>,
    child: Option<Child>,
    /// requestId → channel for streamed lines.
    pending: HashMap<String, StreamSender>,
}

pub struct DaemonClient {
    node_path: PathBuf,
    bridge_dir: PathBuf,
    deps_dir: PathBuf,
    permission_dir: PathBuf,
    provider_config: RwLock<ProviderRuntimeConfig>,
    /// When true, spawn the daemon with `CLAUDE_DEBUG=1` for verbose diagnostics.
    debug: bool,
    request_counter: AtomicU64,
    running: Arc<AtomicBool>,
    inner: Arc<Mutex<Inner>>,
    event_sink: Mutex<Option<EventSink>>,
}

#[derive(Clone)]
struct ProviderRuntimeConfig {
    api_key: Option<String>,
    base_url: Option<String>,
}

fn daemon_env_vars(
    permission_dir: &Path,
    deps_dir: &Path,
    provider_config: &ProviderRuntimeConfig,
) -> Vec<(&'static str, OsString)> {
    let mut vars = vec![
        ("AI_BRIDGE_DEPS_DIR", deps_dir.as_os_str().to_owned()),
        (
            "CLAUDE_PERMISSION_DIR",
            permission_dir.as_os_str().to_owned(),
        ),
        ("CLAUDE_SESSION_ID", OsString::from(SESSION_ID)),
    ];

    if let Some(ref key) = provider_config.api_key {
        vars.push(("ANTHROPIC_AUTH_TOKEN", OsString::from(key)));
    }
    if let Some(ref url) = provider_config.base_url {
        vars.push(("ANTHROPIC_BASE_URL", OsString::from(url)));
    }

    vars
}

impl DaemonClient {
    pub fn new(
        node_path: PathBuf,
        bridge_dir: PathBuf,
        deps_dir: PathBuf,
        permission_dir: PathBuf,
        api_key: Option<String>,
        base_url: Option<String>,
        debug: bool,
    ) -> Self {
        Self {
            node_path,
            bridge_dir,
            deps_dir,
            permission_dir,
            provider_config: RwLock::new(ProviderRuntimeConfig { api_key, base_url }),
            debug,
            request_counter: AtomicU64::new(0),
            running: Arc::new(AtomicBool::new(false)),
            inner: Arc::new(Mutex::new(Inner {
                stdin: None,
                child: None,
                pending: HashMap::new(),
            })),
            event_sink: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Register a sink for daemon lifecycle events. Replaces any previous sink.
    pub async fn set_event_sink(&self, sink: EventSink) {
        *self.event_sink.lock().await = Some(sink);
    }

    fn next_id(&self) -> String {
        let n = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;
        format!("req-{n}")
    }

    /// Spawn the daemon and start the stdout reader loop. Idempotent: a no-op if
    /// already running.
    pub async fn start(&self) -> Result<(), String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let daemon_js = self.bridge_dir.join("daemon.js");
        if !daemon_js.exists() {
            self.running.store(false, Ordering::SeqCst);
            return Err(format!("daemon.js not found at {}", daemon_js.display()));
        }

        // Ensure the deps dir exists so on-demand SDK installs have a target.
        let _ = std::fs::create_dir_all(&self.deps_dir);

        // === Normalize paths: strip Windows UNC prefix (\\?\) ===
        // Node.js ES module loader doesn't support UNC paths, so we convert them
        // to regular paths. This is safe because our paths are all short.
        let normalize_path = |p: &PathBuf| -> PathBuf {
            let s = p.to_string_lossy();
            if s.starts_with(r"\\?\") {
                PathBuf::from(s.trim_start_matches(r"\\?\"))
            } else {
                p.clone()
            }
        };

        let daemon_js_normalized = normalize_path(&daemon_js);
        let bridge_dir_normalized = normalize_path(&self.bridge_dir);

        let mut cmd = Command::new(&self.node_path);
        let path_env = resources::node_execution_path_env(
            &self.node_path,
            None,
            std::env::var_os("PATH").as_deref(),
        );
        cmd.arg(&daemon_js_normalized)
            .current_dir(&bridge_dir_normalized)
            .env("PATH", path_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let provider_config = self.provider_config.read().await.clone();
        for (key, value) in daemon_env_vars(&self.permission_dir, &self.deps_dir, &provider_config)
        {
            cmd.env(key, value);
        }

        // Verbose diagnostics: unlocks the daemon's gated `debugLog` output so the
        // debug log panel can surface SDK/Node failure causes (see api-config.js).
        if self.debug {
            cmd.env("CLAUDE_DEBUG", "1");
        }

        #[cfg(windows)]
        {
            cmd.creation_flags(0x0800_0000);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Daemon stdout unavailable".to_string())?;
        let stderr = child.stderr.take();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Daemon stdin unavailable".to_string())?;

        {
            let mut inner = self.inner.lock().await;
            inner.stdin = Some(stdin);
            inner.child = Some(child);
        }

        // Reader loop: parse NDJSON and route by request id.
        let inner = self.inner.clone();
        let running = self.running.clone();
        let event_sink = self.current_sink().await;
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                let parsed: Result<RawLine, _> = serde_json::from_str(&line);
                let raw = match parsed {
                    Ok(r) => r,
                    Err(_) => {
                        if let Some(sink) = &event_sink {
                            sink(DaemonEvent {
                                event: "log".into(),
                                pid: None,
                                message: Some(line),
                                provider: None,
                            });
                        }
                        continue;
                    }
                };
                Self::route_line(&inner, &event_sink, raw).await;
            }
            Self::handle_stdout_closed(&inner, &running, &event_sink).await;
        });

        // Stderr drain (diagnostics → event sink as logs).
        if let Some(stderr) = stderr {
            let event_sink = self.current_sink().await;
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(sink) = &event_sink {
                        sink(DaemonEvent {
                            event: "stderr".into(),
                            pid: None,
                            message: Some(line),
                            provider: None,
                        });
                    }
                }
            });
        }

        Ok(())
    }

    pub async fn update_provider_config(&self, api_key: Option<String>, base_url: Option<String>) {
        *self.provider_config.write().await = ProviderRuntimeConfig { api_key, base_url };
    }

    async fn current_sink(&self) -> Option<EventSink> {
        self.event_sink.lock().await.clone()
    }

    /// Handle daemon stdout closing unexpectedly or during shutdown.
    async fn handle_stdout_closed(
        inner: &Arc<Mutex<Inner>>,
        running: &Arc<AtomicBool>,
        event_sink: &Option<EventSink>,
    ) {
        let was_running = running.swap(false, Ordering::SeqCst);

        {
            let mut guard = inner.lock().await;
            guard.stdin = None;
            guard.child = None;
            for (_, tx) in guard.pending.drain() {
                let _ = tx.send(StreamLine::Done {
                    success: false,
                    error: Some("daemon exited".into()),
                });
            }
        }

        if was_running {
            if let Some(sink) = event_sink {
                sink(DaemonEvent {
                    event: "shutdown".into(),
                    pid: None,
                    message: Some("daemon exited".into()),
                    provider: None,
                });
            }
        }
    }

    /// Route one parsed stdout line to its request channel or the event sink.
    async fn route_line(inner: &Arc<Mutex<Inner>>, event_sink: &Option<EventSink>, raw: RawLine) {
        // Lifecycle/broadcast events: kind == "daemon".
        if raw.kind.as_deref() == Some("daemon") {
            if let Some(sink) = event_sink {
                let pid = raw.extra.get("pid").and_then(|v| v.as_u64());
                let provider = raw
                    .extra
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let message = raw
                    .extra
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                sink(DaemonEvent {
                    event: raw.event.unwrap_or_else(|| "unknown".into()),
                    pid,
                    message,
                    provider,
                });
            }
            return;
        }

        // Heartbeat / status responses don't carry stream data we forward.
        if matches!(raw.kind.as_deref(), Some("heartbeat") | Some("status")) {
            return;
        }

        let Some(id) = raw.id.clone() else {
            return;
        };

        // Terminal line for a request.
        if raw.done == Some(true) {
            let mut guard = inner.lock().await;
            if let Some(tx) = guard.pending.remove(&id) {
                let _ = tx.send(StreamLine::Done {
                    success: raw.success.unwrap_or(false),
                    error: raw.error,
                });
            }
            return;
        }

        // Streamed line or stderr.
        let item = if let Some(text) = raw.line {
            Some(StreamLine::Line { text })
        } else {
            raw.stderr.map(|text| StreamLine::Stderr { text })
        };
        if let Some(item) = item {
            let guard = inner.lock().await;
            if let Some(tx) = guard.pending.get(&id) {
                let _ = tx.send(item);
            }
        }
    }

    /// Write a request line to the daemon's stdin.
    async fn write_request(&self, req: &DaemonRequest) -> Result<(), String> {
        let mut json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        json.push('\n');
        let mut guard = self.inner.lock().await;
        let stdin = guard
            .stdin
            .as_mut()
            .ok_or_else(|| "Daemon stdin not available".to_string())?;
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to daemon: {e}"))?;
        stdin.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Send a command and stream its lines back through `tx`.
    ///
    /// Returns the assigned request id. The caller consumes the receiver until a
    /// `StreamLine::Done` arrives.
    pub async fn send_streaming(
        &self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<(String, mpsc::UnboundedReceiver<StreamLine>), String> {
        let id = self.next_id();
        let (tx, rx) = mpsc::unbounded_channel();
        {
            let mut guard = self.inner.lock().await;
            guard.pending.insert(id.clone(), tx);
        }
        let req = DaemonRequest::new(id.clone(), method, Some(params));
        if let Err(e) = self.write_request(&req).await {
            let mut guard = self.inner.lock().await;
            guard.pending.remove(&id);
            return Err(e);
        }
        Ok((id, rx))
    }

    /// Send a heartbeat (fire-and-forget; response is ignored by the reader).
    pub async fn heartbeat(&self) -> Result<(), String> {
        let req = DaemonRequest::control(self.next_id(), "heartbeat");
        self.write_request(&req).await
    }

    /// Request the daemon abort the active turn. Bypasses the command queue.
    pub async fn abort(&self) -> Result<(), String> {
        let req = DaemonRequest::control(self.next_id(), "abort");
        self.write_request(&req).await
    }

    /// Gracefully stop the daemon: send shutdown, then kill if still alive.
    pub async fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }
        let req = DaemonRequest::control(self.next_id(), "shutdown");
        let _ = self.write_request(&req).await;

        let mut guard = self.inner.lock().await;
        guard.stdin = None; // closing stdin triggers daemon shutdown
        if let Some(mut child) = guard.child.take() {
            // Give it a moment, then force-kill.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await;
            let _ = child.kill().await;
        }
        guard.pending.clear();
    }

    /// Restart the daemon: stop the current process (if any) and spawn a fresh
    /// one. Used after installing an SDK so the new package is preloaded.
    pub async fn restart(&self) -> Result<(), String> {
        self.stop().await;
        // Brief pause so the OS releases the previous process's handles.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        self.start().await
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::{DaemonEvent, DaemonRequest, RawLine, StreamLine};
    use super::{daemon_env_vars, DaemonClient, EventSink, Inner, ProviderRuntimeConfig};
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::sync::{mpsc, Mutex};

    #[test]
    fn request_serializes_to_ndjson_shape() {
        let req = DaemonRequest::new("req-1", "claude.send", Some(json!({"message": "hi"})));
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"id\":\"req-1\""));
        assert!(s.contains("\"method\":\"claude.send\""));
        assert!(s.contains("\"message\":\"hi\""));
    }

    #[test]
    fn control_request_omits_params() {
        let req = DaemonRequest::control("0", "heartbeat");
        let s = serde_json::to_string(&req).unwrap();
        assert!(!s.contains("params"));
    }

    #[test]
    fn parses_lifecycle_ready_event() {
        let line = r#"{"type":"daemon","event":"ready","pid":12345}"#;
        let raw: RawLine = serde_json::from_str(line).unwrap();
        assert_eq!(raw.kind.as_deref(), Some("daemon"));
        assert_eq!(raw.event.as_deref(), Some("ready"));
        assert_eq!(raw.extra.get("pid").and_then(|v| v.as_u64()), Some(12345));
    }

    #[test]
    fn parses_streamed_line() {
        let line = r#"{"id":"req-1","line":"[CONTENT_DELTA] \"hi\""}"#;
        let raw: RawLine = serde_json::from_str(line).unwrap();
        assert_eq!(raw.id.as_deref(), Some("req-1"));
        assert_eq!(raw.line.as_deref(), Some("[CONTENT_DELTA] \"hi\""));
        assert_eq!(raw.done, None);
    }

    #[test]
    fn parses_done_signal() {
        let line = r#"{"id":"req-1","done":true,"success":true}"#;
        let raw: RawLine = serde_json::from_str(line).unwrap();
        assert_eq!(raw.done, Some(true));
        assert_eq!(raw.success, Some(true));
    }

    #[test]
    fn parses_done_failure_with_error() {
        let line = r#"{"id":"req-1","done":true,"success":false,"error":"boom"}"#;
        let raw: RawLine = serde_json::from_str(line).unwrap();
        assert_eq!(raw.success, Some(false));
        assert_eq!(raw.error.as_deref(), Some("boom"));
    }

    #[test]
    fn parses_heartbeat_response() {
        let line = r#"{"id":"0","type":"heartbeat","ts":1700000000000}"#;
        let raw: RawLine = serde_json::from_str(line).unwrap();
        assert_eq!(raw.kind.as_deref(), Some("heartbeat"));
        assert_eq!(
            raw.extra.get("ts").and_then(|v| v.as_u64()),
            Some(1700000000000)
        );
    }

    #[test]
    fn daemon_env_vars_include_sdk_deps_dir_and_provider_config() {
        let deps_dir = PathBuf::from(r"C:\Users\alice\.ccg-switch\ai-bridge-deps");
        let permission_dir =
            PathBuf::from(r"C:\Users\alice\AppData\Roaming\ccg-switch\permissions");
        let provider_config = ProviderRuntimeConfig {
            api_key: Some("secret-token".into()),
            base_url: Some("https://api.example.invalid".into()),
        };

        let vars: HashMap<_, _> = daemon_env_vars(&permission_dir, &deps_dir, &provider_config)
            .into_iter()
            .map(|(key, value)| (key, value.to_string_lossy().into_owned()))
            .collect();

        assert_eq!(
            vars.get("AI_BRIDGE_DEPS_DIR").map(String::as_str),
            deps_dir.to_str()
        );
        assert_eq!(
            vars.get("CLAUDE_PERMISSION_DIR").map(String::as_str),
            permission_dir.to_str()
        );
        assert_eq!(
            vars.get("CLAUDE_SESSION_ID").map(String::as_str),
            Some("default")
        );
        assert_eq!(
            vars.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("secret-token")
        );
        assert_eq!(
            vars.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://api.example.invalid")
        );
    }

    #[tokio::test]
    async fn stdout_close_marks_stopped_drains_pending_and_emits_shutdown() {
        let inner = Arc::new(Mutex::new(Inner {
            stdin: None,
            child: None,
            pending: HashMap::new(),
        }));
        let running = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = mpsc::unbounded_channel();
        inner.lock().await.pending.insert("req-1".into(), tx);

        let events = Arc::new(StdMutex::new(Vec::<DaemonEvent>::new()));
        let captured_events = events.clone();
        let sink: Option<EventSink> = Some(Arc::new(move |event| {
            captured_events.lock().unwrap().push(event);
        }));

        DaemonClient::handle_stdout_closed(&inner, &running, &sink).await;

        assert!(!running.load(Ordering::SeqCst));
        assert!(inner.lock().await.pending.is_empty());

        let done = rx
            .recv()
            .await
            .expect("pending request should receive daemon exit");
        match done {
            StreamLine::Done { success, error } => {
                assert!(!success);
                assert_eq!(error.as_deref(), Some("daemon exited"));
            }
            other => panic!("expected done event, got {other:?}"),
        }

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "shutdown");
        assert_eq!(events[0].message.as_deref(), Some("daemon exited"));
    }

    #[tokio::test]
    async fn stdout_close_after_stop_does_not_emit_duplicate_shutdown() {
        let inner = Arc::new(Mutex::new(Inner {
            stdin: None,
            child: None,
            pending: HashMap::new(),
        }));
        let running = Arc::new(AtomicBool::new(false));

        let events = Arc::new(StdMutex::new(Vec::<DaemonEvent>::new()));
        let captured_events = events.clone();
        let sink: Option<EventSink> = Some(Arc::new(move |event| {
            captured_events.lock().unwrap().push(event);
        }));

        DaemonClient::handle_stdout_closed(&inner, &running, &sink).await;

        assert!(!running.load(Ordering::SeqCst));
        assert!(inner.lock().await.pending.is_empty());
        assert!(events.lock().unwrap().is_empty());
    }
}

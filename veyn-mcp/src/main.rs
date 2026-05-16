use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "veyn-mcp",
    version,
    about = "MCP stdio server for the VEYN daemon"
)]
struct Cli {
    #[arg(long, env = "VEYN_URL", default_value = "http://127.0.0.1:7700")]
    url: String,

    #[arg(long, env = "VEYN_TOKEN")]
    token: Option<String>,

    #[arg(long, env = "VEYN_NO_AUTH", default_value_t = false)]
    no_auth: bool,

    #[arg(long, default_value_t = 10)]
    timeout: u64,
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, msg: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: msg.into(),
            }),
        }
    }

    fn notification() -> Self {
        Self {
            jsonrpc: "2.0",
            id: Value::Null,
            result: None,
            error: None,
        }
    }

    fn is_notification(&self) -> bool {
        self.id == Value::Null && self.result.is_none() && self.error.is_none()
    }
}

fn tool_list() -> Value {
    json!({
        "tools": [
            {
                "name": "veyn_get_context",
                "description": "Current semantic snapshot — intent string (e.g. 'user in calm/resting state'), confidence score, active devices, and all current metric values (heart_rate, hrv, spo2, alpha/beta/delta/theta bands, etc.).",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            },
            {
                "name": "veyn_get_context_history",
                "description": "Last N context snapshots in reverse-chronological order. Use for trend detection or agent catch-up after inactivity.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "n": { "type": "integer", "description": "Number of snapshots (default 10, max 32).", "default": 10 }
                    },
                    "required": []
                }
            },
            {
                "name": "veyn_get_metric",
                "description": "Latest value for a single metric. Examples: heart_rate (bpm), hrv (ms), spo2 (%), steps, respiratory_rate, skin_temperature, battery, alpha_absolute, delta_absolute, theta_absolute, beta_absolute.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "metric": { "type": "string", "description": "Metric name (lowercase, underscores only)." }
                    },
                    "required": ["metric"]
                }
            },
            {
                "name": "veyn_list_devices",
                "description": "All registered devices — id, name, source adapter (ble/healthkit/eeg/mock), state, last_seen timestamp.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            },
            {
                "name": "veyn_get_presence",
                "description": "Present/absent state per device and how long they have been in that state. Absent means no events within presence_timeout_secs (default 30 s).",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            },
            {
                "name": "veyn_send_notification",
                "description": "Push a notification to the user's Apple Watch or any registered companion device.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Watch face title (keep under 30 chars)." },
                        "body": { "type": "string", "description": "Notification body text." },
                        "target_device": { "type": "string", "description": "Optional device ID. Omit to broadcast to all companions." }
                    },
                    "required": ["title", "body"]
                }
            },
            {
                "name": "veyn_get_health",
                "description": "Daemon health: status, version, uptime_s, event_rate_hz, compression_ratio, connected_devices count. Call first to verify the daemon is running.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            },
            {
                "name": "veyn_list_plugins",
                "description": "Active WASM plugins loaded by the daemon (e.g. garmin-connect, whoop) — name, version, description.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            },
            {
                "name": "veyn_get_recent_events",
                "description": "Raw event ring buffer before semantic compression — id, ts, device_id, source, metric, value, unit, meta.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            },
            {
                "name": "veyn_get_gestures",
                "description": "Recent Apple Watch gesture events (crown scroll, tap) forwarded by the companion app.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }
        ]
    })
}

#[derive(Clone)]
struct VeynClient {
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl VeynClient {
    fn new(base_url: String, token: Option<String>, timeout_secs: u64) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("build HTTP client")?;
        Ok(Self {
            base_url,
            token,
            http,
        })
    }

    async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.get(&url);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.with_context(|| format!("GET {url}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .with_context(|| format!("parse GET {url}"))?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status} on GET {path}: {body}");
        }
        Ok(body)
    }

    async fn post(&self, path: &str, payload: Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.post(&url).json(&payload);
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.with_context(|| format!("POST {url}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .with_context(|| format!("parse POST {url}"))?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status} on POST {path}: {body}");
        }
        Ok(body)
    }
}

fn load_token_from_file() -> Option<String> {
    let base = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                .join(".local/share")
        });
    std::fs::read_to_string(base.join("veyn/token"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn resolve_token(cli_token: Option<String>, no_auth: bool) -> Option<String> {
    if no_auth {
        return None;
    }
    if let Some(t) = cli_token {
        if !t.is_empty() {
            return Some(t);
        }
    }
    load_token_from_file()
}

async fn dispatch_tool(client: &VeynClient, name: &str, args: &Value) -> Result<Value> {
    match name {
        "veyn_get_context" => client.get("/v1/context/current").await,
        "veyn_get_context_history" => {
            let n = args.get("n").and_then(Value::as_u64).unwrap_or(10).min(32);
            client.get(&format!("/v1/context/history?n={n}")).await
        }
        "veyn_get_metric" => {
            let metric = args
                .get("metric")
                .and_then(Value::as_str)
                .context("missing argument: metric")?;
            // block path injection — only lowercase letters, digits, underscores
            if !metric
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                anyhow::bail!("invalid metric name '{metric}'");
            }
            client.get(&format!("/v1/metrics/{metric}")).await
        }
        "veyn_list_devices" => client.get("/v1/devices").await,
        "veyn_get_presence" => client.get("/v1/presence").await,
        "veyn_send_notification" => {
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .context("missing: title")?;
            let body = args
                .get("body")
                .and_then(Value::as_str)
                .context("missing: body")?;
            let mut payload = json!({ "title": title, "body": body });
            if let Some(dev) = args.get("target_device").and_then(Value::as_str) {
                payload["target_device"] = json!(dev);
            }
            client.post("/v1/notify", payload).await
        }
        "veyn_get_health" => client.get("/v1/health").await,
        "veyn_list_plugins" => client.get("/v1/plugins").await,
        "veyn_get_recent_events" => client.get("/v1/events/recent").await,
        "veyn_get_gestures" => client.get("/v1/gestures/recent").await,
        _ => anyhow::bail!("unknown tool '{name}'"),
    }
}

async fn handle_method(client: &VeynClient, method: &str, params: Value) -> Result<Value> {
    match method {
        "initialize" => {
            let client_name = params
                .get("clientInfo")
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            info!(client = %client_name, "MCP initialize");
            Ok(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "veyn-mcp", "version": env!("CARGO_PKG_VERSION") }
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tool_list()),
        "tools/call" => {
            let tool = params
                .get("name")
                .and_then(Value::as_str)
                .context("tools/call missing 'name'")?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            debug!(tool = %tool, "dispatching");
            match dispatch_tool(client, tool, &args).await {
                Ok(data) => Ok(json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&data)? }],
                    "isError": false
                })),
                Err(e) => Ok(json!({
                    "content": [{ "type": "text", "text": format!("Error: {e:#}") }],
                    "isError": true
                })),
            }
        }
        other => anyhow::bail!("method not found: {other}"),
    }
}

async fn handle_request(client: &VeynClient, req: RpcRequest) -> RpcResponse {
    let id = req.id.clone().unwrap_or(Value::Null);
    // one-way notifications — no response sent
    if req.id.is_none() && req.method.starts_with("notifications/") {
        return RpcResponse::notification();
    }
    match handle_method(client, &req.method, req.params).await {
        Ok(v) => RpcResponse::ok(id, v),
        Err(e) => {
            error!("{e:#}");
            let code = if e.to_string().contains("method not found") {
                -32601
            } else {
                -32000
            };
            RpcResponse::err(id, code, format!("{e:#}"))
        }
    }
}

async fn write_response(
    out: &mut tokio::io::BufWriter<tokio::io::Stdout>,
    resp: &RpcResponse,
) -> Result<()> {
    let mut line = serde_json::to_string(resp)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // All tracing goes to stderr — stdout is the MCP channel and must stay clean JSON.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "veyn_mcp=info".into()),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    let cli = Cli::parse();
    let token = resolve_token(cli.token, cli.no_auth);

    match &token {
        Some(_) => info!(url = %cli.url, "auth token loaded"),
        None if cli.no_auth => warn!(url = %cli.url, "auth disabled (--no-auth)"),
        None => warn!("no token found — set VEYN_TOKEN or run daemon first to generate one"),
    }

    let client = VeynClient::new(cli.url, token, cli.timeout)?;
    let mut reader = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::BufWriter::new(tokio::io::stdout());

    info!("veyn-mcp ready");

    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        debug!(len = line.len(), "rx");

        let req: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                error!("JSON-RPC parse error: {e}");
                let resp = RpcResponse::err(Value::Null, -32700, format!("parse error: {e}"));
                write_response(&mut out, &resp).await?;
                continue;
            }
        };

        let resp = handle_request(&client, req).await;
        if !resp.is_notification() {
            write_response(&mut out, &resp).await?;
        }
    }

    info!("stdin closed, exiting");
    Ok(())
}

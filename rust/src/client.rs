//! Raw-HTTP client for the Claude Messages API (there is no official Rust SDK).
//!
//! Wire shape is identical to the Python reference implementation:
//!   * `POST {base_url}/v1/messages`, headers `x-api-key`, `anthropic-version: 2026-10-21`,
//!     `anthropic-beta: server-side-fallback-2026-07-01` (for `"fallbacks": "default"`).
//!   * `model: claude-sonnet-5-5` by default (Opus 5.5 via `--model`), adaptive thinking, explicit `effort`.
//!   * Structured outputs: `output_config.format = {type: json_schema, schema: <response schema>}`.
//!   * Streaming (`stream: true`) because thinking + a large JSON document can take minutes.
//!   * Server tools `web_search_20260209` / `web_fetch_20260209` when `allow_web_research`.
//!   * `pause_turn` resumed by echoing the partial assistant turn; `refusal` / `max_tokens` surfaced.
//!   * System prompt first with `cache_control` so repeated calls hit the prompt cache.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine;

use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::checks::consistency_warnings;
use crate::models::{validate_ranges, AnalysisRequest, AnalysisResponse, ImageAttachment};
use crate::sse::{Message, MessageAccumulator, Progress, SseParser, StreamError};
use std::sync::Arc;

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5"; // user's choice; claude-opus-5-5 for the hardest names
pub const DEFAULT_EFFORT: &str = "high"; // Opus 5.5 defaults to "medium"; set explicitly
pub const DEFAULT_MAX_TOKENS: u32 = 64_000;
pub const API_VERSION: &str = "2026-10-21";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const MAX_PAUSE_RESUMES: usize = 6;
const MAX_ATTEMPTS: u32 = 4; // per HTTP request, for 429 / 5xx / connection errors

/// The system prompt and the response JSON Schema are compiled in from the repo root.
pub const SYSTEM_PROMPT: &str = include_str!("../../prompts/system_prompt.md");
pub const RESPONSE_SCHEMA_JSON: &str = include_str!("../../schema/analysis_response.schema.json");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("API error {status} ({error_type}): {message}{}", request_id.as_deref().map(|r| format!(" [request-id {r}]")).unwrap_or_default())]
    Api {
        status: u16,
        error_type: String,
        message: String,
        request_id: Option<String>,
    },
    #[error("stream error: {0}")]
    Stream(#[from] StreamError),
    #[error("Claude declined the request (category={category:?}): {explanation:?}")]
    Refused {
        category: Option<String>,
        explanation: Option<String>,
    },
    #[error("analysis incomplete: {0}")]
    Incomplete(String),
    #[error("response did not match AnalysisResponse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("response out of range: {0:?}")]
    Range(Vec<String>),
    #[error("invalid input: {0}")]
    Input(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub web_search_requests: u64,
    pub turns: u32,
}

impl Usage {
    fn add(&mut self, u: &serde_json::Map<String, Value>) {
        let g = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        self.input_tokens += g("input_tokens");
        self.output_tokens += g("output_tokens");
        self.cache_read_input_tokens += g("cache_read_input_tokens");
        self.cache_creation_input_tokens += g("cache_creation_input_tokens");
        self.web_search_requests += u
            .get("server_tool_use")
            .and_then(|s| s.get("web_search_requests"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        self.turns += 1;
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub response: AnalysisResponse,
    pub raw_json: String,
    pub model: String,
    pub usage: Usage,
    pub warnings: Vec<String>,
    pub elapsed: Duration,
    pub request_ids: Vec<String>,
}

/// Receives progress notices while a turn streams (searches, fetches, output growth).
pub type ProgressSink = Arc<dyn Fn(&Progress) + Send + Sync>;

/// A sink that logs each notice to stderr with a timestamp offset.
pub fn stderr_progress() -> ProgressSink {
    let start = Instant::now();
    Arc::new(move |p: &Progress| {
        let t = start.elapsed().as_secs();
        eprintln!("[{:>3}:{:02}] {p}", t / 60, t % 60);
    })
}

#[derive(Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub effort: String,
    pub max_tokens: u32,
    pub use_server_fallbacks: bool,
    pub show_thinking_summary: bool,
    pub system_prompt: String,
    /// false (default): the schema goes into the system prompt and the reply is validated
    /// client-side. true: also send it as `output_config.format` so the API constrains decoding.
    /// The API currently rejects this schema's grammar as too large; keep false until that is
    /// resolved (see README).
    pub enforce_schema: bool,
    /// Optional progress sink; `stderr_progress()` prints to stderr. None = silent.
    pub progress: Option<ProgressSink>,
    /// Ask the model to build and validate the JSON in the code-execution sandbox and hand it back
    /// as a file (downloaded via the Files API) instead of re-typing it as output text. Cuts output
    /// tokens by most of the document size and removes transcription risk. Falls back to the text
    /// block when no file comes back.
    pub sandbox_output: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("effort", &self.effort)
            .field("max_tokens", &self.max_tokens)
            .field("use_server_fallbacks", &self.use_server_fallbacks)
            .field("show_thinking_summary", &self.show_thinking_summary)
            .field("enforce_schema", &self.enforce_schema)
            .field("progress", &self.progress.is_some())
            .field("sandbox_output", &self.sandbox_output)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Reads `ANTHROPIC_API_KEY` (required) and `ANTHROPIC_BASE_URL` (optional).
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY is not set"))?;
        Ok(Self {
            api_key,
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| DEFAULT_BASE_URL.into()),
            model: DEFAULT_MODEL.into(),
            effort: DEFAULT_EFFORT.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
            use_server_fallbacks: true,
            show_thinking_summary: false,
            system_prompt: SYSTEM_PROMPT.into(),
            enforce_schema: false,
            progress: None,
            sandbox_output: true,
        })
    }
}

pub struct FundamentalsClient {
    http: reqwest::Client,
    cfg: Config,
    schema: Value,
}

pub const CODE_EXECUTION_TOOL: &str = "code_execution_20260521";

/// Instruction appended to the user message in sandbox-output mode.
pub const SANDBOX_OUTPUT_INSTRUCTION: &str = "Output delivery: build the final JSON object in the code-execution sandbox (compute every figure in Python, assemble the document, check it against the schema, and json.dump it to /tmp/analysis.json). Then run one bash command: cp /tmp/analysis.json \"$OUTPUT_DIR/\" && ls -l \"$OUTPUT_DIR\". The caller downloads that file. Your final reply must then be only the line FILE: analysis.json, with no JSON in the text. If the sandbox is unavailable, return the JSON object as text instead.";

pub fn web_tools() -> Vec<Value> {
    vec![
        json!({"type": "web_search_20260209", "name": "web_search", "max_uses": 30}),
        json!({"type": "web_fetch_20260209", "name": "web_fetch", "max_uses": 30, "max_content_tokens": 60000}),
    ]
}

/// The volatile text part of the prompt; the system prompt stays byte-identical so it caches.
pub fn build_user_message(req: &AnalysisRequest, today: &str) -> String {
    // Images are sent as separate blocks, never inside the JSON payload.
    let mut payload = serde_json::to_value(req).expect("request serialises");
    if let Some(o) = payload.as_object_mut() {
        o.remove("images");
        o.remove("previous_response");
    }
    let payload = serde_json::to_string_pretty(&payload).expect("request serialises");
    let mut s = format!(
        "Analysis date: {today}\nAnalyse the following request using the framework and return the structured result.\n\n```json\n{payload}\n```"
    );
    if let Some(prev) = &req.previous_response {
        let date = prev
            .get("analysis_date")
            .and_then(Value::as_str)
            .unwrap_or("unknown date");
        s.push_str(&format!(
            "\n\nThis is an incremental update. The previous analysis (as of {date}) follows; apply the incremental-update rules.\n\n```json\n{}\n```",
            serde_json::to_string(prev).expect("previous response serialises")
        ));
    }
    if !req.images.is_empty() {
        s.push_str(&format!(
            "\n\n{} screenshot(s) are attached before this text, in this order:",
            req.images.len()
        ));
        for (i, img) in req.images.iter().enumerate() {
            s.push_str(&format!("\n  {}. {}", i + 1, img.caption));
        }
        s.push_str("\nRead figures from them per the screenshot rules; register each as a supplied_screenshot source.");
    }
    if !req.allow_web_research && req.supplied_data.is_none() && req.images.is_empty() {
        s.push_str("\n\nWeb research is disabled and no data was supplied: use recall_only mode and cap reliability at 35.");
    }
    s
}

const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_IMAGES: usize = 100;

fn image_data(img: &ImageAttachment, base_dir: Option<&Path>) -> Result<String, Error> {
    if let Some(d) = &img.data {
        return Ok(d.clone());
    }
    let Some(p) = &img.path else {
        return Err(Error::Input(format!(
            "image '{}' has neither data nor path",
            img.caption
        )));
    };
    let mut path = PathBuf::from(p);
    if path.is_relative() {
        if let Some(b) = base_dir {
            path = b.join(path);
        }
    }
    let raw = std::fs::read(&path).map_err(|e| Error::Input(format!("{}: {e}", path.display())))?;
    if raw.len() > MAX_IMAGE_BYTES {
        return Err(Error::Input(format!(
            "{}: image exceeds the 5 MB API limit",
            path.display()
        )));
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(raw))
}

/// User-message `content`: a plain string without images, otherwise image blocks then the text block.
pub fn build_user_content(
    req: &AnalysisRequest,
    today: &str,
    base_dir: Option<&Path>,
) -> Result<Value, Error> {
    let text = build_user_message(req, today);
    if req.images.is_empty() {
        return Ok(Value::String(text));
    }
    if req.images.len() > MAX_IMAGES {
        return Err(Error::Input(format!(
            "at most {MAX_IMAGES} images per request"
        )));
    }
    let mut blocks = Vec::with_capacity(req.images.len() + 1);
    for img in &req.images {
        blocks.push(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": img.media_type, "data": image_data(img, base_dir)?}
        }));
    }
    blocks.push(json!({"type": "text", "text": text}));
    Ok(Value::Array(blocks))
}

impl FundamentalsClient {
    pub fn new(cfg: Config) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            // The API sends `ping` events during long thinking; a long idle gap means trouble.
            .read_timeout(Duration::from_secs(300))
            .build()?;
        let schema: Value = serde_json::from_str(RESPONSE_SCHEMA_JSON)?;
        let mut cfg = cfg;
        if !cfg.enforce_schema {
            cfg.system_prompt.push_str(
                "\n\n# Output schema\n\nReturn exactly one JSON object, no prose and no code fences, conforming to this JSON Schema:\n\n",
            );
            cfg.system_prompt.push_str(&serde_json::to_string(&schema)?);
        }
        Ok(Self { http, cfg, schema })
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Self::new(Config::from_env()?)
    }

    /// Exposed for tests: the exact JSON body sent to `/v1/messages`.
    pub fn build_body(&self, req: &AnalysisRequest, messages: &[Value]) -> Value {
        let mut body = json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_tokens,
            "stream": true,
            "system": [{
                "type": "text",
                "text": self.cfg.system_prompt,
                "cache_control": {"type": "ephemeral"}
            }],
            "output_config": {"effort": self.cfg.effort},
            "messages": messages,
        });
        if self.cfg.enforce_schema {
            body["output_config"]["format"] = json!({"type": "json_schema", "schema": self.schema});
        }
        if self.cfg.show_thinking_summary {
            body["thinking"] = json!({"type": "adaptive", "display": "summarized"});
        }
        let mut tools = if req.allow_web_research {
            web_tools()
        } else {
            vec![]
        };
        if self.cfg.sandbox_output {
            tools.push(json!({"type": CODE_EXECUTION_TOOL, "name": "code_execution"}));
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if self.cfg.use_server_fallbacks {
            body["fallbacks"] = json!("default");
        }
        body
    }

    fn beta_header(&self) -> Option<&'static str> {
        self.cfg
            .use_server_fallbacks
            .then_some("server-side-fallback-2026-07-01")
    }

    /// Run an analysis. Relative `images[].path` entries resolve against the process working directory.
    pub async fn analyze(&self, req: &AnalysisRequest) -> Result<AnalysisResult, Error> {
        let today = today_iso();
        self.analyze_dated(req, &today, None).await
    }

    /// `image_base_dir` resolves relative `images[].path` entries (the CLI passes the request file's dir).
    pub async fn analyze_dated(
        &self,
        req: &AnalysisRequest,
        today: &str,
        image_base_dir: Option<&Path>,
    ) -> Result<AnalysisResult, Error> {
        let started = Instant::now();
        let mut content = build_user_content(req, today, image_base_dir)?;
        if self.cfg.sandbox_output {
            append_text(&mut content, SANDBOX_OUTPUT_INSTRUCTION);
        }
        let mut messages = vec![json!({"role": "user", "content": content})];
        let mut usage = Usage::default();
        let mut request_ids = Vec::new();
        let mut final_msg: Option<Message> = None;

        let mut container: Option<String> = None;
        for _ in 0..=MAX_PAUSE_RESUMES {
            let mut body = self.build_body(req, &messages);
            if let Some(c) = &container {
                body["container"] = json!(c);
            }
            let (msg, rid) = self.stream_message(&body).await?;
            usage.add(&msg.usage);
            if let Some(r) = rid {
                request_ids.push(r);
            }
            if msg.stop_reason.as_deref() == Some("pause_turn") {
                if msg.container_id.is_some() {
                    container = msg.container_id.clone();
                }
                // Long server-tool turn paused: echo the partial assistant turn and resume.
                messages.push(json!({"role": "assistant", "content": msg.content}));
                continue;
            }
            final_msg = Some(msg);
            break;
        }
        let msg = final_msg.ok_or_else(|| {
            Error::Incomplete(format!(
                "turn still paused after {MAX_PAUSE_RESUMES} resumes"
            ))
        })?;

        match msg.stop_reason.as_deref() {
            Some("refusal") => {
                let d = msg.stop_details.as_ref();
                return Err(Error::Refused {
                    category: d
                        .and_then(|d| d.get("category"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    explanation: d
                        .and_then(|d| d.get("explanation"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
            Some("max_tokens") => {
                return Err(Error::Incomplete(
                    "output hit max_tokens; raise max_tokens or narrow the request".into(),
                ))
            }
            _ => {}
        }
        let mut json_text: Option<String> = None;
        if self.cfg.sandbox_output {
            for fid in file_ids_in(&msg.content) {
                if let Some(sink) = &self.cfg.progress {
                    sink(&Progress::ToolResult {
                        name: "sandbox file".into(),
                        detail: fid.clone(),
                    });
                }
                let (name, bytes) = self.download_file(&fid).await?;
                if name.ends_with(".json") {
                    json_text = Some(String::from_utf8_lossy(&bytes).into_owned());
                }
            }
        }
        let owned_text;
        let text: &str = match &json_text {
            Some(t) => t.as_str(),
            None => {
                owned_text = msg
                    .last_text()
                    .ok_or_else(|| {
                        Error::Incomplete(format!(
                            "no text block (stop_reason={:?})",
                            msg.stop_reason
                        ))
                    })?
                    .to_string();
                owned_text.as_str()
            }
        };
        let response: AnalysisResponse = serde_json::from_str(strip_fences(text))?;
        let range = validate_ranges(&response);
        if !range.is_empty() {
            return Err(Error::Range(range));
        }
        let raw_json = serde_json::to_string_pretty(&response)?;
        let mut warnings = consistency_warnings(&response);
        if !req.images.is_empty()
            && !response
                .sources
                .iter()
                .any(|x| x.source_type == crate::models::SourceType::SuppliedScreenshot)
        {
            warnings.push(format!(
                "{} screenshot(s) were attached but no supplied_screenshot source was registered",
                req.images.len()
            ));
        }
        Ok(AnalysisResult {
            response,
            raw_json,
            model: msg.model,
            usage,
            warnings,
            elapsed: started.elapsed(),
            request_ids,
        })
    }

    /// One streamed request with retries for 429 / 5xx / connection errors.
    async fn stream_message(&self, body: &Value) -> Result<(Message, Option<String>), Error> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match self.stream_once(body).await {
                Ok(ok) => return Ok(ok),
                Err(e) if attempt < MAX_ATTEMPTS && is_retryable(&e) => {
                    let backoff = retry_after(&e)
                        .unwrap_or_else(|| Duration::from_millis(500 * 2u64.pow(attempt)));
                    eprintln!("retrying after {:?}: {e}", backoff);
                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn stream_once(&self, body: &Value) -> Result<(Message, Option<String>), Error> {
        let url = format!("{}/v1/messages", self.cfg.base_url.trim_end_matches('/'));
        let mut rb = self
            .http
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", API_VERSION)
            .json(body);
        if let Some(b) = self.beta_header() {
            rb = rb.header("anthropic-beta", b);
        }
        let resp = rb.send().await?;
        let request_id = resp
            .headers()
            .get("request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let status = resp.status();
        if let Some(sink) = &self.cfg.progress {
            sink(&Progress::Connected {
                status: status.as_u16(),
                request_id: request_id.clone().unwrap_or_else(|| "-".into()),
            });
        }
        if !status.is_success() {
            let retry_hdr = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let text = resp.text().await.unwrap_or_default();
            let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            let mut message = v["error"]["message"].as_str().unwrap_or(&text).to_string();
            if let Some(s) = retry_hdr {
                message.push_str(&format!(" (retry-after {s}s)"));
            }
            return Err(Error::Api {
                status: status.as_u16(),
                error_type: v["error"]["type"].as_str().unwrap_or("unknown").to_string(),
                message,
                request_id,
            });
        }

        let mut parser = SseParser::default();
        let mut acc = MessageAccumulator::default();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            parser.push(&chunk?);
            while let Some(ev) = parser.next_event() {
                if let Some(progress) = acc.apply_with_progress(&ev)? {
                    if let Some(sink) = &self.cfg.progress {
                        sink(&progress);
                    }
                }
            }
        }
        if !acc.is_done() {
            return Err(Error::Incomplete("stream ended before message_stop".into()));
        }
        Ok((acc.message, request_id))
    }
}

/// Tolerate a ```json fence when the schema is not API-enforced.
fn strip_fences(text: &str) -> &str {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.split_once('\n').map(|(_, body)| body).unwrap_or("");
        return rest.trim_end().strip_suffix("```").unwrap_or(rest).trim();
    }
    t
}

/// Outcome of a schema probe: whether the API compiled the grammar for this schema.
#[derive(Debug, Clone)]
pub enum ProbeOutcome {
    Accepted { input_tokens: u64 },
    Rejected { message: String },
}

impl FundamentalsClient {
    /// Send a minimal non-streaming request (`max_tokens: 1`) whose only purpose is to make the API
    /// compile the structured-output grammar for `schema`. A rejection costs nothing; an accepted
    /// probe costs a few hundred input tokens.
    pub async fn probe_schema(&self, schema: &Value) -> Result<ProbeOutcome, Error> {
        let url = format!("{}/v1/messages", self.cfg.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.cfg.model,
            "max_tokens": 1,
            "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": schema}},
            "messages": [{"role": "user", "content": "ping"}],
        });
        let resp = self
            .http
            .post(&url)
            .header("content-type", "application/json")
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(ProbeOutcome::Accepted {
                input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            });
        }
        let message = v["error"]["message"].as_str().unwrap_or(&text).to_string();
        if status.as_u16() == 400 && message.to_lowercase().contains("grammar") {
            return Ok(ProbeOutcome::Rejected { message });
        }
        Err(Error::Api {
            status: status.as_u16(),
            error_type: v["error"]["type"].as_str().unwrap_or("unknown").to_string(),
            message,
            request_id: None,
        })
    }
}

impl FundamentalsClient {
    /// Download a Files API file: returns (filename, bytes).
    async fn download_file(&self, file_id: &str) -> Result<(String, Vec<u8>), Error> {
        let base = self.cfg.base_url.trim_end_matches('/');
        let meta = self
            .http
            .get(format!("{base}/v1/files/{file_id}"))
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await?;
        let name = if meta.status().is_success() {
            meta.json::<Value>()
                .await
                .ok()
                .and_then(|v| v["filename"].as_str().map(str::to_string))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let resp = self
            .http
            .get(format!("{base}/v1/files/{file_id}/content"))
            .header("x-api-key", &self.cfg.api_key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::Api {
                status: resp.status().as_u16(),
                error_type: "file_download".into(),
                message: format!("could not download {file_id}"),
                request_id: None,
            });
        }
        Ok((name, resp.bytes().await?.to_vec()))
    }
}

/// Every `file_id` that appears in code-execution tool results, in order of appearance.
pub fn file_ids_in(content: &[Value]) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                if let Some(id) = o.get("file_id").and_then(Value::as_str) {
                    if !out.iter().any(|x| x == id) {
                        out.push(id.to_string());
                    }
                }
                o.values().for_each(|c| walk(c, out));
            }
            Value::Array(a) => a.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for block in content {
        let t = block.get("type").and_then(Value::as_str).unwrap_or("");
        if t.starts_with("bash_code_execution")
            || t.starts_with("code_execution")
            || t.starts_with("text_editor_code_execution")
        {
            walk(block, &mut out);
        }
    }
    out
}

fn append_text(content: &mut Value, extra: &str) {
    match content {
        Value::String(s) => {
            s.push_str("\n\n");
            s.push_str(extra);
        }
        Value::Array(blocks) => {
            if let Some(last) = blocks
                .iter_mut()
                .rev()
                .find(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            {
                if let Some(Value::String(t)) = last.get_mut("text") {
                    t.push_str("\n\n");
                    t.push_str(extra);
                }
            }
        }
        _ => {}
    }
}

fn is_retryable(e: &Error) -> bool {
    match e {
        Error::Transport(err) => err.is_connect() || err.is_timeout() || err.is_request(),
        Error::Api { status, .. } => {
            *status == StatusCode::TOO_MANY_REQUESTS.as_u16()
                || *status == StatusCode::REQUEST_TIMEOUT.as_u16()
                || *status == StatusCode::CONFLICT.as_u16()
                || *status >= 500
        }
        Error::Stream(StreamError::ApiError(v)) => v["type"].as_str() == Some("overloaded_error"),
        Error::Incomplete(msg) => msg.contains("before message_stop"),
        _ => false,
    }
}

fn retry_after(e: &Error) -> Option<Duration> {
    if let Error::Api { message, .. } = e {
        let s = message.rsplit("(retry-after ").next()?.strip_suffix("s)")?;
        return s
            .parse::<u64>()
            .ok()
            .map(|n| Duration::from_secs(n.min(120)));
    }
    None
}

/// UTC date as YYYY-MM-DD without pulling in a date crate.
pub fn today_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

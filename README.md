# claude-fundamentals

Structured fundamental analysis of a listed company through the Claude API, following the
[Investment Analysis Framework](docs/investment_analysis_framework.md). One request in
(symbol, date range, subset of fundamentals), one JSON document out, with a **reliability
percentage on every measure, every data point, every trend, every pillar and the final signal**.

Two implementations share the same prompt, schema and wire format:

| | Path | Use it for |
|---|---|---|
| **Rust** (raw HTTP, `reqwest` + `serde`) | `rust/` | your application |
| **Python** (official `anthropic` SDK) | `fundamentals_client/` | reference implementation, schema export, quick experiments |

---

## 1. Architecture decision: output format

**JSON.** Not YAML, not TOML. Reasons, in order of weight:

1. **The API can enforce JSON, and only JSON.** Structured outputs (`output_config.format` with a
   JSON Schema) make the model's reply guaranteed-parseable and schema-conformant. There is no
   equivalent for YAML or TOML; you would be back to "ask nicely and parse defensively".
2. **The data is nested arrays of records** (measures containing series of data points). TOML is
   poor at this (arrays of tables get verbose and ordering-sensitive). YAML handles it but adds
   nothing JSON lacks here.
3. **YAML's implicit typing is a liability for financial data** (`NO`, `1e3`, `0.80`, dates,
   unquoted tickers like `ON` or `TRUE` get coerced).
4. **Every Rust/Python/TS stack has a strict JSON parser**; serde's `deny_unknown_fields` plus the
   checked-in schema gives you compile-time types and runtime validation on both ends.

If a consumer wants YAML or TOML (config files, human review), convert client-side after
validation. The Python CLI has `--yaml` for that.

## 2. What the contract looks like

**Request** (`schema/analysis_request.schema.json`):

```json
{
  "symbol": "ASML",
  "exchange": "Euronext Amsterdam",
  "reporting_currency": "EUR",
  "period": { "start": "2016-01-01", "end": "2025-12-31" },
  "frequency": "annual",
  "categories": ["valuation", "profitability", "leverage", "cash_flow_quality", "growth"],
  "measures": null,
  "peer_symbols": ["AMAT", "LRCX", "KLAC"],
  "sector_hint": "semiconductor capital equipment",
  "allow_web_research": true,
  "supplied_data": null,
  "notes": null
}
```

* `categories` selects framework sections 1.1 to 1.8; `measures` optionally whitelists individual
  measure ids (the catalogue is the `MeasureId` enum in `fundamentals_client/models.py` and
  `rust/src/models.rs`).
* `supplied_data` is free-form JSON from your own data provider. When present it is the preferred
  source and reliabilities go to the 95-100 band. See `examples/request_supplied_data.json`.
* `allow_web_research` (default true, **the primary data mode for this project**) lets Claude use
  Anthropic's server-side `web_search` / `web_fetch` tools to find filings and aggregator pages.
* `images` attaches screenshots, typically Interactive Brokers TWS or Client Portal fundamentals and
  ownership pages. Each entry has a `media_type`, a `path` (resolved relative to the request file by
  the CLIs) or base64 `data`, and a `caption` saying what the page is and when it was captured. The
  images travel as image blocks ahead of the request text, never inside the JSON. Limits: PNG, JPEG,
  GIF or WebP, 5 MB each, 100 per request; keep the long edge at or below 1568 px. See
  `examples/request_with_screenshots.json`. Screenshots are the practical route to IB's ownership
  and insider data, which the framework notes is hard to get by API.

**Response** (`schema/analysis_response.schema.json`, enforced by the API). Top level:

```
schema_version, symbol, company_name, analysis_date, data_mode (supplied|retrieved|mixed|recall_only),
sources[]        id, type (primary_filing|company_ir|aggregator|supplied_input|supplied_screenshot|web_other|model_recall), title, url, period_covered, retrieved_at  ("" when n/a)
measures[]       measure_id, category, label, unit, currency, scale,
                 latest {period_end, period_type, value|null, reliability_pct, method, source_ids[]},
                 series[] (same shape, oldest first),
                 trend {direction, cagr_pct, latest_yoy_change_pct, coefficient_of_variation, vs_own_history, vs_peers, reliability_pct, comment},
                 reliability_pct, reliability_basis, notes
ownership        requested, top_holders[], recent_insider_activity, reliability_pct, notes   (requested=false when not asked for)
context          requested, industry, peer_set_used, segment_notes, ..., reliability_pct   (requested=false when not asked for)
pillars[]        valuation | profitability_stability | debt_health | earnings_quality | trends:
                 assessment, score_0_100, key_measures[], rationale, reliability_pct
trend_summary    overall_direction, improving[], stable[], deteriorating[], inflections[], operating_vs_non_operating, reliability_pct
signal           category (extreme_buy|buy|hold|sell|extreme_sell), conviction_pct, reliability_pct, rationale,
                 green_flags[], red_flags[], conditions_for_upgrade[], conditions_for_downgrade[]
sector_adjustments[], data_gaps[], overall_reliability_pct, disclaimer
```

A complete, rule-conformant example is `examples/sample_response.json` (hand-built from the
supplied-data request by `examples/make_sample_response.py`, not model output).

### Reliability: what the percentage means

Reliability is **provenance and arithmetic certainty**, not the probability that the signal makes
money (that is `signal.conviction_pct`, how strongly the framework's criteria are met). The rubric
lives in `prompts/system_prompt.md`:

| Band | Meaning |
|---|---|
| 95-100 | Read directly from `supplied_data` or a primary filing, period matches, units unambiguous |
| 80-94 | Derived arithmetically from 95-100 inputs, one reputable aggregator corroborated by a second source, or a legible dated screenshot of a reputable terminal such as IB |
| 60-79 | Derived with one documented assumption, or a single aggregator |
| 40-59 | Stale, conflicting sources resolved by judgement, or denominator itself 60-79 |
| 1-39 | From memory or extrapolated (`method: estimated`) |
| 0 | Value is null |

Aggregation rules (derived <= min of inputs; trend <= median of its series; pillar <= mean of key
measures; signal <= mean of assessed pillars and <= lowest assessed pillar + 20) are stated in the
prompt and **re-checked client-side** by `consistency_warnings` in both implementations. Violations
are reported as warnings, never silently rewritten, so you can log them and decide.

Two honest caveats:

* A model's self-assessed reliability is a structured judgement, not a calibrated probability.
  The rubric makes it *consistent*; only your own back-testing (compare `value` against figures you
  later verify, bucketed by `reliability_pct`) makes it *calibrated*. Log every response.
* Without `supplied_data` or web research the model is working from memory and the prompt caps
  everything at 35 %. Treat `data_mode: recall_only` output as a sketch.

## 3. How the call is made (both languages, identical wire shape)

```
POST https://api.anthropic.com/v1/messages
x-api-key: $ANTHROPIC_API_KEY
anthropic-version: 2026-10-21
anthropic-beta: server-side-fallback-2026-07-01

{
  "model": "claude-opus-5-5",
  "max_tokens": 64000,
  "stream": true,
  "system": [{"type": "text", "text": "<prompts/system_prompt.md>", "cache_control": {"type": "ephemeral"}}],
  "output_config": {"effort": "high", "format": {"type": "json_schema", "schema": <schema/analysis_response.schema.json>}},
  "tools": [{"type": "web_search_20260209", "name": "web_search", "max_uses": 15},
            {"type": "web_fetch_20260209", "name": "web_fetch", "max_uses": 10, "max_content_tokens": 60000}],
  "fallbacks": "default",
  "messages": [{"role": "user", "content": "Analysis date: ...\n```json\n<request>\n```"}]
}
```

Why each piece:

* **`claude-opus-5-5`, `effort: high`.** Thinking is always on for this model; effort is the depth
  control and its default is `medium`, so it is set explicitly. Use `xhigh` if you find gaps in
  reasoning, `medium` for cheaper screening runs.
* **Streaming.** Thinking plus a large JSON document can take minutes; non-streaming requests risk
  HTTP timeouts. The Rust client parses the SSE events itself (`rust/src/sse.rs`).
* **Structured outputs.** The checked-in schema is sent on every call. The first call with a new
  schema pays a one-time compilation cost; after that it is cached for 24 h. The API compiles the
  schema into a decoding grammar with hard limits: at most 16 union-typed fields (any `anyOf`,
  which is what every nullable `X | null` becomes) and 24 optional fields per request, and unions
  compile exponentially. The first live runs hit the limit (`400 The compiled grammar is too
  large`). The schema is therefore built to a rule, enforced by tests in both languages: **only five
  numeric fields are nullable** (a data point's `value`, the three trend statistics, a holder's
  `pct_of_shares`); absent text is the empty string; `ownership` and `context` are always present
  with a `requested` flag; `measure_id` is a plain string on the wire with the catalogue in the
  prompt and membership checked client-side. If the limit is ever hit again, pass
  `--no-schema-enforcement` (both CLIs; `enforce_schema=false` / `Config.enforce_schema`): the
  schema then goes into the system prompt instead of `output_config.format`, and the reply is
  validated client-side only.
* **Prompt caching.** The system prompt is ~2.5k tokens and byte-identical across calls, so it is
  marked for caching and the volatile request goes in the user message after it. Check
  `usage.cache_read_input_tokens` on the second call.
* **Server tools.** `web_search_20260209` / `web_fetch_20260209` run on Anthropic's side; nothing to
  implement. A long research turn may stop with `stop_reason: pause_turn`; both clients echo the
  partial turn back and resume (bounded).
* **`fallbacks: "default"` (beta).** If a safety classifier declines the request, the API re-runs it
  on Anthropic's recommended fallback model inside the same call. Both clients still check for
  `stop_reason: refusal` and raise a typed error. Pass `--no-fallbacks` to disable.
* **No `temperature` / `top_p` / `top_k`.** API version `2026-10-21` rejects them.

Rough cost per analysis at `high` effort with web research: 100k to 300k input tokens (mostly
fetched pages, partly cached) and 15k to 40k output tokens, roughly USD 1 to 2 at Opus 5.5 list
prices, plus web search at USD 10 per 1,000 searches. With `supplied_data` and no web research,
expect a fraction of that. Both CLIs print the token counts after each run.

## 4. Install and run: Rust

Requirements: Rust 1.75+ (edition 2021), network access to `api.anthropic.com`.

```bash
cd rust
cargo build --release
cargo test                                  # offline tests, no key needed

export ANTHROPIC_API_KEY=...                # see "API key handling" below
./target/release/fundamentals analyze ../examples/request_asml.json --out ../out/asml.json
./target/release/fundamentals analyze ../examples/request_supplied_data.json --effort medium
./target/release/fundamentals analyze ../examples/request_with_screenshots.json   # after dropping PNGs in examples/screenshots/
./target/release/fundamentals check ../out/asml.json     # re-validate + consistency warnings, offline
```

Use it as a library in your app (`Cargo.toml`: `fundamentals-client = { path = "../claude-fundamentals/rust" }`):

```rust
use fundamentals_client::{AnalysisRequest, Category, FundamentalsClient, Signal};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = FundamentalsClient::from_env()?;          // ANTHROPIC_API_KEY, optional ANTHROPIC_BASE_URL

    let mut req = AnalysisRequest::new("ASML", "2016-01-01", "2025-12-31");
    req.categories = vec![Category::Valuation, Category::Profitability, Category::Leverage];
    req.peer_symbols = vec!["AMAT".into(), "LRCX".into()];
    req.images.push(fundamentals_client::ImageAttachment {
        media_type: fundamentals_client::models::ImageMediaType::Png,
        path: Some("/data/ib/asml_ownership.png".into()),
        data: None,
        caption: "IB TWS > Fundamentals > Ownership, captured 2026-10-20".into(),
    });

    let result = client.analyze(&req).await?;              // Result<AnalysisResult, fundamentals_client::Error>
    let r = &result.response;                              // typed AnalysisResponse
    println!("{:?} conviction={}% reliability={}%", r.signal.category, r.signal.conviction_pct, r.signal.reliability_pct);
    for m in &r.measures {
        println!("{:?}: latest={:?} ({}%) trend={:?}", m.measure_id, m.latest.value, m.latest.reliability_pct, m.trend.direction);
    }
    for w in &result.warnings { eprintln!("consistency: {w}"); }
    std::fs::write("asml.json", &result.raw_json)?;        // the JSON document, validated
    if matches!(r.signal.category, Signal::ExtremeBuy | Signal::ExtremeSell) { /* escalate */ }
    Ok(())
}
```

**Runtime and progress.** A web-research run at `high` effort typically takes 10 to 20 minutes:
searches, fetching annual reports, thinking, then 15k to 40k tokens of JSON. The connection stays
alive through the API's ping events. Both CLIs log progress to stderr by default (`--quiet` turns
it off): turn start, each search query and fetched URL, result counts, output size every 2k
characters, and the stop reason. In the Rust library set `Config.progress` to
`fundamentals_client::stderr_progress()` or your own `Arc<dyn Fn(&Progress)>`; in Python pass
`progress=callable` to `FundamentalsClient`. A typical log:

```
starting analysis of ASML (effort high, web research true)
[  0:01] turn started (claude-opus-5-5, msg_01...)
[  0:01] thinking...
[  0:24] web_search: ASML 2024 annual report consolidated income statement
[  0:29] web_search returned 10 results
[  0:41] web_fetch: https://www.asml.com/.../annual-report-2024.pdf
[  1:38] web_fetch returned https://www.asml.com/.../annual-report-2024.pdf
...
[ 9:12] writing output: 2000 chars
[11:40] turn ended: end_turn (23810 output tokens)
```

Error variants to match on: `Error::Refused { category, explanation }`, `Error::Incomplete`
(max_tokens or a turn that never completed), `Error::Api { status, .. }` (4xx/5xx after retries),
`Error::Transport`, `Error::Parse`, `Error::Range`. 429 / 5xx / connection errors are retried up to
four times with backoff, honouring `retry-after`.

The crate embeds `prompts/system_prompt.md` and `schema/analysis_response.schema.json` at compile
time (`include_str!`), so keep the directory layout or adjust the two paths in `rust/src/client.rs`.

## 5. Install and run: Python

```bash
python -m venv .venv && . .venv/bin/activate
pip install -e ".[yaml,dev]"               # or: pip install -r requirements.txt
pytest                                     # offline tests

export ANTHROPIC_API_KEY=...
python -m fundamentals_client analyze examples/request_asml.json --out out/asml.json --yaml
python -m fundamentals_client analyze examples/request_supplied_data.json --effort medium
python -m fundamentals_client check out/asml.json
python -m fundamentals_client schema       # regenerate schema/*.json from the Pydantic models
```

From code: `FundamentalsClient().analyze(AnalysisRequest(...))` returns an `AnalysisResult` with
`.response` (Pydantic `AnalysisResponse`), `.raw_json`, `.usage`, `.warnings`.

**The Pydantic models are the source of truth for the schema.** If you change a field, run
`python -m fundamentals_client schema`, mirror the change in `rust/src/models.rs`, and run both
test suites (`tests/test_models.py` and `rust/tests/offline.rs` both validate the shared examples).

## 6. API key handling

* Create the key in the Anthropic Console in a **dedicated workspace** for this app, with a monthly
  spend limit. Keys are shown once; store them in your secret manager (Vault, AWS Secrets Manager,
  1Password, Kubernetes Secret, systemd `LoadCredential`), never in source control or `Cargo.toml`.
* Inject it as the `ANTHROPIC_API_KEY` environment variable at process start. Both clients read
  only that variable (plus optional `ANTHROPIC_BASE_URL`). `.env` files are fine for local work and
  are git-ignored here.
* Don't log request headers; the clients log `request-id` values instead, which is what Anthropic
  support asks for.
* Rotate on any suspicion of leakage; the Console lets you revoke a key instantly.
* For the Claude Code cloud environment this repository was developed in: open the environment
  menu in the session title bar, choose Edit, and add `ANTHROPIC_API_KEY` under **Network secrets**
  (older app versions label it API credentials). That section stores it as a secret; the plain
  environment-variables field is the one the app discourages for keys. New sessions pick it up.
  Don't paste the key into chat. If you prefer to keep the key off the cloud environment entirely,
  run the trial on your own machine from this branch instead.

## 7. Repository layout

```
docs/investment_analysis_framework.md   the framework this is built on (unchanged)
prompts/system_prompt.md                the system prompt (framework + rubric + computation rules + output discipline)
schema/analysis_request.schema.json     request contract
schema/analysis_response.schema.json    response contract exactly as sent to the API
fundamentals_client/                    Python reference (models.py = schema source of truth, client.py, __main__.py CLI)
rust/                                   Rust crate: models.rs, sse.rs, client.rs, checks.rs, main.rs CLI, tests/offline.rs
examples/                               three requests + make_sample_response.py + sample_response.json
tests/                                  Python offline tests
```

## 8. Status and open items

Verified in this repository without an API key: both clients build, pass their offline tests, and
produce byte-for-byte the intended request body (checked by capturing the Python SDK's outgoing
request and asserting the Rust body against the same shape). **Not yet verified: a live call.**
Expect at most prompt-level tuning after the first real runs, not structural changes.

Open decisions for you:

1. **Data source.** Decided: web research is primary, IB screenshots supplement it (ownership and
   insider data above all). `supplied_data` remains available if a licensed feed is added later and
   lifts reliabilities into the 95-100 band.
2. **Peer sets and sector thresholds.** The prompt lets Claude pick peers when you give none; a
   fixed peer list per symbol is more reproducible.
3. **Calibration logging.** Store every response with its `reliability_pct` values so you can later
   compare against verified figures and tune the rubric bands.
4. **Effort level.** Start at `high`; try `medium` for screening many symbols and `xhigh` for the
   names you act on.

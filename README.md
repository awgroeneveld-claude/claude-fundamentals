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
  "frequency": "both",
  "recent_quarters": 8,
  "categories": ["valuation", "profitability", "leverage", "cash_flow_quality", "growth"],
  "measures": null,
  "peer_symbols": ["AMAT", "LRCX", "KLAC"],
  "sector_hint": "semiconductor capital equipment",
  "allow_web_research": true,
  "supplied_data": null,
  "notes": null
}
```

* Valuation multiples (P/E, P/B, P/S, EV/EBITDA, EV/Sales, FCF yield, dividend yield) come with a
  year-by-year history: the price at each fiscal year-end over the trailing figures as of that date,
  so their `trend` shows re-rating or de-rating and `pe_historical_percentile` places today's
  multiple in that range. The report charts each multiple on its own axis.
* `frequency` defaults to `both`, which is the framework's approach: annual series for every measure
  over the whole period (5-10 years), plus quarterly points for the most recent `recent_quarters`
  (default 8) on the core flow and balance measures (revenue, margins, net income, EPS, operating
  and free cash flow, total and net debt, cash) and on trailing P/E (quarter-end price over
  trailing four-quarter EPS). Trend statistics are computed on the annual points;
  the quarters feed the trend comment, inflection detection and the latest value. `annual` and
  `quarterly` give one kind only, over the whole period. The report charts annual and quarterly
  points separately so a quarter never sits on an annual axis.
* `reporting_currency` is a hint; the statements' own currency wins. `base_currency` (optional) asks
  for an extra converted view, see "Currencies" below.
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
  Two traps the prompt handles: IB's fundamentals pages show LSEG data in USD millions whatever
  the reporting currency (the model converts back or, better, uses the filing), and IB's market
  cap for dual-class companies can cover only the listed class (Porsche SE's 3.776B is the
  preference shares only). `examples/request_pah3.json` carries a real example screenshot.

**Response** (`schema/analysis_response.schema.json`, enforced by the API). Top level:

```
schema_version, symbol, company_name, analysis_date, data_mode (supplied|retrieved|mixed|recall_only),
sources[]        id, type (primary_filing|company_ir|aggregator|supplied_input|supplied_screenshot|web_other|model_recall), title, url, period_covered, retrieved_at  ("" when n/a)
measures[]       measure_id, category, label, unit, currency, scale,
                 latest {period_end, period_type, value|null, value_base|null, reliability_pct, method, source_ids[]},
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
signal_history[] period_end, period_type, category, conviction_pct, reliability_pct, valuation_assessed,
                 pillar_scores {valuation, profitability_stability, debt_health, earnings_quality, trends}, decisive_factor, source_ids[]
currency_notes, fx {reporting_currency, base_currency, rates[] {period_end, period_type, average_rate, closing_rate, source_ids[], reliability_pct}, method_note}
sector_adjustments[], data_gaps[], overall_reliability_pct, disclaimer
```

A complete, rule-conformant example is `examples/sample_response.json` (hand-built from the
supplied-data request by `examples/make_sample_response.py`, not model output).

### Signal over time

`signal_history` (request flag `signal_history`, default true) is the framework applied as of each
fiscal year-end in the period, plus the latest TTM or quarter when it is later: at each date only
figures for periods ending on or before it, trailing multiples from the share price on that date,
trends from the series up to that date. Each point carries the category, conviction, reliability,
the five pillar scores and a one-sentence decisive factor, so you can see when the verdict changed
and why. Where no historical price can be sourced for a date the valuation pillar is marked
unassessed and the extreme categories are unavailable for that point. Two caveats, stated in the
prompt: it is a retrospective application of the framework to reported figures, not what an
analyst knew at the time (no consensus estimates or guidance), and reliability is usually lower
for older points. The report shows the series as a table, a step chart of the category, and charts
of conviction/reliability and pillar scores over time. The current `signal` remains the as-of-today
verdict.

### Currencies

One standard, one optional extra:

* **Every amount, series and trend is in the company's reporting currency**, the currency of its
  financial statements. Historical series are never converted: converting at spot or average rates
  would turn FX moves into fake margin, growth and leverage trends. Both clients warn if a response
  mixes currency codes across measures.
* **Price-side figures are the one exception.** Share price, market cap and enterprise value come
  from the listing named by `exchange` and are converted into the reporting currency when that
  listing trades in another currency (an ADR, or a USD reporter listed in EUR), so valuation
  multiples compare like with like. The rate, its date and its source are registered as a `Source`,
  and `currency_notes` states what was done.
* **Optional base-currency view.** Set `base_currency` in the request (say `EUR` for a euro-based
  portfolio) and every currency or per-share data point also carries `value_base`, converted with
  the accounting-correct rate: flows at the period's average rate, balance-sheet items at the
  closing rate, point-in-time items at spot. The rates themselves are returned in `fx.rates`, one
  row per period with sources, so your app can audit or recompute them. Ratios, percentages and
  multiples are never converted. Without `base_currency`, `value_base` is null everywhere and
  `fx.rates` is empty; the clients check both directions for consistency.

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
  "model": "claude-sonnet-5-5",
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

* **`claude-sonnet-5-5`, `effort: high`.** Sonnet 5.5 is the default at $2/$10 per million tokens;
  pass `--model claude-opus-5-5` ($4/$20) for names where you want the deeper model, and compare
  on a few symbols before settling. Thinking is adaptive on both; effort is the depth control and is
  set explicitly. Use `xhigh` if you find gaps in reasoning, `medium` for cheaper screening runs.
* **Streaming.** Thinking plus a large JSON document can take minutes; non-streaming requests risk
  HTTP timeouts. The Rust client parses the SSE events itself (`rust/src/sse.rs`).
* **Schema: prompt-embedded by default, API-enforced as an option.** The checked-in JSON Schema is
  appended to the system prompt and every reply is validated client-side against it (serde with
  unknown fields rejected in Rust, Pydantic in Python). The first live run produced a 40 KB
  response that parsed cleanly this way. Passing `--enforce-schema` (`enforce_schema=true`) also
  sends the schema as `output_config.format`, which makes the API constrain decoding to it. That
  path currently fails with `400 The compiled grammar is too large` even after the schema was
  reduced to five nullable fields and no optional ones (the documented limits are 16 union-typed
  and 24 optional fields, with undocumented internal size limits on top). Until that is resolved
  with Anthropic support, keep the default. The schema is still kept within the documented
  budget by tests in both languages so enforcement can be switched on without changes.
* **Sandbox output (default).** The first live run showed the model doing all arithmetic in the
  code-execution sandbox that the web tools provide, then re-typing the 86 KB JSON as output text:
  130k output tokens, of which the document itself was about two thirds, plus the risk of a typo
  in a retyped number. Both clients now declare `code_execution_20260521` explicitly and ask the
  model to build and validate the JSON in the sandbox, copy it to `$OUTPUT_DIR`, and reply with
  one line. The file comes back with a `file_id` in the tool result and the client downloads it
  through the Files API. Code execution is free when the web tools are in the request. If no file
  comes back the client falls back to parsing the text block. `--no-sandbox-output` restores the
  typed-output path. **Not yet verified live**; the first run after this change should be watched.
* **Long turns resume in the same sandbox.** A research turn can stop with `pause_turn`; both
  clients resend the partial turn together with the `container` id from the paused response, so
  the files and Python state the model built survive. (The Porsche SE run lost its sandbox at the
  pause and rebuilt it, costing about 25k output tokens.) Web search and fetch are capped at 30
  uses each per request; the earlier caps of 15 and 10 ran out on a nine-year holding-company run.
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
fetched pages, partly cached) and 15k to 40k output tokens, roughly USD 0.5 to 1 at Sonnet 5.5 list prices (double on Opus 5.5), plus web search at USD 10 per 1,000 searches. With `supplied_data` and no web research,
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
./target/release/fundamentals analyze ../examples/request_volcar_b.json --out ../out/volcar_b.json --report
./target/release/fundamentals report ../out/volcar_b.json    # Markdown + SVG charts from a saved JSON
./target/release/fundamentals report ../out/volcar_b.json --charts mermaid --out ../out/volcar_b_mermaid.md
```

`examples/request_volcar_b.json` is a full request for Volvo Car AB (Nasdaq Stockholm: VOLCAR B): all
eight categories, annual plus eight quarters, signal history, a EUR base-currency view, and notes
on the 2021 listing (no valuation before it), the Polestar equity-accounted results and impairments
under non-operating items, and Geely's majority ownership. `examples/request_vow3.json` does the
same for Volkswagen preference shares, with notes on the ordinary/preference split and the
captive financial-services debt. `examples/request_pah3.json` covers Porsche Automobil Holding SE (XETRA: PAH3), a
holding company whose assets are equity-accounted stakes in Volkswagen and Porsche AG: the notes
reinterpret the framework (equity-accounted result as the core result, impairments as one-time
items, margins not meaningful, leverage and cash-flow quality at holding level, and the NAV
discount as the decisive valuation measure, reported in context since the catalogue has no id
for it).

### Markdown report

`fundamentals report <response.json>` (or `analyze --report`) renders the JSON as a Markdown
document with charts:

* signal with conviction and reliability, rationale, green and red flags, and the conditions that
  would move it up or down;
* pillar scorecard; trend summary with inflections and the operating vs non-operating split;
* one table per category with latest value, period, trend direction, CAGR, year-on-year change,
  position vs own history and peers, reliability and method, followed by the charts, followed by
  collapsible per-measure series tables (value, base-currency value when requested, reliability,
  method, source ids) and notes;
* ownership and context when requested, currency notes and the FX table, sector adjustments, data
  gaps, the source list with links, consistency warnings, and the disclaimer.

Charts are static SVG files written to `<name>_charts/` next to the Markdown and referenced by
relative path, so they render in any Markdown viewer, GitHub included. `--charts mermaid` embeds
Mermaid `xychart-beta` blocks instead (single file; renders on GitHub, GitLab, Obsidian, VS Code),
one chart per series since that chart type has no legend. `--charts none` omits them. Measures are
grouped per chart by unit, at most four series per chart: all percentages of a category together,
currency amounts with the same currency and scale together, ratios and multiples one per chart.
Measures with fewer than two data points are not charted. Line charts only, one y-axis per chart,
in the reporting currency; the base-currency view is in the series tables.

For your own app the same is available as a library call: `render_report(&response,
&ReportOptions { charts, chart_dir, warnings })` returns the Markdown and the chart files.

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

## 6. Quarterly update workflow

The intended operation is one run per symbol per quarter, after the new interim or annual report
is out, **adding to the stored document rather than regenerating it**. Two modes:

* **Full run** (no previous result): everything is researched from scratch. For the first analysis
  of a symbol, or when you change the prompt and want a clean baseline.
* **Incremental run** (`--previous last.json` on the CLI, `previous_response` in the request): the
  model receives the stored analysis and returns a **delta**: only series points for periods that
  end after the ones it already has, plus explicitly restated points (latest fiscal year and two
  quarters are re-verified against the newest filing), refreshed point-in-time items (price,
  multiples, ownership), and complete recomputed trends, pillars, signal and new `signal_history`
  points. The client then merges the delta into the stored document deterministically
  (`merge_incremental` in both languages): points are keyed by period end and type, a restated
  point replaces the old one, everything else is untouched, the quarterly window is trimmed to
  `recent_quarters`, sources and signal-history points are appended, and the verdict blocks are
  replaced. `signal.rationale` states what changed since last time. If nothing new has been
  published the delta says so in `data_gaps` and the merged document only gains fresh
  point-in-time values. Output tokens drop to a fraction of a full run, and history can only
  change through a named restatement.

```bash
# first run
./target/release/fundamentals analyze req.json --out store/VOLCAR_B/2026-10-25.json --report
# next quarter: adds to the previous document
./target/release/fundamentals analyze req.json --previous store/VOLCAR_B/2026-10-25.json \
    --out store/VOLCAR_B/2027-02-10.json --report
```

In your app:

```rust
let prev: Option<AnalysisResponse> = store.latest(symbol)?;
let mut req = AnalysisRequest::new(symbol, "2018-01-01", today);
req.previous_response = prev.as_ref().map(|p| serde_json::to_value(p).unwrap());
let result = client.analyze(&req).await?;                       // result.response is the delta
let doc = match &prev {
    Some(p) => merge_incremental(p, &result.response, Some(req.recent_quarters)),
    None => result.response.clone(),
};
store.save(symbol, &doc)?;                                      // keyed by symbol + analysis_date
```

Scheduling: reporting dates are published in each company's IR calendar, so run a symbol a day or
two after each one, or run the whole watchlist monthly and let the incremental mode report "no new
period" cheaply. Keep every merged document. The sequence of as-run `signal` values is your real
track record, distinct from the retrospective `signal_history`, and the per-run `reliability_pct`
values are what you later compare against verified figures to calibrate the rubric.

## 7. API key handling

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

## 8. Repository layout

```
docs/investment_analysis_framework.md   the framework this is built on (unchanged)
prompts/system_prompt.md                the system prompt (framework + rubric + computation rules + output discipline)
schema/analysis_request.schema.json     request contract
schema/analysis_response.schema.json    response contract exactly as sent to the API
fundamentals_client/                    Python reference (models.py = schema source of truth, client.py, __main__.py CLI)
rust/                                   Rust crate: models.rs, sse.rs, client.rs, checks.rs, report.rs, chart.rs, main.rs CLI, tests/offline.rs
examples/                               seven requests (ASML, AAPL quarterly, supplied data, screenshots, Volkswagen, Volvo Cars, Porsche SE) + make_sample_response.py + sample_response.json
tests/                                  Python offline tests
```

## 9. Status and open items

Verified live (Rust CLI, Sonnet 5.5, prompt-embedded schema): ASML (40 KB) and Volvo Cars
(139 KB: 31 measures, 8 quarterly series, 8-point signal history, EUR base view, 26-chart report)
both parsed through the strict types with one consistency warning, since fixed. Runtime 15 min,
8 searches, 131k output tokens for Volvo Cars. Verified offline: both test suites pass.
**Open:** API-side schema enforcement fails on grammar size even for a five-field control schema
in the probe (next step is the pruned probe table, then Anthropic support); the model dropped
about half the catalogue "to keep the output compact" (now forbidden in the prompt, to be
confirmed on the next run); sandbox file output and incremental delta runs are implemented and
tested offline but not yet exercised live.

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

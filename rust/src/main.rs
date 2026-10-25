//! CLI: fundamentals analyze <request.json> [--out out.json] [--effort high] [--model claude-sonnet-5-5]
//!      fundamentals check <response.json>

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use fundamentals_client::{
    consistency_warnings, render_report, AnalysisRequest, AnalysisResponse, ChartMode, Config,
    Error, FundamentalsClient, ReportOptions,
};

#[derive(Parser)]
#[command(
    name = "fundamentals",
    about = "Fundamental analysis via the Claude API with per-measure reliability"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run an analysis against the Claude API
    Analyze {
        /// JSON file matching AnalysisRequest
        request: PathBuf,
        /// Write the JSON result here (default: stdout)
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = fundamentals_client::client::DEFAULT_MODEL)]
        model: String,
        #[arg(long, default_value = fundamentals_client::client::DEFAULT_EFFORT, value_parser = ["low", "medium", "high", "xhigh", "max"])]
        effort: String,
        #[arg(long, default_value_t = fundamentals_client::client::DEFAULT_MAX_TOKENS)]
        max_tokens: u32,
        /// Disable the server-side refusal fallback (beta)
        #[arg(long)]
        no_fallbacks: bool,
        /// Also send the schema as output_config.format (API-side grammar). Off by default: the API
        /// currently rejects this schema's grammar as too large.
        #[arg(long)]
        enforce_schema: bool,
        /// Also write a Markdown report (with SVG charts) next to --out
        #[arg(long)]
        report: bool,
        /// Suppress the progress log on stderr
        #[arg(long)]
        quiet: bool,
        /// Previous result JSON for this symbol: run incrementally (delta) and merge into it
        #[arg(long)]
        previous: Option<PathBuf>,
        /// Have the model type the JSON as output text instead of handing back a sandbox file
        #[arg(long)]
        no_sandbox_output: bool,
    },
    /// Validate a saved response and print consistency warnings (offline)
    Check { file: PathBuf },
    /// Send cut-down variants of the response schema to find what the API's grammar compiler rejects.
    /// Rejections are free; an accepted probe costs a few hundred input tokens.
    ProbeSchema,
    /// Render a saved response as a Markdown report with charts (offline)
    Report {
        /// AnalysisResponse JSON file
        file: PathBuf,
        /// Markdown output path (default: same name with .md)
        #[arg(long)]
        out: Option<PathBuf>,
        /// svg: chart files next to the report; mermaid: embedded blocks; none
        #[arg(long, default_value = "svg", value_parser = ["svg", "mermaid", "none"])]
        charts: String,
    },
}

fn write_report(
    resp: &AnalysisResponse,
    md_path: &std::path::Path,
    charts: ChartMode,
) -> anyhow::Result<()> {
    let stem = md_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("report")
        .to_string();
    let chart_dir_name = format!("{stem}_charts");
    let opts = ReportOptions {
        charts,
        chart_dir: chart_dir_name.clone(),
        warnings: consistency_warnings(resp),
    };
    let report = render_report(resp, &opts);
    if let Some(dir) = md_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(md_path, &report.markdown)?;
    if !report.charts.is_empty() {
        let dir = md_path
            .parent()
            .map(|d| d.join(&chart_dir_name))
            .unwrap_or_else(|| chart_dir_name.clone().into());
        std::fs::create_dir_all(&dir)?;
        for (name, svg) in &report.charts {
            std::fs::write(dir.join(name), svg)?;
        }
    }
    println!(
        "wrote {} ({} charts)",
        md_path.display(),
        report.charts.len()
    );
    Ok(())
}

fn chart_mode(s: &str) -> ChartMode {
    match s {
        "mermaid" => ChartMode::Mermaid,
        "none" => ChartMode::None,
        _ => ChartMode::Svg,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Check { file } => {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            let resp: AnalysisResponse = serde_json::from_str(&text)?;
            println!(
                "valid AnalysisResponse: {} signal={:?} reliability={}%",
                resp.symbol, resp.signal.category, resp.overall_reliability_pct
            );
            for w in consistency_warnings(&resp) {
                println!("  warning: {w}");
            }
            Ok(())
        }
        Cmd::ProbeSchema => {
            use fundamentals_client::probe::{describe_size, variants};
            let client = FundamentalsClient::new(Config::from_env()?)?;
            let full: serde_json::Value =
                serde_json::from_str(fundamentals_client::client::RESPONSE_SCHEMA_JSON)?;
            println!("{:<26} {:<10} size / note", "variant", "result");
            for v in variants(&full) {
                let size = describe_size(&v.schema);
                match client.probe_schema(&v.schema).await {
                    Ok(fundamentals_client::ProbeOutcome::Accepted { input_tokens }) => {
                        println!(
                            "{:<26} {:<10} {size} | {} ({} input tokens)",
                            v.name, "ACCEPTED", v.describe, input_tokens
                        )
                    }
                    Ok(fundamentals_client::ProbeOutcome::Rejected { .. }) => {
                        println!("{:<26} {:<10} {size} | {}", v.name, "rejected", v.describe)
                    }
                    Err(e) => println!(
                        "{:<26} {:<10} {size} | {}: {e}",
                        v.name, "ERROR", v.describe
                    ),
                }
            }
            Ok(())
        }
        Cmd::Report { file, out, charts } => {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            let resp: AnalysisResponse = serde_json::from_str(&text)?;
            let md_path = out.unwrap_or_else(|| file.with_extension("md"));
            write_report(&resp, &md_path, chart_mode(&charts))
        }
        Cmd::Analyze {
            request,
            out,
            model,
            effort,
            max_tokens,
            no_fallbacks,
            enforce_schema,
            report,
            quiet,
            previous,
            no_sandbox_output,
        } => {
            let text = std::fs::read_to_string(&request)
                .with_context(|| format!("reading {}", request.display()))?;
            let mut req: AnalysisRequest =
                serde_json::from_str(&text).context("request does not match AnalysisRequest")?;
            let previous_resp: Option<AnalysisResponse> = match &previous {
                Some(p) => {
                    let t = std::fs::read_to_string(p)
                        .with_context(|| format!("reading {}", p.display()))?;
                    let r: AnalysisResponse = serde_json::from_str(&t)
                        .context("previous result does not match AnalysisResponse")?;
                    req.previous_response = Some(serde_json::to_value(&r)?);
                    Some(r)
                }
                None => None,
            };
            let mut cfg = Config::from_env()?;
            cfg.model = model;
            cfg.effort = effort;
            cfg.max_tokens = max_tokens;
            cfg.use_server_fallbacks = !no_fallbacks;
            cfg.enforce_schema = enforce_schema;
            cfg.sandbox_output = !no_sandbox_output;
            if !quiet {
                cfg.progress = Some(fundamentals_client::stderr_progress());
                eprintln!(
                    "starting analysis of {} (effort {}, web research {})",
                    req.symbol, cfg.effort, req.allow_web_research
                );
            }
            let client = FundamentalsClient::new(cfg)?;

            let base_dir = request
                .canonicalize()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let today = fundamentals_client::client::today_iso();
            let mut result = match client
                .analyze_dated(&req, &today, base_dir.as_deref())
                .await
            {
                Ok(r) => r,
                Err(Error::Refused {
                    category,
                    explanation,
                }) => {
                    eprintln!("refused: category={category:?} explanation={explanation:?}");
                    std::process::exit(3);
                }
                Err(Error::Incomplete(m)) => {
                    eprintln!("incomplete: {m}");
                    std::process::exit(4);
                }
                Err(e) => return Err(e.into()),
            };

            if let Some(prev) = &previous_resp {
                let merged = fundamentals_client::merge_incremental(
                    prev,
                    &result.response,
                    Some(req.recent_quarters),
                );
                result.warnings = consistency_warnings(&merged);
                result.raw_json = serde_json::to_string_pretty(&merged)?;
                result.response = merged;
            }
            match &out {
                Some(p) => {
                    if let Some(dir) = p.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(p, format!("{}\n", result.raw_json))?;
                    println!("wrote {}", p.display());
                    if report {
                        write_report(&result.response, &p.with_extension("md"), ChartMode::Svg)?;
                    }
                }
                None => println!("{}", result.raw_json),
            }
            let r = &result.response;
            let u = &result.usage;
            eprintln!(
                "\n{}: {:?} (conviction {}%, reliability {}%), overall reliability {}%, data_mode={:?}",
                r.symbol, r.signal.category, r.signal.conviction_pct, r.signal.reliability_pct,
                r.overall_reliability_pct, r.data_mode
            );
            eprintln!(
                "model={} turns={} in={} cache_read={} cache_write={} out={} web_searches={} elapsed={:.0?} request_ids={:?}",
                result.model,
                u.turns,
                u.input_tokens,
                u.cache_read_input_tokens,
                u.cache_creation_input_tokens, u.output_tokens,
                u.web_search_requests, result.elapsed, result.request_ids
            );
            for w in &result.warnings {
                eprintln!("  warning: {w}");
            }
            Ok(())
        }
    }
}

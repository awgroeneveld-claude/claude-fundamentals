//! CLI: fundamentals analyze <request.json> [--out out.json] [--effort high] [--model claude-opus-5-5]
//!      fundamentals check <response.json>

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use fundamentals_client::{
    consistency_warnings, AnalysisRequest, AnalysisResponse, Config, Error, FundamentalsClient,
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
        /// Do not send the schema as output_config.format; put it in the prompt and validate client-side
        #[arg(long)]
        no_schema_enforcement: bool,
    },
    /// Validate a saved response and print consistency warnings (offline)
    Check { file: PathBuf },
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
        Cmd::Analyze {
            request,
            out,
            model,
            effort,
            max_tokens,
            no_fallbacks,
            no_schema_enforcement,
        } => {
            let text = std::fs::read_to_string(&request)
                .with_context(|| format!("reading {}", request.display()))?;
            let req: AnalysisRequest =
                serde_json::from_str(&text).context("request does not match AnalysisRequest")?;
            let mut cfg = Config::from_env()?;
            cfg.model = model;
            cfg.effort = effort;
            cfg.max_tokens = max_tokens;
            cfg.use_server_fallbacks = !no_fallbacks;
            cfg.enforce_schema = !no_schema_enforcement;
            let client = FundamentalsClient::new(cfg)?;

            let base_dir = request
                .canonicalize()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let today = fundamentals_client::client::today_iso();
            let result = match client
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

            match &out {
                Some(p) => {
                    if let Some(dir) = p.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(p, format!("{}\n", result.raw_json))?;
                    println!("wrote {}", p.display());
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
                "model={} turns={} in={} cached={} out={} web_searches={} elapsed={:.0?} request_ids={:?}",
                result.model, u.turns, u.input_tokens, u.cache_read_input_tokens, u.output_tokens,
                u.web_search_requests, result.elapsed, result.request_ids
            );
            for w in &result.warnings {
                eprintln!("  warning: {w}");
            }
            Ok(())
        }
    }
}

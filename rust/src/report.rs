//! Markdown report (with charts) rendered from an `AnalysisResponse`.
//!
//! `render` is pure: it returns the Markdown text and the chart files to write next to it.
//! Charts are SVG files referenced by relative path (renders everywhere), or Mermaid blocks
//! embedded in the Markdown (single file; renders on GitHub, GitLab, Obsidian, VS Code).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::chart::{fmt_num, line_chart_svg, mermaid_blocks, ChartSpec, Series, MAX_SERIES};
use crate::models::{
    AnalysisResponse, Assessment, Category, DataPoint, Direction, Measure, PeriodType, Scale,
    Signal, SignalPoint, Unit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartMode {
    Svg,
    Mermaid,
    None,
}

#[derive(Debug, Clone)]
pub struct ReportOptions {
    pub charts: ChartMode,
    /// Directory, relative to the Markdown file, where SVG charts are referenced from.
    pub chart_dir: String,
    /// Consistency warnings to print in the report (from `consistency_warnings`).
    pub warnings: Vec<String>,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            charts: ChartMode::Svg,
            chart_dir: "charts".into(),
            warnings: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub markdown: String,
    /// (file name inside `chart_dir`, SVG content)
    pub charts: Vec<(String, String)>,
}

// ----------------------------------------------------------------------------- formatting

fn snake(d: impl std::fmt::Debug) -> String {
    // Enum Debug names are CamelCase; the wire names are snake_case. Good enough for display.
    let s = format!("{d:?}");
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in s.chars() {
        if c == '_' {
            out.push(' ');
            up = true;
        } else if up {
            out.push(c.to_ascii_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn period_label(p: &DataPoint) -> String {
    let year = p.period_end.get(0..4).unwrap_or(&p.period_end);
    match p.period_type {
        PeriodType::FY => format!("FY{year}"),
        PeriodType::TTM => format!("TTM {}", p.period_end.get(0..7).unwrap_or(&p.period_end)),
        PeriodType::FWD => format!("FWD {year}"),
        PeriodType::Q => format!("Q {}", p.period_end.get(0..7).unwrap_or(&p.period_end)),
        PeriodType::POINT => p.period_end.clone(),
    }
}

fn scale_label(s: Scale) -> &'static str {
    match s {
        Scale::Units => "",
        Scale::Thousands => "k",
        Scale::Millions => "m",
        Scale::Billions => "bn",
        Scale::NotApplicable => "",
    }
}

pub fn unit_label(m: &Measure) -> String {
    match m.unit {
        Unit::Currency => format!("{} {}", m.currency, long_scale(m.scale))
            .trim()
            .to_string(),
        Unit::PerShare => format!("{} per share", m.currency),
        Unit::Percent => "percent".into(),
        Unit::Ratio => "ratio".into(),
        Unit::Multiple => "multiple (x)".into(),
        Unit::Count => "count".into(),
        Unit::Score => "score".into(),
    }
}

fn long_scale(s: Scale) -> &'static str {
    match s {
        Scale::Units => "",
        Scale::Thousands => "thousands",
        Scale::Millions => "millions",
        Scale::Billions => "billions",
        Scale::NotApplicable => "",
    }
}

pub fn fmt_value(m: &Measure, v: Option<f64>) -> String {
    let Some(v) = v else { return "n/a".into() };
    match m.unit {
        Unit::Currency => format!("{} {}{}", m.currency, fmt_num(v), scale_label(m.scale)),
        Unit::PerShare => format!("{} {}", m.currency, fmt_num(v)),
        Unit::Percent => format!("{} %", fmt_num(v)),
        Unit::Multiple => format!("{}x", fmt_num(v)),
        Unit::Ratio | Unit::Count | Unit::Score => fmt_num(v),
    }
}

fn signal_word(s: Signal) -> &'static str {
    match s {
        Signal::ExtremeBuy => "EXTREME BUY",
        Signal::Buy => "BUY",
        Signal::Hold => "HOLD",
        Signal::Sell => "SELL",
        Signal::ExtremeSell => "EXTREME SELL",
    }
}

fn direction_word(d: Direction) -> &'static str {
    match d {
        Direction::Improving => "improving",
        Direction::Stable => "stable",
        Direction::Deteriorating => "deteriorating",
        Direction::Volatile => "volatile",
        Direction::InsufficientData => "insufficient data",
    }
}

fn signal_score(s: Signal) -> f64 {
    match s {
        Signal::ExtremeSell => -2.0,
        Signal::Sell => -1.0,
        Signal::Hold => 0.0,
        Signal::Buy => 1.0,
        Signal::ExtremeBuy => 2.0,
    }
}

fn sp_label(sp: &SignalPoint) -> String {
    let year = sp.period_end.get(0..4).unwrap_or(&sp.period_end);
    match sp.period_type {
        PeriodType::FY => format!("FY{year}"),
        PeriodType::TTM => format!("TTM {}", sp.period_end.get(0..7).unwrap_or(&sp.period_end)),
        PeriodType::Q => format!("Q {}", sp.period_end.get(0..7).unwrap_or(&sp.period_end)),
        _ => sp.period_end.clone(),
    }
}

fn opt_pct(v: Option<f64>) -> String {
    v.map(|x| format!("{} %", fmt_num(x)))
        .unwrap_or_else(|| "n/a".into())
}

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

// ----------------------------------------------------------------------------- charts

/// Group chartable measures of one category into chart specs (<= 4 series each, same unit,
/// same currency/scale; ratios, multiples, scores and counts one per chart).
fn chart_groups<'a>(measures: &[&'a Measure]) -> Vec<(String, Vec<&'a Measure>)> {
    let mut groups: BTreeMap<String, Vec<&Measure>> = BTreeMap::new();
    for m in measures {
        let chartable = m.series.iter().filter(|p| p.value.is_some()).count() >= 2;
        if !chartable {
            continue;
        }
        let key = match m.unit {
            Unit::Percent => "percent".to_string(),
            Unit::Currency => format!("{} {}", m.currency, long_scale(m.scale)),
            Unit::PerShare => format!("{} per share", m.currency),
            _ => format!("solo:{}", snake(m.measure_id)),
        };
        groups.entry(key).or_default().push(m);
    }
    let mut out = Vec::new();
    for (key, ms) in groups {
        for chunk in ms.chunks(MAX_SERIES) {
            let label = if key.starts_with("solo:") {
                chunk[0].label.clone()
            } else {
                key.clone()
            };
            out.push((label, chunk.to_vec()));
        }
    }
    out
}

/// Which chart a data point belongs to: annual-like (FY, TTM, FWD, POINT) or quarterly.
fn is_quarterly(p: &DataPoint) -> bool {
    p.period_type == PeriodType::Q
}

fn spec_for(title: &str, group: &[&Measure], quarterly: bool) -> Option<ChartSpec> {
    // Union of period labels across the group, in chronological order of period_end.
    let mut periods: Vec<(String, String)> = Vec::new(); // (period_end, label)
    for m in group {
        for p in m.series.iter().filter(|p| is_quarterly(p) == quarterly) {
            if !periods.iter().any(|(e, _)| e == &p.period_end) {
                periods.push((p.period_end.clone(), period_label(p)));
            }
        }
    }
    periods.sort();
    let series: Vec<Series> = group
        .iter()
        .map(|m| Series {
            name: m.label.clone(),
            points: periods
                .iter()
                .map(|(e, _)| {
                    m.series
                        .iter()
                        .find(|p| &p.period_end == e && is_quarterly(p) == quarterly)
                        .and_then(|p| p.value)
                })
                .collect(),
        })
        .filter(|s| s.points.iter().flatten().count() >= 2)
        .collect();
    if series.is_empty() {
        return None;
    }
    let unit = unit_label(group[0]);
    let base = if group.len() == 1 {
        group[0].label.clone()
    } else {
        title.to_string()
    };
    Some(ChartSpec {
        title: if quarterly {
            format!("{base}, quarterly")
        } else {
            base
        },
        unit_label: unit,
        categories: periods.into_iter().map(|(_, l)| l).collect(),
        series,
        y_labels: vec![],
        step: false,
    })
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

// ----------------------------------------------------------------------------- render

pub fn render(r: &AnalysisResponse, opts: &ReportOptions) -> Report {
    let mut md = String::new();
    let mut charts: Vec<(String, String)> = Vec::new();
    let name = if r.company_name.is_empty() {
        r.symbol.clone()
    } else {
        format!("{} ({})", r.company_name, r.symbol)
    };

    let _ = writeln!(md, "# {name}: fundamental analysis\n");
    let _ = writeln!(
        md,
        "*Analysis date {} · data mode `{}` · overall reliability **{} %** · reporting currency {}{}*\n",
        r.analysis_date,
        snake(r.data_mode),
        r.overall_reliability_pct,
        if r.fx.reporting_currency.is_empty() { "n/a".to_string() } else { r.fx.reporting_currency.clone() },
        if r.fx.base_currency.is_empty() { String::new() } else { format!(" · base currency {}", r.fx.base_currency) }
    );

    // ---- Signal
    let s = &r.signal;
    let _ = writeln!(md, "## Signal: {}\n", signal_word(s.category));
    let _ = writeln!(
        md,
        "| Conviction | Reliability |\n|---:|---:|\n| {} % | {} % |\n",
        s.conviction_pct, s.reliability_pct
    );
    let _ = writeln!(md, "{}\n", s.rationale);
    if !s.green_flags.is_empty() {
        let _ = writeln!(md, "**Green flags**\n");
        for g in &s.green_flags {
            let _ = writeln!(md, "- {g}");
        }
        md.push('\n');
    }
    if !s.red_flags.is_empty() {
        let _ = writeln!(md, "**Red flags**\n");
        for g in &s.red_flags {
            let _ = writeln!(md, "- {g}");
        }
        md.push('\n');
    }
    let _ = writeln!(
        md,
        "| Would move the signal up | Would move the signal down |\n|---|---|"
    );
    let n = s
        .conditions_for_upgrade
        .len()
        .max(s.conditions_for_downgrade.len());
    for i in 0..n {
        let _ = writeln!(
            md,
            "| {} | {} |",
            md_cell(
                s.conditions_for_upgrade
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or("")
            ),
            md_cell(
                s.conditions_for_downgrade
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or("")
            )
        );
    }
    md.push('\n');

    // ---- Signal history
    if !r.signal_history.is_empty() {
        let _ = writeln!(md, "## Signal over time\n");
        let _ = writeln!(md, "The framework applied as of each date with only the figures available then (no look-ahead). Pillar scores 0-100, 100 = most favourable; valuation shown as – and extremes unavailable where no price could be sourced for the date.\n");
        let _ = writeln!(md, "| As of | Signal | Conviction | Reliability | Valuation | Profitability | Debt | Earnings quality | Trends | Decisive factor |\n|---|---|---:|---:|---:|---:|---:|---:|---:|---|");
        for sp in &r.signal_history {
            let ps = &sp.pillar_scores;
            let _ = writeln!(
                md,
                "| {} | {} | {} % | {} % | {} | {} | {} | {} | {} | {} |",
                sp_label(sp),
                signal_word(sp.category),
                sp.conviction_pct,
                sp.reliability_pct,
                if sp.valuation_assessed {
                    ps.valuation.to_string()
                } else {
                    "–".to_string()
                },
                ps.profitability_stability,
                ps.debt_health,
                ps.earnings_quality,
                ps.trends,
                md_cell(&sp.decisive_factor)
            );
        }
        md.push('\n');
        if opts.charts != ChartMode::None {
            let cats: Vec<String> = r.signal_history.iter().map(sp_label).collect();
            let col = |f: &dyn Fn(&SignalPoint) -> f64| -> Vec<Option<f64>> {
                r.signal_history.iter().map(|sp| Some(f(sp))).collect()
            };
            let mut specs = vec![
                ChartSpec {
                    title: "Signal over time".into(),
                    unit_label: "framework category".into(),
                    categories: cats.clone(),
                    series: vec![Series {
                        name: "Signal".into(),
                        points: col(&|sp| signal_score(sp.category)),
                    }],
                    y_labels: vec![
                        (-2.0, "Extreme sell".into()),
                        (-1.0, "Sell".into()),
                        (0.0, "Hold".into()),
                        (1.0, "Buy".into()),
                        (2.0, "Extreme buy".into()),
                    ],
                    step: true,
                },
                ChartSpec {
                    title: "Conviction and reliability over time".into(),
                    unit_label: "percent".into(),
                    categories: cats.clone(),
                    series: vec![
                        Series {
                            name: "Conviction".into(),
                            points: col(&|sp| sp.conviction_pct as f64),
                        },
                        Series {
                            name: "Reliability".into(),
                            points: col(&|sp| sp.reliability_pct as f64),
                        },
                    ],
                    y_labels: vec![],
                    step: false,
                },
                ChartSpec {
                    title: "Pillar scores over time".into(),
                    unit_label: "score (0-100)".into(),
                    categories: cats.clone(),
                    series: vec![
                        Series {
                            name: "Profitability".into(),
                            points: col(&|sp| sp.pillar_scores.profitability_stability as f64),
                        },
                        Series {
                            name: "Debt health".into(),
                            points: col(&|sp| sp.pillar_scores.debt_health as f64),
                        },
                        Series {
                            name: "Earnings quality".into(),
                            points: col(&|sp| sp.pillar_scores.earnings_quality as f64),
                        },
                        Series {
                            name: "Trends".into(),
                            points: col(&|sp| sp.pillar_scores.trends as f64),
                        },
                    ],
                    y_labels: vec![],
                    step: false,
                },
            ];
            let valuation_pts: Vec<Option<f64>> = r
                .signal_history
                .iter()
                .map(|sp| {
                    sp.valuation_assessed
                        .then_some(sp.pillar_scores.valuation as f64)
                })
                .collect();
            if valuation_pts.iter().flatten().count() >= 2 {
                specs.push(ChartSpec {
                    title: "Valuation pillar over time".into(),
                    unit_label: "score (0-100)".into(),
                    categories: cats,
                    series: vec![Series {
                        name: "Valuation".into(),
                        points: valuation_pts,
                    }],
                    y_labels: vec![],
                    step: false,
                });
            }
            for spec in specs {
                match opts.charts {
                    ChartMode::Svg => {
                        let file = format!("{}.svg", slug(&spec.title));
                        let _ = writeln!(
                            md,
                            "![{}]({}/{})\n",
                            spec.title,
                            opts.chart_dir.trim_end_matches('/'),
                            file
                        );
                        charts.push((file, line_chart_svg(&spec)));
                    }
                    ChartMode::Mermaid => md.push_str(&mermaid_blocks(&spec)),
                    ChartMode::None => {}
                }
            }
        }
    }

    // ---- Pillars
    let _ = writeln!(md, "## Pillar scorecard\n");
    let _ = writeln!(md, "| Pillar | Assessment | Score (0-100) | Reliability | Rationale |\n|---|---|---:|---:|---|");
    for p in &r.pillars {
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} % | {} |",
            title_case(&snake(p.pillar)),
            if p.assessment == Assessment::NotAssessed {
                "not assessed".to_string()
            } else {
                snake(p.assessment)
            },
            if p.assessment == Assessment::NotAssessed {
                "–".to_string()
            } else {
                p.score_0_100.to_string()
            },
            p.reliability_pct,
            md_cell(&p.rationale)
        );
    }
    md.push('\n');

    // ---- Trend summary
    let t = &r.trend_summary;
    let _ = writeln!(
        md,
        "## Trends: {} (reliability {} %)\n",
        direction_word(t.overall_direction),
        t.reliability_pct
    );
    let list = |ids: &Vec<crate::models::MeasureId>| -> String {
        if ids.is_empty() {
            "–".into()
        } else {
            ids.iter()
                .map(|i| format!("`{}`", snake(i)))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let _ = writeln!(
        md,
        "| Improving | Stable | Deteriorating |\n|---|---|---|\n| {} | {} | {} |\n",
        list(&t.improving),
        list(&t.stable),
        list(&t.deteriorating)
    );
    if !t.inflections.is_empty() {
        let _ = writeln!(md, "**Inflections**\n");
        for i in &t.inflections {
            let _ = writeln!(
                md,
                "- `{}` at {}: {}",
                snake(i.measure_id),
                i.period_end,
                i.description
            );
        }
        md.push('\n');
    }
    let _ = writeln!(
        md,
        "**Operating vs non-operating:** {}\n",
        t.operating_vs_non_operating
    );

    // ---- Measures by category
    let _ = writeln!(md, "## Measures\n");
    let _ = writeln!(md, "All amounts in the reporting currency. Reliability per the rubric (95-100 primary filing or supplied data; 80-94 derived or corroborated aggregator; 60-79 one assumption or single source; 40-59 approximation; 1-39 estimate).\n");
    let order = [
        Category::Valuation,
        Category::Profitability,
        Category::NonOperating,
        Category::Leverage,
        Category::CashFlowQuality,
        Category::Growth,
        Category::Ownership,
        Category::Context,
    ];
    for cat in order {
        let ms: Vec<&Measure> = r.measures.iter().filter(|m| m.category == cat).collect();
        if ms.is_empty() {
            continue;
        }
        let _ = writeln!(md, "### {}\n", title_case(&snake(cat)));
        let _ = writeln!(md, "| Measure | Latest | Period | Trend | CAGR | YoY | vs own history | vs peers | Reliability | Method |\n|---|---:|---|---|---:|---:|---|---|---:|---|");
        for m in &ms {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} % | {} |",
                md_cell(&m.label),
                fmt_value(m, m.latest.value),
                period_label(&m.latest),
                direction_word(m.trend.direction),
                opt_pct(m.trend.cagr_pct),
                opt_pct(m.trend.latest_yoy_change_pct),
                snake(m.trend.vs_own_history).replace('_', " "),
                snake(m.trend.vs_peers).replace('_', " "),
                m.reliability_pct,
                snake(m.latest.method)
            );
        }
        md.push('\n');

        // charts
        if opts.charts != ChartMode::None {
            for (title, group) in chart_groups(&ms) {
                let title = format!("{} ({})", title_case(&snake(cat)), title);
                let specs = [false, true]
                    .into_iter()
                    .filter_map(|q| spec_for(&title, &group, q));
                for spec in specs {
                    match opts.charts {
                        ChartMode::Svg => {
                            let file = format!("{}.svg", slug(&spec.title));
                            let _ = writeln!(
                                md,
                                "![{}]({}/{})\n",
                                spec.title,
                                opts.chart_dir.trim_end_matches('/'),
                                file
                            );
                            charts.push((file, line_chart_svg(&spec)));
                        }
                        ChartMode::Mermaid => md.push_str(&mermaid_blocks(&spec)),
                        ChartMode::None => {}
                    }
                }
            }
        }

        // detail: series tables + notes, collapsed
        for m in &ms {
            let _ = writeln!(
                md,
                "<details><summary><b>{}</b> — {} · reliability {} %: {}</summary>\n",
                m.label,
                md_cell(&m.trend.comment),
                m.reliability_pct,
                md_cell(&m.reliability_basis)
            );
            let has_base = m.series.iter().any(|p| p.value_base.is_some());
            if has_base {
                let _ = writeln!(md, "| Period | Value | In {} | Reliability | Method | Sources |\n|---|---:|---:|---:|---|---|", r.fx.base_currency);
            } else {
                let _ = writeln!(
                    md,
                    "| Period | Value | Reliability | Method | Sources |\n|---|---:|---:|---|---|"
                );
            }
            for p in &m.series {
                if has_base {
                    let _ = writeln!(
                        md,
                        "| {} | {} | {} | {} % | {} | {} |",
                        period_label(p),
                        fmt_value(m, p.value),
                        fmt_value(m, p.value_base),
                        p.reliability_pct,
                        snake(p.method),
                        p.source_ids.join(", ")
                    );
                } else {
                    let _ = writeln!(
                        md,
                        "| {} | {} | {} % | {} | {} |",
                        period_label(p),
                        fmt_value(m, p.value),
                        p.reliability_pct,
                        snake(p.method),
                        p.source_ids.join(", ")
                    );
                }
            }
            if !m.notes.is_empty() {
                let _ = writeln!(md, "\n*Notes:* {}", m.notes);
            }
            let _ = writeln!(md, "\n</details>\n");
        }
    }

    // ---- Ownership / context
    if r.ownership.requested {
        let o = &r.ownership;
        let _ = writeln!(md, "## Ownership (reliability {} %)\n", o.reliability_pct);
        if !o.top_holders.is_empty() {
            let _ = writeln!(
                md,
                "| Holder | Type | % of shares | As of | Reliability |\n|---|---|---:|---|---:|"
            );
            for h in &o.top_holders {
                let _ = writeln!(
                    md,
                    "| {} | {} | {} | {} | {} % |",
                    md_cell(&h.name),
                    snake(h.holder_type),
                    h.pct_of_shares
                        .map(|v| format!("{} %", fmt_num(v)))
                        .unwrap_or_else(|| "n/a".into()),
                    h.as_of,
                    h.reliability_pct
                );
            }
            md.push('\n');
        }
        if !o.recent_insider_activity.is_empty() {
            let _ = writeln!(md, "**Insider activity:** {}\n", o.recent_insider_activity);
        }
        if !o.notes.is_empty() {
            let _ = writeln!(md, "{}\n", o.notes);
        }
    }
    if r.context.requested {
        let c = &r.context;
        let _ = writeln!(md, "## Context (reliability {} %)\n", c.reliability_pct);
        for (k, v) in [
            ("Industry", &c.industry),
            ("Segments", &c.segment_notes),
            ("Management commentary", &c.management_commentary),
            ("Dividend policy", &c.dividend_policy),
            ("Accounting flags", &c.accounting_flags),
        ] {
            if !v.is_empty() {
                let _ = writeln!(md, "- **{k}:** {v}");
            }
        }
        if !c.peer_set_used.is_empty() {
            let _ = writeln!(md, "- **Peer set:** {}", c.peer_set_used.join(", "));
        }
        md.push('\n');
    }

    // ---- Currency / FX
    let _ = writeln!(md, "## Currency\n\n{}\n", r.currency_notes);
    if !r.fx.rates.is_empty() {
        let _ = writeln!(
            md,
            "Rates: {} per 1 {}. {}\n",
            r.fx.base_currency, r.fx.reporting_currency, r.fx.method_note
        );
        let _ = writeln!(
            md,
            "| Period | Average | Closing | Reliability | Sources |\n|---|---:|---:|---:|---|"
        );
        for f in &r.fx.rates {
            let _ = writeln!(
                md,
                "| {} | {:.4} | {:.4} | {} % | {} |",
                f.period_end,
                f.average_rate,
                f.closing_rate,
                f.reliability_pct,
                f.source_ids.join(", ")
            );
        }
        md.push('\n');
    }

    // ---- Sector adjustments, gaps, sources, warnings
    if !r.sector_adjustments.is_empty() {
        let _ = writeln!(md, "## Sector adjustments\n");
        for a in &r.sector_adjustments {
            let _ = writeln!(md, "- {a}");
        }
        md.push('\n');
    }
    if !r.data_gaps.is_empty() {
        let _ = writeln!(md, "## Data gaps\n");
        for g in &r.data_gaps {
            if g.measure_id.is_empty() {
                let _ = writeln!(md, "- {}", g.description);
            } else {
                let _ = writeln!(md, "- `{}`: {}", g.measure_id, g.description);
            }
        }
        md.push('\n');
    }
    let _ = writeln!(md, "## Sources\n");
    let _ = writeln!(
        md,
        "| Id | Type | Title | Period | Retrieved |\n|---|---|---|---|---|"
    );
    for src in &r.sources {
        let title = if src.url.is_empty() {
            md_cell(&src.title)
        } else {
            format!("[{}]({})", md_cell(&src.title), src.url)
        };
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} |",
            src.id,
            snake(src.source_type),
            title,
            md_cell(&src.period_covered),
            src.retrieved_at
        );
    }
    md.push('\n');
    if !opts.warnings.is_empty() {
        let _ = writeln!(md, "## Consistency warnings\n");
        for w in &opts.warnings {
            let _ = writeln!(md, "- {w}");
        }
        md.push('\n');
    }
    let _ = writeln!(md, "---\n\n*{}*", r.disclaimer);

    Report {
        markdown: md,
        charts,
    }
}

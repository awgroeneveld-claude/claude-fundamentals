//! Typed request/response models. These mirror `fundamentals_client/models.py` and the JSON
//! Schema in `schema/analysis_response.schema.json`, which is what the API enforces.
//!
//! `deny_unknown_fields` keeps deserialisation as strict as the API's `additionalProperties: false`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
    };
}

str_enum!(Category {
    Valuation,
    Profitability,
    NonOperating,
    Leverage,
    CashFlowQuality,
    Growth,
    Ownership,
    Context
});

str_enum!(MeasureId {
    // valuation
    SharePrice,
    MarketCap,
    EnterpriseValue,
    PeTrailing,
    PeForward,
    Pb,
    Ps,
    EvEbitda,
    EvSales,
    FcfYield,
    EarningsYield,
    Peg,
    DividendYield,
    PayoutRatio,
    PeHistoricalPercentile,
    EvEbitdaHistoricalPercentile,
    // profitability
    Revenue,
    GrossProfit,
    GrossMargin,
    OperatingIncome,
    OperatingMargin,
    Ebitda,
    EbitdaMargin,
    NetIncome,
    NetMargin,
    EpsDiluted,
    Roe,
    Roa,
    Roic,
    MarginStabilityCv,
    EarningsVariabilityCv,
    // non-operating
    InterestIncome,
    InterestExpense,
    InvestmentGainsLosses,
    FxAndDerivativeResult,
    OtherIncomeExpense,
    NetNonOperatingResult,
    NonOperatingPctRevenue,
    NonOperatingPctOperatingIncome,
    OneTimeItems,
    // leverage
    LongTermDebt,
    ShortTermDebt,
    TotalDebt,
    CashAndEquivalents,
    NetDebt,
    ShareholdersEquity,
    TotalAssets,
    DebtToEquity,
    DebtToAssets,
    LtDebtToAssets,
    NetDebtToEbitda,
    InterestCoverage,
    CurrentRatio,
    // cash flow quality
    OperatingCashFlow,
    Capex,
    FreeCashFlow,
    FcfMargin,
    Accruals,
    CashConversion,
    AccrualRatio,
    SloanAccrualRatio,
    PiotroskiFScore,
    WorkingCapitalChange,
    NetShareIssuance,
    CapitalReturnedPctFcf,
    // growth
    RevenueGrowthYoy,
    RevenueCagr,
    OperatingIncomeCagr,
    NetIncomeCagr,
    FcfCagr,
    EpsGrowthYoy,
    AssetTurnover,
    // ownership
    InstitutionalOwnershipPct,
    InsiderOwnershipPct,
    InsiderNetBuying,
    SharesOutstanding,
    SharesOutstandingChangePct,
});

str_enum!(Unit {
    Currency,
    Percent,
    Ratio,
    Multiple,
    Count,
    Score,
    PerShare
});
str_enum!(Scale {
    Units,
    Thousands,
    Millions,
    Billions
});
str_enum!(Method {
    Reported,
    Derived,
    Estimated,
    Unavailable
});
str_enum!(Frequency {
    Annual,
    Quarterly,
    Both
});
str_enum!(Direction {
    Improving,
    Stable,
    Deteriorating,
    Volatile,
    InsufficientData
});
str_enum!(RelativePosition {
    BottomQuartile,
    BelowMedian,
    AboveMedian,
    TopQuartile,
    Unknown
});
str_enum!(PeerComparison {
    Better,
    InLine,
    Worse,
    Unknown
});
str_enum!(SourceType {
    PrimaryFiling,
    CompanyIr,
    Aggregator,
    SuppliedInput,
    WebOther,
    ModelRecall
});
str_enum!(Signal {
    ExtremeBuy,
    Buy,
    Hold,
    Sell,
    ExtremeSell
});
str_enum!(PillarId {
    Valuation,
    ProfitabilityStability,
    DebtHealth,
    EarningsQuality,
    Trends
});
str_enum!(Assessment {
    Strong,
    Favorable,
    Neutral,
    Weak,
    Poor,
    NotAssessed
});
str_enum!(DataMode {
    Supplied,
    Retrieved,
    Mixed,
    RecallOnly
});
str_enum!(HolderType {
    Institution,
    Insider,
    Strategic,
    Fund,
    Other
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PeriodType {
    FY,
    Q,
    TTM,
    FWD,
    POINT,
}

// ----------------------------------------------------------------------------
// Request
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Period {
    pub start: String,
    pub end: String,
}

/// Supported image media types for screenshot attachments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageMediaType {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
}

/// A screenshot (e.g. Interactive Brokers fundamentals / ownership pages) sent as an image block.
/// Provide either `path` (resolved against the request file's directory by the CLI) or `data`
/// (base64, no `data:` prefix). <= 5 MB each, at most 100 per request, long edge <= 1568 px.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageAttachment {
    #[serde(default = "default_png")]
    pub media_type: ImageMediaType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// What the image shows and when it was captured.
    pub caption: String,
}

fn default_png() -> ImageMediaType {
    ImageMediaType::Png
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRequest {
    pub symbol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporting_currency: Option<String>,
    pub period: Period,
    #[serde(default = "default_frequency")]
    pub frequency: Frequency,
    #[serde(default = "all_categories")]
    pub categories: Vec<Category>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measures: Option<Vec<MeasureId>>,
    #[serde(default)]
    pub peer_symbols: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sector_hint: Option<String>,
    #[serde(default = "default_true")]
    pub allow_web_research: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplied_data: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn default_frequency() -> Frequency {
    Frequency::Annual
}
fn default_true() -> bool {
    true
}
fn all_categories() -> Vec<Category> {
    use Category::*;
    vec![
        Valuation,
        Profitability,
        NonOperating,
        Leverage,
        CashFlowQuality,
        Growth,
        Ownership,
        Context,
    ]
}

impl AnalysisRequest {
    pub fn new(
        symbol: impl Into<String>,
        start: impl Into<String>,
        end: impl Into<String>,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            exchange: None,
            company_name: None,
            reporting_currency: None,
            period: Period {
                start: start.into(),
                end: end.into(),
            },
            frequency: Frequency::Annual,
            categories: all_categories(),
            measures: None,
            peer_symbols: vec![],
            sector_hint: None,
            allow_web_research: true,
            supplied_data: None,
            images: vec![],
            notes: None,
        }
    }
}

// ----------------------------------------------------------------------------
// Response
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub title: String,
    pub url: Option<String>,
    pub period_covered: Option<String>,
    pub retrieved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataPoint {
    pub period_end: String,
    pub period_type: PeriodType,
    pub value: Option<f64>,
    pub reliability_pct: u8,
    pub method: Method,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trend {
    pub direction: Direction,
    pub cagr_pct: Option<f64>,
    pub latest_yoy_change_pct: Option<f64>,
    pub coefficient_of_variation: Option<f64>,
    pub vs_own_history: RelativePosition,
    pub vs_peers: PeerComparison,
    pub reliability_pct: u8,
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measure {
    pub measure_id: MeasureId,
    pub category: Category,
    pub label: String,
    pub unit: Unit,
    pub currency: Option<String>,
    pub scale: Option<Scale>,
    pub latest: DataPoint,
    pub series: Vec<DataPoint>,
    pub trend: Trend,
    pub reliability_pct: u8,
    pub reliability_basis: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holder {
    pub name: String,
    pub pct_of_shares: Option<f64>,
    pub holder_type: HolderType,
    pub as_of: Option<String>,
    pub reliability_pct: u8,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ownership {
    pub top_holders: Vec<Holder>,
    pub recent_insider_activity: Option<String>,
    pub reliability_pct: u8,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub industry: Option<String>,
    pub peer_set_used: Vec<String>,
    pub segment_notes: Option<String>,
    pub management_commentary: Option<String>,
    pub dividend_policy: Option<String>,
    pub accounting_flags: Option<String>,
    pub reliability_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pillar {
    pub pillar: PillarId,
    pub assessment: Assessment,
    pub score_0_100: u8,
    pub key_measures: Vec<MeasureId>,
    pub rationale: String,
    pub reliability_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inflection {
    pub measure_id: MeasureId,
    pub period_end: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendSummary {
    pub overall_direction: Direction,
    pub improving: Vec<MeasureId>,
    pub stable: Vec<MeasureId>,
    pub deteriorating: Vec<MeasureId>,
    pub inflections: Vec<Inflection>,
    pub operating_vs_non_operating: String,
    pub reliability_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalBlock {
    pub category: Signal,
    pub conviction_pct: u8,
    pub reliability_pct: u8,
    pub rationale: String,
    pub green_flags: Vec<String>,
    pub red_flags: Vec<String>,
    pub conditions_for_upgrade: Vec<String>,
    pub conditions_for_downgrade: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataGap {
    pub measure_id: Option<MeasureId>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEcho {
    pub symbol: String,
    pub company_name: Option<String>,
    pub exchange: Option<String>,
    pub reporting_currency: Option<String>,
    pub period_start: String,
    pub period_end: String,
    pub frequency: Frequency,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisResponse {
    pub schema_version: String,
    pub request: RequestEcho,
    pub analysis_date: String,
    pub data_mode: DataMode,
    pub sources: Vec<Source>,
    pub measures: Vec<Measure>,
    pub ownership: Option<Ownership>,
    pub context: Option<Context>,
    pub pillars: Vec<Pillar>,
    pub trend_summary: TrendSummary,
    pub signal: SignalBlock,
    pub sector_adjustments: Vec<String>,
    pub data_gaps: Vec<DataGap>,
    pub overall_reliability_pct: u8,
    pub disclaimer: String,
}

/// Validate the numeric ranges the JSON Schema cannot express (the API strips `minimum`/`maximum`).
pub fn validate_ranges(r: &AnalysisResponse) -> Vec<String> {
    let mut errs = Vec::new();
    let chk = |v: u8, what: &str, errs: &mut Vec<String>| {
        if v > 100 {
            errs.push(format!("{what}: {v} > 100"));
        }
    };
    for m in &r.measures {
        chk(
            m.reliability_pct,
            &format!("{:?}.reliability_pct", m.measure_id),
            &mut errs,
        );
        chk(
            m.trend.reliability_pct,
            &format!("{:?}.trend.reliability_pct", m.measure_id),
            &mut errs,
        );
        for p in std::iter::once(&m.latest).chain(m.series.iter()) {
            chk(
                p.reliability_pct,
                &format!("{:?}@{}", m.measure_id, p.period_end),
                &mut errs,
            );
        }
    }
    for p in &r.pillars {
        chk(
            p.reliability_pct,
            &format!("pillar {:?}", p.pillar),
            &mut errs,
        );
        chk(
            p.score_0_100,
            &format!("pillar {:?} score", p.pillar),
            &mut errs,
        );
    }
    chk(r.signal.conviction_pct, "signal.conviction_pct", &mut errs);
    chk(
        r.signal.reliability_pct,
        "signal.reliability_pct",
        &mut errs,
    );
    chk(
        r.overall_reliability_pct,
        "overall_reliability_pct",
        &mut errs,
    );
    chk(
        r.trend_summary.reliability_pct,
        "trend_summary.reliability_pct",
        &mut errs,
    );
    errs
}

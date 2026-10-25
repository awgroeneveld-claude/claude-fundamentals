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
    Billions,
    NotApplicable
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
    SuppliedScreenshot,
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
    /// Optional ISO 4217 code; when set, currency data points also carry `value_base` and the
    /// response includes the fx rates used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_currency: Option<String>,
    pub period: Period,
    /// annual: fiscal years (+TTM); quarterly: fiscal quarters; both (default): annual series for
    /// every measure over the full period plus quarterly series for the core flow and balance
    /// measures over the most recent `recent_quarters` quarters.
    #[serde(default = "default_frequency")]
    pub frequency: Frequency,
    #[serde(default = "default_recent_quarters")]
    pub recent_quarters: u32,
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
    /// The AnalysisResponse from the previous run for this symbol. When present the run is
    /// incremental: prior series kept (verified for restatements), only newer periods fetched,
    /// point-in-time items refreshed, trends and signal recomputed, signal_history extended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_response: Option<Value>,
    /// Also evaluate the framework as of each fiscal year-end in the period (point-in-time).
    #[serde(default = "default_true")]
    pub signal_history: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn default_frequency() -> Frequency {
    Frequency::Both
}
fn default_recent_quarters() -> u32 {
    8
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
            base_currency: None,
            period: Period {
                start: start.into(),
                end: end.into(),
            },
            frequency: Frequency::Both,
            recent_quarters: 8,
            categories: all_categories(),
            measures: None,
            peer_symbols: vec![],
            sector_hint: None,
            allow_web_research: true,
            supplied_data: None,
            previous_response: None,
            signal_history: true,
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
    /// Empty string for supplied input / recall.
    pub url: String,
    pub period_covered: String,
    pub retrieved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataPoint {
    pub period_end: String,
    pub period_type: PeriodType,
    /// In the reporting currency.
    pub value: Option<f64>,
    /// Converted into the request's base currency (flows at average rate, stocks at closing rate,
    /// point-in-time at spot); None when no base currency, value is None, or unit is not money.
    /// Defaults to None when absent, so result files from before this field still load.
    #[serde(default)]
    pub value_base: Option<f64>,
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
    /// ISO 4217 for currency / per_share units, otherwise empty.
    pub currency: String,
    pub scale: Scale,
    pub latest: DataPoint,
    pub series: Vec<DataPoint>,
    pub trend: Trend,
    pub reliability_pct: u8,
    pub reliability_basis: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holder {
    pub name: String,
    pub pct_of_shares: Option<f64>,
    pub holder_type: HolderType,
    pub as_of: String,
    pub reliability_pct: u8,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ownership {
    /// false when the ownership category was not requested (other fields empty / 0).
    pub requested: bool,
    pub top_holders: Vec<Holder>,
    pub recent_insider_activity: String,
    pub reliability_pct: u8,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    /// false when the context category was not requested (other fields empty / 0).
    pub requested: bool,
    pub industry: String,
    pub peer_set_used: Vec<String>,
    pub segment_notes: String,
    pub management_commentary: String,
    pub dividend_policy: String,
    pub accounting_flags: String,
    pub reliability_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FxRate {
    pub period_end: String,
    pub period_type: PeriodType,
    /// Base-currency units per 1 reporting-currency unit, period average.
    pub average_rate: f64,
    /// Base-currency units per 1 reporting-currency unit at period end.
    pub closing_rate: f64,
    pub source_ids: Vec<String>,
    pub reliability_pct: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fx {
    pub reporting_currency: String,
    /// From the request, or empty when none was given.
    pub base_currency: String,
    /// One entry per period in the series; empty when no base currency.
    pub rates: Vec<FxRate>,
    pub method_note: String,
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

/// Point-in-time pillar scores, 0-100 (100 = most favourable).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PillarScores {
    pub valuation: u8,
    pub profitability_stability: u8,
    pub debt_health: u8,
    pub earnings_quality: u8,
    pub trends: u8,
}

/// The framework applied as of `period_end`, using only information available at that date.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalPoint {
    pub period_end: String,
    pub period_type: PeriodType,
    pub category: Signal,
    pub conviction_pct: u8,
    pub reliability_pct: u8,
    /// false when no price / multiples existed for that date; extreme categories then unavailable.
    pub valuation_assessed: bool,
    pub pillar_scores: PillarScores,
    pub decisive_factor: String,
    pub source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataGap {
    /// A catalogue id, or empty string for a general gap.
    pub measure_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisResponse {
    pub schema_version: String,
    pub symbol: String,
    /// Empty if unknown.
    pub company_name: String,
    pub analysis_date: String,
    pub data_mode: DataMode,
    pub sources: Vec<Source>,
    pub measures: Vec<Measure>,
    pub ownership: Ownership,
    pub context: Context,
    pub pillars: Vec<Pillar>,
    pub trend_summary: TrendSummary,
    pub signal: SignalBlock,
    /// Chronological, one point per fiscal year-end (plus latest TTM/quarter); empty when not requested.
    /// Defaults to empty when absent (result files from before this field).
    #[serde(default)]
    pub signal_history: Vec<SignalPoint>,
    /// Reporting currency, listing currency and the FX rate used for price-side conversion, or "no conversion".
    /// Defaults to empty when absent (result files from before this field).
    #[serde(default)]
    pub currency_notes: String,
    /// Defaults to an empty block when absent (result files from before this field).
    #[serde(default)]
    pub fx: Fx,
    pub sector_adjustments: Vec<String>,
    pub data_gaps: Vec<DataGap>,
    pub overall_reliability_pct: u8,
    pub disclaimer: String,
}

impl MeasureId {
    /// The catalogue category each measure belongs to (mirrors the catalogue in the system prompt).
    pub fn category(self) -> Category {
        use MeasureId::*;
        match self {
            SharePrice
            | MarketCap
            | EnterpriseValue
            | PeTrailing
            | PeForward
            | Pb
            | Ps
            | EvEbitda
            | EvSales
            | FcfYield
            | EarningsYield
            | Peg
            | DividendYield
            | PayoutRatio
            | PeHistoricalPercentile
            | EvEbitdaHistoricalPercentile => Category::Valuation,
            Revenue
            | GrossProfit
            | GrossMargin
            | OperatingIncome
            | OperatingMargin
            | Ebitda
            | EbitdaMargin
            | NetIncome
            | NetMargin
            | EpsDiluted
            | Roe
            | Roa
            | Roic
            | MarginStabilityCv
            | EarningsVariabilityCv => Category::Profitability,
            InterestIncome
            | InterestExpense
            | InvestmentGainsLosses
            | FxAndDerivativeResult
            | OtherIncomeExpense
            | NetNonOperatingResult
            | NonOperatingPctRevenue
            | NonOperatingPctOperatingIncome
            | OneTimeItems => Category::NonOperating,
            LongTermDebt | ShortTermDebt | TotalDebt | CashAndEquivalents | NetDebt
            | ShareholdersEquity | TotalAssets | DebtToEquity | DebtToAssets | LtDebtToAssets
            | NetDebtToEbitda | InterestCoverage | CurrentRatio => Category::Leverage,
            OperatingCashFlow
            | Capex
            | FreeCashFlow
            | FcfMargin
            | Accruals
            | CashConversion
            | AccrualRatio
            | SloanAccrualRatio
            | PiotroskiFScore
            | WorkingCapitalChange
            | NetShareIssuance
            | CapitalReturnedPctFcf => Category::CashFlowQuality,
            RevenueGrowthYoy | RevenueCagr | OperatingIncomeCagr | NetIncomeCagr | FcfCagr
            | EpsGrowthYoy | AssetTurnover => Category::Growth,
            InstitutionalOwnershipPct
            | InsiderOwnershipPct
            | InsiderNetBuying
            | SharesOutstanding
            | SharesOutstandingChangePct => Category::Ownership,
        }
    }

    /// Every catalogue measure, in catalogue order.
    pub fn all() -> Vec<MeasureId> {
        use MeasureId::*;
        vec![
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
            InterestIncome,
            InterestExpense,
            InvestmentGainsLosses,
            FxAndDerivativeResult,
            OtherIncomeExpense,
            NetNonOperatingResult,
            NonOperatingPctRevenue,
            NonOperatingPctOperatingIncome,
            OneTimeItems,
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
            RevenueGrowthYoy,
            RevenueCagr,
            OperatingIncomeCagr,
            NetIncomeCagr,
            FcfCagr,
            EpsGrowthYoy,
            AssetTurnover,
            InstitutionalOwnershipPct,
            InsiderOwnershipPct,
            InsiderNetBuying,
            SharesOutstanding,
            SharesOutstandingChangePct,
        ]
    }
}

/// Catalogue measures the request asks for: the `measures` whitelist if given, otherwise every
/// catalogue measure in the requested categories.
pub fn requested_measures(req: &AnalysisRequest) -> Vec<MeasureId> {
    match &req.measures {
        Some(list) => list.clone(),
        None => MeasureId::all()
            .into_iter()
            .filter(|m| req.categories.contains(&m.category()))
            .collect(),
    }
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
    for sp in &r.signal_history {
        chk(
            sp.conviction_pct,
            &format!("signal_history {} conviction", sp.period_end),
            &mut errs,
        );
        chk(
            sp.reliability_pct,
            &format!("signal_history {} reliability", sp.period_end),
            &mut errs,
        );
        for (n, v) in [
            ("valuation", sp.pillar_scores.valuation),
            (
                "profitability_stability",
                sp.pillar_scores.profitability_stability,
            ),
            ("debt_health", sp.pillar_scores.debt_health),
            ("earnings_quality", sp.pillar_scores.earnings_quality),
            ("trends", sp.pillar_scores.trends),
        ] {
            chk(
                v,
                &format!("signal_history {} {n}", sp.period_end),
                &mut errs,
            );
        }
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

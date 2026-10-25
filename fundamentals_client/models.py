"""Pydantic models for the fundamentals-analysis request and the structured response.

The response model (`AnalysisResponse`) is handed to the Anthropic SDK as
`output_format`; the SDK converts it to a JSON Schema and the API constrains
Claude's output to that schema (structured outputs). Numeric bounds such as
`ge=0, le=100` are stripped from the schema sent to the API and validated
client-side by the SDK/Pydantic instead.

Design rules for this file (structured-outputs constraints):
  * no recursive models
  * every object has all fields required (Optional fields use `| None`)
  * no `additionalProperties` other than false (Pydantic default + SDK transform)
"""

from __future__ import annotations

from enum import Enum
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field


class Strict(BaseModel):
    """Base model: forbid unknown keys so client-side validation is as strict as the API."""

    model_config = ConfigDict(extra="forbid")


# --------------------------------------------------------------------------- #
# Catalogue of measures (ids are stable identifiers your app can key on)
# --------------------------------------------------------------------------- #


class Category(str, Enum):
    valuation = "valuation"
    profitability = "profitability"
    non_operating = "non_operating"
    leverage = "leverage"
    cash_flow_quality = "cash_flow_quality"
    growth = "growth"
    ownership = "ownership"
    context = "context"


class MeasureId(str, Enum):
    # 1.1 Valuation
    share_price = "share_price"
    market_cap = "market_cap"
    enterprise_value = "enterprise_value"
    pe_trailing = "pe_trailing"
    pe_forward = "pe_forward"
    pb = "pb"
    ps = "ps"
    ev_ebitda = "ev_ebitda"
    ev_sales = "ev_sales"
    fcf_yield = "fcf_yield"
    earnings_yield = "earnings_yield"
    peg = "peg"
    dividend_yield = "dividend_yield"
    payout_ratio = "payout_ratio"
    pe_historical_percentile = "pe_historical_percentile"
    ev_ebitda_historical_percentile = "ev_ebitda_historical_percentile"
    # 1.2 Profitability / margins / returns
    revenue = "revenue"
    gross_profit = "gross_profit"
    gross_margin = "gross_margin"
    operating_income = "operating_income"
    operating_margin = "operating_margin"
    ebitda = "ebitda"
    ebitda_margin = "ebitda_margin"
    net_income = "net_income"
    net_margin = "net_margin"
    eps_diluted = "eps_diluted"
    roe = "roe"
    roa = "roa"
    roic = "roic"
    margin_stability_cv = "margin_stability_cv"
    earnings_variability_cv = "earnings_variability_cv"
    # 1.3 Non-operating items
    interest_income = "interest_income"
    interest_expense = "interest_expense"
    investment_gains_losses = "investment_gains_losses"
    fx_and_derivative_result = "fx_and_derivative_result"
    other_income_expense = "other_income_expense"
    net_non_operating_result = "net_non_operating_result"
    non_operating_pct_revenue = "non_operating_pct_revenue"
    non_operating_pct_operating_income = "non_operating_pct_operating_income"
    one_time_items = "one_time_items"
    # 1.4 Debt / leverage
    long_term_debt = "long_term_debt"
    short_term_debt = "short_term_debt"
    total_debt = "total_debt"
    cash_and_equivalents = "cash_and_equivalents"
    net_debt = "net_debt"
    shareholders_equity = "shareholders_equity"
    total_assets = "total_assets"
    debt_to_equity = "debt_to_equity"
    debt_to_assets = "debt_to_assets"
    lt_debt_to_assets = "lt_debt_to_assets"
    net_debt_to_ebitda = "net_debt_to_ebitda"
    interest_coverage = "interest_coverage"
    current_ratio = "current_ratio"
    # 1.5 Cash flow / earnings quality
    operating_cash_flow = "operating_cash_flow"
    capex = "capex"
    free_cash_flow = "free_cash_flow"
    fcf_margin = "fcf_margin"
    accruals = "accruals"
    cash_conversion = "cash_conversion"
    accrual_ratio = "accrual_ratio"
    sloan_accrual_ratio = "sloan_accrual_ratio"
    piotroski_f_score = "piotroski_f_score"
    working_capital_change = "working_capital_change"
    net_share_issuance = "net_share_issuance"
    capital_returned_pct_fcf = "capital_returned_pct_fcf"
    # 1.6 Growth / efficiency
    revenue_growth_yoy = "revenue_growth_yoy"
    revenue_cagr = "revenue_cagr"
    operating_income_cagr = "operating_income_cagr"
    net_income_cagr = "net_income_cagr"
    fcf_cagr = "fcf_cagr"
    eps_growth_yoy = "eps_growth_yoy"
    asset_turnover = "asset_turnover"
    # 1.7 Ownership
    institutional_ownership_pct = "institutional_ownership_pct"
    insider_ownership_pct = "insider_ownership_pct"
    insider_net_buying = "insider_net_buying"
    shares_outstanding = "shares_outstanding"
    shares_outstanding_change_pct = "shares_outstanding_change_pct"


class Unit(str, Enum):
    currency = "currency"  # absolute amount in `currency`, in `scale` units
    percent = "percent"  # 12.5 means 12.5 %
    ratio = "ratio"  # e.g. D/E 0.45, cash conversion 1.1
    multiple = "multiple"  # e.g. P/E 18.2x
    count = "count"  # shares, holders
    score = "score"  # Piotroski 0-9, percentile 0-100
    per_share = "per_share"  # EPS, DPS in `currency`


class Scale(str, Enum):
    units = "units"
    thousands = "thousands"
    millions = "millions"
    billions = "billions"


class Method(str, Enum):
    reported = "reported"  # figure read directly from a source
    derived = "derived"  # computed arithmetically from reported figures
    estimated = "estimated"  # approximated, extrapolated, or from memory
    unavailable = "unavailable"  # value is null


class PeriodType(str, Enum):
    FY = "FY"
    Q = "Q"
    TTM = "TTM"
    FWD = "FWD"  # forward estimate
    POINT = "POINT"  # point-in-time snapshot (price, ownership)


class Frequency(str, Enum):
    annual = "annual"
    quarterly = "quarterly"
    both = "both"


class Direction(str, Enum):
    improving = "improving"
    stable = "stable"
    deteriorating = "deteriorating"
    volatile = "volatile"
    insufficient_data = "insufficient_data"


class RelativePosition(str, Enum):
    bottom_quartile = "bottom_quartile"
    below_median = "below_median"
    above_median = "above_median"
    top_quartile = "top_quartile"
    unknown = "unknown"


class PeerComparison(str, Enum):
    better = "better"
    in_line = "in_line"
    worse = "worse"
    unknown = "unknown"


class SourceType(str, Enum):
    primary_filing = "primary_filing"  # 10-K, 10-Q, annual report, proxy, 13F
    company_ir = "company_ir"  # company IR site, earnings release, presentation
    aggregator = "aggregator"  # Yahoo Finance, Morningstar, Macrotrends, etc.
    supplied_input = "supplied_input"  # data the calling app passed in the request
    supplied_screenshot = "supplied_screenshot"  # figure read from an image the caller attached (e.g. IB)
    web_other = "web_other"  # any other web page
    model_recall = "model_recall"  # no retrievable source; from the model's memory


class Signal(str, Enum):
    extreme_buy = "extreme_buy"
    buy = "buy"
    hold = "hold"
    sell = "sell"
    extreme_sell = "extreme_sell"


class PillarId(str, Enum):
    valuation = "valuation"
    profitability_stability = "profitability_stability"
    debt_health = "debt_health"
    earnings_quality = "earnings_quality"
    trends = "trends"


class Assessment(str, Enum):
    strong = "strong"
    favorable = "favorable"
    neutral = "neutral"
    weak = "weak"
    poor = "poor"
    not_assessed = "not_assessed"


class DataMode(str, Enum):
    supplied = "supplied"  # all figures from data passed in the request
    retrieved = "retrieved"  # all figures from web research
    mixed = "mixed"
    recall_only = "recall_only"  # no data supplied and web research disabled


# --------------------------------------------------------------------------- #
# Request (what your app sends; rendered as JSON into the user message)
# --------------------------------------------------------------------------- #


class Period(Strict):
    start: str = Field(description="ISO date, inclusive, e.g. 2016-01-01")
    end: str = Field(description="ISO date, inclusive, e.g. 2025-12-31")


class ImageAttachment(Strict):
    """A screenshot (e.g. Interactive Brokers fundamentals / ownership pages) sent as an image block.

    Provide either `path` (resolved by the CLI relative to the request file) or `data` (base64).
    Supported: image/png, image/jpeg, image/gif, image/webp; <= 5 MB each, at most 100 per request.
    Keep the long edge <= 1568 px: larger images are downscaled server-side anyway.
    """

    media_type: Literal["image/png", "image/jpeg", "image/gif", "image/webp"] = "image/png"
    path: str | None = Field(default=None, description="File path; the CLI reads and base64-encodes it")
    data: str | None = Field(default=None, description="Base64-encoded image bytes (no data: prefix)")
    caption: str = Field(
        description="What the image shows and when it was captured, e.g. "
        "'IB TWS > Fundamentals > Financial Summary, annual, captured 2026-10-20'"
    )


class AnalysisRequest(Strict):
    symbol: str = Field(description="Ticker symbol, e.g. AAPL, ASML, 'ASML.AS'")
    exchange: str | None = Field(default=None, description="Exchange hint, e.g. NASDAQ, Euronext Amsterdam")
    company_name: str | None = None
    reporting_currency: str | None = Field(default=None, description="ISO 4217, e.g. USD, EUR")
    period: Period
    frequency: Frequency = Frequency.annual
    categories: list[Category] = Field(
        default_factory=lambda: [c for c in Category],
        description="Subset of the framework's data categories to analyse.",
    )
    measures: list[MeasureId] | None = Field(
        default=None,
        description="Optional whitelist of measure ids. When set, only these are produced "
        "(plus whatever the signal needs, flagged in notes).",
    )
    peer_symbols: list[str] = Field(default_factory=list)
    sector_hint: str | None = Field(default=None, description="e.g. 'regulated utility', 'enterprise software'")
    allow_web_research: bool = Field(
        default=True,
        description="Let Claude use the web_search / web_fetch server tools to gather figures.",
    )
    supplied_data: dict[str, Any] | None = Field(
        default=None,
        description="Raw financial-statement data from your own provider (any JSON shape; "
        "describe units/periods inside it). When present, it is the preferred source.",
    )
    images: list[ImageAttachment] = Field(
        default_factory=list,
        description="Screenshots to read figures from; sent as image blocks, not inside the JSON payload.",
    )
    notes: str | None = Field(default=None, description="Free-text instructions within the framework's scope.")


# --------------------------------------------------------------------------- #
# Response (schema enforced by the API via structured outputs)
# --------------------------------------------------------------------------- #


class Source(Strict):
    id: str = Field(description="Short handle referenced by data points, e.g. S1")
    type: SourceType
    title: str
    url: str | None
    period_covered: str | None = Field(description="e.g. 'FY2024 10-K', 'as of 2025-09-30'")
    retrieved_at: str | None = Field(description="ISO date when the page was fetched, or null for supplied/recall")


class DataPoint(Strict):
    period_end: str = Field(description="ISO date of the period end (or snapshot date)")
    period_type: PeriodType
    value: float | None = Field(description="null when unavailable; never invent a number")
    reliability_pct: int = Field(ge=0, le=100, description="0-100 per the rubric in the system prompt")
    method: Method
    source_ids: list[str] = Field(description="ids from `sources`; empty only when method is unavailable")


class Trend(Strict):
    direction: Direction
    cagr_pct: float | None = Field(description="Compound annual growth rate over the series, % per year")
    latest_yoy_change_pct: float | None
    coefficient_of_variation: float | None = Field(description="stdev / mean of the series, as a ratio")
    vs_own_history: RelativePosition = Field(description="Where the latest value sits in the company's own range")
    vs_peers: PeerComparison
    reliability_pct: int = Field(ge=0, le=100)
    comment: str = Field(description="One or two sentences; what drives the trend")


class Measure(Strict):
    measure_id: MeasureId
    category: Category
    label: str
    unit: Unit
    currency: str | None = Field(description="ISO 4217 when unit is currency/per_share, else null")
    scale: Scale | None = Field(description="Scale for currency amounts, null otherwise")
    latest: DataPoint
    series: list[DataPoint] = Field(description="Chronological, oldest first, within the requested period")
    trend: Trend
    reliability_pct: int = Field(ge=0, le=100, description="Overall reliability of this measure")
    reliability_basis: str = Field(description="Why this reliability: source quality, period alignment, assumptions")
    notes: str | None


class Holder(Strict):
    name: str
    pct_of_shares: float | None
    holder_type: Literal["institution", "insider", "strategic", "fund", "other"]
    as_of: str | None
    reliability_pct: int = Field(ge=0, le=100)
    source_ids: list[str]


class Ownership(Strict):
    top_holders: list[Holder]
    recent_insider_activity: str | None = Field(description="Summary of buys/sells in the period, or null")
    reliability_pct: int = Field(ge=0, le=100)
    notes: str | None


class Context(Strict):
    industry: str | None
    peer_set_used: list[str]
    segment_notes: str | None
    management_commentary: str | None
    dividend_policy: str | None
    accounting_flags: str | None = Field(description="Restatements, auditor notes, off-balance-sheet items")
    reliability_pct: int = Field(ge=0, le=100)


class Pillar(Strict):
    pillar: PillarId
    assessment: Assessment
    score_0_100: int = Field(ge=0, le=100, description="100 = most favourable for a long position")
    key_measures: list[MeasureId]
    rationale: str
    reliability_pct: int = Field(ge=0, le=100)


class Inflection(Strict):
    measure_id: MeasureId
    period_end: str
    description: str


class TrendSummary(Strict):
    overall_direction: Direction
    improving: list[MeasureId]
    stable: list[MeasureId]
    deteriorating: list[MeasureId]
    inflections: list[Inflection]
    operating_vs_non_operating: str = Field(
        description="Whether core operating performance differs from reported results"
    )
    reliability_pct: int = Field(ge=0, le=100)


class SignalBlock(Strict):
    category: Signal
    conviction_pct: int = Field(ge=0, le=100, description="How strongly the evidence supports the category")
    reliability_pct: int = Field(ge=0, le=100, description="Bounded by the reliability of the inputs")
    rationale: str
    green_flags: list[str]
    red_flags: list[str]
    conditions_for_upgrade: list[str]
    conditions_for_downgrade: list[str]


class DataGap(Strict):
    measure_id: MeasureId | None
    description: str


class RequestEcho(Strict):
    symbol: str
    company_name: str | None
    exchange: str | None
    reporting_currency: str | None
    period_start: str
    period_end: str
    frequency: Frequency
    categories: list[Category]


class AnalysisResponse(Strict):
    schema_version: Literal["1.0"]
    request: RequestEcho
    analysis_date: str = Field(description="ISO date the analysis was produced")
    data_mode: DataMode
    sources: list[Source]
    measures: list[Measure]
    ownership: Ownership | None
    context: Context | None
    pillars: list[Pillar]
    trend_summary: TrendSummary
    signal: SignalBlock
    sector_adjustments: list[str] = Field(description="Threshold adjustments applied for the sector")
    data_gaps: list[DataGap]
    overall_reliability_pct: int = Field(ge=0, le=100)
    disclaimer: str

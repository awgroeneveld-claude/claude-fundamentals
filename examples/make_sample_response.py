"""Builds examples/sample_response.json from examples/request_supplied_data.json.

This is NOT model output; it is a hand-built reference showing the shape, the
reliability rubric and the aggregation rules applied correctly, so that tests
and integrators have a known-good document to compare against.
"""

from __future__ import annotations

import json
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parent
req = json.loads((ROOT / "request_supplied_data.json").read_text())
sd = req["supplied_data"]
years = ["2021", "2022", "2023", "2024"]
IS, BS, CF = sd["income_statement"], sd["balance_sheet"], sd["cash_flow"]

REPORTED, DERIVED, TREND_REPORTED, TREND_DERIVED = 100, 92, 95, 90
S1 = "S1"


def r(x, n=2):
    return None if x is None else round(x, n)


def point(year, value, method, rel):
    return {
        "period_end": f"{year}-12-31",
        "period_type": "FY",
        "value": r(value),
        "value_base": None,
        "reliability_pct": rel if value is not None else 0,
        "method": method if value is not None else "unavailable",
        "source_ids": [S1] if value is not None else [],
    }


def trend(values, favourable_up=True, rel=TREND_DERIVED, comment=""):
    vals = [v for v in values if v is not None]
    if len(vals) < 2:
        return dict(direction="insufficient_data", cagr_pct=None, latest_yoy_change_pct=None,
                    coefficient_of_variation=None, vs_own_history="unknown", vs_peers="unknown",
                    reliability_pct=0, comment=comment or "Not enough data points.")
    n = len(vals) - 1
    cagr = ((vals[-1] / vals[0]) ** (1 / n) - 1) * 100 if vals[0] > 0 and vals[-1] > 0 else None
    yoy = (vals[-1] / vals[-2] - 1) * 100 if vals[-2] not in (0, None) and vals[-2] > 0 else None
    mean = statistics.fmean(vals)
    cv = statistics.pstdev(vals) / abs(mean) if mean else None
    change = vals[-1] - vals[0]
    good = change > 0 if favourable_up else change < 0
    if cv is not None and cv > 0.35 and abs(change) < 0.1 * abs(mean):
        direction = "volatile"
    elif abs(change) < 0.03 * abs(mean):
        direction = "stable"
    else:
        direction = "improving" if good else "deteriorating"
    srt = sorted(vals)
    rank = srt.index(vals[-1]) / (len(srt) - 1)
    pos = ("bottom_quartile" if rank <= 0.25 else "below_median" if rank < 0.5
           else "above_median" if rank < 0.75 else "top_quartile")
    return dict(direction=direction, cagr_pct=r(cagr), latest_yoy_change_pct=r(yoy),
                coefficient_of_variation=r(cv, 3), vs_own_history=pos, vs_peers="unknown",
                reliability_pct=rel, comment=comment)


def measure(mid, cat, label, unit, values, method, rel, trend_rel, favourable_up, basis, notes="",
            currency="", scale="not_applicable", comment=""):
    series = [point(y, v, method, rel) for y, v in zip(years, values)]
    return {
        "measure_id": mid, "category": cat, "label": label, "unit": unit,
        "currency": currency, "scale": scale,
        "latest": series[-1], "series": series,
        "trend": trend(values, favourable_up, trend_rel, comment),
        "reliability_pct": rel if any(v is not None for v in values) else 0,
        "reliability_basis": basis, "notes": notes,
    }


rev = [IS[y]["revenue"] for y in years]
op_m = [IS[y]["operating_income"] / IS[y]["revenue"] * 100 for y in years]
net_m = [IS[y]["net_income"] / IS[y]["revenue"] * 100 for y in years]
de = [BS[y]["total_debt"] / BS[y]["equity"] for y in years]
cov = [IS[y]["operating_income"] / IS[y]["interest_expense"] for y in years]
ocf = [CF[y]["operating_cash_flow"] for y in years]
fcf = [CF[y]["operating_cash_flow"] - CF[y]["capex"] for y in years]
conv = [CF[y]["operating_cash_flow"] / IS[y]["net_income"] for y in years]
nde = [None] * 4  # EBITDA not supplied

GBP = dict(currency="GBP", scale="millions")
sup = "Read directly from supplied_data (audited annual reports); period and units match."
der = "Derived arithmetically from supplied figures rated 100; derivation band 80-94."

measures = [
    measure("revenue", "profitability", "Revenue", "currency", rev, "reported", REPORTED, TREND_REPORTED, True, sup,
            comment="Growth slowed from 8.5% (2022) to 2.2% (2024).", **GBP),
    measure("operating_margin", "profitability", "Operating margin", "percent", op_m, "derived", DERIVED, TREND_DERIVED, True, der,
            comment="Peaked at 12.5% in FY2023, fell to 11.3% in FY2024 as revenue growth stalled."),
    measure("net_margin", "profitability", "Net profit margin", "percent", net_m, "derived", DERIVED, TREND_DERIVED, True, der,
            comment="Compressed to 6.5% in FY2024; rising interest expense is the main driver below EBIT."),
    measure("debt_to_equity", "leverage", "Debt-to-equity", "ratio", de, "derived", DERIVED, TREND_DERIVED, False, der,
            comment="Total debt grew 66% over the period against 13% equity growth."),
    measure("interest_coverage", "leverage", "Interest coverage (EBIT / interest)", "ratio", cov, "derived", DERIVED, TREND_DERIVED, True, der,
            comment="Coverage halved from 9.1x to 4.4x; interest expense more than doubled."),
    measure("net_debt_to_ebitda", "leverage", "Net debt / EBITDA", "ratio", nde, "unavailable", 0, 0, False,
            "EBITDA (depreciation and amortisation) not present in supplied_data; web research disabled.",
            notes="Net debt itself is computable (GBP 890m at FY2024) but no EBITDA denominator was supplied."),
    measure("operating_cash_flow", "cash_flow_quality", "Operating cash flow", "currency", ocf, "reported", REPORTED, TREND_REPORTED, True, sup,
            comment="Declined every year while net income rose until FY2023.", **GBP),
    measure("free_cash_flow", "cash_flow_quality", "Free cash flow", "currency", fcf, "derived", DERIVED, TREND_DERIVED, True, der,
            comment="Turned negative in FY2024 (GBP -19m) as capex rose and OCF fell.", **GBP),
    measure("cash_conversion", "cash_flow_quality", "Cash conversion (OCF / net income)", "ratio", conv, "derived", DERIVED, TREND_DERIVED, True, der,
            comment="Fell below 1.0 in FY2023 and FY2024: earnings increasingly accrual-based."),
]

by_id = {m["measure_id"]: m for m in measures}


def pillar(pid, assessment, score, keys, rationale):
    rels = [by_id[k]["reliability_pct"] for k in keys]  # unavailable key measures (0) count: they weaken the pillar
    rel = int(statistics.fmean(rels)) if rels else 0
    return dict(pillar=pid, assessment=assessment, score_0_100=score, key_measures=keys, rationale=rationale,
                reliability_pct=rel)


pillars = [
    pillar("valuation", "not_assessed", 50, [], "Valuation category not requested and no price data supplied."),
    pillar("profitability_stability", "weak", 38, ["revenue", "operating_margin", "net_margin"],
           "Margins are stable in level (CV 0.04) but turned down in FY2024; net margin at a four-year low of 6.5%."),
    pillar("debt_health", "weak", 30, ["debt_to_equity", "interest_coverage", "net_debt_to_ebitda"],
           "D/E up from 0.55 to 0.80 and coverage down from 9.1x to 4.4x in three years; net debt/EBITDA unavailable."),
    pillar("earnings_quality", "poor", 22, ["operating_cash_flow", "free_cash_flow", "cash_conversion"],
           "Cash conversion 0.87 and negative FCF in FY2024 while dividends and buybacks (GBP 120m) were funded with debt."),
    pillar("trends", "weak", 30, ["revenue", "operating_margin", "debt_to_equity", "cash_conversion"],
           "Revenue growth decelerating, margins compressing, leverage rising, cash conversion falling: all four series point the same way."),
]
assessed = [p["reliability_pct"] for p in pillars if p["assessment"] != "not_assessed"]
signal_rel = min(int(statistics.fmean(assessed)), min(assessed) + 20)

all_points = [p for m in measures for p in m["series"] if p["value"] is not None]
overall = int(round(statistics.fmean(p["reliability_pct"] for p in all_points)))

resp = {
    "schema_version": "1.0",
    "symbol": req["symbol"],
    "company_name": req.get("company_name") or "",
    "analysis_date": "2026-10-25",
    "data_mode": "supplied",
    "sources": [{
        "id": S1, "type": "supplied_input", "title": "Caller-supplied statements: audited annual reports via internal data warehouse",
        "url": "", "period_covered": "FY2021-FY2024, GBP millions", "retrieved_at": "",
    }],
    "measures": measures,
    "ownership": {"requested": False, "top_holders": [], "recent_insider_activity": "", "reliability_pct": 0, "notes": ""},
    "context": {"requested": False, "industry": "", "peer_set_used": [], "segment_notes": "", "management_commentary": "",
                "dividend_policy": "", "accounting_flags": "", "reliability_pct": 0},
    "pillars": pillars,
    "trend_summary": {
        "overall_direction": "deteriorating",
        "improving": [],
        "stable": ["revenue"],
        "deteriorating": ["operating_margin", "net_margin", "debt_to_equity", "interest_coverage", "operating_cash_flow", "free_cash_flow", "cash_conversion"],
        "inflections": [
            {"measure_id": "free_cash_flow", "period_end": "2024-12-31", "description": "FCF turned negative for the first time in the period."},
            {"measure_id": "cash_conversion", "period_end": "2023-12-31", "description": "OCF fell below net income and stayed there."},
        ],
        "operating_vs_non_operating": "Core operating margin eroded ~1pt from its FY2023 peak; the larger hit to net margin comes from interest expense (finance cost) rising from 1.3% to 2.6% of revenue.",
        "reliability_pct": 90,
    },
    "signal": {
        "category": "sell",
        "conviction_pct": 72,
        "reliability_pct": signal_rel,
        "rationale": "Three assessed pillars are weak-to-poor and move in the same direction: leverage and interest burden rising, cash conversion below 1.0 for two years, FCF negative while shareholder distributions were debt-funded. Valuation was not assessed, so extreme_sell is not available under the alignment rule.",
        "green_flags": ["Revenue still growing (5.7% CAGR)", "Gross margin broadly stable at ~36%"],
        "red_flags": ["Interest coverage 4.4x and falling", "Negative FCF in FY2024", "Cash conversion 0.87", "D/E 0.80, up 46% since FY2021"],
        "conditions_for_upgrade": ["Cash conversion back above 1.0 for two consecutive years", "Capex normalising so FCF covers distributions", "Interest coverage above 6x"],
        "conditions_for_downgrade": ["Valuation data showing P/E or EV/EBITDA above peer median", "Further coverage decline below 3x", "Dividend maintained with continued negative FCF"],
    },
    "signal_history": [
        {"period_end": "2021-12-31", "period_type": "FY", "category": "hold", "conviction_pct": 55, "reliability_pct": 85,
         "valuation_assessed": False, "pillar_scores": {"valuation": 0, "profitability_stability": 62, "debt_health": 66, "earnings_quality": 70, "trends": 58},
         "decisive_factor": "Cash conversion 1.36 and coverage 9.1x; nothing deteriorating yet.", "source_ids": ["S1"]},
        {"period_end": "2022-12-31", "period_type": "FY", "category": "hold", "conviction_pct": 50, "reliability_pct": 85,
         "valuation_assessed": False, "pillar_scores": {"valuation": 0, "profitability_stability": 58, "debt_health": 60, "earnings_quality": 62, "trends": 50},
         "decisive_factor": "Margins flat, debt up 13%, cash conversion still above 1.0.", "source_ids": ["S1"]},
        {"period_end": "2023-12-31", "period_type": "FY", "category": "sell", "conviction_pct": 58, "reliability_pct": 85,
         "valuation_assessed": False, "pillar_scores": {"valuation": 0, "profitability_stability": 55, "debt_health": 42, "earnings_quality": 38, "trends": 40},
         "decisive_factor": "Cash conversion fell below 1.0 while debt rose 28% in a year.", "source_ids": ["S1"]},
        {"period_end": "2024-12-31", "period_type": "FY", "category": "sell", "conviction_pct": 72, "reliability_pct": 81,
         "valuation_assessed": False, "pillar_scores": {"valuation": 0, "profitability_stability": 38, "debt_health": 30, "earnings_quality": 22, "trends": 30},
         "decisive_factor": "Negative FCF, coverage 4.4x, net margin at a four-year low.", "source_ids": ["S1"]},
    ],
    "currency_notes": "Reporting currency GBP (statements); no listing price supplied; no conversion.",
    "fx": {"reporting_currency": "GBP", "base_currency": "", "rates": [], "method_note": "No base currency requested."},
    "sector_adjustments": ["Industrial machinery: D/E up to ~1.0 and net debt/EBITDA up to ~2.5x treated as moderate rather than high."],
    "data_gaps": [
        {"measure_id": "net_debt_to_ebitda", "description": "No depreciation/amortisation in supplied_data; cannot compute EBITDA."},
        {"measure_id": "", "description": "Valuation category not requested; no price, market cap or share count supplied."},
    ],
    "overall_reliability_pct": overall,
    "disclaimer": "Analytical output for research purposes; not personalised investment advice.",
}

out = ROOT / "sample_response.json"
out.write_text(json.dumps(resp, indent=2) + "\n", encoding="utf-8")
print(f"wrote {out}: signal={resp['signal']['category']} rel={signal_rel} overall={overall}")

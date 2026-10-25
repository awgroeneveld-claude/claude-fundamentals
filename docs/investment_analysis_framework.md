# Investment Analysis Framework: Identifying Trends and Determining Entry/Exit Points

**Author perspective:** Investment analyst focused on fundamental analysis for long-term and medium-term equity decisions.  
**Purpose:** Define the data to gather from company filings and reports to identify trends in business quality, valuation, and financial health, then map those into a practical buy/hold/sell framework with refined categories (Extreme Buy / Buy / Hold / Sell / Extreme Sell).

This framework emphasizes multi-year trends (typically 5–10 years where available), comparison to industry peers and the company’s own history, and quality of earnings rather than single-period snapshots. Thresholds are illustrative guidelines; they must be adjusted for sector norms (e.g., utilities and banks tolerate higher leverage than software or consumer staples).

---

## 1. Core Data Categories to Gather

Gather raw figures from income statements, balance sheets, cash-flow statements, and notes, preferably for at least the trailing 5–10 years plus the latest trailing twelve months (TTM) and forward estimates where reliable.

### 1.1 Valuation
- Current share price and market capitalization
- Trailing and forward Price/Earnings (P/E) ratio
- Price/Book (P/B), Price/Sales (P/S), Enterprise Value / EBITDA (EV/EBITDA)
- Free Cash Flow yield (FCF / Market Cap or EV)
- Dividend yield and payout ratio (if applicable)
- Historical P/E range (e.g., 5-year or 10-year average, min, max, percentiles)

### 1.2 Profitability and Earnings as % of Turnover (Margins)
- Revenue (turnover)
- Gross profit and Gross Margin
- Operating profit (EBIT) and Operating Margin
- Net income and Net Profit Margin (earnings as % of turnover)
- EBITDA and EBITDA margin
- Stability/variability: multi-year series of margins; calculate standard deviation, coefficient of variation, or simple trend (improving/stable/declining)
- Return on Equity (ROE), Return on Assets (ROA), Return on Invested Capital (ROIC)

### 1.3 Profit/Loss from Finance and Investing (Non-Operating Items)
- Interest income and interest expense
- Gains/losses on investments, asset sales, foreign exchange, derivatives
- Other income / other expense
- Net non-operating result (as absolute amount and as % of revenue or operating income)
- Distinction between recurring vs. one-time items (from notes)
- Impact on pretax income and net income

### 1.4 Long-Term Debt and Debt Ratios
- Long-term debt (and short-term debt / current portion of long-term debt)
- Total debt
- Cash and equivalents, short-term investments
- Net debt
- Shareholders’ equity, total assets
- Interest coverage (EBIT / Interest Expense)
- Debt maturity schedule (from notes)

### 1.5 Cash Flow and Earnings Quality
- Operating Cash Flow (OCF / CFO)
- Capital expenditures (Capex)
- Free Cash Flow (FCF = OCF – Capex)
- Accruals (Net Income – OCF)
- Working capital changes
- Share issuance or buybacks

### 1.6 Growth and Trend Series
- Year-over-year and multi-year CAGR for revenue, operating income, net income, FCF
- Margin trends
- Debt trends (absolute and ratios)
- Asset turnover and efficiency metrics

### 1.7 Ownership and Shareholder Structure
- Major shareholders / beneficial owners (top holders, % of shares)
- Institutional ownership percentage
- Insider ownership percentage
- Recent insider buying/selling activity
- Share count changes (dilution or buybacks)
- **Note:** Ownership data is harder to obtain consistently via APIs. Interactive Brokers provides it in their fundamentals interface, but scraping or API access has historically been restricted or broken. Prefer primary sources: company proxy statements (DEF 14A), 13F filings, company IR pages, or established data providers (Yahoo Finance, Bloomberg, Refinitiv, FactSet, etc.).

### 1.8 Supporting Context
- Industry and peer set
- Segment reporting (if material)
- Management commentary on trends, risks, capital allocation
- Dividend history and policy
- One-time items, restatements, auditor notes

---

## 2. Key Derived Figures and Ratios

These are calculated from the raw data above to surface trends and relative attractiveness.

### Valuation
- **PEG Ratio** = P/E / Expected earnings growth rate (%)
- **Earnings Yield** = EPS / Price (or 1 / P/E)
- **FCF Yield** = Free Cash Flow / Market Capitalization (or Enterprise Value)
- **EV/EBITDA** and **EV/Sales**
- Historical percentile rank of current P/E, EV/EBITDA, etc.

### Profitability and Stability
- **Net Profit Margin** = Net Income / Revenue
- **Operating Margin** = Operating Income / Revenue
- **Margin Stability**: Standard deviation or coefficient of variation of margins over 5–10 years; or year-over-year change direction
- **ROE**, **ROA**, **ROIC** (and DuPont decomposition of ROE if useful)
- **Earnings Variability**: Coefficient of variation of EPS or Net Income over the period

### Leverage and Solvency
- **Debt-to-Equity (D/E)** = Total Debt / Shareholders’ Equity
- **Debt-to-Assets** = Total Debt / Total Assets
- **Long-term Debt / Total Assets**
- **Net Debt / EBITDA**
- **Interest Coverage** = EBIT / Interest Expense
- Trend in each ratio (improving / stable / deteriorating)

### Earnings Quality
- **Cash Conversion** = Operating Cash Flow / Net Income (ideally ≥ 1.0 over time)
- **Accrual Ratio (simple)** = (Net Income – Operating Cash Flow) / Average Total Assets  
  (Lower or negative is better; high positive values indicate lower quality)
- **Sloan-style Accrual Ratio** ≈ (Net Income – CFO – CFI) / Total Assets
- **Piotroski F-Score** (0–9): Binary tests on profitability (positive NI, positive OCF, improving ROA, OCF > NI), leverage/liquidity (declining long-term debt/assets, improving current ratio, no share issuance), and efficiency (improving gross margin, improving asset turnover). Higher scores (8–9) indicate improving financial strength.

### Other
- **FCF Margin** = FCF / Revenue
- **Capital Allocation**: % of FCF returned via dividends + buybacks vs. reinvested
- **Non-operating income as % of Operating Income** (flag if persistently large or volatile)

---

## 3. Identifying Trends

1. Collect 5–10 years of annual (and recent quarterly) data for the key series.
2. Plot or tabulate: revenue growth, margin levels and direction, ROE/ROIC, debt ratios, FCF, accruals.
3. Compare current levels to the company’s own historical range and to a relevant peer group / industry median.
4. Flag improving, stable, or deteriorating trajectories. Pay special attention to:
   - Margin expansion or compression
   - Consistency of earnings and cash conversion
   - Changes in leverage (especially rising long-term debt relative to assets or earnings)
   - Quality signals (accruals, F-Score components)
5. Separate core operating performance from non-operating finance/investing gains or losses.
6. Note ownership changes that may signal insider confidence or institutional accumulation/distribution.

---

## 4. Buy / Hold / Sell Strategy

The framework uses a multi-factor view rather than any single metric. Categories are directional signals based on the combination of valuation, quality/stability, trend direction, and financial health. Always adjust for sector and company-specific context. Position sizing and risk management remain essential.

### Extreme Buy
Strong confluence of attractive valuation **and** high-quality, improving or stable fundamentals:
- Valuation in attractive range vs. history and peers (e.g., P/E or EV/EBITDA in lower quartile of own history or below peer median; PEG < 1; solid FCF yield)
- High and stable/improving margins (earnings as % of turnover consistently strong; low variability)
- Strong profitability (ROE/ROIC well above cost of capital or peer average)
- Healthy or improving balance sheet (moderate/low D/E, Net Debt/EBITDA comfortably low for the sector, strong interest coverage, declining or stable long-term debt)
- High earnings quality (OCF ≥ Net Income, low/negative accrual ratio, high Piotroski F-Score 8–9)
- Positive growth trends or clear inflection
- Optional supporting: insider buying or rising institutional ownership

### Buy
Most of the above positive, with one or two milder caveats (e.g., valuation fair rather than cheap, or margins stable but not expanding). Still favorable risk/reward for entry or adding.

### Hold
Balanced or mixed signals:
- Valuation roughly fair relative to history and peers
- Stable (neither strongly improving nor deteriorating) margins, profitability, and debt metrics
- Adequate earnings quality
- No clear catalyst for significant upside or downside in the near-to-medium term
- Suitable for existing positions; new capital may wait for a better entry or clearer trend.

### Sell
Deteriorating fundamentals, elevated valuation, or both:
- Valuation stretched vs. history/peers (high P/E, high EV/EBITDA, low FCF yield relative to growth)
- Declining or volatile margins (earnings as % of turnover compressing)
- Rising leverage (increasing long-term debt, worsening D/E or Net Debt/EBITDA, weakening interest coverage)
- Declining earnings quality (high accruals, OCF lagging NI, falling F-Score)
- Slowing or negative growth trends without offsetting positives
- Non-operating items masking weak core operations

### Extreme Sell
Multiple red flags, high risk of permanent capital impairment or significant underperformance:
- Very high valuation combined with deteriorating core business
- High and rising debt with weak coverage or negative FCF
- Persistently high accruals / low cash conversion and low F-Score (0–3)
- Sharp margin collapse or multi-year earnings instability
- Significant non-operating losses or reliance on one-time gains
- Optional: heavy insider selling or declining institutional ownership amid weak fundamentals

**Practical application:**
- Score or checklist the main pillars (Valuation, Profitability & Stability, Debt/Health, Earnings Quality, Trends).
- Require alignment across pillars for Extreme Buy/Sell.
- Reassess periodically (quarterly or on material news) and on price moves that change valuation.
- Combine with qualitative judgment (competitive position, management, industry outlook).
- Entry is not solely price; exit is not solely valuation — deteriorating quality can override a “cheap” multiple.

---

## 5. Data Sources and Practical Considerations

- Primary: Company annual reports (10-K), quarterly reports (10-Q), earnings releases, investor presentations, proxy statements.
- Aggregators: Interactive Brokers fundamentals (useful for ownership and many ratios when available in the UI), Yahoo Finance, company IR sites, SEC EDGAR.
- **API / scraping note:** Ownership and some detailed fundamentals available in Interactive Brokers have historically been difficult or blocked for automated API access or scraping. Prefer official filings or licensed data feeds for systematic work.
- Always cross-check calculated ratios against at least one independent source and review the financial statement notes for non-recurring items, accounting changes, or off-balance-sheet items.

---

## 6. Summary Checklist for a New Position or Review

- [ ] Multi-year revenue, margins (earnings % of turnover), and variability
- [ ] Operating vs. non-operating income split and trends
- [ ] Long-term debt, D/E, Net Debt/EBITDA, interest coverage, and trends
- [ ] Cash flow vs. earnings (accruals, FCF)
- [ ] Valuation multiples vs. history and peers
- [ ] Derived quality scores (Piotroski F-Score, accrual ratio, ROIC)
- [ ] Ownership snapshot and recent insider activity (where available)
- [ ] Overall trend assessment and category (Extreme Buy → Extreme Sell)

This framework prioritizes sustainable, cash-backed earnings, reasonable leverage, and valuation discipline while remaining flexible enough for sector differences. Regular updates to the multi-year series allow trend identification that single-period ratios miss.

*This is an analytical framework for research purposes, not personalized investment advice.*

"""Merge an incremental delta (model output for a run with `previous_response`) into the stored
analysis. Deterministic: history changes only where the delta explicitly restates a point."""

from __future__ import annotations

from .models import AnalysisResponse, DataPoint, FxRate, Measure, PeriodType


def _key(p: DataPoint) -> tuple[str, str]:
    return (p.period_end, p.period_type.value)


def _merge_series(prev: list[DataPoint], new: list[DataPoint], recent_quarters: int | None) -> list[DataPoint]:
    by_key = {_key(p): p for p in prev}
    for p in new:
        by_key[_key(p)] = p  # new period, or restated point replacing the old one
    merged = sorted(by_key.values(), key=lambda p: (p.period_end, p.period_type.value))
    if recent_quarters is not None:
        quarters = [p for p in merged if p.period_type == PeriodType.Q]
        drop = {_key(p) for p in quarters[: max(0, len(quarters) - recent_quarters)]}
        merged = [p for p in merged if _key(p) not in drop]
    return merged


def merge_incremental(previous: AnalysisResponse, delta: AnalysisResponse, recent_quarters: int | None = None) -> AnalysisResponse:
    prev_measures = {m.measure_id: m for m in previous.measures}
    measures: list[Measure] = []
    seen = set()
    for d in delta.measures:
        seen.add(d.measure_id)
        pm = prev_measures.get(d.measure_id)
        series = _merge_series(pm.series if pm else [], d.series, recent_quarters)
        measures.append(d.model_copy(update={"series": series}))
    for mid, pm in prev_measures.items():
        if mid not in seen:
            measures.append(pm)  # measure the delta did not mention: carried forward unchanged

    sources = {s.id: s for s in previous.sources}
    for s in delta.sources:
        sources[s.id] = s

    history = {sp.period_end: sp for sp in previous.signal_history}
    for sp in delta.signal_history:
        history[sp.period_end] = sp

    rates = {(r.period_end, r.period_type.value): r for r in previous.fx.rates}
    for r in delta.fx.rates:
        rates[(r.period_end, r.period_type.value)] = r
    fx = delta.fx.model_copy(update={"rates": sorted(rates.values(), key=lambda r: (r.period_end, r.period_type.value))})

    return delta.model_copy(
        update={
            "measures": measures,
            "sources": list(sources.values()),
            "signal_history": sorted(history.values(), key=lambda sp: sp.period_end),
            "fx": fx,
        }
    )

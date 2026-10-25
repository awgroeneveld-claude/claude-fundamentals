//! Merge an incremental delta (model output for a run with `previous_response`) into the stored
//! analysis. Deterministic: history changes only where the delta explicitly restates a point.

use std::collections::BTreeMap;

use crate::models::{AnalysisResponse, DataPoint, FxRate, Measure, PeriodType};

fn key(p: &DataPoint) -> (String, String) {
    (p.period_end.clone(), format!("{:?}", p.period_type))
}

fn merge_series(
    prev: &[DataPoint],
    new: &[DataPoint],
    recent_quarters: Option<u32>,
) -> Vec<DataPoint> {
    let mut by_key: BTreeMap<(String, String), DataPoint> =
        prev.iter().map(|p| (key(p), p.clone())).collect();
    for p in new {
        by_key.insert(key(p), p.clone()); // new period, or restated point replacing the old one
    }
    let mut merged: Vec<DataPoint> = by_key.into_values().collect();
    if let Some(n) = recent_quarters {
        let quarters = merged
            .iter()
            .filter(|p| p.period_type == PeriodType::Q)
            .count();
        let mut to_drop = quarters.saturating_sub(n as usize);
        merged.retain(|p| {
            if p.period_type == PeriodType::Q && to_drop > 0 {
                to_drop -= 1;
                false
            } else {
                true
            }
        });
    }
    merged
}

pub fn merge_incremental(
    previous: &AnalysisResponse,
    delta: &AnalysisResponse,
    recent_quarters: Option<u32>,
) -> AnalysisResponse {
    let mut measures: Vec<Measure> = Vec::new();
    for d in &delta.measures {
        let prev_series: &[DataPoint] = previous
            .measures
            .iter()
            .find(|m| m.measure_id == d.measure_id)
            .map(|m| m.series.as_slice())
            .unwrap_or(&[]);
        let mut m = d.clone();
        m.series = merge_series(prev_series, &d.series, recent_quarters);
        measures.push(m);
    }
    for pm in &previous.measures {
        if !delta.measures.iter().any(|d| d.measure_id == pm.measure_id) {
            measures.push(pm.clone()); // not mentioned by the delta: carried forward unchanged
        }
    }

    let mut sources = previous.sources.clone();
    for s in &delta.sources {
        match sources.iter_mut().find(|x| x.id == s.id) {
            Some(existing) => *existing = s.clone(),
            None => sources.push(s.clone()),
        }
    }

    let mut history = previous.signal_history.clone();
    for sp in &delta.signal_history {
        match history.iter_mut().find(|x| x.period_end == sp.period_end) {
            Some(existing) => *existing = sp.clone(),
            None => history.push(sp.clone()),
        }
    }
    history.sort_by(|a, b| a.period_end.cmp(&b.period_end));

    let mut rates: BTreeMap<(String, String), FxRate> = previous
        .fx
        .rates
        .iter()
        .map(|r| {
            (
                (r.period_end.clone(), format!("{:?}", r.period_type)),
                r.clone(),
            )
        })
        .collect();
    for r in &delta.fx.rates {
        rates.insert(
            (r.period_end.clone(), format!("{:?}", r.period_type)),
            r.clone(),
        );
    }
    let mut fx = delta.fx.clone();
    fx.rates = rates.into_values().collect();

    let mut out = delta.clone();
    out.measures = measures;
    out.sources = sources;
    out.signal_history = history;
    out.fx = fx;
    out
}

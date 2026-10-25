//! Post-hoc checks on the reliability aggregation rules from the system prompt.
//! Port of `consistency_warnings` in `fundamentals_client/client.py`: reports, never rewrites.

use std::collections::{HashMap, HashSet};

use crate::models::{AnalysisResponse, Assessment, DataMode, MeasureId, Scale, Signal, Unit};

fn mean(xs: &[u8]) -> Option<f64> {
    if xs.is_empty() {
        None
    } else {
        Some(xs.iter().map(|&x| x as f64).sum::<f64>() / xs.len() as f64)
    }
}

pub fn consistency_warnings(r: &AnalysisResponse) -> Vec<String> {
    let mut w = Vec::new();
    let source_ids: HashSet<&str> = r.sources.iter().map(|s| s.id.as_str()).collect();
    let by_id: HashMap<MeasureId, u8> = r
        .measures
        .iter()
        .map(|m| (m.measure_id, m.reliability_pct))
        .collect();

    for m in &r.measures {
        let id = format!("{:?}", m.measure_id);
        for p in std::iter::once(&m.latest).chain(m.series.iter()) {
            match p.value {
                None if p.reliability_pct != 0 => w.push(format!(
                    "{id}: null value with reliability {}",
                    p.reliability_pct
                )),
                Some(v) if p.reliability_pct == 0 => {
                    w.push(format!("{id}: value {v} with reliability 0"))
                }
                Some(_) if p.source_ids.is_empty() => {
                    w.push(format!("{id}@{}: value without source ids", p.period_end))
                }
                _ => {}
            }
            for sid in &p.source_ids {
                if !source_ids.contains(sid.as_str()) {
                    w.push(format!("{id}@{}: unknown source id {sid}", p.period_end));
                }
            }
        }
        // Trend statistics are computed on the annual-like points (FY/TTM/FWD/POINT); quarterly
        // points only feed comments and inflections, so they don't bound the trend's reliability.
        let annual_like: Vec<u8> = m
            .series
            .iter()
            .filter(|p| p.value.is_some() && p.period_type != crate::models::PeriodType::Q)
            .map(|p| p.reliability_pct)
            .collect();
        let mut series_rel: Vec<u8> = if annual_like.is_empty() {
            m.series
                .iter()
                .filter(|p| p.value.is_some())
                .map(|p| p.reliability_pct)
                .collect()
        } else {
            annual_like
        };
        series_rel.sort_unstable();
        if !series_rel.is_empty() {
            let median = series_rel[series_rel.len() / 2];
            if m.trend.reliability_pct > median {
                w.push(format!(
                    "{id}: trend reliability {} > series median {median}",
                    m.trend.reliability_pct
                ));
            }
        }
        if series_rel.len() < 3 && m.trend.reliability_pct > 50 {
            w.push(format!(
                "{id}: trend reliability {} with <3 points",
                m.trend.reliability_pct
            ));
        }
        if m.series
            .windows(2)
            .any(|p| p[0].period_end > p[1].period_end)
        {
            w.push(format!("{id}: series not chronological"));
        }
    }

    let mut pillar_rels = Vec::new();
    for p in &r.pillars {
        if p.assessment == Assessment::NotAssessed {
            continue; // excluded from aggregation by the rubric
        }
        pillar_rels.push(p.reliability_pct);
        let keyed: Vec<u8> = p
            .key_measures
            .iter()
            .filter_map(|k| by_id.get(k).copied())
            .collect();
        if let Some(km) = mean(&keyed) {
            if p.reliability_pct as f64 > km + 0.5 {
                w.push(format!(
                    "pillar {:?}: reliability {} > mean of key measures {km:.0}",
                    p.pillar, p.reliability_pct
                ));
            }
        }
        let missing: Vec<String> = p
            .key_measures
            .iter()
            .filter(|k| !by_id.contains_key(k))
            .map(|k| format!("{k:?}"))
            .collect();
        if !missing.is_empty() {
            w.push(format!(
                "pillar {:?}: key measures not in output: {missing:?}",
                p.pillar
            ));
        }
    }

    if let Some(pm) = mean(&pillar_rels) {
        if r.signal.reliability_pct as f64 > pm + 0.5 {
            w.push(format!(
                "signal reliability {} > mean pillar reliability {pm:.0}",
                r.signal.reliability_pct
            ));
        }
    }
    if let Some(&lo) = pillar_rels.iter().min() {
        if r.signal.reliability_pct > lo.saturating_add(20) {
            w.push(format!(
                "signal reliability {} > lowest pillar + 20 ({lo})",
                r.signal.reliability_pct
            ));
        }
    }

    // One reporting currency per analysis: every currency / per_share measure must carry the same code.
    let is_money = |u: Unit| matches!(u, Unit::Currency | Unit::PerShare);
    let mut codes: Vec<&str> = r
        .measures
        .iter()
        .filter(|m| is_money(m.unit))
        .map(|m| m.currency.as_str())
        .collect();
    codes.sort_unstable();
    codes.dedup();
    if codes.len() > 1 {
        w.push(format!("mixed currencies across measures: {codes:?}"));
    }
    if codes.contains(&"") {
        w.push("currency unit without a currency code".to_string());
    }
    for m in &r.measures {
        if is_money(m.unit) && m.scale == Scale::NotApplicable {
            w.push(format!(
                "{:?}: currency amount without a scale",
                m.measure_id
            ));
        }
        if !is_money(m.unit)
            && (!m.currency.is_empty()
                || (m.scale != Scale::NotApplicable && m.unit != Unit::Count))
        {
            w.push(format!(
                "{:?}: {:?} measure carries currency/scale",
                m.measure_id, m.unit
            ));
        }
    }

    // Base-currency conversion is optional extra value; when requested it must be complete and consistent.
    let has_base = !r.fx.base_currency.is_empty();
    for m in &r.measures {
        let money = is_money(m.unit);
        for p in std::iter::once(&m.latest).chain(m.series.iter()) {
            if p.value_base.is_some() && (!has_base || !money || p.value.is_none()) {
                w.push(format!(
                    "{:?}@{}: value_base set but no base currency / not a currency / value null",
                    m.measure_id, p.period_end
                ));
            }
            if has_base && money && p.value.is_some() && p.value_base.is_none() {
                w.push(format!(
                    "{:?}@{}: base currency requested but value_base is null",
                    m.measure_id, p.period_end
                ));
            }
        }
    }
    if has_base && r.fx.rates.is_empty() {
        w.push("base currency requested but fx.rates is empty".to_string());
    }
    if !has_base && !r.fx.rates.is_empty() {
        w.push("fx.rates given without a base currency".to_string());
    }
    for fr in &r.fx.rates {
        for sid in &fr.source_ids {
            if !source_ids.contains(sid.as_str()) {
                w.push(format!(
                    "fx rate {}: unknown source id {sid}",
                    fr.period_end
                ));
            }
        }
    }

    // Signal history: chronological, extremes only with valuation assessed, last point consistent.
    if r.signal_history
        .windows(2)
        .any(|p| p[0].period_end > p[1].period_end)
    {
        w.push("signal_history not chronological".to_string());
    }
    for sp in &r.signal_history {
        if matches!(sp.category, Signal::ExtremeBuy | Signal::ExtremeSell) && !sp.valuation_assessed
        {
            w.push(format!(
                "signal_history {}: {:?} without valuation assessed",
                sp.period_end, sp.category
            ));
        }
        for sid in &sp.source_ids {
            if !source_ids.contains(sid.as_str()) {
                w.push(format!(
                    "signal_history {}: unknown source id {sid}",
                    sp.period_end
                ));
            }
        }
    }
    if let Some(last) = r.signal_history.last() {
        if last.reliability_pct > r.signal.reliability_pct.saturating_add(10) {
            w.push(
                "signal_history last point is rated more reliable than the current signal"
                    .to_string(),
            );
        }
    }

    if r.data_mode == DataMode::RecallOnly {
        let too_high: Vec<String> = r
            .measures
            .iter()
            .filter(|m| m.reliability_pct > 35)
            .map(|m| format!("{:?}", m.measure_id))
            .collect();
        if !too_high.is_empty() {
            w.push(format!(
                "recall_only mode but reliability > 35 for: {too_high:?}"
            ));
        }
    }

    if matches!(r.signal.category, Signal::ExtremeBuy | Signal::ExtremeSell) {
        let weak: Vec<String> = r
            .pillars
            .iter()
            .filter(|p| matches!(p.assessment, Assessment::Neutral | Assessment::NotAssessed))
            .map(|p| format!("{:?}", p.pillar))
            .collect();
        if !weak.is_empty() {
            w.push(format!(
                "{:?} with non-aligned pillars: {weak:?}",
                r.signal.category
            ));
        }
    }
    w
}

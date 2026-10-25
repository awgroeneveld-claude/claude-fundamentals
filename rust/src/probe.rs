//! Schema variants for `fundamentals probe-schema`: cut-down versions of the response schema that
//! locate which part of the structure the API's grammar compiler rejects.

use serde_json::{json, Map, Value};

pub struct Variant {
    pub name: &'static str,
    pub describe: &'static str,
    pub schema: Value,
}

fn remove_props(obj: &mut Value, names: &[&str]) {
    if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
        for n in names {
            props.remove(*n);
        }
    }
    if let Some(req) = obj.get_mut("required").and_then(Value::as_array_mut) {
        req.retain(|r| !names.contains(&r.as_str().unwrap_or("")));
    }
}

fn keep_props(obj: &mut Value, names: &[&str]) {
    let all: Vec<String> = obj
        .get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default();
    let drop: Vec<&str> = all
        .iter()
        .filter(|k| !names.contains(&k.as_str()))
        .map(String::as_str)
        .collect();
    remove_props(obj, &drop);
}

fn def_mut<'a>(schema: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    schema.get_mut("$defs").and_then(|d| d.get_mut(name))
}

/// Replace every enum with a plain string (the $defs for enums are string types with `enum`).
fn strip_enums(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if o.contains_key("enum") {
                let mut n = Map::new();
                n.insert("type".into(), json!("string"));
                *o = n;
                return;
            }
            for child in o.values_mut() {
                strip_enums(child);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(strip_enums),
        _ => {}
    }
}

/// Replace every `anyOf: [X, null]` with X (no nullable fields at all).
fn strip_nullables(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if let Some(Value::Array(opts)) = o.get("anyOf") {
                let non_null: Vec<Value> = opts
                    .iter()
                    .filter(|x| x.get("type").and_then(Value::as_str) != Some("null"))
                    .cloned()
                    .collect();
                if non_null.len() == 1 {
                    let keep = non_null[0].clone();
                    let desc = o.get("description").cloned();
                    *o = keep.as_object().cloned().unwrap_or_default();
                    if let Some(d) = desc {
                        o.insert("description".into(), d);
                    }
                    return;
                }
            }
            for child in o.values_mut() {
                strip_nullables(child);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(strip_nullables),
        _ => {}
    }
}

fn count_props(v: &Value) -> usize {
    match v {
        Value::Object(o) => {
            let here = o
                .get("properties")
                .and_then(Value::as_object)
                .map(|p| p.len())
                .unwrap_or(0);
            here + o.values().map(count_props).sum::<usize>()
        }
        Value::Array(a) => a.iter().map(count_props).sum(),
        _ => 0,
    }
}

pub fn variants(full: &Value) -> Vec<Variant> {
    let mut out = Vec::new();
    let mut push = |name: &'static str, describe: &'static str, schema: Value| {
        out.push(Variant {
            name,
            describe,
            schema,
        })
    };

    push("full", "the schema as shipped", full.clone());

    let mut v = full.clone();
    strip_enums(&mut v);
    push("no-enums", "every enum replaced by a plain string", v);

    let mut v = full.clone();
    strip_nullables(&mut v);
    push(
        "no-nullables",
        "the five nullable numbers made plain numbers",
        v,
    );

    let mut v = full.clone();
    strip_enums(&mut v);
    strip_nullables(&mut v);
    push("no-enums-no-nullables", "both of the above", v);

    let mut v = full.clone();
    if let Some(m) = def_mut(&mut v, "Measure") {
        remove_props(m, &["series"]);
    }
    push(
        "no-series",
        "Measure without the series array (latest + trend only)",
        v,
    );

    let mut v = full.clone();
    if let Some(m) = def_mut(&mut v, "Measure") {
        remove_props(m, &["trend"]);
    }
    push("no-trend", "Measure without the trend object", v);

    let mut v = full.clone();
    if let Some(m) = def_mut(&mut v, "Measure") {
        remove_props(m, &["series", "trend"]);
    }
    push("no-series-no-trend", "Measure with latest only", v);

    let mut v = full.clone();
    remove_props(
        &mut v,
        &[
            "pillars",
            "trend_summary",
            "fx",
            "ownership",
            "context",
            "data_gaps",
            "sector_adjustments",
        ],
    );
    push(
        "measures-signal-sources",
        "top level reduced to measures, signal and sources",
        v,
    );

    let mut v = full.clone();
    keep_props(&mut v, &["schema_version", "symbol", "measures"]);
    push("measures-only", "top level: symbol + measures", v);

    let mut v = full.clone();
    keep_props(
        &mut v,
        &[
            "schema_version",
            "symbol",
            "signal",
            "pillars",
            "trend_summary",
        ],
    );
    push("no-measures", "everything except the measures array", v);

    let mut v = full.clone();
    keep_props(&mut v, &["schema_version", "symbol", "signal"]);
    push("signal-only", "symbol + signal block", v);

    let mut v = full.clone();
    keep_props(&mut v, &["schema_version", "symbol", "measures"]);
    if let Some(m) = def_mut(&mut v, "Measure") {
        keep_props(
            m,
            &["measure_id", "label", "unit", "latest", "reliability_pct"],
        );
    }
    strip_enums(&mut v);
    push(
        "flat-measures",
        "symbol + measures with 5 fields each, no enums",
        v,
    );

    out
}

pub fn describe_size(v: &Value) -> String {
    let raw = serde_json::to_string(v).unwrap_or_default();
    format!(
        "{} bytes, {} properties, {} enums, {} anyOf",
        raw.len(),
        count_props(v),
        raw.matches("\"enum\"").count(),
        raw.matches("\"anyOf\"").count()
    )
}

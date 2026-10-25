//! Offline tests: no network. Parse the shared examples, check body shape, check consistency rules.

use fundamentals_client::{
    consistency_warnings, AnalysisRequest, AnalysisResponse, Config, FundamentalsClient,
};
use serde_json::{json, Value};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn cfg() -> Config {
    Config {
        api_key: "sk-ant-test".into(),
        base_url: "http://127.0.0.1:9".into(),
        model: "claude-opus-5-5".into(),
        effort: "high".into(),
        max_tokens: 64_000,
        use_server_fallbacks: true,
        show_thinking_summary: false,
        system_prompt: fundamentals_client::client::SYSTEM_PROMPT.into(),
        enforce_schema: true,
        progress: None,
    }
}

#[test]
fn example_requests_parse() {
    for name in [
        "request_asml.json",
        "request_subset_quarterly.json",
        "request_supplied_data.json",
    ] {
        let text = std::fs::read_to_string(root().join("examples").join(name)).unwrap();
        let req: AnalysisRequest = serde_json::from_str(&text).unwrap();
        assert!(!req.symbol.is_empty());
        let v: Value = serde_json::to_value(&req).unwrap();
        assert_eq!(v["symbol"], req.symbol);
    }
}

#[test]
fn unknown_measure_is_rejected() {
    let bad = json!({"symbol": "X", "period": {"start": "2020-01-01", "end": "2021-01-01"}, "measures": ["not_a_measure"]});
    assert!(serde_json::from_value::<AnalysisRequest>(bad).is_err());
}

#[test]
fn sample_response_parses_and_is_consistent() {
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let resp: AnalysisResponse = serde_json::from_str(&text).unwrap();
    assert_eq!(format!("{:?}", resp.signal.category), "Sell");
    assert!(
        consistency_warnings(&resp).is_empty(),
        "{:?}",
        consistency_warnings(&resp)
    );
    let again: AnalysisResponse =
        serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
    assert_eq!(again.measures.len(), resp.measures.len());
}

#[test]
fn consistency_flags_violations() {
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["signal"]["reliability_pct"] = json!(100);
    v["measures"][0]["latest"]["value"] = Value::Null;
    let resp: AnalysisResponse = serde_json::from_value(v).unwrap();
    let w = consistency_warnings(&resp);
    assert!(w.iter().any(|x| x.contains("signal reliability")), "{w:?}");
    assert!(w.iter().any(|x| x.contains("null value")), "{w:?}");
}

#[test]
fn request_body_matches_python_reference_shape() {
    let client = FundamentalsClient::new(cfg()).unwrap();
    let text = std::fs::read_to_string(root().join("examples/request_asml.json")).unwrap();
    let req: AnalysisRequest = serde_json::from_str(&text).unwrap();
    let msgs = vec![
        json!({"role": "user", "content": fundamentals_client::build_user_message(&req, "2026-10-25")}),
    ];
    let body = client.build_body(&req, &msgs);

    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "fallbacks",
            "max_tokens",
            "messages",
            "model",
            "output_config",
            "stream",
            "system",
            "tools"
        ]
    );
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["stream"], true);
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert_eq!(
        body["output_config"]["format"]["schema"]["additionalProperties"],
        false
    );
    assert_eq!(
        body["output_config"]["format"]["schema"]["required"]
            .as_array()
            .unwrap()
            .len(),
        18
    );
    // MeasureId must be a plain string on the wire (grammar size), enum-validated client-side.
    let defs = &body["output_config"]["format"]["schema"]["$defs"];
    assert!(
        defs.get("MeasureId")
            .is_none_or(|d| d.get("enum").is_none()),
        "MeasureId must not be an enum in the wire schema"
    );
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    let tools: Vec<&str> = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["type"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["web_search_20260209", "web_fetch_20260209"]);
    assert!(body["messages"][0]["content"]
        .as_str()
        .unwrap()
        .starts_with("Analysis date: 2026-10-25"));
    assert!(
        body.get("temperature").is_none(),
        "sampling params are rejected on 2026-10-21"
    );
}

#[test]
fn no_tools_when_web_research_disabled() {
    let client = FundamentalsClient::new(cfg()).unwrap();
    let text = std::fs::read_to_string(root().join("examples/request_supplied_data.json")).unwrap();
    let req: AnalysisRequest = serde_json::from_str(&text).unwrap();
    let body = client.build_body(&req, &[]);
    assert!(body.get("tools").is_none());
}

#[test]
fn images_become_blocks_before_the_text() {
    use base64::Engine;
    let dir = std::env::temp_dir().join(format!("fc-img-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 1x1 transparent PNG
    let png = base64::engine::general_purpose::STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        .unwrap();
    std::fs::write(dir.join("ib.png"), &png).unwrap();

    let text =
        std::fs::read_to_string(root().join("examples/request_with_screenshots.json")).unwrap();
    let mut req: AnalysisRequest = serde_json::from_str(&text).unwrap();
    assert_eq!(req.images.len(), 2);
    for img in &mut req.images {
        img.path = Some("ib.png".into());
    }
    let content = fundamentals_client::build_user_content(&req, "2026-10-25", Some(&dir)).unwrap();
    let blocks = content.as_array().unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0]["type"], "image");
    assert_eq!(blocks[0]["source"]["media_type"], "image/png");
    assert!(blocks[0]["source"]["data"]
        .as_str()
        .unwrap()
        .starts_with("iVBOR"));
    let t = blocks[2]["text"].as_str().unwrap();
    assert!(t.contains("2 screenshot(s) are attached"));
    assert!(
        !t.contains("\"images\""),
        "image payload must not be duplicated in the JSON text"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn schema_enforcement_can_be_turned_off() {
    let mut c = cfg();
    c.enforce_schema = false;
    let client = FundamentalsClient::new(c).unwrap();
    let req = AnalysisRequest::new("X", "2020-01-01", "2024-12-31");
    let body = client.build_body(&req, &[]);
    assert!(body["output_config"].get("format").is_none());
    assert!(body["system"][0]["text"]
        .as_str()
        .unwrap()
        .contains("# Output schema"));
}

#[test]
fn wire_schema_stays_within_union_budget() {
    // API limits: <= 16 union-typed (anyOf / nullable) fields and <= 24 optional fields per request.
    let schema: Value =
        serde_json::from_str(fundamentals_client::client::RESPONSE_SCHEMA_JSON).unwrap();
    fn walk(v: &Value, unions: &mut usize, optionals: &mut usize) {
        if let Some(obj) = v.as_object() {
            if let Some(props) = obj.get("properties").and_then(Value::as_object) {
                let required: Vec<&str> = obj
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                for (k, p) in props {
                    if !required.contains(&k.as_str()) {
                        *optionals += 1;
                    }
                    if p.get("anyOf").is_some()
                        || p.get("oneOf").is_some()
                        || p.get("type").is_some_and(Value::is_array)
                    {
                        *unions += 1;
                    }
                }
            }
            for child in obj.values() {
                walk(child, unions, optionals);
            }
        } else if let Some(arr) = v.as_array() {
            for child in arr {
                walk(child, unions, optionals);
            }
        }
    }
    let (mut unions, mut optionals) = (0, 0);
    walk(&schema, &mut unions, &mut optionals);
    assert!(
        unions <= 6,
        "{unions} union-typed fields; API limit is 16, keep margin"
    );
    assert_eq!(optionals, 0);
}

#[test]
fn mixed_currency_is_flagged() {
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["measures"][0]["currency"] = json!("USD");
    let resp: AnalysisResponse = serde_json::from_value(v).unwrap();
    let w = consistency_warnings(&resp);
    assert!(w.iter().any(|x| x.contains("mixed currencies")), "{w:?}");
}

#[test]
fn base_currency_completeness_is_checked() {
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["fx"]["base_currency"] = json!("EUR");
    v["fx"]["rates"] = json!([{"period_end": "2024-12-31", "period_type": "FY", "average_rate": 1.18,
                               "closing_rate": 1.21, "source_ids": ["S1"], "reliability_pct": 90}]);
    let resp: AnalysisResponse = serde_json::from_value(v).unwrap();
    let w = consistency_warnings(&resp);
    assert!(w.iter().any(|x| x.contains("value_base is null")), "{w:?}");
}

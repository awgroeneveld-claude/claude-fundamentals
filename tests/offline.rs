//! Offline tests: no network. Parse the shared examples, check body shape, check consistency rules.

use fundamentals_client::{consistency_warnings, AnalysisRequest, AnalysisResponse, Config, FundamentalsClient};
use serde_json::{json, Value};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
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
    }
}

#[test]
fn example_requests_parse() {
    for name in ["request_asml.json", "request_subset_quarterly.json", "request_supplied_data.json"] {
        let text = std::fs::read_to_string(root().join("examples").join(name)).unwrap();
        let req: AnalysisRequest = serde_json::from_str(&text).unwrap();
        assert!(!req.symbol.is_empty());
        // round-trips through the same JSON the Python client sends
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
    assert!(consistency_warnings(&resp).is_empty(), "{:?}", consistency_warnings(&resp));
    // serialise -> parse round trip is lossless
    let again: AnalysisResponse = serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
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
    let msgs = vec![json!({"role": "user", "content": fundamentals_client::build_user_message(&req, "2026-10-25")})];
    let body = client.build_body(&req, &msgs);

    let mut keys: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["fallbacks", "max_tokens", "messages", "model", "output_config", "stream", "system", "tools"]);
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["stream"], true);
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert_eq!(body["output_config"]["format"]["schema"]["additionalProperties"], false);
    assert_eq!(body["output_config"]["format"]["schema"]["required"].as_array().unwrap().len(), 15);
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    let tools: Vec<&str> = body["tools"].as_array().unwrap().iter().map(|t| t["type"].as_str().unwrap()).collect();
    assert_eq!(tools, ["web_search_20260209", "web_fetch_20260209"]);
    assert!(body["messages"][0]["content"].as_str().unwrap().starts_with("Analysis date: 2026-10-25"));
    assert!(body.get("temperature").is_none(), "sampling params are rejected on 2026-10-21");
}

#[test]
fn no_tools_when_web_research_disabled() {
    let client = FundamentalsClient::new(cfg()).unwrap();
    let text = std::fs::read_to_string(root().join("examples/request_supplied_data.json")).unwrap();
    let req: AnalysisRequest = serde_json::from_str(&text).unwrap();
    let body = client.build_body(&req, &[]);
    assert!(body.get("tools").is_none());
}

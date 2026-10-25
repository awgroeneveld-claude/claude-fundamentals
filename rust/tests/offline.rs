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
        sandbox_output: false,
    }
}

#[test]
fn example_requests_parse() {
    for name in [
        "request_asml.json",
        "request_subset_quarterly.json",
        "request_supplied_data.json",
        "request_vow3.json",
        "request_volcar_b.json",
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
        19
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

#[test]
fn report_renders_sample_with_charts() {
    use fundamentals_client::{render_report, ChartMode, ReportOptions};
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let resp: AnalysisResponse = serde_json::from_str(&text).unwrap();
    let rep = render_report(
        &resp,
        &ReportOptions {
            charts: ChartMode::Svg,
            chart_dir: "c".into(),
            warnings: vec![],
        },
    );
    let md = &rep.markdown;
    assert!(md.starts_with("# Example Industries plc (EXMP): fundamental analysis"));
    for h in [
        "## Signal: SELL",
        "## Pillar scorecard",
        "## Trends:",
        "## Measures",
        "### Profitability",
        "### Leverage",
        "### Cash Flow Quality",
        "## Sources",
        "## Data gaps",
    ] {
        assert!(md.contains(h), "missing section {h}");
    }
    assert!(md.contains("GBP 2,150m"), "currency formatting");
    assert!(md.contains("11.3 %"), "percent formatting");
    assert!(!rep.charts.is_empty());
    for (name, svg) in &rep.charts {
        assert!(name.ends_with(".svg"));
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert!(
            md.contains(&format!("](c/{name})")),
            "chart {name} referenced"
        );
    }
    // the unavailable measure (net debt / EBITDA) is not charted
    assert!(!rep
        .charts
        .iter()
        .any(|(n, _)| n.contains("net-debt-ebitda")));
    let mermaid = render_report(
        &resp,
        &ReportOptions {
            charts: ChartMode::Mermaid,
            chart_dir: "c".into(),
            warnings: vec![],
        },
    );
    assert!(mermaid.charts.is_empty() && mermaid.markdown.contains("xychart-beta"));
}

#[test]
fn result_files_from_before_currency_fields_still_load() {
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    let obj = v.as_object_mut().unwrap();
    obj.remove("currency_notes");
    obj.remove("fx");
    for m in v["measures"].as_array_mut().unwrap() {
        m["latest"].as_object_mut().unwrap().remove("value_base");
        for p in m["series"].as_array_mut().unwrap() {
            p.as_object_mut().unwrap().remove("value_base");
        }
    }
    let resp: AnalysisResponse = serde_json::from_value(v).unwrap();
    assert!(resp.fx.base_currency.is_empty() && resp.currency_notes.is_empty());
    let rep =
        fundamentals_client::render_report(&resp, &fundamentals_client::ReportOptions::default());
    assert!(rep.markdown.contains("## Currency"));
}

#[test]
fn probe_variants_are_valid_subsets() {
    use fundamentals_client::probe::{describe_size, variants};
    let full: Value =
        serde_json::from_str(fundamentals_client::client::RESPONSE_SCHEMA_JSON).unwrap();
    let vs = variants(&full);
    assert!(vs.len() >= 10);
    let full_len = serde_json::to_string(&full).unwrap().len();
    for v in vs.iter().filter(|v| v.name != "full") {
        let len = serde_json::to_string(&v.schema).unwrap().len();
        assert!(
            len < full_len,
            "{} not smaller: {}",
            v.name,
            describe_size(&v.schema)
        );
        // every required name still exists as a property, at every object
        fn check(o: &Value) {
            if let Some(obj) = o.as_object() {
                if let (Some(props), Some(req)) = (obj.get("properties"), obj.get("required")) {
                    for r in req.as_array().unwrap() {
                        assert!(
                            props.get(r.as_str().unwrap()).is_some(),
                            "dangling required {r}"
                        );
                    }
                }
                obj.values().for_each(check);
            } else if let Some(a) = o.as_array() {
                a.iter().for_each(check);
            }
        }
        check(&v.schema);
    }
    let signal_only = vs.iter().find(|v| v.name == "signal-only").unwrap();
    let defs = signal_only.schema["$defs"]
        .as_object()
        .map(|d| d.len())
        .unwrap_or(0);
    assert!(defs <= 2, "signal-only still carries {defs} $defs");
    assert!(vs.iter().any(|v| v.name == "control-tiny"));
    let no_enums = vs.iter().find(|v| v.name == "no-enums").unwrap();
    assert!(!serde_json::to_string(&no_enums.schema)
        .unwrap()
        .contains("\"enum\""));
}

#[test]
fn quarterly_points_get_their_own_chart() {
    use fundamentals_client::{render_report, ChartMode, ReportOptions};
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    // append four quarterly revenue points after the annual ones
    let series = v["measures"][0]["series"].as_array_mut().unwrap();
    for (d, val) in [
        ("2024-03-31", 520.0),
        ("2024-06-30", 530.0),
        ("2024-09-30", 545.0),
        ("2024-12-31", 555.0),
    ] {
        series.push(
            json!({"period_end": d, "period_type": "Q", "value": val, "value_base": null,
                           "reliability_pct": 100, "method": "reported", "source_ids": ["S1"]}),
        );
    }
    let resp: AnalysisResponse = serde_json::from_value(v).unwrap();
    let rep = render_report(
        &resp,
        &ReportOptions {
            charts: ChartMode::Svg,
            chart_dir: "c".into(),
            warnings: vec![],
        },
    );
    let names: Vec<&str> = rep.charts.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"revenue.svg"), "{names:?}");
    assert!(names.contains(&"revenue-quarterly.svg"), "{names:?}");
    let (_, q) = rep
        .charts
        .iter()
        .find(|(n, _)| n == "revenue-quarterly.svg")
        .unwrap();
    assert_eq!(q.matches("<circle").count(), 4);
    assert!(q.contains("Q 2024-12"));
}

#[test]
fn signal_history_is_checked_and_charted() {
    use fundamentals_client::{render_report, ChartMode, ReportOptions};
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let resp: AnalysisResponse = serde_json::from_str(&text).unwrap();
    assert_eq!(resp.signal_history.len(), 4);
    let rep = render_report(
        &resp,
        &ReportOptions {
            charts: ChartMode::Svg,
            chart_dir: "c".into(),
            warnings: vec![],
        },
    );
    let names: Vec<&str> = rep.charts.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"signal-over-time.svg") && names.contains(&"pillar-scores-over-time.svg"),
        "{names:?}"
    );
    assert!(
        !names.contains(&"valuation-pillar-over-time.svg"),
        "valuation never assessed in sample"
    );

    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["signal_history"][0]["category"] = json!("extreme_sell");
    v["signal_history"][0]["period_end"] = json!("2025-01-01");
    let bad: AnalysisResponse = serde_json::from_value(v).unwrap();
    let w = consistency_warnings(&bad);
    assert!(
        w.iter().any(|x| x.contains("without valuation assessed")),
        "{w:?}"
    );
    assert!(w.iter().any(|x| x.contains("not chronological")), "{w:?}");
}

#[test]
fn previous_response_goes_in_its_own_block() {
    let prev: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap(),
    )
    .unwrap();
    let text = std::fs::read_to_string(root().join("examples/request_supplied_data.json")).unwrap();
    let mut req: AnalysisRequest = serde_json::from_str(&text).unwrap();
    req.previous_response = Some(prev);
    let msg = fundamentals_client::build_user_message(&req, "2026-10-25");
    assert!(msg.contains("incremental update") && msg.contains("as of 2026-10-25"));
    assert_eq!(msg.matches("```json").count(), 2);
    assert!(!msg
        .split("incremental update")
        .next()
        .unwrap()
        .contains("\"previous_response\""));
}

#[test]
fn incremental_merge_adds_without_rewriting() {
    use fundamentals_client::merge_incremental;
    let text = std::fs::read_to_string(root().join("examples/sample_response.json")).unwrap();
    let prev: AnalysisResponse = serde_json::from_str(&text).unwrap();
    let mut d: Value = serde_json::from_str(&text).unwrap();
    d["analysis_date"] = json!("2027-03-01");
    for m in d["measures"].as_array_mut().unwrap() {
        if m["measure_id"] == "revenue" {
            m["series"] = json!([
                {"period_end": "2025-12-31", "period_type": "FY", "value": 2230.0, "value_base": null, "reliability_pct": 100, "method": "reported", "source_ids": ["S2"]},
                {"period_end": "2024-12-31", "period_type": "FY", "value": 2155.0, "value_base": null, "reliability_pct": 100, "method": "reported", "source_ids": ["S2"]}
            ]);
            m["latest"] = m["series"][0].clone();
        } else {
            m["series"] = json!([]);
        }
    }
    d["sources"] = json!([{"id": "S2", "type": "primary_filing", "title": "FY2025 annual report", "url": "", "period_covered": "FY2025", "retrieved_at": "2027-03-01"}]);
    d["signal_history"] = json!([{"period_end": "2025-12-31", "period_type": "FY", "category": "hold", "conviction_pct": 50, "reliability_pct": 80,
        "valuation_assessed": false, "pillar_scores": {"valuation": 0, "profitability_stability": 50, "debt_health": 45, "earnings_quality": 55, "trends": 50},
        "decisive_factor": "Recovery in cash conversion.", "source_ids": ["S2"]}]);
    let delta: AnalysisResponse = serde_json::from_value(d).unwrap();
    let merged = merge_incremental(&prev, &delta, Some(8));
    let rev = merged
        .measures
        .iter()
        .find(|m| format!("{:?}", m.measure_id) == "Revenue")
        .unwrap();
    let ends: Vec<&str> = rev.series.iter().map(|p| p.period_end.as_str()).collect();
    assert_eq!(
        ends,
        [
            "2021-12-31",
            "2022-12-31",
            "2023-12-31",
            "2024-12-31",
            "2025-12-31"
        ]
    );
    assert_eq!(
        rev.series
            .iter()
            .find(|p| p.period_end == "2024-12-31")
            .unwrap()
            .value,
        Some(2155.0)
    );
    assert_eq!(
        rev.series
            .iter()
            .find(|p| p.period_end == "2023-12-31")
            .unwrap()
            .value,
        Some(2104.0)
    );
    let om = merged
        .measures
        .iter()
        .find(|m| format!("{:?}", m.measure_id) == "OperatingMargin")
        .unwrap();
    assert_eq!(om.series.len(), 4);
    let ids: Vec<&str> = merged.sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["S1", "S2"]);
    assert_eq!(merged.signal_history.len(), 5);
    assert_eq!(merged.analysis_date, "2027-03-01");
    assert!(
        consistency_warnings(&merged).is_empty(),
        "{:?}",
        consistency_warnings(&merged)
    );
}

#[test]
fn file_ids_are_extracted_from_tool_results() {
    let content = vec![
        json!({"type": "text", "text": "FILE: analysis.json", "file_id": "not-a-tool-result"}),
        json!({"type": "bash_code_execution_tool_result", "tool_use_id": "x", "content": {
            "type": "bash_code_execution_result", "stdout": "", "stderr": "", "return_code": 0,
            "content": [{"type": "code_execution_output", "file_id": "file_abc"}, {"type": "code_execution_output", "file_id": "file_def"}]}}),
        json!({"type": "web_fetch_tool_result", "content": {"type": "web_fetch_result", "url": "u", "content": {"type": "document", "source": {"type": "text", "data": "x"}, "file_id": "file_zzz"}}}),
    ];
    assert_eq!(
        fundamentals_client::file_ids_in(&content),
        vec!["file_abc", "file_def"]
    );
}

#[test]
fn sandbox_mode_adds_code_execution_and_instruction() {
    let mut c = cfg();
    c.sandbox_output = true;
    let client = FundamentalsClient::new(c).unwrap();
    let req = AnalysisRequest::new("X", "2020-01-01", "2024-12-31");
    let body = client.build_body(&req, &[]);
    let tools: Vec<&str> = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        tools,
        [
            "web_search_20260209",
            "web_fetch_20260209",
            "code_execution_20260521"
        ]
    );
}

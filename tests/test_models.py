"""Offline tests: schema shape, request/response validation, consistency checks. No API calls."""

import json
from pathlib import Path

import pytest
from anthropic.lib._parse._transform import transform_schema

from fundamentals_client import AnalysisRequest, AnalysisResponse, consistency_warnings
from fundamentals_client.client import build_user_message

ROOT = Path(__file__).resolve().parent.parent


def _walk(schema, path="$"):
    if isinstance(schema, dict):
        if schema.get("type") == "object":
            yield path, schema
        for k, v in schema.items():
            yield from _walk(v, f"{path}.{k}")
    elif isinstance(schema, list):
        for i, v in enumerate(schema):
            yield from _walk(v, f"{path}[{i}]")


def test_response_schema_is_structured_outputs_compatible():
    schema = transform_schema(AnalysisResponse)
    for path, obj in _walk(schema):
        assert obj.get("additionalProperties") is False, path
        props = obj.get("properties", {})
        assert set(obj.get("required", [])) == set(props), f"{path}: all properties must be required"
    dumped = json.dumps(schema)
    for banned in ('"minimum"', '"maximum"', '"minLength"', '"maxLength"'):
        assert banned not in dumped, f"{banned} must be stripped before sending"


@pytest.mark.parametrize(
    "name",
    ["request_asml.json", "request_subset_quarterly.json", "request_supplied_data.json", "request_vow3.json", "request_volcar_b.json", "request_pah3.json", "request_with_screenshots.json"],
)
def test_example_requests_validate(name):
    req = AnalysisRequest.model_validate_json((ROOT / "examples" / name).read_text())
    msg = build_user_message(req, "2026-10-25")
    assert req.symbol in msg
    assert "Analysis date: 2026-10-25" in msg


def test_request_rejects_unknown_measure():
    with pytest.raises(Exception):
        AnalysisRequest.model_validate(
            {"symbol": "X", "period": {"start": "2020-01-01", "end": "2021-01-01"}, "measures": ["not_a_measure"]}
        )


def test_sample_response_validates_and_is_consistent():
    resp = AnalysisResponse.model_validate_json((ROOT / "examples" / "sample_response.json").read_text())
    assert resp.signal.category.value in {"extreme_buy", "buy", "hold", "sell", "extreme_sell"}
    assert consistency_warnings(resp) == []


def test_consistency_flags_violations():
    data = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    data["signal"]["reliability_pct"] = 100  # above pillar mean
    data["measures"][0]["latest"]["value"] = None  # null with non-zero reliability
    resp = AnalysisResponse.model_validate(data)
    w = consistency_warnings(resp)
    assert any("signal reliability" in x for x in w)
    assert any("null value" in x for x in w)


def test_images_become_blocks_before_the_text(tmp_path):
    import base64

    from fundamentals_client.client import build_user_content

    png = base64.b64decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="
    )
    (tmp_path / "ib.png").write_bytes(png)
    req = AnalysisRequest.model_validate_json((ROOT / "examples" / "request_with_screenshots.json").read_text())
    assert len(req.images) == 2
    for img in req.images:
        img.path = "ib.png"
    content = build_user_content(req, "2026-10-25", tmp_path)
    assert isinstance(content, list) and len(content) == 3
    assert content[0]["type"] == "image" and content[0]["source"]["media_type"] == "image/png"
    assert content[0]["source"]["data"].startswith("iVBOR")
    text = content[2]["text"]
    assert "2 screenshot(s) are attached" in text
    assert '"images"' not in text


def test_request_without_images_is_plain_string():
    from fundamentals_client.client import build_user_content

    req = AnalysisRequest.model_validate_json((ROOT / "examples" / "request_asml.json").read_text())
    assert isinstance(build_user_content(req, "2026-10-25"), str)


def test_union_and_optional_budget():
    """API limits: <= 16 union-typed (anyOf / nullable) fields and <= 24 optional ones per request,
    and they compile exponentially. Keep a wide margin."""
    schema = transform_schema(AnalysisResponse)
    unions = 0
    optionals = 0
    for _, obj in _walk(schema):
        props = obj.get("properties", {})
        optionals += len(set(props) - set(obj.get("required", [])))
        for v in props.values():
            if "anyOf" in v or "oneOf" in v or isinstance(v.get("type"), list):
                unions += 1
    assert unions <= 8, f"{unions} union-typed fields; API limit 16, keep margin"
    assert optionals == 0


def test_mixed_currency_is_flagged():
    data = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    data["measures"][0]["currency"] = "USD"  # revenue in USD while the rest is GBP
    w = consistency_warnings(AnalysisResponse.model_validate(data))
    assert any("mixed currencies" in x for x in w), w


def test_base_currency_completeness_is_checked():
    data = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    data["fx"]["base_currency"] = "EUR"
    data["fx"]["rates"] = [{"period_end": "2024-12-31", "period_type": "FY", "average_rate": 1.18,
                            "closing_rate": 1.21, "source_ids": ["S1"], "reliability_pct": 90}]
    w = consistency_warnings(AnalysisResponse.model_validate(data))
    assert any("value_base is null" in x for x in w), w


def test_signal_history_checks():
    data = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    data["signal_history"][0]["category"] = "extreme_sell"  # without valuation assessed
    data["signal_history"][0]["period_end"] = "2025-01-01"  # breaks chronology
    w = consistency_warnings(AnalysisResponse.model_validate(data))
    assert any("without valuation assessed" in x for x in w), w
    assert any("not chronological" in x for x in w), w


def test_previous_response_goes_in_its_own_block():
    from fundamentals_client.client import build_user_message

    prev = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    req = AnalysisRequest.model_validate_json((ROOT / "examples" / "request_supplied_data.json").read_text())
    req.previous_response = prev
    msg = build_user_message(req, "2026-10-25")
    assert "incremental update" in msg and "as of 2026-10-25" in msg
    assert msg.count("```json") == 2
    assert '"previous_response"' not in msg.split("incremental update")[0]


def test_incremental_merge_adds_without_rewriting():
    from fundamentals_client import merge_incremental

    prev = AnalysisResponse.model_validate_json((ROOT / "examples" / "sample_response.json").read_text())
    d = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    d["analysis_date"] = "2027-03-01"
    for m in d["measures"]:
        if m["measure_id"] == "revenue":
            m["series"] = [{"period_end": "2025-12-31", "period_type": "FY", "value": 2230.0, "value_base": None,
                            "reliability_pct": 100, "method": "reported", "source_ids": ["S2"]},
                           {"period_end": "2024-12-31", "period_type": "FY", "value": 2155.0, "value_base": None,
                            "reliability_pct": 100, "method": "reported", "source_ids": ["S2"]}]  # restated
            m["latest"] = m["series"][0]
        else:
            m["series"] = []
    d["sources"] = [{"id": "S2", "type": "primary_filing", "title": "FY2025 annual report", "url": "", "period_covered": "FY2025", "retrieved_at": "2027-03-01"}]
    d["signal_history"] = [{"period_end": "2025-12-31", "period_type": "FY", "category": "hold", "conviction_pct": 50, "reliability_pct": 80,
                            "valuation_assessed": False, "pillar_scores": {"valuation": 0, "profitability_stability": 50, "debt_health": 45, "earnings_quality": 55, "trends": 50},
                            "decisive_factor": "Recovery in cash conversion.", "source_ids": ["S2"]}]
    delta = AnalysisResponse.model_validate(d)
    merged = merge_incremental(prev, delta, recent_quarters=8)
    rev = next(m for m in merged.measures if m.measure_id.value == "revenue")
    ends = [p.period_end for p in rev.series]
    assert ends == ["2021-12-31", "2022-12-31", "2023-12-31", "2024-12-31", "2025-12-31"]
    assert next(p for p in rev.series if p.period_end == "2024-12-31").value == 2155.0  # restated
    assert next(p for p in rev.series if p.period_end == "2023-12-31").value == 2104.0  # untouched
    other = next(m for m in merged.measures if m.measure_id.value == "operating_margin")
    assert len(other.series) == 4  # empty delta series keeps history
    assert [s.id for s in merged.sources] == ["S1", "S2"]
    assert [sp.period_end for sp in merged.signal_history][-2:] == ["2024-12-31", "2025-12-31"]
    assert merged.analysis_date == "2027-03-01"
    assert consistency_warnings(merged) == []


def test_file_ids_in_tool_results():
    from fundamentals_client.client import file_ids_in

    content = [
        {"type": "text", "text": "FILE: analysis.json", "file_id": "nope"},
        {"type": "bash_code_execution_tool_result", "content": {"type": "bash_code_execution_result",
         "content": [{"type": "code_execution_output", "file_id": "file_abc"}]}},
    ]
    assert file_ids_in(content) == ["file_abc"]


def test_quarterly_points_do_not_bound_trend_reliability():
    data = json.loads((ROOT / "examples" / "sample_response.json").read_text())
    series = data["measures"][0]["series"]
    for d in ["2024-09-30", "2024-12-31"]:
        series.append({"period_end": d, "period_type": "Q", "value": 500.0, "value_base": None,
                       "reliability_pct": 40, "method": "derived", "source_ids": ["S1"]})
    series.sort(key=lambda p: p["period_end"])
    w = consistency_warnings(AnalysisResponse.model_validate(data))
    assert not any("trend reliability" in x for x in w), w


def test_catalogue_mapping_matches_enum():
    from fundamentals_client.models import MEASURE_CATEGORY, MeasureId, requested_measures

    assert set(MEASURE_CATEGORY) == {m.value for m in MeasureId}
    req = AnalysisRequest.model_validate_json((ROOT / "examples" / "request_supplied_data.json").read_text())
    ids = requested_measures(req)
    assert ids and all(MEASURE_CATEGORY[i] in {c.value for c in req.categories} for i in ids)

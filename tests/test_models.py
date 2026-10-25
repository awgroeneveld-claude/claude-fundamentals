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


@pytest.mark.parametrize("name", ["request_asml.json", "request_subset_quarterly.json", "request_supplied_data.json"])
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

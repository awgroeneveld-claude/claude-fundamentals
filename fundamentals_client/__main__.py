"""CLI: python -m fundamentals_client analyze examples/request_asml.json --out out/asml.json [--yaml]

Also:  python -m fundamentals_client schema   -> writes schema/*.schema.json from the Pydantic models
       python -m fundamentals_client check out/asml.json -> validate + consistency warnings offline
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .models import AnalysisRequest, AnalysisResponse


def cmd_schema(args: argparse.Namespace) -> int:
    from anthropic.lib._parse._transform import transform_schema

    out = Path(args.out_dir)
    out.mkdir(parents=True, exist_ok=True)
    (out / "analysis_request.schema.json").write_text(
        json.dumps(AnalysisRequest.model_json_schema(), indent=2) + "\n", encoding="utf-8"
    )
    # The response schema exactly as the SDK sends it to the API (structured-outputs form).
    (out / "analysis_response.schema.json").write_text(
        json.dumps(transform_schema(AnalysisResponse), indent=2) + "\n", encoding="utf-8"
    )
    print(f"wrote {out/'analysis_request.schema.json'} and {out/'analysis_response.schema.json'}")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    from .client import consistency_warnings

    data = Path(args.file).read_text(encoding="utf-8")
    resp = AnalysisResponse.model_validate_json(data)
    warnings = consistency_warnings(resp)
    print(f"valid AnalysisResponse: {resp.request.symbol} signal={resp.signal.category.value} "
          f"reliability={resp.overall_reliability_pct}%")
    for w in warnings:
        print(f"  warning: {w}")
    return 0


def cmd_analyze(args: argparse.Namespace) -> int:
    from .client import AnalysisIncomplete, AnalysisRefused, FundamentalsClient

    req = AnalysisRequest.model_validate_json(Path(args.request).read_text(encoding="utf-8"))
    client = FundamentalsClient(model=args.model, effort=args.effort, max_tokens=args.max_tokens)
    try:
        result = client.analyze(req, image_base_dir=Path(args.request).resolve().parent)
    except AnalysisRefused as e:
        print(f"refused: {e}", file=sys.stderr)
        return 3
    except AnalysisIncomplete as e:
        print(f"incomplete: {e}", file=sys.stderr)
        return 4

    out = Path(args.out) if args.out else None
    if out:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(result.raw_json + "\n", encoding="utf-8")
        if args.yaml:
            try:
                import yaml  # type: ignore
            except ImportError:
                print("PyYAML not installed: pip install pyyaml", file=sys.stderr)
            else:
                out.with_suffix(".yaml").write_text(
                    yaml.safe_dump(json.loads(result.raw_json), sort_keys=False, allow_unicode=True),
                    encoding="utf-8",
                )
        print(f"wrote {out}")
    else:
        print(result.raw_json)

    r = result.response
    u = result.usage
    print(
        f"\n{r.request.symbol}: {r.signal.category.value} "
        f"(conviction {r.signal.conviction_pct}%, reliability {r.signal.reliability_pct}%), "
        f"overall reliability {r.overall_reliability_pct}%, data_mode={r.data_mode.value}",
        file=sys.stderr,
    )
    print(
        f"model={result.model} turns={u.turns} in={u.input_tokens} cached={u.cache_read_input_tokens} "
        f"out={u.output_tokens} web_searches={u.web_search_requests} elapsed={result.elapsed_s:.0f}s",
        file=sys.stderr,
    )
    for w in result.warnings:
        print(f"  warning: {w}", file=sys.stderr)
    return 0


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(prog="fundamentals_client")
    sub = p.add_subparsers(dest="cmd", required=True)

    a = sub.add_parser("analyze", help="run an analysis against the Claude API")
    a.add_argument("request", help="path to a JSON file matching AnalysisRequest")
    a.add_argument("--out", help="write the JSON result here (default: stdout)")
    a.add_argument("--yaml", action="store_true", help="also write a .yaml next to --out")
    a.add_argument("--model", default="claude-opus-5-5")
    a.add_argument("--effort", default="high", choices=["low", "medium", "high", "xhigh", "max"])
    a.add_argument("--max-tokens", type=int, default=64_000)
    a.set_defaults(fn=cmd_analyze)

    s = sub.add_parser("schema", help="export JSON Schemas for request and response")
    s.add_argument("--out-dir", default="schema")
    s.set_defaults(fn=cmd_schema)

    c = sub.add_parser("check", help="validate a saved response and print consistency warnings")
    c.add_argument("file")
    c.set_defaults(fn=cmd_check)

    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())

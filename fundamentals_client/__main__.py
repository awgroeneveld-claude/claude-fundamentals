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

    data = json.loads(Path(args.file).read_text(encoding="utf-8"))
    # Result files from before the currency fields: fill the defaults so they still validate.
    data.setdefault("currency_notes", "")
    data.setdefault("fx", {"reporting_currency": "", "base_currency": "", "rates": [], "method_note": ""})
    for m in data.get("measures", []):
        for p in [m.get("latest", {}), *m.get("series", [])]:
            p.setdefault("value_base", None)
    resp = AnalysisResponse.model_validate(data)
    warnings = consistency_warnings(resp)
    print(f"valid AnalysisResponse: {resp.symbol} signal={resp.signal.category.value} "
          f"reliability={resp.overall_reliability_pct}%")
    for w in warnings:
        print(f"  warning: {w}")
    return 0


def cmd_analyze(args: argparse.Namespace) -> int:
    from .client import AnalysisIncomplete, AnalysisRefused, FundamentalsClient, consistency_warnings

    req = AnalysisRequest.model_validate_json(Path(args.request).read_text(encoding="utf-8"))
    previous = None
    if args.previous:
        previous = AnalysisResponse.model_validate_json(Path(args.previous).read_text(encoding="utf-8"))
        req.previous_response = previous.model_dump(mode="json")
    progress = None if args.quiet else (lambda line: print(line, file=sys.stderr, flush=True))
    if progress:
        progress(f"starting analysis of {req.symbol} (effort {args.effort}, web research {req.allow_web_research})")
    client = FundamentalsClient(
        model=args.model,
        effort=args.effort,
        max_tokens=args.max_tokens,
        enforce_schema=args.enforce_schema,
        progress=progress,
        sandbox_output=not args.no_sandbox_output,
    )
    try:
        result = client.analyze(req, image_base_dir=Path(args.request).resolve().parent)
    except AnalysisRefused as e:
        print(f"refused: {e}", file=sys.stderr)
        return 3
    except AnalysisIncomplete as e:
        print(f"incomplete: {e}", file=sys.stderr)
        return 4

    if previous is not None:
        from .merge import merge_incremental

        merged = merge_incremental(previous, result.response, recent_quarters=req.recent_quarters)
        result.response = merged
        result.raw_json = merged.model_dump_json(indent=2)
        result.warnings = consistency_warnings(merged)
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
        f"\n{r.symbol}: {r.signal.category.value} "
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
    a.add_argument("--model", default="claude-sonnet-5-5")
    a.add_argument("--effort", default="high", choices=["low", "medium", "high", "xhigh", "max"])
    a.add_argument("--max-tokens", type=int, default=64_000)
    a.add_argument(
        "--enforce-schema",
        action="store_true",
        help="also send the schema as output_config.format (API-side grammar); off by default, see README",
    )
    a.add_argument("--previous", help="previous result JSON for this symbol: run incrementally and merge")
    a.add_argument(
        "--no-sandbox-output",
        action="store_true",
        help="have the model type the JSON as output text instead of handing back a sandbox file",
    )
    a.add_argument("--quiet", action="store_true", help="suppress the progress log on stderr")
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

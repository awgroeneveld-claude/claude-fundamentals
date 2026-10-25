"""Call Claude with the fundamentals system prompt and get a validated AnalysisResponse back.

Usage from your app:

    from fundamentals_client import FundamentalsClient, AnalysisRequest, Period

    client = FundamentalsClient()                      # reads ANTHROPIC_API_KEY (or `ant auth login` profile)
    req = AnalysisRequest(symbol="ASML", period=Period(start="2016-01-01", end="2025-12-31"))
    result = client.analyze(req)                       # -> AnalysisResult
    result.response.signal.category                    # Signal.buy, ...
    result.response.model_dump_json(indent=2)          # the JSON your app stores

Design notes
------------
* Model: claude-opus-5-5, adaptive thinking (always on for this model), effort set explicitly.
* Structured outputs: the Pydantic `AnalysisResponse` is passed as `output_format`; the API
  constrains generation to its JSON Schema, the SDK validates and parses it (`parsed_output`).
* Streaming: large outputs + thinking make non-streaming requests prone to HTTP timeouts.
* Server tools: web_search / web_fetch run on Anthropic's side when `allow_web_research` is true.
  A long research turn can stop with `stop_reason == "pause_turn"`; we resume it (bounded).
* Refusals: `fallbacks="default"` (beta) re-runs a declined request on Anthropic's recommended
  fallback model server-side; we still check `stop_reason == "refusal"` on the final message.
* Prompt caching: the system prompt is stable and marked `cache_control`; the request JSON goes
  in the user message after it, so repeated calls reuse the cached prefix.
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

import anthropic

from .models import AnalysisRequest, AnalysisResponse

DEFAULT_MODEL = "claude-opus-5-5"
DEFAULT_EFFORT = "high"  # Opus 5.5 defaults to "medium"; set explicitly. low|medium|high|xhigh|max
DEFAULT_MAX_TOKENS = 64_000  # thinking + JSON; streaming so timeouts are not a concern
MAX_PAUSE_RESUMES = 6
PROMPT_PATH = Path(__file__).resolve().parent.parent / "prompts" / "system_prompt.md"

WEB_TOOLS: list[dict[str, Any]] = [
    {"type": "web_search_20260209", "name": "web_search", "max_uses": 15},
    {"type": "web_fetch_20260209", "name": "web_fetch", "max_uses": 10, "max_content_tokens": 60_000},
]


@dataclass
class Usage:
    input_tokens: int = 0
    output_tokens: int = 0
    cache_read_input_tokens: int = 0
    cache_creation_input_tokens: int = 0
    web_search_requests: int = 0
    turns: int = 0

    def add(self, usage: Any) -> None:
        self.input_tokens += getattr(usage, "input_tokens", 0) or 0
        self.output_tokens += getattr(usage, "output_tokens", 0) or 0
        self.cache_read_input_tokens += getattr(usage, "cache_read_input_tokens", 0) or 0
        self.cache_creation_input_tokens += getattr(usage, "cache_creation_input_tokens", 0) or 0
        server = getattr(usage, "server_tool_use", None)
        if server is not None:
            self.web_search_requests += getattr(server, "web_search_requests", 0) or 0
        self.turns += 1


@dataclass
class AnalysisResult:
    response: AnalysisResponse
    raw_json: str
    model: str
    usage: Usage
    warnings: list[str] = field(default_factory=list)
    elapsed_s: float = 0.0
    request_ids: list[str] = field(default_factory=list)


class AnalysisRefused(RuntimeError):
    def __init__(self, category: str | None, explanation: str | None):
        super().__init__(f"Claude declined the request (category={category}): {explanation}")
        self.category = category
        self.explanation = explanation


class AnalysisIncomplete(RuntimeError):
    """stop_reason was max_tokens or the turn never completed."""


def load_system_prompt(path: Path = PROMPT_PATH) -> str:
    return path.read_text(encoding="utf-8")


def build_user_message(req: AnalysisRequest, today: str) -> str:
    """The volatile part of the prompt. Keep the system prompt untouched so it stays cached."""
    payload = req.model_dump(mode="json", exclude_none=True)
    lines = [
        f"Analysis date: {today}",
        "Analyse the following request using the framework and return the structured result.",
        "",
        "```json",
        json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False),
        "```",
    ]
    if not req.allow_web_research and req.supplied_data is None:
        lines.append(
            "\nWeb research is disabled and no data was supplied: use recall_only mode and cap reliability at 35."
        )
    return "\n".join(lines)


class FundamentalsClient:
    def __init__(
        self,
        client: anthropic.Anthropic | None = None,
        model: str = DEFAULT_MODEL,
        effort: str = DEFAULT_EFFORT,
        max_tokens: int = DEFAULT_MAX_TOKENS,
        system_prompt: str | None = None,
        use_server_fallbacks: bool = True,
        show_thinking_summary: bool = False,
    ):
        self.client = client or anthropic.Anthropic()  # credentials from env / `ant auth login`
        self.model = model
        self.effort = effort
        self.max_tokens = max_tokens
        self.system_prompt = system_prompt or load_system_prompt()
        self.use_server_fallbacks = use_server_fallbacks
        self.show_thinking_summary = show_thinking_summary

    # ------------------------------------------------------------------ #
    def analyze(self, req: AnalysisRequest, today: str | None = None) -> AnalysisResult:
        today = today or time.strftime("%Y-%m-%d")
        messages: list[dict[str, Any]] = [{"role": "user", "content": build_user_message(req, today)}]
        tools = WEB_TOOLS if req.allow_web_research else []

        kwargs: dict[str, Any] = dict(
            model=self.model,
            max_tokens=self.max_tokens,
            system=[{"type": "text", "text": self.system_prompt, "cache_control": {"type": "ephemeral"}}],
            output_config={"effort": self.effort},
            output_format=AnalysisResponse,
        )
        if self.show_thinking_summary:
            kwargs["thinking"] = {"type": "adaptive", "display": "summarized"}
        if tools:
            kwargs["tools"] = tools
        betas: list[str] = []
        if self.use_server_fallbacks:
            betas.append("server-side-fallback-2026-07-01")
            kwargs["fallbacks"] = "default"
        if betas:
            kwargs["betas"] = betas

        usage = Usage()
        request_ids: list[str] = []
        started = time.monotonic()
        final = None
        for attempt in range(MAX_PAUSE_RESUMES + 1):
            with self.client.beta.messages.stream(messages=messages, **kwargs) as stream:
                final = stream.get_final_message()
            usage.add(final.usage)
            if getattr(final, "_request_id", None):
                request_ids.append(final._request_id)
            if final.stop_reason != "pause_turn":
                break
            # Long server-tool turn paused: append the partial assistant turn and resume.
            messages.append({"role": "assistant", "content": final.content})
        else:
            raise AnalysisIncomplete(f"turn still paused after {MAX_PAUSE_RESUMES} resumes")

        assert final is not None
        if final.stop_reason == "refusal":
            details = getattr(final, "stop_details", None)
            raise AnalysisRefused(
                getattr(details, "category", None), getattr(details, "explanation", None)
            )
        if final.stop_reason == "max_tokens":
            raise AnalysisIncomplete("output hit max_tokens; raise max_tokens or narrow the request")

        parsed: AnalysisResponse | None = getattr(final, "parsed_output", None)
        text_blocks = [b for b in final.content if b.type == "text"]
        if parsed is None:
            if not text_blocks:
                raise AnalysisIncomplete(f"no text block in final message (stop_reason={final.stop_reason})")
            parsed = AnalysisResponse.model_validate_json(text_blocks[-1].text)
        raw_json = parsed.model_dump_json(indent=2)

        warnings = consistency_warnings(parsed)
        return AnalysisResult(
            response=parsed,
            raw_json=raw_json,
            model=final.model,
            usage=usage,
            warnings=warnings,
            elapsed_s=time.monotonic() - started,
            request_ids=request_ids,
        )


# ---------------------------------------------------------------------- #
# Post-hoc checks on the reliability rules (the model is asked to obey them;
# this verifies and reports instead of silently rewriting the model's numbers).
# ---------------------------------------------------------------------- #


def _mean(xs: Iterable[int]) -> float | None:
    xs = list(xs)
    return sum(xs) / len(xs) if xs else None


def consistency_warnings(r: AnalysisResponse) -> list[str]:
    w: list[str] = []
    source_ids = {s.id for s in r.sources}
    by_id = {m.measure_id: m for m in r.measures}

    for m in r.measures:
        pts = [m.latest, *m.series]
        for p in pts:
            if p.value is None and p.reliability_pct != 0:
                w.append(f"{m.measure_id.value}: null value with reliability {p.reliability_pct}")
            if p.value is not None and p.reliability_pct == 0:
                w.append(f"{m.measure_id.value}: value {p.value} with reliability 0")
            if p.value is not None and not p.source_ids:
                w.append(f"{m.measure_id.value}@{p.period_end}: value without source ids")
            for sid in p.source_ids:
                if sid not in source_ids:
                    w.append(f"{m.measure_id.value}@{p.period_end}: unknown source id {sid}")
        series_rel = sorted(p.reliability_pct for p in m.series if p.value is not None)
        if series_rel:
            median = series_rel[len(series_rel) // 2]
            if m.trend.reliability_pct > median:
                w.append(f"{m.measure_id.value}: trend reliability {m.trend.reliability_pct} > series median {median}")
        if len([p for p in m.series if p.value is not None]) < 3 and m.trend.reliability_pct > 50:
            w.append(f"{m.measure_id.value}: trend reliability {m.trend.reliability_pct} with <3 points")
        dates = [p.period_end for p in m.series]
        if dates != sorted(dates):
            w.append(f"{m.measure_id.value}: series not chronological")

    pillar_rels = []
    for p in r.pillars:
        if p.assessment.value == "not_assessed":
            continue  # excluded from aggregation by the rubric
        pillar_rels.append(p.reliability_pct)
        keyed = [by_id[k].reliability_pct for k in p.key_measures if k in by_id]
        km = _mean(keyed)
        if km is not None and p.reliability_pct > km + 0.5:
            w.append(f"pillar {p.pillar.value}: reliability {p.reliability_pct} > mean of key measures {km:.0f}")
        missing = [k.value for k in p.key_measures if k not in by_id]
        if missing:
            w.append(f"pillar {p.pillar.value}: key measures not in output: {missing}")

    pm = _mean(pillar_rels)
    if pm is not None and r.signal.reliability_pct > pm + 0.5:
        w.append(f"signal reliability {r.signal.reliability_pct} > mean pillar reliability {pm:.0f}")
    if pillar_rels and r.signal.reliability_pct > min(pillar_rels) + 20:
        w.append(f"signal reliability {r.signal.reliability_pct} > lowest pillar + 20 ({min(pillar_rels)})")

    if r.data_mode.value == "recall_only":
        too_high = [m.measure_id.value for m in r.measures if m.reliability_pct > 35]
        if too_high:
            w.append(f"recall_only mode but reliability > 35 for: {too_high}")

    if r.signal.category.value in ("extreme_buy", "extreme_sell"):
        weak = [p.pillar.value for p in r.pillars if p.assessment.value in ("neutral", "not_assessed")]
        if weak:
            w.append(f"{r.signal.category.value} with non-aligned pillars: {weak}")
    return w

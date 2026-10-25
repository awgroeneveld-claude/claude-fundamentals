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
* Model: claude-sonnet-5-5 by default (half the price of Opus 5.5; adaptive thinking on, effort set
  explicitly); pass model="claude-opus-5-5" for the hardest names.
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

import base64
import json
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Iterable

import anthropic

from .models import AnalysisRequest, AnalysisResponse

DEFAULT_MODEL = "claude-sonnet-5-5"  # user's choice; claude-opus-5-5 for the hardest names
DEFAULT_EFFORT = "high"  # Opus 5.5 defaults to "medium"; set explicitly. low|medium|high|xhigh|max
DEFAULT_MAX_TOKENS = 64_000  # thinking + JSON; streaming so timeouts are not a concern
MAX_PAUSE_RESUMES = 6
PROMPT_PATH = Path(__file__).resolve().parent.parent / "prompts" / "system_prompt.md"

CODE_EXECUTION_TOOL: dict[str, Any] = {"type": "code_execution_20260521", "name": "code_execution"}
SANDBOX_OUTPUT_INSTRUCTION = (
    "Output delivery: build the final JSON object in the code-execution sandbox (compute every figure in "
    "Python, assemble the document, check it against the schema, and json.dump it to /tmp/analysis.json). "
    'Then run one bash command: cp /tmp/analysis.json "$OUTPUT_DIR/" && ls -l "$OUTPUT_DIR". The caller '
    "downloads that file. Your final reply must then be only the line FILE: analysis.json, with no JSON in "
    "the text. If the sandbox is unavailable, return the JSON object as text instead."
)

WEB_TOOLS: list[dict[str, Any]] = [
    {"type": "web_search_20260209", "name": "web_search", "max_uses": 50},
    {"type": "web_fetch_20260209", "name": "web_fetch", "max_uses": 30, "max_content_tokens": 100000},
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
    """The volatile text part of the prompt. Keep the system prompt untouched so it stays cached."""
    payload = req.model_dump(mode="json", exclude_none=True, exclude={"images", "previous_response"})
    lines = [
        f"Analysis date: {today}",
        "Analyse the following request using the framework and return the structured result.",
        "",
        "```json",
        json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False),
        "```",
    ]
    if req.previous_response is not None:
        prev_date = req.previous_response.get("analysis_date", "unknown date")
        lines.append("")
        lines.append(f"This is an incremental update. The previous analysis (as of {prev_date}) follows; apply the "
                     "incremental-update rules.")
        lines.append("")
        lines.append("```json")
        lines.append(json.dumps(req.previous_response, indent=1, sort_keys=True, ensure_ascii=False))
        lines.append("```")
    if req.images:
        lines.append("")
        lines.append(f"{len(req.images)} screenshot(s) are attached before this text, in this order:")
        for i, img in enumerate(req.images, 1):
            lines.append(f"  {i}. {img.caption}")
        lines.append("Read figures from them per the screenshot rules; register each as a supplied_screenshot source.")
    if not req.allow_web_research and req.supplied_data is None and not req.images:
        lines.append(
            "\nWeb research is disabled and no data was supplied: use recall_only mode and cap reliability at 35."
        )
    return "\n".join(lines)


def _image_data(img: Any, base_dir: Path | None) -> str:
    if img.data:
        return img.data
    if not img.path:
        raise ValueError(f"image '{img.caption}' has neither data nor path")
    path = Path(img.path)
    if not path.is_absolute() and base_dir is not None:
        path = base_dir / path
    raw = path.read_bytes()
    if len(raw) > 5 * 1024 * 1024:
        raise ValueError(f"{path}: image exceeds the 5 MB API limit")
    return base64.standard_b64encode(raw).decode("ascii")


def build_user_content(req: AnalysisRequest, today: str, base_dir: Path | None = None) -> str | list[dict[str, Any]]:
    """Plain string when there are no images; otherwise image blocks first, then the text block."""
    text = build_user_message(req, today)
    if not req.images:
        return text
    if len(req.images) > 100:
        raise ValueError("at most 100 images per request")
    blocks: list[dict[str, Any]] = [
        {
            "type": "image",
            "source": {"type": "base64", "media_type": img.media_type, "data": _image_data(img, base_dir)},
        }
        for img in req.images
    ]
    blocks.append({"type": "text", "text": text})
    return blocks


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
        enforce_schema: bool = False,
        progress: Callable[[str], None] | None = None,
        sandbox_output: bool = True,
    ):
        """`enforce_schema=False` (default) puts the schema in the system prompt and validates the reply
        client-side. `True` also sends it as `output_config.format` so the API constrains decoding;
        the API currently rejects this schema's grammar as too large, see README."""
        self.client = client or anthropic.Anthropic()  # credentials from env / `ant auth login`
        self.model = model
        self.effort = effort
        self.max_tokens = max_tokens
        self.system_prompt = system_prompt or load_system_prompt()
        self.use_server_fallbacks = use_server_fallbacks
        self.show_thinking_summary = show_thinking_summary
        self.enforce_schema = enforce_schema
        self.progress = progress  # called with a one-line notice per search / fetch / output milestone
        # Build + validate the JSON in the code-execution sandbox and hand it back as a file (Files API)
        # instead of re-typing it as output text; falls back to the text block when no file comes back.
        self.sandbox_output = sandbox_output
        if not enforce_schema:
            from anthropic.lib._parse._transform import transform_schema

            self.system_prompt += (
                "\n\n# Output schema\n\nReturn exactly one JSON object, no prose and no code fences, "
                "conforming to this JSON Schema:\n\n" + json.dumps(transform_schema(AnalysisResponse), separators=(",", ":"))
            )

    # ------------------------------------------------------------------ #
    def _report_progress(self, stream: Any) -> None:
        """Walk the raw stream events and emit one-line notices; get_final_message() still works after."""
        started = time.monotonic()
        reported_chars = 0

        def say(msg: str) -> None:
            t = int(time.monotonic() - started)
            self.progress(f"[{t // 60:>3}:{t % 60:02}] {msg}")  # type: ignore[misc]

        for event in stream:
            et = event.type
            if et == "message_start":
                say(f"turn started ({event.message.model}, {event.message.id})")
            elif et == "content_block_start":
                b = event.content_block
                if b.type == "thinking":
                    say("thinking...")
                elif b.type == "web_search_tool_result":
                    n = len(b.content) if isinstance(b.content, list) else 0
                    say(f"web_search returned {n} results")
                elif b.type == "web_fetch_tool_result":
                    say("web_fetch returned document")
            elif et == "content_block_stop":
                b = event.content_block
                if b.type in ("server_tool_use", "tool_use"):
                    inp = b.input if isinstance(b.input, dict) else {}
                    say(f"{b.name}: {inp.get('query') or inp.get('url') or inp}")
            elif et == "text":
                total = len(event.snapshot)
                if total >= reported_chars + 2000:
                    reported_chars = total
                    say(f"writing output: {total} chars")
            elif et == "message_delta":
                say(f"turn ended: {event.delta.stop_reason} ({event.usage.output_tokens} output tokens)")

    def analyze(
        self, req: AnalysisRequest, today: str | None = None, image_base_dir: Path | None = None
    ) -> AnalysisResult:
        """`image_base_dir` resolves relative `images[].path` entries (the CLI passes the request file's dir)."""
        today = today or time.strftime("%Y-%m-%d")
        content = build_user_content(req, today, image_base_dir)
        if self.sandbox_output:
            if isinstance(content, str):
                content += "\n\n" + SANDBOX_OUTPUT_INSTRUCTION
            else:
                content[-1]["text"] += "\n\n" + SANDBOX_OUTPUT_INSTRUCTION
        messages: list[dict[str, Any]] = [{"role": "user", "content": content}]
        tools = list(WEB_TOOLS) if req.allow_web_research else []
        if self.sandbox_output:
            tools.append(CODE_EXECUTION_TOOL)

        kwargs: dict[str, Any] = dict(
            model=self.model,
            max_tokens=self.max_tokens,
            system=[{"type": "text", "text": self.system_prompt, "cache_control": {"type": "ephemeral"}}],
            output_config={"effort": self.effort},
        )
        if self.enforce_schema:
            kwargs["output_format"] = AnalysisResponse
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
        container_id: str | None = None
        for attempt in range(MAX_PAUSE_RESUMES + 1):
            if container_id:
                kwargs["container"] = container_id  # keep the sandbox (files, Python state) on resume
            with self.client.beta.messages.stream(messages=messages, **kwargs) as stream:
                if self.progress:
                    rid = stream.response.headers.get("request-id", "-")
                    self.progress(f"connected: HTTP {stream.response.status_code}, request-id {rid}")
                    self._report_progress(stream)
                final = stream.get_final_message()
            usage.add(final.usage)
            if getattr(final, "_request_id", None):
                request_ids.append(final._request_id)
            if final.stop_reason != "pause_turn":
                break
            # Long server-tool turn paused: append the partial assistant turn and resume in the same container.
            container = getattr(final, "container", None)
            if container is not None and getattr(container, "id", None):
                container_id = container.id
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
        if self.sandbox_output:
            for fid in file_ids_in(final.content):
                if self.progress:
                    self.progress(f"sandbox file {fid}")
                meta = self.client.files.retrieve_metadata(fid)
                if str(getattr(meta, "filename", "")).endswith(".json"):
                    data = self.client.files.download(fid).read()
                    parsed = AnalysisResponse.model_validate_json(data)
        if parsed is None:
            if not text_blocks:
                raise AnalysisIncomplete(f"no text block in final message (stop_reason={final.stop_reason})")
            parsed = AnalysisResponse.model_validate_json(_strip_fences(text_blocks[-1].text))
        raw_json = parsed.model_dump_json(indent=2)

        warnings = consistency_warnings(parsed)
        if req.previous_response is None:
            from .models import requested_measures

            have = {m.measure_id.value for m in parsed.measures}
            missing = [m for m in requested_measures(req) if m not in have]
            if missing:
                warnings.append(f"{len(missing)} requested catalogue measures missing: {missing}")
        if req.images and not any(src.type.value == "supplied_screenshot" for src in parsed.sources):
            warnings.append(f"{len(req.images)} screenshot(s) were attached but no supplied_screenshot source was registered")
        return AnalysisResult(
            response=parsed,
            raw_json=raw_json,
            model=final.model,
            usage=usage,
            warnings=warnings,
            elapsed_s=time.monotonic() - started,
            request_ids=request_ids,
        )


def file_ids_in(content: Iterable[Any]) -> list[str]:
    """Every file_id in code-execution tool results, in order (SDK objects or plain dicts)."""
    out: list[str] = []

    def walk(v: Any) -> None:
        if isinstance(v, dict):
            fid = v.get("file_id")
            if isinstance(fid, str) and fid not in out:
                out.append(fid)
            for c in v.values():
                walk(c)
        elif isinstance(v, list):
            for c in v:
                walk(c)

    for block in content:
        d = block if isinstance(block, dict) else (block.model_dump() if hasattr(block, "model_dump") else {})
        t = str(d.get("type", ""))
        if t.startswith(("bash_code_execution", "code_execution", "text_editor_code_execution")):
            walk(d)
    return out


def _strip_fences(text: str) -> str:
    t = text.strip()
    if t.startswith("```"):
        t = t.split("\n", 1)[1] if "\n" in t else ""
        if t.rstrip().endswith("```"):
            t = t.rstrip()[:-3]
    return t.strip()


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
        # Trend statistics use annual-like points; quarterly points don't bound the trend's reliability.
        annual_like = [p.reliability_pct for p in m.series if p.value is not None and p.period_type.value != "Q"]
        series_rel = sorted(annual_like or [p.reliability_pct for p in m.series if p.value is not None])
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

    # One reporting currency per analysis: every currency / per_share measure must carry the same code.
    codes = {m.currency for m in r.measures if m.unit.value in ("currency", "per_share")}
    if len(codes) > 1:
        w.append(f"mixed currencies across measures: {sorted(codes)}")
    if "" in codes:
        w.append("currency unit without a currency code")
    for m in r.measures:
        if m.unit.value in ("currency", "per_share") and m.scale.value == "not_applicable":
            w.append(f"{m.measure_id.value}: currency amount without a scale")
        if m.unit.value not in ("currency", "per_share") and (
            m.currency or (m.scale.value != "not_applicable" and m.unit.value != "count")
        ):
            w.append(f"{m.measure_id.value}: {m.unit.value} measure carries currency/scale")

    # Base-currency conversion is optional extra value; when requested it must be complete and consistent.
    has_base = bool(r.fx.base_currency)
    for m in r.measures:
        money = m.unit.value in ("currency", "per_share")
        for p in [m.latest, *m.series]:
            if p.value_base is not None and (not has_base or not money or p.value is None):
                w.append(f"{m.measure_id.value}@{p.period_end}: value_base set but no base currency / not a currency / value null")
            if has_base and money and p.value is not None and p.value_base is None:
                w.append(f"{m.measure_id.value}@{p.period_end}: base currency requested but value_base is null")
    if has_base and not r.fx.rates:
        w.append("base currency requested but fx.rates is empty")
    if not has_base and r.fx.rates:
        w.append("fx.rates given without a base currency")
    for fr in r.fx.rates:
        for sid in fr.source_ids:
            if sid not in source_ids:
                w.append(f"fx rate {fr.period_end}: unknown source id {sid}")

    # Signal history: chronological, extremes only with valuation assessed, last point consistent with signal.
    ends = [sp.period_end for sp in r.signal_history]
    if ends != sorted(ends):
        w.append("signal_history not chronological")
    for sp in r.signal_history:
        if sp.category.value in ("extreme_buy", "extreme_sell") and not sp.valuation_assessed:
            w.append(f"signal_history {sp.period_end}: {sp.category.value} without valuation assessed")
        for sid in sp.source_ids:
            if sid not in source_ids:
                w.append(f"signal_history {sp.period_end}: unknown source id {sid}")
    if r.signal_history and r.signal_history[-1].reliability_pct > r.signal.reliability_pct + 10:
        w.append("signal_history last point is rated more reliable than the current signal")

    if r.data_mode.value == "recall_only":
        too_high = [m.measure_id.value for m in r.measures if m.reliability_pct > 35]
        if too_high:
            w.append(f"recall_only mode but reliability > 35 for: {too_high}")

    if r.signal.category.value in ("extreme_buy", "extreme_sell"):
        weak = [p.pillar.value for p in r.pillars if p.assessment.value in ("neutral", "not_assessed")]
        if weak:
            w.append(f"{r.signal.category.value} with non-aligned pillars: {weak}")
    return w

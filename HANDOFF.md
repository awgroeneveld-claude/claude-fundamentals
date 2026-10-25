# Handoff: state on 2026-10-25

Branch: `claude/wizardly-thompson-1b980g`. The README covers usage, the quarterly workflow (§6) and API key handling (§7).

## Where things stand

- The Rust client (`rust/`) and the Python reference (`fundamentals_client/`) work end to end against the live API. The default model is `claude-sonnet-5-5` with effort high, plus web search/fetch and sandbox file output.
- Schema enforcement (`--enforce-schema`) is off by default because the API rejects the grammar as too large. The schema is embedded in the system prompt instead, and every reply is validated client-side.
- Incremental quarterly updates (`--previous <old.json>`) are verified. The model returns a delta and `merge_incremental` adds it without rewriting history.
- Tests: 23 Rust (`cargo test --manifest-path rust/Cargo.toml`) and 22 Python (`python -m pytest tests`), all passing.

## Last verified run: Volvo Cars "complete" incremental run

`out/volcar_b_complete.json`, made from `--previous out/volcar_b_2026q3.json`:

- 77 catalogue measures (45 newly filled). No previous series point was lost or changed, and all 9 signal-history points were kept.
- Signal: Sell, 68% conviction, overall reliability 80%.
- Cost: about 668 s, 84k output tokens, 7.46M cache-read tokens and 11 searches, roughly $3.
- One warning: `RevenueCagr: trend reliability 85 with <3 points`. The model rated a single-figure CAGR above the rubric cap. A prompt rule that caps summary-statistic measures (CAGR, CV, percentile, F-Score) at 50 was added in the commit that also added this file. It takes effect on the next run.

## Next steps

1. **PAH3 incremental run** once Porsche SE publishes Q3 (IB lists 6 Nov 2026). Set the end date in `examples/request_pah3.json` to the release date, then run:
   `./target/release/fundamentals analyze ../examples/request_pah3.json --previous ../out/pah3_v2.json --out ../out/pah3_2026q3.json --report ../out/pah3_2026q3.md`
2. **Volvo Cars Q4/FY2026** (late January 2027): run the same command with `--previous ../out/volcar_b_complete.json`.
3. **Probe table still to share:** the output of `./target/release/fundamentals probe-schema`, with the pruned variants. It shows whether any part of the schema can be enforced.
4. **Known gaps:**
   - PAH3 peers are missing.
   - The current PAH3 NAV discount needs today's VW and Porsche AG prices. Supply these as screenshots or `supplied_data` if web research misses them.
5. **Known behaviour:** full re-runs can classify past signal-history dates differently (PAH3 2022 and 2023 came back once as buy and once as hold). Use incremental runs so that history stays fixed.

## Things to keep in mind

- Never paste the API key into chat. Use Network secrets or a local `ANTHROPIC_API_KEY` from a secret manager (README §7).
- If a run warns about missing catalogue measures, re-run it incrementally. The merge adds them without touching existing points.

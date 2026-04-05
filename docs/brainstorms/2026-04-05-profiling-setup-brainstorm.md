# Profiling Setup for Agent-Driven Performance Iteration

**Date:** 2026-04-05
**Status:** Ready for planning

## What We're Building

A profiling setup that enables an AI agent (Claude Code) to:
1. Run the app with profiling enabled
2. Read structured performance data as text
3. Identify bottlenecks
4. Make changes and re-profile in a tight iteration loop

## Why This Approach

### Chrome Tracing (primary)
- Outputs structured JSON that Claude Code can read directly
- Zero GUI dependency — fully CLI/agent-driven
- Bevy has built-in support via `bevy/trace_chrome` feature
- Perfetto UI available for human inspection when needed

### Custom CSV Summary (companion)
- Chrome trace JSON is verbose (every span instance, nested)
- A lightweight post-processing step that summarizes per-span aggregate stats (mean, median, max, count) into a simple CSV
- Much easier for an agent to parse and compare across runs
- Could be a small Rust script or a shell pipeline over the JSON

### Span Granularity: Key Systems
Instrument these functions with `info_span!`:
- `softbody_step` (overall fixed-update tick)
- `solver::solve_iteration` (constraint solve per iteration)
- Verlet integration loop (inside softbody_step)
- Bounce/bounds checking
- Transform writeback
- `rebuild_outline_cache` (debug rendering)

This gives enough resolution to pinpoint which phase is expensive without drowning in noise.

## Key Decisions

1. **Chrome tracing as primary backend** — agent-readable JSON output
2. **Custom CSV summarizer** — aggregates span stats for easy comparison across runs
3. **Cargo feature flag `profile`** — gates all instrumentation, zero overhead when disabled
4. **Key-systems-only spans** — 6-8 spans on the hot path, not every function
5. **`--release` profiling** — no point profiling unoptimized builds

## Agent Workflow

```
cargo run --release --features profile  # run briefly, quit
# → produces trace-<timestamp>.json

# summarizer extracts per-span stats
cargo run --bin summarize-trace -- trace-*.json
# → produces trace-summary.csv

# agent reads CSV, identifies bottleneck, makes change, repeats
```

## Benchmark Scene

Deterministic benchmark scene for reproducible profiling runs.

**Scene setup:**
- Single soft body matching current demo (16-point ring)
- Fixed window size (e.g. 1280x720) — identical bounds across runs
- Fixed random seed for future-proofing
- Headless rendering option to isolate physics cost from GPU

**Scripted effector input:**
- Hard-coded synthetic mouse path (e.g. circle around the body, click-drag across)
- Feeds directly into `MouseEffector` resource, bypassing real cursor input
- Fully deterministic, no recording step needed

**Agent workflow (updated):**
```
cargo run --release --features profile --bin benchmark
# → runs benchmark scene for N frames, headless
# → outputs trace JSON to traces/
# → summarizer produces CSV

# agent reads CSV, makes change, re-runs, compares
```

## Resolved Questions

1. **CSV summarizer** — Rust `[[bin]]` target in the same Cargo.toml. Type-safe, no external deps.
2. **Auto-quit** — Yes, configurable frame count via `--frames` flag or `PROFILE_FRAMES` env var for reproducible traces.
3. **Output directory** — `traces/` directory, .gitignored. Keep root clean.

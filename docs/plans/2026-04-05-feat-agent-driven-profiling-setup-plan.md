---
title: "feat: Agent-driven profiling and benchmark setup"
type: feat
status: active
date: 2026-04-05
origin: docs/brainstorms/2026-04-05-profiling-setup-brainstorm.md
---

# Agent-Driven Profiling and Benchmark Setup

## Overview

Set up profiling infrastructure that lets an AI agent (Claude Code) run a deterministic benchmark, read structured performance data, identify bottlenecks, make changes, and re-profile in a tight iteration loop.

## Problem Statement

The codebase has no profiling instrumentation and no way to reproducibly benchmark physics performance. After the architecture refactor (issues #1-#5), we need to measure what's actually slow before optimizing.

## Proposed Solution

Three components gated behind a `profile` cargo feature:

1. **Tracing instrumentation** — `info_span!` on key physics systems
2. **Benchmark binary** — headless deterministic physics simulation
3. **Trace summarizer binary** — reads chrome trace JSON, outputs CSV stats

(see brainstorm: `docs/brainstorms/2026-04-05-profiling-setup-brainstorm.md`)

## Technical Approach

### Phase 1: Plugin split for headless support

`PhysicsPlugin` currently registers everything (physics + rendering + input). Split into:

- **`PhysicsCorePlugin`** — resources (`PhysicsParams`, `DemoConfig`, `WorldBounds`, `MouseEffector`, `SubstepCounter`, `OutlineDirty`) + `FixedUpdate` systems (`softbody_step`) + `Update` system (`reset_substep_counter`)
- **`PhysicsRenderPlugin`** — camera, gizmos, cursor tracking, outline rendering, window bounds update, quit-on-esc

`PhysicsPlugin` becomes a convenience wrapper adding both. The benchmark binary uses only `PhysicsCorePlugin`.

**Files:**
- `src/physics/mod.rs` — split `PhysicsPlugin::build` into `PhysicsCorePlugin` + `PhysicsRenderPlugin`

### Phase 2: Headless spawn and benchmark binary

**Headless spawn function** — `spawn_soft_body_headless` creates `SoftBody` + `Point` components without `Mesh2d`/`MeshMaterial2d`/`Transform`/`Visibility`. Lives in `soft_body.rs` alongside existing `spawn_soft_body`.

**Benchmark binary** at `src/bin/benchmark.rs`:

```
App::new()
    .add_plugins(MinimalPlugins.set(
        ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))
    ))
    .add_plugins(LogPlugin { ... })  // with trace_chrome when profile feature active
    .add_plugins(PhysicsCorePlugin)
    .insert_resource(WorldBounds { half: Vec2::new(640.0, 360.0) })
    .insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
    .add_systems(Startup, spawn_benchmark_scene)
    .add_systems(Update, (scripted_effector, auto_quit))
    .run();
```

Key systems in the benchmark:
- `spawn_benchmark_scene` — calls `spawn_soft_body_headless` with `DemoConfig` defaults, fixed center position
- `scripted_effector` — writes to `MouseEffector` + synthetic `ButtonInput<MouseButton>` state following a circular sweep pattern around the body
- `auto_quit` — reads `FrameCount`, exits after N frames (default 300 = 5s at 60fps simulated)

**WorldBounds**: fixed at 1280x720 (half = 640x360) — matches brainstorm decision.

**ScheduleRunnerPlugin timing**: `run_loop(1/60s)` simulates realistic 60fps frame pacing. At 120Hz physics, this fires ~2 FixedUpdate ticks per Update — realistic workload distribution.

**Files:**
- `src/bin/benchmark.rs` (new)
- `src/physics/soft_body.rs` — add `spawn_soft_body_headless`
- `Cargo.toml` — add `[[bin]]` target

### Phase 3: Tracing instrumentation

Add `info_span!` calls (always present, not feature-gated — ~1ns overhead without subscriber):

| Span name | Location | What it measures |
|-----------|----------|-----------------|
| `softbody_step` | `soft_body.rs` | Full fixed-update tick |
| `verlet_integration` | `soft_body.rs` | Verlet + bounce loop |
| `constraint_solve` | `soft_body.rs` | All solver iterations for one body |
| `solve_iteration` | `solver.rs` | Single Gauss-Seidel iteration |
| `transform_writeback` | `soft_body.rs` | ECS position → Transform sync |
| `rebuild_outline_cache` | `debug.rs` | Outline smoothing (render plugin only) |

**Files:**
- `src/physics/soft_body.rs` — 4 spans
- `src/physics/solver.rs` — 1 span
- `src/physics/debug.rs` — 1 span

### Phase 4: Cargo feature and trace output

Add to `Cargo.toml`:
```toml
[features]
profile = ["bevy/trace", "bevy/trace_chrome"]
```

The benchmark binary configures trace output path via `TRACE_CHROME` env var pointing to `traces/trace.json`. The benchmark's `main()` sets this before app construction:

```rust
std::env::set_var("TRACE_CHROME", "traces/trace.json");
```

Also: add `LogPlugin` to the benchmark (not in `MinimalPlugins` by default) so the chrome layer activates.

**Files:**
- `Cargo.toml` — feature flag
- `src/bin/benchmark.rs` — env var + LogPlugin
- `.gitignore` — add `traces/`
- `traces/` — create directory (or auto-create in benchmark main)

### Phase 5: Trace summarizer binary

`src/bin/summarize_trace.rs` — reads chrome trace JSON from stdin or file arg, outputs CSV to stdout.

**CSV columns:** `name,calls,total_us,mean_us,median_us,max_us,min_us`

**Logic:**
1. Parse JSON array
2. Filter to `ph: "X"` events (complete spans)
3. Group by `name`
4. Compute aggregates
5. Sort by `total_us` descending
6. Print CSV

Reads one file (passed as CLI arg). Agent workflow: `cargo run --bin summarize-trace -- traces/trace.json`

**Files:**
- `src/bin/summarize_trace.rs` (new)
- `Cargo.toml` — add `[[bin]]` target + `serde_json` dependency (only needed for this binary)

## Acceptance Criteria

- [ ] `cargo run` (normal app) works unchanged, zero tracing overhead
- [ ] `cargo run --release --features profile --bin benchmark` runs headless, exits after N frames, produces `traces/trace.json`
- [ ] `cargo run --bin summarize-trace -- traces/trace.json` outputs readable CSV with per-span stats
- [ ] CSV includes at minimum: `softbody_step`, `verlet_integration`, `constraint_solve`, `solve_iteration`
- [ ] Benchmark is fully deterministic — same CSV numbers across identical runs
- [ ] `PhysicsCorePlugin` usable independently of `PhysicsRenderPlugin`
- [ ] `traces/` directory is gitignored

## Agent Iteration Workflow

```bash
# 1. Run benchmark
cargo run --release --features profile --bin benchmark

# 2. Summarize
cargo run --bin summarize-trace -- traces/trace.json

# 3. Read CSV, identify bottleneck
# 4. Make code change
# 5. Repeat from step 1
```

## Dependencies & Risks

- **`MinimalPlugins` + `FixedUpdate` timing** — needs verification that `ScheduleRunnerPlugin::run_loop(1/60s)` correctly accumulates `Time<Fixed>` for 120Hz physics ticks. If wall-clock deltas are too imprecise, may need to manually advance time.
- **`LogPlugin` in headless** — must be added explicitly with `MinimalPlugins`. Need `bevy/bevy_log` feature.
- **`serde_json` dependency** — only needed by summarizer binary. Consider gating behind the `profile` feature to avoid adding it to the main build.
- **`FrameCount` import path** — verify whether it's `bevy::diagnostic::FrameCount` or `bevy::core::FrameCount` in Bevy 0.18.

## Sources & References

- **Origin brainstorm:** [docs/brainstorms/2026-04-05-profiling-setup-brainstorm.md](../brainstorms/2026-04-05-profiling-setup-brainstorm.md) — key decisions: chrome tracing backend, CSV summarizer, benchmark scene with scripted effector, `profile` feature flag
- Bevy profiling docs: `docs/profiling.md` (local)
- Chrome Trace Event Format: `ph: "X"` complete events with `ts` and `dur` in microseconds
- Bevy headless pattern: `MinimalPlugins` + `ScheduleRunnerPlugin`
- Architecture issues: #1-#6 (completed)

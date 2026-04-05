---
title: "Soft-body physics architecture refactor + profiling infrastructure"
date: 2026-04-05
category: architecture
tags: [bevy, ecs, soft-body, pbd, profiling, tracing, refactor, testability]
severity: tech-debt / observability gap
components:
  - src/physics/geometry.rs
  - src/physics/solver.rs
  - src/physics/soft_body.rs
  - src/physics/mod.rs
  - src/config.rs
  - src/bin/benchmark.rs
  - src/bin/summarize_trace.rs
related_issues: ["#1", "#2", "#3", "#4", "#5", "#6", "#7", "#8"]
---

# Soft-Body Physics: Architecture Refactor + Profiling Infrastructure

## Problem

The soft-body simulation was a single-file monolith. Math, constraint solving, ECS orchestration, rendering, and config were all coupled together:

- Pure math (shoelace area, dilation corrections, capsule collision, Chaikin smoothing) entangled with ECS systems — untestable in isolation
- Constraint solver logic inlined in `softbody_step` — no unit tests possible without full Bevy world
- Effector collision duplicated (once per-point, once in solver pass)
- All physics tuning constants hardcoded as flat `const` — no runtime adjustment, wildcard-imported everywhere
- Monolithic `PhysicsPlugin` — can't run headless without a window
- No profiling instrumentation — no way to identify bottlenecks

## Solution

### Architecture Refactor

**Module extraction** — dependency flows one direction: `geometry.rs` (pure math) -> `solver.rs` (algorithm, ECS-free) -> `soft_body.rs` (ECS integration).

| Module | Responsibility | ECS dependency |
|--------|---------------|----------------|
| `geometry.rs` | polygon_area_signed, dilation_corrections, collide_point_with_swept_effector, chaikin_closed_once | None — `bevy::math::Vec2` only |
| `solver.rs` | solve_iteration (Gauss-Seidel constraint pass) | None — takes `&mut [Vec2]` + `EffectorInput` struct |
| `soft_body.rs` | ECS system orchestration, spawn | Full ECS (Query, Res, Commands) |
| `config.rs` | PhysicsParams, DemoConfig | Bevy `Resource` |

**Plugin split**:
```rust
// Core: resources + FixedUpdate, no window
pub struct PhysicsCorePlugin;

// Render: camera, input, gizmos, spawn
pub struct PhysicsRenderPlugin;

// Convenience wrapper
pub struct PhysicsPlugin; // adds both
```

**Spawn dedup** — single function with optional visuals:
```rust
pub fn spawn_soft_body(
    commands: &mut Commands,
    // ... physics params ...
    visuals: Option<&SoftBodyVisuals>,
) -> Entity {
    let e = if let Some(vis) = visuals {
        commands.spawn((Mesh2d(vis.mesh.clone()), ..., point)).id()
    } else {
        commands.spawn(point).id()
    };
}
```

**Config as resources** — `PhysicsParams` (gravity, damping, iterations, substeps) and `DemoConfig` (spawn params) are `Resource + Default`, runtime-tunable. Only `PHYSICS_HZ` stays `const`.

### Profiling Infrastructure

- **`benchmark` binary** — headless via `MinimalPlugins` + `ScheduleRunnerPlugin` + `PhysicsCorePlugin`. Scripted circular effector sweep, auto-quits after 300 frames.
- **`summarize-trace` binary** — parses chrome trace JSON (B/E pairs by `(name, tid)`), outputs CSV: `name,calls,total_us,mean_us,median_us,max_us,min_us`
- **`info_span!`** on hot path: `softbody_step`, `verlet_integration`, `constraint_solve`, `solve_iteration`, `transform_writeback`, `rebuild_outline_cache`
- **`profile` feature** gates `bevy/trace`, `bevy/trace_chrome`, `bevy/bevy_log`, `dep:serde`, `dep:serde_json`

Agent workflow:
```bash
cargo run --release --features profile --bin benchmark
cargo run --features profile --bin summarize-trace -- traces/trace.json
```

## Gotchas

- **`unsafe set_var`** — `std::env::set_var` is unsafe in Rust 2024 edition. Must happen in `main()` before any threads spawn. Documented with `// SAFETY:` comment.
- **B/E tid matching** — chrome trace events have per-thread IDs. Hardcoding `tid=0` breaks multi-threaded traces. Always key on `(name, tid)`.
- **`info_span!` name field** — first arg IS the span name. `info_span!("foo", name = "foo")` is redundant and inflates trace output.
- **serde dep bloat** — unconditional serde deps leak into all binaries including WASM. Gate behind optional features.
- **Benchmark wall-clock timing** — `ScheduleRunnerPlugin::run_loop` uses real sleep. CPU load affects substep count. Run on idle machines for reproducible results.
- **`MouseEffector` in CorePlugin** — currently lives in Core because `softbody_step` reads it. Design smell; long-term should decouple via generic external force.

## Prevention Checklist

### Adding New Physics Code
- [ ] Core math lives in pure functions taking slices/primitives, not `Query` params
- [ ] Pure functions have unit tests
- [ ] ECS systems are thin wrappers: query, extract, call pure fn, write back

### Adding New Binaries
- [ ] Shared logic uses `Option<T>` for visual components, not copy-pasted headless variants
- [ ] No binary pulls in deps it doesn't use

### Adding New Dependencies
- [ ] If only one binary needs it: `optional = true` + feature flag
- [ ] Verify with `cargo tree` it's not leaking into unrelated targets

### Adding New Tracing
- [ ] `info_span!("name")` — don't duplicate with `name = "name"` field
- [ ] `env::set_var` happens pre-thread-spawn only
- [ ] B/E matching uses actual `tid`, never hardcoded

### Code Review Checks
1. Can this function be tested without spinning up a Bevy `App`? If no, extract the math.
2. Does this new dep appear in `cargo tree` for binaries that don't use it?
3. Is there a near-duplicate of this function already? Parameterize, don't fork.
4. Does the new plugin register visual/render systems? If yes, it belongs in RenderPlugin, not Core.

## Test Coverage Added

- 11 geometry tests (area, collision, chaikin, dilation)
- 7 point integration tests (verlet step, bounce)
- 3 solver tests (convergence, contraction, effector push)
- **Total: 21 unit tests**, all pure functions, no ECS dependency

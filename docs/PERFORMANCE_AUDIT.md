# Performance Audit Report

## 1. Project Overview

| Item | Detail |
|---|---|
| **Bevy version** | 0.18.1 |
| **Rust edition** | 2024, stable toolchain |
| **Crate layout** | Single crate, no workspace |
| **Binaries** | `bevy-scratchpad` (app), `benchmark` (headless), `summarize-trace` (profile feature gated) |
| **Key deps** | `bevy` (selective features), `rand`, `tracing`, `serde`/`serde_json` (optional) |
| **Bevy features (app)** | `bevy_winit`, `bevy_window`, `bevy_render`, `bevy_core_pipeline`, `bevy_sprite`, `bevy_sprite_render`, `webgpu`, `bevy_gizmos`, `bevy_gizmos_render` -- no default features |
| **Profile feature** | `profile` enables `bevy/trace`, `bevy/trace_chrome`, `bevy/bevy_log`, `serde`, `serde_json` |
| **Release profile** | `codegen-units = 1`, `lto = "thin"`, `panic = "abort"` |
| **Dev profile** | `opt-level = 1` for crate, `opt-level = 3` for deps |
| **WASM profile** | `wasm-release` inherits release, `opt-level = "z"`, strips debuginfo |
| **No Criterion benches** | `benches/` directory does not exist |

### Where the simulation lives

- **Entry**: `src/main.rs` -- sets up `FixedUpdate` at 120 Hz, adds `PhysicsPlugin`
- **Plugin wiring**: `src/physics/mod.rs` -- `PhysicsCorePlugin` (headless) + `PhysicsRenderPlugin` (visual)
- **Hot path**: `src/physics/soft_body.rs:softbody_step` -> calls `src/physics/solver.rs:solve_iteration` x `constraint_iterations` (default 10)
- **Math**: `src/physics/geometry.rs` -- `polygon_area_signed`, `dilation_corrections`, `collide_point_with_swept_effector`, `chaikin_closed_once`
- **Config**: `src/config.rs` -- `PHYSICS_HZ=120`, `constraint_iterations=10`, `max_substeps_per_frame=3`, `num_points=16`

## 2. Existing Measurement Infrastructure

### 2.1 Headless Benchmark Binary -- `src/bin/benchmark.rs`

- **How to run**: `cargo run --bin benchmark --release --features profile`
- **What it does**: 300 frames at 60 FPS via `ScheduleRunnerPlugin`, headless (no window/rendering), scripted mouse effector sweep, auto-quits
- **What it measures**: Wall-clock cost of physics systems under tracing. Emits `traces/trace.json` in Chrome trace format.
- **Assessment**: **Useful but limited.** Only 16 points. No scaling tests. Wall-clock sleep between frames introduces noise (documented in code comment). No rendering cost captured. No statistical output -- must manually run `summarize-trace`.

### 2.2 Trace Summarizer -- `src/bin/summarize_trace.rs`

- **How to run**: `cargo run --bin summarize-trace --release --features profile -- traces/trace.json`
- **What it does**: Parses Chrome trace JSON, matches B/E pairs and X events, outputs CSV with calls/total/mean/median/max/min per span
- **Assessment**: **Useful.** Provides actionable per-span breakdown. Could benefit from percentile output (p95, p99).

### 2.3 Tracing Spans (manual)

Present in:

- `soft_body.rs:171` -- `softbody_step`
- `soft_body.rs:179` -- `verlet_integration`
- `soft_body.rs:195` -- `constraint_solve`
- `soft_body.rs:236` -- `transform_writeback`
- `solver.rs:33` -- `solve_iteration`
- `debug.rs:36` -- `rebuild_outline_cache`

**Assessment**: Good coverage of the hot path. Missing: `dilation_corrections`, `polygon_area_signed`, `bounce_in_bounds`, `chaikin_closed_once`. But at n=16 these are sub-microsecond.

### 2.4 Docs

- `docs/profiling.md` -- comprehensive reference for Tracy, Chrome tracing, flamegraph, GPU profiling. Generic Bevy doc, not repo-specific commands.
- `docs/OPTIMIZATION_GUIDE.md` -- design rationale doc. Not measurement.

### 2.5 What's Missing

- No Criterion / `#[bench]` microbenchmarks
- No scaling benchmarks (varying `num_points`)
- No render-path benchmarks
- No flamegraph scripts
- No CI regression checks
- No Tracy integration tested
- No allocation profiler (DHAT, jemalloc stats)
- No deterministic timestep benchmark (wall-clock sleep adds noise)

## 3. Simulation Hot Path Walkthrough

### Per fixed-timestep tick (120 Hz), `softbody_step` does:

1. **Verlet integration** -- O(n): for each point, `p.verlet_step(dt, damping)` + `p.bounce_in_bounds(half)`. Simple Vec2 math. ~0.9 us mean at n=16.

2. **Constraint solve** -- O(n x iterations):
   - Copy positions to scratch buffer -- O(n)
   - For each of `constraint_iterations` (10):
     - `solve_iteration`:
       - Distance constraints -- O(n): ring neighbors only, not all-pairs
       - `dilation_corrections` -- O(n): shoelace area + per-vertex normal offset
       - Apply averaged displacements -- O(n)
       - Effector collision -- O(n) when active
   - Write positions back -- O(n)
   - Total: ~11.3 us mean at n=16

3. **Transform writeback** -- O(n): copy `Point.position` -> `Transform.translation`. ~0.5 us mean.

### Scaling behavior

The simulation is **O(n x constraint_iterations) per tick**. No all-pairs interactions. Distance constraints are ring-neighbor only. The `dilation_corrections` function does a full polygon area computation (O(n)) each iteration -- 10 area computations per tick.

At n=16, each tick is ~15 us. This is trivially fast. The overhead is dominated by Bevy scheduling/tracing, not simulation math.

### Per render frame (Update), relevant work:

- `rebuild_outline_cache` -- gated by `OutlineDirty`. Chaikin smoothing O(n), producing 2n vertices.
- `draw_blob_outline` -- `gizmos.linestrip_2d` every frame.
- `draw_effector_gizmo` -- single circle gizmo every frame.

## 4. What I Ran

### Commands executed

```
cargo test                                                    # 21 tests pass, 0.00s
cargo run --bin benchmark --release --features profile        # 300 frames, trace.json produced
cargo run --bin summarize-trace --release --features profile -- traces/trace.json  # CSV output
```

### Key trace results (n=16, 300 frames, 698 FixedUpdate ticks)

| Span | Calls | Mean us | Median us | Max us |
|---|---|---|---|---|
| `softbody_step` | 698 | 15.2 | 13.1 | 74.2 |
| `constraint_solve` | 698 | 11.3 | 9.5 | 50.3 |
| `solve_iteration` | 6980 | 0.5 | 0.4 | 23.6 |
| `verlet_integration` | 698 | 0.9 | 0.7 | 27.2 |
| `transform_writeback` | 698 | 0.5 | 0.4 | 5.8 |

Observation: 698 FixedUpdate ticks / 300 frames = 2.33 ticks/frame. At 120 Hz physics and 60 Hz frame rate, expected is 2. The extra ticks are from wall-clock sleep overshoot causing catch-up (documented caveat).

## 5. Measurement Quality Assessment

### What's trustworthy

- **Relative proportions**: constraint_solve dominates softbody_step (~74%). This ratio is reliable.
- **Scaling relationship**: solve_iteration is called 10x per tick (matches `constraint_iterations=10`). Validated.
- **Span coverage**: The hot path is well-instrumented.

### What's not trustworthy

- **Absolute timing**: Wall-clock sleep in `ScheduleRunnerPlugin` means frame deltas vary. Max spikes (74 us vs 13 us median) may be OS scheduling, not simulation cost.
- **Frame count consistency**: 698 ticks for 300 frames isn't deterministic -- depends on host load.
- **No rendering cost**: Benchmark is headless. Cannot answer "is this GPU-bound in practice?"
- **Tracing overhead**: `info_span!` in `solve_iteration` is called 6980 times. At ~0.5 us mean, tracing overhead may be a significant fraction of actual work at this scale.
- **Scale**: n=16 is too small to reveal algorithmic bottlenecks. Everything is sub-microsecond.

### Can the repo answer these questions?

| Question | Answer |
|---|---|
| Hottest simulation function? | **Yes** -- `constraint_solve` / `solve_iteration` |
| Hottest system? | **Partially** -- system names show as `<Enable debug feature>` |
| CPU vs GPU bottleneck? | **No** -- no rendering in benchmark, no GPU profiling |
| Cost per frame? | **Yes** -- ~115 us mean per update (headless) |
| Cost per substep/iteration? | **Yes** -- ~0.5 us per solve_iteration |
| Scaling as n increases? | **No** -- only n=16 tested |
| Allocations in hot paths? | **No** -- no allocation profiler. Scratch buffers suggest awareness but unverified. |
| Regression detection? | **No** -- no CI, no history, no statistical comparison |

## 6. Top Bottlenecks

### 1. Scale -- n=16 is trivially small

- **Category**: Measurement infrastructure
- **Why it matters**: At n=16, the entire physics tick is 15 us. Bevy scheduling overhead and tracing span entry/exit likely dominate actual math. You can't optimize what you can't measure.
- **Evidence**: `solve_iteration` mean is 0.5 us -- comparable to tracing span creation overhead.
- **Confidence**: **Very high**
- **Payoff**: Prerequisite for all other work. Testing at n=256, n=1024, n=4096 would reveal real bottlenecks.
- **Difficulty**: Low -- parameterize `DemoConfig::num_points` in benchmark.

### 2. `dilation_corrections` calls `polygon_area_signed` every iteration

- **File**: `src/physics/geometry.rs:21-49`
- **Category**: Simulation
- **Why it matters**: Full shoelace area computation (O(n)) runs 10x per tick. At large n, this is redundant -- area delta could be computed incrementally from position changes.
- **Evidence**: Called inside `solve_iteration` which runs 10x per tick. At n=16 it's negligible; at n=1024 it's 10x1024 = 10,240 cross-multiplications per tick.
- **Confidence**: **Medium** -- only matters at high n.
- **Payoff**: ~2x reduction in dilation cost if incremental. Medium payoff.
- **Difficulty**: Medium -- incremental area update is a well-known technique.

### 3. `normalize()` calls in inner loops

- **Files**: `geometry.rs:43` (`dilation_corrections`), `geometry.rs:71` (`collide_point_with_swept_effector`), `solver.rs:48` (implicit via `diff / len`)
- **Category**: Simulation
- **Why it matters**: `normalize()` involves `sqrt`. The distance constraint in `solver.rs:46` already computes `len = diff.length()` (one sqrt per edge per iteration = 10n sqrt per tick).
- **Evidence**: `diff.length()` at `solver.rs:46`, `secant.length_squared() == 0.0` guard + `normalize()` at `geometry.rs:43-46`.
- **Confidence**: **Medium** -- only significant at high n. Bevy's Vec2 normalize is likely SIMD-optimized.
- **Payoff**: Small per call, but high call count at scale.
- **Difficulty**: Low -- `normalize_or_zero()` -> fast reciprocal sqrt, or precompute.

### 4. ECS query overhead per point

- **File**: `soft_body.rs:180-186`, `soft_body.rs:227-230`, `soft_body.rs:238-241`
- **Category**: ECS/scheduling
- **Why it matters**: Three separate loops over `soft.points` doing `q_points.get_mut(e)` by entity ID. ECS random-access by entity is slower than iteration. At n=16 this is irrelevant; at n=1024+ it could show cache pressure.
- **Evidence**: Pattern of `for &e in &soft.points { q_points.get_mut(e) }` appears 3x in the function.
- **Confidence**: **Low-medium** -- Bevy 0.18 entity lookup is O(1) amortized but not cache-friendly for random access patterns.
- **Payoff**: Could be significant at high n. Architecture change to store positions in a `Vec<Vec2>` directly on `SoftBody` instead of per-entity would eliminate ECS overhead entirely for the hot path.
- **Difficulty**: Medium -- requires restructuring how positions are stored.

### 5. Tracing span overhead in `solve_iteration`

- **File**: `solver.rs:33`
- **Category**: Measurement infrastructure
- **Why it matters**: `info_span!("solve_iteration").entered()` is called 6,980 times in 300 frames. Each span entry/exit has non-trivial cost. When actual work is 0.5 us, tracing overhead may be 30-50% of measured time.
- **Evidence**: 6,980 calls x ~0.5 us = 3,507 us total. Removing the span and re-measuring would quantify this.
- **Confidence**: **High** that it distorts measurements; **low** that it matters for production (feature-gated).
- **Payoff**: Cleaner measurements. Could move span to outer `constraint_solve` level only.
- **Difficulty**: Trivial.

## 7. Ranked Next Steps

### Quick wins (< 1 hour each)

1. **Parameterized scaling benchmark** -- modify `benchmark.rs` to accept `--num-points N` via env var or CLI arg. Run at n=16, 64, 256, 1024. This is the single highest-value change.

2. **Remove `solve_iteration` inner span** -- move it to the outer `constraint_solve` span only. Reduces tracing noise 10x.

3. **Add `--features bevy/debug`** to benchmark command so system names appear in traces instead of `<Enable debug feature>`.

4. **Deterministic timestep benchmark** -- replace `ScheduleRunnerPlugin::run_loop` with `run_once` in a manual loop, calling `app.update()` directly with a fixed `Time` advance. Eliminates wall-clock jitter.

### Medium-effort improvements (hours)

5. **Store positions directly on `SoftBody`** -- instead of per-entity `Point` components queried by entity ID, store `Vec<Vec2>` positions + velocities directly on the `SoftBody` component. Eliminates ECS random access in the hot path. Write back to `Transform` only for rendering.

6. **Incremental area computation** -- cache polygon area between iterations, update incrementally from position deltas instead of recomputing shoelace every iteration.

7. **SIMD-friendly constraint loop** -- restructure position data as SOA (separate x/y arrays) for autovectorization of distance + dilation math.

8. **Add allocation profiling** -- run with DHAT or `#[global_allocator]` counting allocator to verify zero allocations in the hot path. The `Local<Vec<...>>` pattern should prevent allocations, but verify.

### Structural / architectural changes (days)

9. **Decouple simulation data from ECS** -- run the solver on raw `Vec<Vec2>` data, only sync to ECS entities for rendering. This is the biggest potential win for scale. The solver already works on `&mut [Vec2]` (`pos_buf`), so the architecture is half-there.

10. **Multi-body benchmark** -- spawn multiple soft bodies to test scheduling and parallelism. Currently only one body exists.

11. **Rendering benchmark** -- create a headed benchmark variant (or use `bevy_render` diagnostic) to measure actual frame times including GPU work.

### Measurement improvements needed first

- **Before optimizing**: Add scaling benchmarks (item 1). Without them, you're optimizing 15 us ticks.
- **Before trusting absolute numbers**: Fix deterministic timing (item 4).
- **Before ECS restructuring**: Profile at n=1024 to see if ECS overhead is actually measurable.
- **Microbench candidates**: `solve_iteration` at various n, `dilation_corrections` alone, `polygon_area_signed` alone.
- **Tracing spans to add**: `dilation_corrections` and `distance_constraints` (separate the two phases within `solve_iteration`) -- but only after removing per-iteration span overhead.
- **Defer**: GPU profiling, WASM profiling, multi-body parallelism -- until simulation cost is actually meaningful.

## 8. Handoff Summary

```
Bevy version: 0.18.1
Rust edition: 2024 (stable)
Simulation: Verlet + PBD ring constraints, O(n x iterations) per tick

Profiling tools found:
- Headless benchmark binary (src/bin/benchmark.rs) -- 300 frames, ScheduleRunnerPlugin
- Chrome trace output (--features profile -> traces/trace.json)
- Trace summarizer (src/bin/summarize_trace.rs) -- CSV stats per span
- Manual tracing spans on hot path (softbody_step, constraint_solve, solve_iteration,
  verlet_integration, transform_writeback)
- No Criterion, no flamegraph scripts, no Tracy wired up, no CI

Commands that worked:
- cargo test -> 21 pass
- cargo run --bin benchmark --release --features profile -> 300 frames, trace.json
- cargo run --bin summarize-trace --release --features profile -- traces/trace.json -> CSV

Top 5 bottlenecks:
1. MEASUREMENT: n=16 is too small to reveal real bottlenecks (everything is <1 us)
2. SIMULATION: dilation_corrections recomputes full polygon area 10x per tick
3. SIMULATION: normalize/sqrt in inner constraint loop (10n per tick)
4. ECS: Random entity lookup pattern in softbody_step (3 separate loops)
5. MEASUREMENT: solve_iteration tracing span overhead distorts sub-us timings

Biggest measurement gaps:
- No scaling benchmarks (only n=16 tested)
- Non-deterministic frame timing (wall-clock sleep)
- No rendering cost measured (benchmark is headless)
- No allocation profiling
- System names hidden (need --features bevy/debug)
- No regression detection / CI

Research questions for next phase:
- What is actual cost at n=256, 1024, 4096?
- At what n does ECS query-by-entity become measurable vs direct Vec access?
- Is incremental shoelace area feasible within PBD constraint framework?
- Can Bevy's FixedUpdate be driven deterministically in a benchmark?
- What does flamegraph show at n=1024 -- is it solver math, ECS, or memory?
- Is SIMD autovectorization happening for the Vec2 math in release mode?
```

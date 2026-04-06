# Phase A Optimization — Handoff

## What was done

Branch: `feat/phase-a-optimization`

### SSBO outline rendering (issue #11, closed)
- Custom `Material2d` + WGSL vertex-pulling shader works
- Root cause of original failure: replacing the `ShaderStorageBuffer` asset creates a new GPU buffer, but the material bind group still referenced the old one
- Fix: touch the material via `materials.get_mut()` after SSBO update to force bind group recreation
- Both Mesh2d and SSBO paths now work; SSBO is wired as the active render path

### Measurement infrastructure
- **Uncapped headed benchmark**: `VSYNC=0` sets `PresentMode::AutoNoVsync`
- **Multi-body**: `NUM_BODIES` env var on both benchmark binaries
- **Area mode A/B**: `AREA_MODE=once_per_step` on both binaries + Criterion
- **Parallel**: `PARALLEL=1` on headless benchmark, uses `ComputeTaskPool`
- Substep counter fixed: now per-tick not per-body (was starving multi-body)

## Key findings

### Render cost
- Uncapped headed bench shows ~3.6ms/frame at 1x256, ~3.9ms at 16x256
- GPU render pipeline is NOT the bottleneck at current scales
- The SSBO upload + present call dominates frame time

### Once-per-step area: not viable
- **2.26x slower** at N=4096 (241µs vs 107µs)
- Stale dilation offset causes solver to fight itself across iterations
- Area preservation is terrible (>250x error vs per-iteration)
- Conclusive: per-iteration area is both faster and more correct

### Multi-body scaling
- Cost is perfectly linear with total point count
- 1x256 = 0.034ms/frame, 64x256 = 2.148ms/frame (63x work, 63x slower)
- Single-body optimization has diminishing returns

### CPU parallelism
- Parallel is slower at all scales except 64x4096 (~5% win)
- Bottleneck: per-body `SolverScratch` allocation every frame
- Fix path: store scratch buffers in the `SoftBody` component

## What to do next

### Immediate (highest value)
1. **Pre-allocate per-body scratch buffers** — store `SolverScratch` in `SoftBody` component instead of allocating fresh each parallel frame. Re-measure parallel with zero-alloc stepping.
2. **Re-measure parallel** after #1. Expect real wins at >=16 bodies.

### Medium term
3. **Inter-body collision** — currently bodies pass through each other. Adding spatial hash or grid-based collision would make multi-body scenarios realistic.
4. **Larger stress scales** — test at 256+ bodies, 16K+ total points to find where CPU ceiling actually is.
5. **Render scaling** — the SSBO outline only renders one body currently. For multi-body headed bench, either spawn multiple SSBO outlines or use instanced rendering.

### Future (only if data justifies)
6. **Compute shader prototype** — only worthwhile once CPU parallel is tuned and render pipeline becomes the bottleneck (probably >100 bodies x >1K points each).
7. **SIMD intrinsics** — LLVM autovectorizes the inner loops already; manual SIMD unlikely to help unless profiler shows missed opportunities.

## Commands reference

```bash
# Criterion microbenches (area mode A/B)
cargo bench --bench solver -- --output-format bencher | grep full_step

# Headless multi-body scaling
NUM_POINTS=256 NUM_BODIES=16 BENCH_FRAMES=300 cargo run --bin benchmark --release

# Headless parallel comparison
NUM_POINTS=4096 NUM_BODIES=64 PARALLEL=0 BENCH_FRAMES=100 cargo run --bin benchmark --release
NUM_POINTS=4096 NUM_BODIES=64 PARALLEL=1 BENCH_FRAMES=100 cargo run --bin benchmark --release

# Headed uncapped
VSYNC=0 NUM_POINTS=256 BENCH_FRAMES=300 cargo run --bin headed-bench --release

# Area mode comparison
AREA_MODE=per_iteration NUM_POINTS=4096 BENCH_FRAMES=300 cargo run --bin benchmark --release
AREA_MODE=once_per_step NUM_POINTS=4096 BENCH_FRAMES=300 cargo run --bin benchmark --release
```

## Architecture notes

- `AreaMode` enum lives in `config.rs`, threaded through `PhysicsParams` → `softbody_step` → `solver_core::step` → `solve_constraints`
- `ParallelPhysics` resource flag selects between `softbody_step` and `softbody_step_parallel` via Bevy run conditions
- `solve_iteration` accepts `dilation_offset: Option<f32>` — `None` = compute per-iteration, `Some` = use cached offset
- `compute_dilation_offset` and `apply_dilation_offset` are public for benchmarking

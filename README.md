# Bevy Scratchpad

## Credits

- Mainly following [this tutorial](https://youtu.be/GXh0Vxg7AnQ) by [argonaut](https://www.youtube.com/@argonautcode) on YouTube.
  - This project is basically a rust port of my [pygame-soft-body project](https://github.com/cmoyates/pygame-soft-body/tree/main) from a few months ago, which itself was mainly following this tutorial.

## Optimization overview

This project includes a number of optimizations tailored for Bevy + WASM soft-body simulation. Highlights:

- ECS scheduling

  - Physics runs in `FixedUpdate` (stable simulation rate via `Time::<Fixed>::from_hz(PHYSICS_HZ)`).
  - Rendering/interaction run in `Update`, decoupled from physics.

- Math & data hot-path

  - Constraint solver uses pre-allocated scratch buffers via `Local<Vec<...>>` to avoid per-iteration allocations.
  - Area/dilation corrections are computed in-place (`dilation_corrections_into`), reusing buffers.
  - Uses `f32` and squared distance checks in tight loops.

- Rendering efficiency

  - Outline (bevy_polyline) updates are gated by a `OutlineDirty` flag set during physics; the GPU vertex buffer is rebuilt only when point positions actually change.
  - Outline updates occur in `Update` to align with the render loop.

- Timestep & stability

  - Fixed timestep at 120 Hz (`PHYSICS_HZ`) with a simple per-frame substep cap (`MAX_SUBSTEPS_PER_FRAME`) to avoid long catch-up spikes after stalls.

- WASM specifics

  - Firefox focus/refocus workaround enabled on web: `WinitSettings` forces continuous updates when focused/unfocused.

- Build profiles
  - Dev builds: dependencies compiled with higher `opt-level` for reasonable runtime perf without losing fast local iteration.
  - Release builds: thin LTO, `panic = "abort"`, and a `wasm-release` profile with `opt-level = 'z'` for smaller web binaries.

### References

- Unofficial Bevy Cheat Book: [Optimize for Size (WASM)](https://bevy-cheatbook.github.io/platforms/wasm/size-opt.html)
- Unofficial Bevy Cheat Book: [Performance pitfalls](https://bevy-cheatbook.github.io/pitfalls/performance.html)

### Where things live

- Scheduling and system wiring

  - `src/main.rs`: fixed timestep clock via `Time::<Fixed>::from_hz(PHYSICS_HZ)`; WASM `WinitSettings` (continuous updates); window setup.
  - `src/physics/mod.rs`: systems assigned to `FixedUpdate` (physics) and `Update` (render/input); registers resources (`WorldBounds`, `CursorWorld`, `EffectorState`, `OutlineDirty`, `SubstepCounter`).

- Physics hot-path

  - `src/physics/soft_body.rs`:
    - `softbody_step`: uses `Local<Vec<_>>` scratch buffers; sets `OutlineDirty` when any point moves; honors `MAX_SUBSTEPS_PER_FRAME` via `SubstepCounter`.
    - `dilation_corrections_into`: in-place area correction using provided buffers.
  - `src/physics/point.rs`: particle component and helpers.

- Rendering (outline)

  - `src/physics/debug.rs`: polyline setup/material and 3D camera for bevy_polyline overlay.
  - `src/physics/systems.rs`: `update_blob_outline` gated by `OutlineDirty` and run in `Update`.

- Timestep & stability controls

  - `src/config.rs`: `PHYSICS_HZ`, `CONSTRAINT_ITERATIONS`, `DAMPING_PER_SECOND`, `MAX_SUBSTEPS_PER_FRAME`, sizes and defaults.
  - `src/physics/systems.rs`: `SubstepCounter` and `reset_substep_counter` (cleared each `Update`).

- Build profiles
  - `Cargo.toml`:
    - Dev: `[profile.dev]` and `[profile.dev.package."*"]` for fast iteration with decent runtime perf.
    - Release: `[profile.release]` thin LTO, `panic = "abort"`, `codegen-units = 1`.
    - Web: `[profile.wasm-release]` inherits release, size-first `opt-level = 'z'`.

### Tuning knobs

- Simulation rate: `PHYSICS_HZ` in `src/config.rs`.
- Constraint quality: `CONSTRAINT_ITERATIONS` in `src/config.rs`.
- Damping: `DAMPING_PER_SECOND` (per-second parameter; frame-independent).
- Catch-up control: `MAX_SUBSTEPS_PER_FRAME` to cap heavy work per render frame.
- Outline cost: adjust when `OutlineDirty` is set (currently on any movement) or reduce smoothing density.

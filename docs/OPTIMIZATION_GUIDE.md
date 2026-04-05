# Bevy + WASM Soft-Body Simulation Optimization Guide

## 1. ECS & System Scheduling

- **Use `FixedUpdate` for physics, `Update` for rendering**  
  Physics (e.g. constraint solving) should run in `FixedUpdate`; rendering and interactivity go in the regular `Update` schedule. This keeps simulation stable and visuals smooth.  
  :contentReference[oaicite:1]{index=1}

- **Batch constraint iterations**  
  Instead of having multiple systems for each sub-step, fold all iterations into a single `satisfy_constraints` system using a reusable `Local<Vec<...>>` buffer to avoid allocations.

- **Fine-grained queries**  
  Use read-only `Query<&Component>` where possible; limit mutable queries to only the components that actually change per system to reduce borrow conflicts and improve scheduling.

- **Avoid frequent spawn/despawn**  
  Instead of creating/destroying entities every frame, toggle their `Visibility` or reuse entities to reduce ECS churn.

## 2. Math & Data Hot-Path

- **Use `f32` and squared distances (`length_squared()`)**  
  Avoid `sqrt`—compare squared distances when testing proximity or collisions.

- **Pre-allocate buffers**  
  Use `Local<T>` or resources to store scratch buffers (e.g. for neighbor lists), and reuse them to avoid per-frame allocations.

- **Compact component design**  
  Keep hot components (e.g., position, prev_position) minimal and cache-friendly. Offload optional data (like debug flags) to separate components.

- **Minimize branching**  
  Separate dynamic vs static points with marker components (e.g., `With<FixedPoint>`) to avoid branching in tight loops.

## 3. Rendering (outline)

- **Update only on change**  
  Track a “dirty” flag for your outline geometry; only update GPU vertex buffers when the positions actually change.

- **Leverage optimized line-rendering crates**  
  If using `bevy_polyline`, let it batch and manage GPU data efficiently—update only vertex arrays, not recreate pipelines.  
  :contentReference[oaicite:2]{index=2}

- **Conditional debugging visuals**  
  Run debug overlays at lower frequency, gate them with `run_if(debug_enabled)` or frame-modulo logic (e.g., once every 3rd frame).

## 4. Timestep & Simulation Stability

- **Fixed-rate physics with cap on substeps**  
  Use a reasonably high fixed-step rate (e.g., 120 Hz) and cap the number of substeps per frame to avoid long catch-up loops.

- **Favor smaller dt over higher stiffness**  
  Smaller fixed timesteps yield more stable constraint behavior and are often cheaper than simulating stiff constraints at low fps.  
  :contentReference[oaicite:3]{index=3}

## 5. WebAssembly Specifics

- **Assume single-threaded**  
  Bevy on WASM generally runs on a single thread unless you explicitly configure multithreading. Avoid thread-based parallelism unless necessary.  
  :contentReference[oaicite:4]{index=4}

- **Handle focus/refocus issues**  
  On Firefox, Bevy WASM builds can drop frames when refocusing. A known workaround is configuring `WinitSettings` to use `UpdateMode::Continuous` for both focused and unfocused modes.  
  :contentReference[oaicite:5]{index=5}

## 6. Build & Size Profiling

- **Optimize release profile**
  ```toml
  [profile.release]
  opt-level = 'z'  # or 's'
  lto = "thin"     # or true with codegen-units = 1
  panic = "abort"
  ```

([Bevy Cheat Book][1])

- **Use `wasm-opt` post-build**

  ```bash
  wasm-opt -Oz -o output.wasm input.wasm     # for size
  wasm-opt -O3 -o output.wasm input.wasm     # for speed
  ```

  ([Bevy Cheat Book][1])

- **Avoid logging in release**
  Disable `console_error_panic_hook`, tracing, and diagnostics for production builds to reduce overhead.

- **Profile early**
  Use browser DevTools to profile CPU usage and flame charts. Confirm where time is spent (physics vs JS binding overhead).

## 7. (Optional) Dev-Time Speedups

- **Speed up debug builds**

  ```toml
  [profile.dev.package."*"]
  opt-level = 3
  [profile.dev]
  opt-level = 1
  ```

  This gives faster debug builds without sacrificing iteration speed.
  ([Bevy Cheat Book][2])

---

### TL;DR Cheat Sheet for IDE

| Category        | What to Do                                       |
| --------------- | ------------------------------------------------ |
| ECS Scheduling  | `FixedUpdate` → physics, `Update` → rendering    |
| Constraint Loop | Single system + `Local<Vec>` scratch buffer      |
| Math & Data     | Use `f32`, squared checks, compact struct layout |
| Rendering       | Dirty-flag outline, update only changed vertices |
| Timestep        | Fixed dt (\~120 Hz) + substep cap                |
| WASM            | Single-thread, Firefox winit workaround          |
| Build/Profiling | Release optimizations + `wasm-opt`, trim logs    |
| Debug Build     | Higher `opt-level` for dependencies              |

---

[1]: https://bevy-cheatbook.github.io/platforms/wasm/size-opt.html?utm_source=chatgpt.com "Optimize for Size - Unofficial Bevy Cheat Book"
[2]: https://bevy-cheatbook.github.io/pitfalls/performance.html?utm_source=chatgpt.com "Slow Performance - Unofficial Bevy Cheat Book"

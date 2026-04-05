# Plan: Make the effector feel rock-solid in your Verlet/PBD blob

1. **Use a swept effector (capsule) instead of a point-in-circle**

   - **What:** Treat the effector motion from `prev -> curr` as a _capsule_ (line segment + radius). For every particle, project it to the capsule boundary if it intersects.
   - **Where:** Keep what you already added in `src/physics/systems.rs`:

     - `EffectorState { prev, curr, radius }` (resource)
     - `update_cursor_world` maintains `prev/curr`
     - `collide_point_with_swept_effector(..)` helper
     - `effector_swept_collision_system` runs **before** `softbody_step` in `FixedUpdate`.

   - **Why:** Continuous (swept) collision avoids tunneling when the mouse moves farther than a single tick. This is the standard PBD trick: treat collisions as _position constraints_ and project to a swept shape.

2. **Add “speculative padding” to the effector radius**

   - **What:** Expand the effective collision radius by the effector travel _this frame_:

     ```rust
     let delta = eff.curr - eff.prev;
     let r_spec = eff.radius + delta.length();
     collide_point_with_swept_effector(&mut pos, eff.prev, eff.curr, r_spec);
     ```

   - **Where:** Inside `effector_swept_collision_system` when you call the helper.
   - **Why:** This anticipates fast motion and catches near-misses. It’s a lightweight version of _speculative contacts_, widely used to reduce CCD misses in real-time engines.

3. **Interleave collision with your PBD constraints**

   - **What:** During your solver loop (you already iterate `CONSTRAINT_ITERATIONS`), call the effector collision _inside_ each iteration so the constraint solver and collision agree sooner:

     ```rust
     for _ in 0..CONSTRAINT_ITERATIONS {
         solve_edge_lengths(..);  // existing
         solve_area_constraint(..); // existing
         // NEW: quick pass over points using r_spec + swept capsule
         effector_project_points(..);
     }
     ```

     (You can refactor the core of `effector_swept_collision_system` into a callable `effector_project_points(..)` that takes a `&mut [Point]` or your ECS query.)

   - **Where:** `src/physics/soft_body.rs`, inside your current constraint loop in `softbody_step`.
   - **Why:** In PBD/XPBD, collisions are just position constraints; interleaving improves convergence and prevents re-penetration during relaxation.

4. **Adaptive micro-substeps when the effector is “too fast”**

   - **What:** If `|eff.curr - eff.prev| > speed_threshold` (e.g., > 0.5× radius), split the sweep into 2–3 mini sweeps **in that frame**:

     ```rust
     let steps = ((delta.length() / (eff.radius * 0.5)).ceil() as u32).clamp(1, 3);
     for i in 0..steps {
         let a = eff.prev.lerp(eff.curr, (i    ) as f32 / steps as f32);
         let b = eff.prev.lerp(eff.curr, (i + 1) as f32 / steps as f32);
         collide_point_with_swept_effector(&mut pos, a, b, eff.radius);
     }
     ```

   - **Where:** Inside your effector collision pass (either the standalone system in `FixedUpdate` or the interleaved call in step 3).
   - **Why:** Classic _sub-stepping_ reduces tunneling by shrinking the effective time step only when needed; it’s a standard mitigation in real-time physics.

5. **Frictiony “stickiness” that doesn’t inject energy (optional polish)**

   - **What:** Instead of adding effector velocity to `previous_position` (which exploded), remove only the _inward_ normal component of the _relative_ velocity and lightly damp the tangent:

     ```rust
     // inside collision branch, after pushing pos to boundary
     let n   = (pos - q).normalize();          // contact normal
     let vp  = pos - prev_pos;                 // particle vel (Verlet)
     let ve  = eff.curr - eff.prev;            // effector vel
     let vrel = vp - ve;                       // relative vel at contact
     let vn = vrel.dot(n);                     // normal component
     let vt = vrel - vn * n;                   // tangential component
     let friction = 0.4;                       // 0..1
     let vn_new = vn.min(0.0);                 // kill *inward* only
     let vrel_new = vt * (1.0 - friction) + n * vn_new;
     let vp_new = vrel_new + ve;
     prev_pos = pos - vp_new;                  // write back without adding energy
     ```

   - **Where:** In your capsule projection code path (where you already have `q` = closest point on the swept segment).
   - **Why:** PBD resolves contacts as position constraints; this velocity-side tweak only _removes_ energy (inward normal + tangential damping), so it’s stable and gives that “grippy” feel. If you later want stiffness that’s time-/iteration-independent, upgrade this to an **XPBD** contact with small compliance.

---

## Notes for the agent

- Keep `effector_swept_collision_system` **before** `softbody_step` in `FixedUpdate`. After step 3, also call the projection **inside** the constraint loop.
- Use the existing gizmo to visualize `EffectorState.curr` and the (optional) speculative radius if helpful.
- Tune in this order: `r_spec` (step 2) → _interleaving_ (step 3) → _substeps threshold_ (step 4) → _friction_ (step 5).
- If you ever migrate to a rigid-body engine, use CCD/TOI settings for the kinematic effector; conceptually the same fix in a different solver. (Background on CCD & speculative contacts.)

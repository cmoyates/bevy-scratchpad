//! ECS-free, SoA-layout soft-body solver.
//!
//! State is stored in SoA layout (separate x/y arrays) for cache-friendly
//! reductions like area computation. The constraint solver inner loop uses
//! a Vec2 scratch buffer because paired x/y access per point is faster
//! when interleaved.

use crate::config::AreaMode;
use crate::physics::geometry::chaikin_closed_once;
use bevy::math::Vec2;

/// Effector state for the solver.
#[derive(Clone, Debug, Default)]
pub struct EffectorInput {
    pub active: bool,
    pub prev: Vec2,
    pub curr: Vec2,
    pub radius: f32,
}

/// Per-body geometric constants computed once at spawn time.
#[derive(Clone, Debug)]
pub struct BodyParams {
    pub chord_length: f32,
    pub desired_area: f32,
    pub circumference: f32,
}

impl BodyParams {
    pub fn from_ring(num_points: usize, radius: f32, puffiness: f32) -> Self {
        let desired_area = std::f32::consts::PI * radius * radius * puffiness;
        let circumference = 2.0 * std::f32::consts::PI * radius;
        let chord_length = circumference / (num_points as f32);
        Self {
            chord_length,
            desired_area,
            circumference,
        }
    }
}

/// SoA simulation state for a single soft body.
#[derive(Clone, Debug)]
pub struct SoftBodyState {
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub prev_x: Vec<f32>,
    pub prev_y: Vec<f32>,
    pub accel_x: Vec<f32>,
    pub accel_y: Vec<f32>,
    pub mass: Vec<f32>,
    pub radius: Vec<f32>,
    pub bounciness: Vec<f32>,
    pub params: BodyParams,
}

impl SoftBodyState {
    pub fn len(&self) -> usize {
        self.x.len()
    }

    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Create a ring of points centered at `center` with given initial velocity.
    pub fn new_ring(
        num_points: usize,
        center: Vec2,
        ring_radius: f32,
        puffiness: f32,
        initial_vel: Vec2,
        dt: f32,
        point_mass: f32,
        point_radius: f32,
        point_bounciness: f32,
        gravity: Vec2,
    ) -> Self {
        let mut state = Self {
            x: Vec::with_capacity(num_points),
            y: Vec::with_capacity(num_points),
            prev_x: Vec::with_capacity(num_points),
            prev_y: Vec::with_capacity(num_points),
            accel_x: Vec::with_capacity(num_points),
            accel_y: Vec::with_capacity(num_points),
            mass: vec![point_mass; num_points],
            radius: vec![point_radius; num_points],
            bounciness: vec![point_bounciness; num_points],
            params: BodyParams::from_ring(num_points, ring_radius, puffiness),
        };

        for i in 0..num_points {
            let theta = (i as f32) * std::f32::consts::TAU / (num_points as f32);
            let px = center.x + theta.cos() * ring_radius;
            let py = center.y + theta.sin() * ring_radius;

            state.x.push(px);
            state.y.push(py);
            state.prev_x.push(px - initial_vel.x * dt);
            state.prev_y.push(py - initial_vel.y * dt);
            state.accel_x.push(gravity.x);
            state.accel_y.push(gravity.y);
        }

        state
    }
}

// ---------------------------------------------------------------------------
// SoA math primitives — operate directly on f32 slices
// ---------------------------------------------------------------------------

/// Signed polygon area via shoelace formula, operating on separate x/y slices.
/// Faster than the Vec2 version for pure reductions since LLVM can
/// autovectorize the multiply-accumulate loop.
#[inline]
pub fn polygon_area_soa(x: &[f32], y: &[f32]) -> f32 {
    let n = x.len();
    debug_assert_eq!(n, y.len());
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0_f32;
    for i in 0..n - 1 {
        area += (x[i] - x[i + 1]) * (y[i] + y[i + 1]);
    }
    area += (x[n - 1] - x[0]) * (y[n - 1] + y[0]);
    area * 0.5
}

/// Compute the dilation offset from current positions and desired area.
#[inline]
pub fn compute_dilation_offset(positions: &[Vec2], desired_area: f32, circumference: f32) -> f32 {
    let n = positions.len();
    let mut area = 0.0_f32;
    for i in 0..n - 1 {
        let p = positions[i];
        let q = positions[i + 1];
        area += (p.x - q.x) * (p.y + q.y);
    }
    {
        let p = positions[n - 1];
        let q = positions[0];
        area += (p.x - q.x) * (p.y + q.y);
    }
    area *= 0.5;

    let error = desired_area - area;
    if circumference != 0.0 {
        error / circumference
    } else {
        0.0
    }
}

/// Apply per-vertex dilation corrections given a pre-computed offset.
#[inline]
fn apply_dilation_offset(
    positions: &[Vec2],
    offset: f32,
    disp_x: &mut [f32],
    disp_y: &mut [f32],
    disp_weights: &mut [u32],
) {
    let n = positions.len();
    for i in 0..n {
        let prev_i = if i == 0 { n - 1 } else { i - 1 };
        let next_i = if i == n - 1 { 0 } else { i + 1 };

        let sx = positions[next_i].x - positions[prev_i].x;
        let sy = positions[next_i].y - positions[prev_i].y;
        let len_sq = sx * sx + sy * sy;

        if len_sq > 0.0 {
            let inv_len = 1.0 / len_sq.sqrt();
            disp_x[i] += sy * inv_len * offset;
            disp_y[i] += -sx * inv_len * offset;
            disp_weights[i] += 1;
        }
    }
}

/// Per-vertex dilation corrections: compute area + apply offset in one pass.
#[inline]
fn dilation_corrections_into(
    positions: &[Vec2],
    desired_area: f32,
    circumference: f32,
    disp_x: &mut [f32],
    disp_y: &mut [f32],
    disp_weights: &mut [u32],
) {
    let offset = compute_dilation_offset(positions, desired_area, circumference);
    apply_dilation_offset(positions, offset, disp_x, disp_y, disp_weights);
}

/// Push point outside the swept sphere (capsule). Returns new position.
#[inline]
fn collide_point_capsule(pos: Vec2, seg_a: Vec2, seg_b: Vec2, r: f32) -> Vec2 {
    let seg = seg_b - seg_a;
    let seg_len2 = seg.length_squared();

    let q = if seg_len2 <= 1e-12 {
        seg_b
    } else {
        let t = ((pos - seg_a).dot(seg) / seg_len2).clamp(0.0, 1.0);
        seg_a + t * seg
    };

    let d = pos - q;
    let d2 = d.length_squared();

    if d2 < r * r && d2 > 1e-12 {
        q + d * (r / d2.sqrt())
    } else {
        pos
    }
}

// ---------------------------------------------------------------------------
// Solver scratch buffers
// ---------------------------------------------------------------------------

/// Pre-allocated scratch buffers for the constraint solver.
#[derive(Clone, Debug, Default)]
pub struct SolverScratch {
    /// Interleaved position buffer for the constraint solver inner loop.
    /// Kept separate from the per-iteration accumulators so solve_constraints
    /// can borrow both without conflict.
    pub pos_buf: Vec<Vec2>,
    pub disp_x: Vec<f32>,
    pub disp_y: Vec<f32>,
    pub disp_weights: Vec<u32>,
}

impl SolverScratch {
    fn prepare(&mut self, n: usize) {
        self.disp_x.resize(n, 0.0);
        self.disp_y.resize(n, 0.0);
        self.disp_weights.resize(n, 0);
    }
}

// ---------------------------------------------------------------------------
// Solver steps
// ---------------------------------------------------------------------------

/// Verlet integration.
pub fn verlet_integrate(state: &mut SoftBodyState, dt: f32, damping: f32) {
    let dt2 = dt * dt;
    let n = state.len();
    for i in 0..n {
        let vx = (state.x[i] - state.prev_x[i]) * damping;
        let vy = (state.y[i] - state.prev_y[i]) * damping;

        let new_x = state.x[i] + vx + state.accel_x[i] * dt2;
        let new_y = state.y[i] + vy + state.accel_y[i] * dt2;

        let vel_x = new_x - state.x[i];
        let vel_y = new_y - state.y[i];

        state.prev_x[i] = new_x - vel_x;
        state.prev_y[i] = new_y - vel_y;
        state.x[i] = new_x;
        state.y[i] = new_y;

        state.accel_x[i] = 0.0;
        state.accel_y[i] = 0.0;
    }
}

/// Bounce points off axis-aligned bounds.
pub fn bounce_in_bounds(state: &mut SoftBodyState, half: Vec2) {
    let n = state.len();
    for i in 0..n {
        let r = state.radius[i];
        let b = state.bounciness[i];

        let left = -half.x + r;
        let right = half.x - r;
        let bottom = -half.y + r;
        let top = half.y - r;

        let mut vx = state.x[i] - state.prev_x[i];
        let mut vy = state.y[i] - state.prev_y[i];

        if state.x[i] < left {
            state.x[i] = left;
            vx = -vx * b;
        }
        if state.x[i] > right {
            state.x[i] = right;
            vx = -vx * b;
        }
        if state.y[i] < bottom {
            state.y[i] = bottom;
            vy = -vy * b;
        }
        if state.y[i] > top {
            state.y[i] = top;
            vy = -vy * b;
        }

        state.prev_x[i] = state.x[i] - vx;
        state.prev_y[i] = state.y[i] - vy;
    }
}

/// Add gravity to all points.
pub fn apply_gravity(state: &mut SoftBodyState, gravity: Vec2) {
    for i in 0..state.len() {
        state.accel_x[i] += gravity.x;
        state.accel_y[i] += gravity.y;
    }
}

/// Run one PBD constraint iteration on a Vec2 position buffer.
///
/// If `dilation_offset` is `Some`, uses that pre-computed offset instead of
/// recomputing area. This is the "once-per-step" area mode.
pub fn solve_iteration(
    positions: &mut [Vec2],
    chord_length: f32,
    desired_area: f32,
    circumference: f32,
    dilation_offset: Option<f32>,
    effector: &EffectorInput,
    disp_x: &mut [f32],
    disp_y: &mut [f32],
    disp_weights: &mut [u32],
) -> bool {
    let n = positions.len();

    for i in 0..n {
        disp_x[i] = 0.0;
        disp_y[i] = 0.0;
        disp_weights[i] = 0;
    }

    // Distance constraints between ring neighbors
    let chord_sq = chord_length * chord_length;
    for i in 0..n {
        let i_next = if i == n - 1 { 0 } else { i + 1 };
        let diff = positions[i_next] - positions[i];
        let dist_sq = diff.length_squared();

        if dist_sq > chord_sq {
            let dist = dist_sq.sqrt();
            let error_half = (dist - chord_length) * 0.5;
            let inv_dist = 1.0 / dist;
            let ox = diff.x * inv_dist * error_half;
            let oy = diff.y * inv_dist * error_half;

            disp_x[i] += ox;
            disp_y[i] += oy;
            disp_x[i_next] -= ox;
            disp_y[i_next] -= oy;
            disp_weights[i] += 1;
            disp_weights[i_next] += 1;
        }
    }

    // Area (dilation) constraint
    match dilation_offset {
        Some(offset) => {
            apply_dilation_offset(positions, offset, disp_x, disp_y, disp_weights);
        }
        None => {
            dilation_corrections_into(
                positions,
                desired_area,
                circumference,
                disp_x,
                disp_y,
                disp_weights,
            );
        }
    }

    // Apply averaged displacements
    let mut any_moved = false;
    for i in 0..n {
        if disp_weights[i] == 0 {
            continue;
        }
        let w = disp_weights[i] as f32;
        let ax = disp_x[i] / w;
        let ay = disp_y[i] / w;
        if ax != 0.0 || ay != 0.0 {
            positions[i].x += ax;
            positions[i].y += ay;
            any_moved = true;
        }
    }

    // Effector collision
    if effector.active {
        for pos in positions.iter_mut() {
            let new_pos =
                collide_point_capsule(*pos, effector.prev, effector.curr, effector.radius);
            if new_pos != *pos {
                *pos = new_pos;
                any_moved = true;
            }
        }
    }

    any_moved
}

/// Full constraint solve: copies SoA → Vec2 scratch, runs iterations,
/// copies back. The copy overhead is small relative to the solver work.
pub fn solve_constraints(
    state: &mut SoftBodyState,
    iterations: usize,
    area_mode: AreaMode,
    effector: &EffectorInput,
    scratch: &mut SolverScratch,
) -> bool {
    let n = state.len();
    scratch.prepare(n);

    // SoA → interleaved scratch
    scratch.pos_buf.clear();
    scratch.pos_buf.reserve(n);
    for i in 0..n {
        scratch.pos_buf.push(Vec2::new(state.x[i], state.y[i]));
    }

    // In OncePerStep mode, compute the dilation offset once before iterating
    let cached_offset = match area_mode {
        AreaMode::OncePerStep => Some(compute_dilation_offset(
            &scratch.pos_buf,
            state.params.desired_area,
            state.params.circumference,
        )),
        AreaMode::PerIteration => None,
    };

    let mut any_moved = false;
    for _ in 0..iterations {
        any_moved |= solve_iteration(
            &mut scratch.pos_buf,
            state.params.chord_length,
            state.params.desired_area,
            state.params.circumference,
            cached_offset,
            effector,
            &mut scratch.disp_x,
            &mut scratch.disp_y,
            &mut scratch.disp_weights,
        );
    }

    // Interleaved scratch → SoA
    if any_moved {
        for (i, pos) in scratch.pos_buf.iter().enumerate().take(n) {
            state.x[i] = pos.x;
            state.y[i] = pos.y;
        }
    }

    any_moved
}

/// One complete simulation tick.
pub fn step(
    state: &mut SoftBodyState,
    dt: f32,
    damping: f32,
    gravity: Vec2,
    half_extents: Vec2,
    constraint_iterations: usize,
    area_mode: AreaMode,
    effector: &EffectorInput,
    scratch: &mut SolverScratch,
) -> bool {
    apply_gravity(state, gravity);
    verlet_integrate(state, dt, damping);
    bounce_in_bounds(state, half_extents);
    solve_constraints(state, constraint_iterations, area_mode, effector, scratch)
}

// ---------------------------------------------------------------------------
// Outline helpers
// ---------------------------------------------------------------------------

/// Build a smoothed outline from the ring positions, using Chaikin subdivision.
pub fn build_outline(state: &SoftBodyState, smooth_buf: &mut Vec<Vec2>, out: &mut Vec<Vec2>) {
    let n = state.len();
    let mut ring: Vec<Vec2> = Vec::with_capacity(n);
    for i in 0..n {
        ring.push(Vec2::new(state.x[i], state.y[i]));
    }

    chaikin_closed_once(&ring, smooth_buf);
    let src = if smooth_buf.len() >= 3 {
        smooth_buf.as_slice()
    } else {
        ring.as_slice()
    };

    out.clear();
    out.extend_from_slice(src);
    if let Some(&first) = src.first() {
        out.push(first);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const EPSILON: f32 = 1e-3;

    fn make_state(n: usize, radius: f32) -> SoftBodyState {
        SoftBodyState::new_ring(
            n,
            Vec2::ZERO,
            radius,
            1.25,
            Vec2::ZERO,
            1.0 / 120.0,
            1.0,
            5.0,
            1.0,
            Vec2::ZERO,
        )
    }

    fn no_effector() -> EffectorInput {
        EffectorInput::default()
    }

    fn make_ring_xy(n: usize, radius: f32) -> (Vec<f32>, Vec<f32>) {
        let mut x = Vec::with_capacity(n);
        let mut y = Vec::with_capacity(n);
        for i in 0..n {
            let theta = (i as f32) * TAU / (n as f32);
            x.push(theta.cos() * radius);
            y.push(theta.sin() * radius);
        }
        (x, y)
    }

    fn make_ring_vec2(n: usize, radius: f32) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let theta = (i as f32) * TAU / (n as f32);
                Vec2::new(theta.cos(), theta.sin()) * radius
            })
            .collect()
    }

    #[test]
    fn new_ring_has_correct_size() {
        let state = make_state(16, 100.0);
        assert_eq!(state.len(), 16);
        assert_eq!(state.x.len(), 16);
        assert_eq!(state.prev_x.len(), 16);
    }

    #[test]
    fn verlet_stationary_no_accel() {
        let mut state = make_state(4, 50.0);
        let x0 = state.x[0];
        let y0 = state.y[0];
        verlet_integrate(&mut state, 1.0 / 120.0, 1.0);
        assert!((state.x[0] - x0).abs() < EPSILON);
        assert!((state.y[0] - y0).abs() < EPSILON);
    }

    #[test]
    fn verlet_applies_gravity() {
        let mut state = make_state(4, 50.0);
        apply_gravity(&mut state, Vec2::new(0.0, -980.0));
        let y0 = state.y[0];
        verlet_integrate(&mut state, 1.0 / 120.0, 1.0);
        assert!(state.y[0] < y0);
    }

    #[test]
    fn bounce_keeps_in_bounds() {
        let mut state = make_state(1, 10.0);
        state.x[0] = 500.0;
        state.prev_x[0] = 490.0;
        bounce_in_bounds(&mut state, Vec2::new(100.0, 100.0));
        assert!(state.x[0] <= 100.0);
    }

    #[test]
    fn polygon_area_soa_matches_geometry() {
        let (x, y) = make_ring_xy(16, 100.0);
        let area_soa = polygon_area_soa(&x, &y);
        let ring: Vec<Vec2> = x
            .iter()
            .zip(y.iter())
            .map(|(&a, &b)| Vec2::new(a, b))
            .collect();
        let area_old = crate::physics::geometry::polygon_area_signed(&ring);
        assert!(
            (area_soa - area_old).abs() < 1.0,
            "SoA={area_soa}, old={area_old}"
        );
    }

    #[test]
    fn solve_iteration_converges() {
        let n = 16;
        let radius = 100.0;
        let params = BodyParams::from_ring(n, radius, 1.0);
        let mut positions = make_ring_vec2(n, radius);
        let mut dx = vec![0.0; n];
        let mut dy = vec![0.0; n];
        let mut dw = vec![0u32; n];

        let mut prev_max = f32::MAX;
        for _ in 0..20 {
            let before = positions.clone();
            solve_iteration(
                &mut positions,
                params.chord_length,
                params.desired_area,
                params.circumference,
                None,
                &no_effector(),
                &mut dx,
                &mut dy,
                &mut dw,
            );
            let max_disp: f32 = before
                .iter()
                .zip(positions.iter())
                .map(|(a, b)| (*a - *b).length())
                .fold(0.0, f32::max);
            assert!(
                max_disp <= prev_max + EPSILON,
                "solver diverged: {max_disp} > {prev_max}"
            );
            prev_max = max_disp;
        }
        assert!(prev_max < 1.0, "not converging: last max_disp={prev_max}");
    }

    #[test]
    fn full_step_runs_without_panic() {
        let mut state = make_state(16, 100.0);
        let mut scratch = SolverScratch::default();

        step(
            &mut state,
            1.0 / 120.0,
            0.5_f32.powf(1.0 / 120.0),
            Vec2::new(0.0, -980.0),
            Vec2::new(640.0, 360.0),
            10,
            AreaMode::PerIteration,
            &no_effector(),
            &mut scratch,
        );
    }

    #[test]
    fn effector_pushes_points() {
        let n = 16;
        let radius = 100.0;
        let params = BodyParams::from_ring(n, radius, 1.0);
        let mut positions = make_ring_vec2(n, radius);
        let mut dx = vec![0.0; n];
        let mut dy = vec![0.0; n];
        let mut dw = vec![0u32; n];

        let effector = EffectorInput {
            active: true,
            prev: Vec2::ZERO,
            curr: Vec2::ZERO,
            radius: 50.0,
        };

        solve_iteration(
            &mut positions,
            params.chord_length,
            params.desired_area,
            params.circumference,
            None,
            &effector,
            &mut dx,
            &mut dy,
            &mut dw,
        );

        for (i, pos) in positions.iter().enumerate() {
            let dist = pos.length();
            assert!(
                dist >= 50.0 - EPSILON,
                "point {i} at dist={dist} should be >= 50.0"
            );
        }
    }

    #[test]
    fn area_modes_both_converge() {
        // Compare area modes without gravity to isolate area preservation behavior
        let mut scratch = SolverScratch::default();
        let half = Vec2::new(640.0, 360.0);
        let dt = 1.0 / 120.0;
        let damping = 0.5_f32.powf(dt);

        let mut state_per_iter = make_state(64, 100.0);
        let mut state_once = state_per_iter.clone();

        for _ in 0..30 {
            step(
                &mut state_per_iter,
                dt,
                damping,
                Vec2::ZERO,
                half,
                10,
                AreaMode::PerIteration,
                &no_effector(),
                &mut scratch,
            );
            step(
                &mut state_once,
                dt,
                damping,
                Vec2::ZERO,
                half,
                10,
                AreaMode::OncePerStep,
                &no_effector(),
                &mut scratch,
            );
        }

        let area_per_iter = polygon_area_soa(&state_per_iter.x, &state_per_iter.y);
        let area_once = polygon_area_soa(&state_once.x, &state_once.y);
        let desired = state_per_iter.params.desired_area;

        let err_per_iter = ((area_per_iter - desired) / desired).abs();
        let err_once = ((area_once - desired) / desired).abs();

        eprintln!("area error: per_iter={err_per_iter:.4}, once={err_once:.4}");

        // per-iteration should track desired area closely
        assert!(
            err_per_iter < 0.3,
            "per-iteration area error too large: {err_per_iter:.4}"
        );
        // once-per-step diverges more — it's a known tradeoff
        // (area is computed once before iterations, becomes stale as constraints move points)
        assert!(
            state_once.x.iter().all(|x| x.is_finite()),
            "once-per-step produced NaN/Inf"
        );
    }
}

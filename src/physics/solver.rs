use bevy::math::Vec2;

use crate::physics::geometry::{collide_point_with_swept_effector, dilation_corrections};

/// Effector state passed into the solver (avoids coupling to ECS resources).
pub struct EffectorInput {
    pub active: bool,
    pub prev: Vec2,
    pub curr: Vec2,
    pub radius: f32,
}

/// Result of one full constraint solve pass.
pub struct SolveResult {
    pub any_moved: bool,
}

/// Run one Gauss-Seidel constraint iteration on a ring of positions.
///
/// Applies distance constraints, area/dilation corrections, and optionally
/// effector collision. Modifies `positions` in place.
pub fn solve_iteration(
    positions: &mut [Vec2],
    chord_length: f32,
    desired_area: f32,
    circumference: f32,
    effector: &EffectorInput,
    disp_accum: &mut Vec<Vec2>,
    disp_weights: &mut Vec<u32>,
    corrections: &mut Vec<Vec2>,
) -> SolveResult {
    let n = positions.len();

    disp_accum.clear();
    disp_accum.resize(n, Vec2::ZERO);
    disp_weights.clear();
    disp_weights.resize(n, 0);

    // Distance constraints between ring neighbors
    for i in 0..n {
        let i_next = (i + 1) % n;
        let diff = positions[i_next] - positions[i];
        let len = diff.length();
        if len > 0.0 && len > chord_length {
            let error = (len - chord_length) * 0.5;
            let offset = diff / len * error;
            disp_accum[i] += offset;
            disp_accum[i_next] -= offset;
            disp_weights[i] += 1;
            disp_weights[i_next] += 1;
        }
    }

    // Area (dilation) constraint
    dilation_corrections(positions, desired_area, circumference, corrections);
    for (i, c) in corrections.iter().copied().enumerate() {
        disp_accum[i] += c;
        disp_weights[i] += 1;
    }

    // Apply averaged displacements
    let mut any_moved = false;
    for i in 0..n {
        if disp_weights[i] == 0 {
            continue;
        }
        let avg = disp_accum[i] / (disp_weights[i] as f32);
        if avg.x != 0.0 || avg.y != 0.0 {
            positions[i] += avg;
            any_moved = true;
        }
    }

    // Effector collision projection
    if effector.active {
        for pos in positions.iter_mut() {
            let mut new_pos = *pos;
            collide_point_with_swept_effector(
                &mut new_pos,
                effector.prev,
                effector.curr,
                effector.radius,
            );
            if new_pos != *pos {
                *pos = new_pos;
                any_moved = true;
            }
        }
    }

    SolveResult { any_moved }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{PI, TAU};

    const EPSILON: f32 = 1e-3;

    fn make_ring(n: usize, radius: f32) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let theta = (i as f32) * TAU / (n as f32);
                Vec2::new(theta.cos(), theta.sin()) * radius
            })
            .collect()
    }

    fn no_effector() -> EffectorInput {
        EffectorInput {
            active: false,
            prev: Vec2::ZERO,
            curr: Vec2::ZERO,
            radius: 0.0,
        }
    }

    #[test]
    fn relaxed_ring_converges() {
        let n = 16;
        let radius = 100.0;
        let mut positions = make_ring(n, radius);

        let circumference = TAU * radius;
        let chord_length = circumference / n as f32;
        let desired_area = PI * radius * radius;

        let mut disp = vec![];
        let mut weights = vec![];
        let mut corr = vec![];

        // Run many iterations — displacement per iteration should shrink
        let mut prev_max = f32::MAX;
        for _ in 0..20 {
            let before = positions.clone();
            solve_iteration(
                &mut positions,
                chord_length,
                desired_area,
                circumference,
                &no_effector(),
                &mut disp,
                &mut weights,
                &mut corr,
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
        // After 20 iterations, per-iteration displacement should be small
        assert!(prev_max < 1.0, "not converging: last max_disp={prev_max}");
    }

    #[test]
    fn stretched_ring_contracts() {
        let n = 16;
        let natural_radius = 100.0;
        let stretched_radius = 150.0;
        let mut positions = make_ring(n, stretched_radius);

        let circumference = TAU * natural_radius;
        let chord_length = circumference / n as f32;
        let desired_area = PI * natural_radius * natural_radius;

        let mut disp = vec![];
        let mut weights = vec![];
        let mut corr = vec![];

        // Run several iterations
        for _ in 0..10 {
            solve_iteration(
                &mut positions,
                chord_length,
                desired_area,
                circumference,
                &no_effector(),
                &mut disp,
                &mut weights,
                &mut corr,
            );
        }

        // Should have contracted toward natural radius
        let avg_radius: f32 = positions.iter().map(|p| p.length()).sum::<f32>() / n as f32;
        assert!(
            avg_radius < stretched_radius,
            "avg_radius={avg_radius} should be < {stretched_radius}"
        );
    }

    #[test]
    fn effector_pushes_points_away() {
        let n = 16;
        let radius = 100.0;
        let mut positions = make_ring(n, radius);

        let circumference = TAU * radius;
        let chord_length = circumference / n as f32;
        let desired_area = PI * radius * radius;

        let effector = EffectorInput {
            active: true,
            prev: Vec2::ZERO,
            curr: Vec2::ZERO,
            radius: 50.0,
        };

        let mut disp = vec![];
        let mut weights = vec![];
        let mut corr = vec![];

        solve_iteration(
            &mut positions,
            chord_length,
            desired_area,
            circumference,
            &effector,
            &mut disp,
            &mut weights,
            &mut corr,
        );

        // All points should be at least effector.radius from origin
        for (i, pos) in positions.iter().enumerate() {
            let dist = pos.length();
            assert!(
                dist >= 50.0 - EPSILON,
                "point {i} at dist={dist} should be >= 50.0"
            );
        }
    }
}

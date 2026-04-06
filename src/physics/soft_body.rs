use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;
use bevy::window::PrimaryWindow;

use tracing::info_span;

use super::point::Point;
use super::solver_core::{self, EffectorInput, SoftBodyState, SolverScratch};
use crate::config::{DemoConfig, PHYSICS_HZ, PhysicsParams};
use crate::physics::systems::MouseEffector;
use crate::physics::systems::SubstepCounter;

/// A soft body whose physics state lives in contiguous CPU-owned arrays.
/// Point entities exist only for rendering (Transform sync).
#[derive(Component)]
pub struct SoftBody {
    /// Render-facing entity handles (one per point).
    pub point_entities: Vec<Entity>,
    /// CPU-owned SoA simulation state -- the solver operates on this directly.
    pub state: SoftBodyState,
}

/// Resource: window half-extents (origin at center in Bevy 2D).
#[derive(Resource, Default, Copy, Clone, Debug)]
pub struct WorldBounds {
    pub half: Vec2,
}

/// Keep `WorldBounds` up to date (resizes / DPI changes).
pub fn update_world_bounds(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut bounds: ResMut<WorldBounds>,
) {
    if let Ok(w) = windows.single() {
        bounds.half = 0.5 * w.size();
    }
}

/// Visual assets for rendering soft body points. Pass `None` for headless.
pub struct SoftBodyVisuals {
    pub mesh: Handle<Mesh>,
    pub material: Handle<ColorMaterial>,
}

/// Spawn a soft body ring. Pass `Some(visuals)` for rendered, `None` for headless.
pub fn spawn_soft_body(
    commands: &mut Commands,
    center: Vec2,
    num_points: usize,
    ring_radius: f32,
    puffiness: f32,
    initial_vel: Vec2,
    gravity: Vec2,
    particle_vis_radius: f32,
    mass: f32,
    bounciness: f32,
    visuals: Option<&SoftBodyVisuals>,
) -> Entity {
    let dt = 1.0 / PHYSICS_HZ as f32;

    let state = SoftBodyState::new_ring(
        num_points,
        center,
        ring_radius,
        puffiness,
        initial_vel,
        dt,
        mass,
        particle_vis_radius,
        bounciness,
        gravity,
    );

    let mut point_entities = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let px = state.x[i];
        let py = state.y[i];

        let point = Point::new(Vec2::new(px, py), i);

        let e = if let Some(vis) = visuals {
            commands
                .spawn((
                    Mesh2d(vis.mesh.clone()),
                    MeshMaterial2d(vis.material.clone()),
                    Transform::from_xyz(px, py, 0.0),
                    Visibility::Hidden,
                    point,
                ))
                .id()
        } else {
            commands.spawn(point).id()
        };

        point_entities.push(e);
    }

    commands
        .spawn(SoftBody {
            point_entities,
            state,
        })
        .id()
}

/// Spawn one soft body like the Python demo.
pub fn spawn_demo_like_python(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    demo: Res<DemoConfig>,
    physics: Res<PhysicsParams>,
) {
    commands.spawn(Camera2d);

    let Ok(win) = windows.single() else {
        return;
    };
    let half = 0.5 * win.size();

    let origin_world = Vec2::new(0.0, half.y - (win.height() / 3.0));

    let visuals = SoftBodyVisuals {
        mesh: meshes.add(Circle::new(demo.particle_vis_radius)),
        material: materials.add(Color::srgb(0.2, 0.7, 1.0)),
    };

    spawn_soft_body(
        &mut commands,
        origin_world,
        demo.num_points,
        demo.ring_radius,
        demo.puffiness,
        demo.initial_vel,
        physics.gravity,
        demo.particle_vis_radius,
        demo.default_mass,
        demo.default_bounciness,
        Some(&visuals),
    );
}

/// Fixed-timestep physics: runs the SoA solver, then syncs positions
/// back to Point components and Transforms for rendering.
pub fn softbody_step(
    time: Res<Time>,
    bounds: Res<WorldBounds>,
    physics: Res<PhysicsParams>,
    mut q_soft: Query<&mut SoftBody>,
    mut q_tf: Query<&mut Transform>,
    mut q_points: Query<&mut Point>,
    buttons: Res<ButtonInput<MouseButton>>,
    effector: Res<MouseEffector>,
    mut scratch: Local<SolverScratch>,
    mut outline_dirty: ResMut<crate::physics::systems::OutlineDirty>,
    mut substeps: ResMut<SubstepCounter>,
) {
    let _span = info_span!("softbody_step").entered();

    let dt = time.delta_secs();
    let damping_per_tick = physics.damping_per_second.powf(dt);

    // Skip physics until window bounds are initialized — zero bounds
    // would crush all points to origin via bounce_in_bounds.
    if bounds.half == Vec2::ZERO {
        return;
    }

    let effector_input = EffectorInput {
        active: buttons.pressed(MouseButton::Left),
        prev: effector.prev,
        curr: effector.curr,
        radius: effector.radius,
    };

    // Substep counter limits FixedUpdate ticks per frame, not per-body iterations.
    // Increment once per tick, then process all bodies.
    if substeps.0 >= physics.max_substeps_per_frame {
        return;
    }
    substeps.0 += 1;

    for mut soft in &mut q_soft {
        let any_moved = {
            let _span = info_span!("solver_core_step").entered();
            solver_core::step(
                &mut soft.state,
                dt,
                damping_per_tick,
                physics.gravity,
                bounds.half,
                physics.constraint_iterations,
                physics.area_mode,
                &effector_input,
                &mut scratch,
            )
        };

        // Sync SoA state back to ECS for rendering
        if any_moved {
            let _span = info_span!("render_sync").entered();
            outline_dirty.0 = true;
            for (i, &e) in soft.point_entities.iter().enumerate() {
                if let Ok(mut p) = q_points.get_mut(e) {
                    p.position.x = soft.state.x[i];
                    p.position.y = soft.state.y[i];
                }
                if let Ok(mut tf) = q_tf.get_mut(e) {
                    tf.translation.x = soft.state.x[i];
                    tf.translation.y = soft.state.y[i];
                }
            }
        }
    }
}

/// Resource flag: when present, `softbody_step_parallel` is used instead of `softbody_step`.
#[derive(Resource)]
pub struct ParallelPhysics;

/// Parallel variant: steps all bodies concurrently via ComputeTaskPool,
/// then syncs back to ECS sequentially.
pub fn softbody_step_parallel(
    time: Res<Time>,
    bounds: Res<WorldBounds>,
    physics: Res<PhysicsParams>,
    mut q_soft: Query<&mut SoftBody>,
    mut q_tf: Query<&mut Transform>,
    mut q_points: Query<&mut Point>,
    buttons: Res<ButtonInput<MouseButton>>,
    effector: Res<MouseEffector>,
    mut outline_dirty: ResMut<crate::physics::systems::OutlineDirty>,
    mut substeps: ResMut<SubstepCounter>,
) {
    let _span = info_span!("softbody_step_parallel").entered();

    let dt = time.delta_secs();
    let damping_per_tick = physics.damping_per_second.powf(dt);

    if bounds.half == Vec2::ZERO {
        return;
    }
    if substeps.0 >= physics.max_substeps_per_frame {
        return;
    }
    substeps.0 += 1;

    let effector_input = EffectorInput {
        active: buttons.pressed(MouseButton::Left),
        prev: effector.prev,
        curr: effector.curr,
        radius: effector.radius,
    };

    // Collect mutable state refs for parallel stepping
    let mut bodies: Vec<Mut<SoftBody>> = q_soft.iter_mut().collect();
    let gravity = physics.gravity;
    let half = bounds.half;
    let iterations = physics.constraint_iterations;
    let area_mode = physics.area_mode;

    // Step all bodies in parallel — each gets its own scratch buffer
    let pool = ComputeTaskPool::get();
    let moved_flags: Vec<bool> = pool.scope(|s| {
        for body in bodies.iter_mut() {
            let state = &mut body.state as *mut SoftBodyState;
            let eff = &effector_input;
            s.spawn(async move {
                let mut scratch = SolverScratch::default();
                // SAFETY: each body's state is independent, no aliasing
                let state = unsafe { &mut *state };
                solver_core::step(
                    state,
                    dt,
                    damping_per_tick,
                    gravity,
                    half,
                    iterations,
                    area_mode,
                    eff,
                    &mut scratch,
                )
            });
        }
    });

    // Sync back to ECS sequentially
    for (body, &moved) in bodies.iter().zip(moved_flags.iter()) {
        if moved {
            outline_dirty.0 = true;
            for (i, &e) in body.point_entities.iter().enumerate() {
                if let Ok(mut p) = q_points.get_mut(e) {
                    p.position.x = body.state.x[i];
                    p.position.y = body.state.y[i];
                }
                if let Ok(mut tf) = q_tf.get_mut(e) {
                    tf.translation.x = body.state.x[i];
                    tf.translation.y = body.state.y[i];
                }
            }
        }
    }
}

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::point::Point;
use crate::config::{DemoConfig, PhysicsParams, PHYSICS_HZ};
use crate::physics::solver::{self, EffectorInput};
use crate::physics::systems::MouseEffector;
use crate::physics::systems::SubstepCounter;

/// A soft body made of ring-connected `Point` particles (n-gon).
/// Stores parameters and the spawned point entity IDs.
#[derive(Component)]
pub struct SoftBody {
    pub points: Vec<Entity>,
    pub num_points: usize,
    pub radius: f32,        // nominal ring radius
    pub puffiness: f32,     // scales target area
    pub desired_area: f32,  // target polygon area
    pub circumference: f32, // 2πr
    pub chord_length: f32,  // target edge length
}

impl SoftBody {
    pub fn new(num_points: usize, radius: f32, puffiness: f32) -> Self {
        // These are also available in config as constants; we keep them here
        // so a SoftBody instance can have its own parameters if needed.
        let desired_area = std::f32::consts::PI * radius * radius * puffiness;
        let circumference = 2.0 * std::f32::consts::PI * radius;
        let chord_length = circumference / (num_points as f32);
        Self {
            points: Vec::with_capacity(num_points),
            num_points,
            radius,
            puffiness,
            desired_area,
            circumference,
            chord_length,
        }
    }
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

/// Spawn a soft body as a ring (n-gon) around `center`, each point with the same initial velocity.
/// Returns the SoftBody entity.
pub fn spawn_soft_body(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    num_points: usize,
    ring_radius: f32,
    puffiness: f32,
    initial_vel: Vec2,
    gravity: Vec2,
    particle_vis_radius: f32,
    mass: f32,
    bounciness: f32,
) -> Entity {
    // visual for each point
    let mesh = meshes.add(Circle::new(particle_vis_radius));
    let mat = materials.add(Color::srgb(0.2, 0.7, 1.0));

    // encode v0 in previous_position with the fixed dt
    let dt = 1.0 / PHYSICS_HZ as f32;

    let mut soft = SoftBody::new(num_points, ring_radius, puffiness);

    for i in 0..num_points {
        let theta = (i as f32) * std::f32::consts::TAU / (num_points as f32);
        let curr = center + Vec2::new(theta.cos(), theta.sin()) * ring_radius;

        let mut point = Point::with_initial_velocity(curr, initial_vel, dt, i);
        point.mass = mass;
        point.radius = particle_vis_radius;
        point.bounciness = bounciness;
        point.acceleration = gravity;

        let e = commands
            .spawn((
                // render
                Mesh2d(mesh.clone()),
                MeshMaterial2d(mat.clone()),
                Transform::from_xyz(curr.x, curr.y, 0.0),
                Visibility::Hidden, // hide individual point sprites
                // physics
                point,
            ))
            .id();

        soft.points.push(e);
    }

    commands.spawn(soft).id()
}

/// Spawn one soft body like the Python demo: origin at (WIDTH/2, HEIGHT/3) in window space,
/// converted to Bevy world coords (origin at the center). Also spawns a 2D camera if needed.
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

    // Python used top-left origin; Bevy 2D uses center origin with +Y up.
    let origin_world = Vec2::new(0.0, half.y - (win.height() / 3.0));

    spawn_soft_body(
        &mut commands,
        &mut meshes,
        &mut materials,
        origin_world,
        demo.num_points,
        demo.ring_radius,
        demo.puffiness,
        demo.initial_vel,
        physics.gravity,
        demo.particle_vis_radius,
        demo.default_mass,
        demo.default_bounciness,
    );
}

/// Fixed-timestep integration: Verlet with per-second damping, then
/// PBD-style constraints (distance + area), then write positions to `Transform`.
pub fn softbody_step(
    time: Res<Time>,
    bounds: Res<WorldBounds>,
    physics: Res<PhysicsParams>,
    mut q_points: Query<&mut Point>,
    mut q_tf: Query<&mut Transform>,
    mut q_soft: Query<&mut SoftBody>,
    buttons: Res<ButtonInput<MouseButton>>,
    effector: Res<MouseEffector>,
    mut pos_buf: Local<Vec<Vec2>>,
    mut disp_accum_buf: Local<Vec<Vec2>>,
    mut disp_weight_buf: Local<Vec<u32>>,
    mut corrections_buf: Local<Vec<Vec2>>,
    mut outline_dirty: ResMut<crate::physics::systems::OutlineDirty>,
    mut substeps: ResMut<SubstepCounter>,
) {
    let dt = time.delta_secs();
    let half = bounds.half;
    let damping_per_tick = physics.damping_per_second.powf(dt);

    for soft in &mut q_soft {
        for &e in &soft.points {
            if let Ok(mut p) = q_points.get_mut(e) {
                p.acceleration += physics.gravity;
                p.verlet_step(dt, damping_per_tick);
                p.bounce_in_bounds(half);
            }
        }

        if substeps.0 >= physics.max_substeps_per_frame {
            break;
        }
        substeps.0 += 1;

        pos_buf.clear();
        for &e in &soft.points {
            let pos = q_points.get(e).map(|p| p.position).unwrap_or(Vec2::ZERO);
            pos_buf.push(pos);
        }

        let effector_input = EffectorInput {
            active: buttons.pressed(MouseButton::Left),
            prev: effector.prev,
            curr: effector.curr,
            radius: effector.radius,
        };

        let mut any_moved = false;
        for _ in 0..physics.constraint_iterations {
            let result = solver::solve_iteration(
                &mut pos_buf,
                soft.chord_length,
                soft.desired_area,
                soft.circumference,
                &effector_input,
                &mut disp_accum_buf,
                &mut disp_weight_buf,
                &mut corrections_buf,
            );
            any_moved |= result.any_moved;
        }

        if any_moved {
            outline_dirty.0 = true;
            // Write solved positions back to ECS
            for (i, &e) in soft.points.iter().enumerate() {
                if let Ok(mut p) = q_points.get_mut(e) {
                    p.position = pos_buf[i];
                }
            }
        }

        // --- 3) Write back to Transform for rendering
        for &e in &soft.points {
            if let (Ok(p), Ok(mut tf)) = (q_points.get_mut(e), q_tf.get_mut(e)) {
                tf.translation.x = p.position.x;
                tf.translation.y = p.position.y;
            }
        }
    }
}


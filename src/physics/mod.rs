use bevy::prelude::*;

pub mod point;
pub mod soft_body;
pub use soft_body::WorldBounds;
pub mod systems;

use soft_body::{softbody_step, spawn_demo_like_python, update_world_bounds};

use crate::physics::systems::{CursorWorld, EffectorState, effector_swept_collision_system};

pub mod debug; // <-- add

/// Plug this into your App with `.add_plugins(PhysicsPlugin)`.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app
            // Track window half-extents (origin at window center in Bevy 2D)
            .init_resource::<WorldBounds>()
            .init_resource::<CursorWorld>()
            .init_resource::<EffectorState>()
            // Spawn a camera + one soft body (replace with your own spawner as needed)
            .add_systems(Startup, spawn_demo_like_python)
            // Keep bounds current (window resize / scaling)
            .add_systems(
                Update,
                (
                    update_world_bounds,
                    systems::update_cursor_world, // your cursor tracker
                    debug::draw_effector_gizmo,   // <-- draw gizmo in Update
                    systems::exit_on_esc_or_q_if_native,
                ),
            )
            // Verlet + constraint solve at a fixed timestep (set rate in main via Time::<Fixed>)
            // add the mouse push before the main physics step so constraints
            // can relax the contact right away
            .add_systems(
                FixedUpdate,
                (
                    effector_swept_collision_system, // NEW
                    softbody_step,                   // your existing physics loop
                )
                    .chain(), // ensure ordering: effector before softbody_step
            );
        // Native-only quit shortcut (Esc or Q)
    }
}

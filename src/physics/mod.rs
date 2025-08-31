use bevy::prelude::*;

pub mod point;
pub mod soft_body;
pub use soft_body::WorldBounds;
pub mod systems;

use soft_body::{softbody_step, spawn_demo_like_python, update_world_bounds};

use crate::physics::systems::{
    CursorWorld, EffectorState, OutlineDirty, SubstepCounter, effector_swept_collision_system,
    reset_substep_counter, update_blob_outline,
};

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
            .insert_resource(OutlineDirty(true))
            .init_resource::<SubstepCounter>()
            // Spawn a camera + one soft body (replace with your own spawner as needed)
            .add_systems(
                Startup,
                (
                    spawn_demo_like_python,
                    debug::spawn_blob_outline,
                    debug::spawn_polyline_camera_3d,
                ),
            )
            // Keep bounds current (window resize / scaling)
            .add_systems(
                Update,
                (
                    update_world_bounds,
                    systems::update_cursor_world, // your cursor tracker
                    debug::draw_effector_gizmo,   // effector gizmo
                    systems::exit_on_esc_or_q_if_native,
                    reset_substep_counter, // reset substep count each render frame
                ),
            )
            // Verlet + constraint solve at a fixed timestep (set rate in main via Time::<Fixed>)
            // add the mouse push before the main physics step so constraints
            // can relax the contact right away
            .add_systems(FixedUpdate, effector_swept_collision_system)
            .add_systems(
                FixedUpdate,
                softbody_step.after(effector_swept_collision_system),
            )
            // Update outline once per render frame when dirty
            .add_systems(Update, update_blob_outline);
        // Native-only quit shortcut (Esc or Q)
    }
}

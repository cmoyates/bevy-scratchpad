use bevy::prelude::*;

pub mod geometry;
pub mod point;
pub mod soft_body;
pub use soft_body::WorldBounds;
pub mod systems;

use soft_body::{softbody_step, spawn_demo_like_python, update_world_bounds};

use crate::physics::systems::{
    CursorWorld, EffectorState, OutlineDirty, SubstepCounter, effector_swept_collision_system,
    reset_substep_counter,
};

pub mod debug;

/// Plug this into your App with `.add_plugins(PhysicsPlugin)`.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldBounds>()
            .init_resource::<CursorWorld>()
            .init_resource::<EffectorState>()
            .insert_resource(OutlineDirty(true))
            .init_resource::<SubstepCounter>()
            .init_resource::<debug::OutlineCache>()
            .add_systems(Startup, spawn_demo_like_python)
            .add_systems(
                Update,
                (
                    update_world_bounds,
                    systems::update_cursor_world,
                    debug::draw_effector_gizmo,
                    systems::exit_on_esc_or_q_if_native,
                    reset_substep_counter,
                    debug::rebuild_outline_cache,
                    debug::draw_blob_outline,
                ),
            )
            .add_systems(FixedUpdate, effector_swept_collision_system)
            .add_systems(
                FixedUpdate,
                softbody_step.after(effector_swept_collision_system),
            );
    }
}

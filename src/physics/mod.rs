use bevy::prelude::*;

pub mod geometry;
pub mod point;
pub mod soft_body;
pub mod solver;
pub use soft_body::WorldBounds;
pub mod solver_core;
pub mod systems;

use soft_body::{softbody_step, spawn_demo_like_python, update_world_bounds};

use crate::config::{DemoConfig, PhysicsParams};
use crate::physics::systems::{
    CursorWorld, MouseEffector, OutlineDirty, SubstepCounter, reset_substep_counter,
};

pub mod debug;
pub mod outline_render;

/// Core physics: resources + FixedUpdate systems. No rendering, no window.
pub struct PhysicsCorePlugin;

impl Plugin for PhysicsCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PhysicsParams>()
            .init_resource::<DemoConfig>()
            .init_resource::<WorldBounds>()
            .init_resource::<MouseEffector>()
            .insert_resource(OutlineDirty(true))
            .init_resource::<SubstepCounter>()
            .add_systems(Update, reset_substep_counter)
            .add_systems(FixedUpdate, softbody_step);
    }
}

/// Rendering, input, and debug visualization. Requires a window.
pub struct PhysicsRenderPlugin;

impl Plugin for PhysicsRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorWorld>()
            .init_resource::<debug::OutlineCache>()
            .add_systems(Startup, (spawn_demo_like_python, debug::spawn_outline_mesh))
            .add_systems(
                Update,
                (
                    update_world_bounds,
                    systems::update_cursor_world,
                    debug::draw_effector_gizmo,
                    systems::exit_on_esc_or_q_if_native,
                    debug::update_outline_mesh,
                ),
            );
    }
}

/// Convenience: adds both core physics and rendering.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((PhysicsCorePlugin, PhysicsRenderPlugin));
    }
}

use bevy::prelude::*;

pub mod geometry;
pub mod point;
pub mod soft_body;
pub mod solver;
pub use soft_body::WorldBounds;
pub mod solver_core;
pub mod systems;

use soft_body::{
    ParallelPhysics, softbody_step, softbody_step_parallel, spawn_demo_like_python,
    update_world_bounds,
};

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
            .add_systems(
                FixedUpdate,
                (
                    softbody_step.run_if(not(resource_exists::<ParallelPhysics>)),
                    softbody_step_parallel.run_if(resource_exists::<ParallelPhysics>),
                ),
            );
    }
}

/// Rendering, input, and debug visualization. Requires a window.
pub struct PhysicsRenderPlugin;

impl Plugin for PhysicsRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorWorld>()
            .init_resource::<outline_render::OutlineScratch>()
            .add_plugins(bevy::sprite_render::Material2dPlugin::<
                outline_render::OutlineMaterial,
            >::default())
            .add_systems(
                Startup,
                (spawn_demo_like_python, outline_render::spawn_ssbo_outline),
            )
            .add_systems(
                Update,
                (
                    update_world_bounds,
                    systems::update_cursor_world,
                    debug::draw_effector_gizmo,
                    systems::exit_on_esc_or_q_if_native,
                    outline_render::update_ssbo_outline,
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

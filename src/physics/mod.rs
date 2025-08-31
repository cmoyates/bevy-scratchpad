use bevy::prelude::*;

pub mod point;
pub mod soft_body;

pub use point::Point;
pub use soft_body::WorldBounds;

use soft_body::{
    exit_on_esc_or_q_if_native, softbody_step, spawn_demo_softbody, update_world_bounds,
};

/// Plug this into your App with `.add_plugins(PhysicsPlugin)`.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app
            // Track window half-extents (origin at window center in Bevy 2D)
            .init_resource::<WorldBounds>()
            // Spawn a camera + one soft body (replace with your own spawner as needed)
            .add_systems(Startup, spawn_demo_softbody)
            // Keep bounds current (window resize / scaling)
            .add_systems(Update, update_world_bounds)
            // Verlet + constraint solve at a fixed timestep (set rate in main via Time::<Fixed>)
            .add_systems(FixedUpdate, softbody_step)
            // Native-only quit shortcut (Esc or Q)
            .add_systems(Update, exit_on_esc_or_q_if_native);
    }
}

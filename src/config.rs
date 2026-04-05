use bevy::prelude::*;

/// Physics timing — used for Bevy's `Time<Fixed>` setup, stays compile-time.
pub const PHYSICS_HZ: f64 = 120.0;

/// Mouse effector visual/collision radius — used by MouseEffector::default().
pub const MOUSE_RADIUS: f32 = 30.0;

/// Runtime-tunable physics parameters.
#[derive(Resource, Debug, Clone)]
pub struct PhysicsParams {
    pub gravity: Vec2,
    pub damping_per_second: f32,
    pub constraint_iterations: usize,
    pub max_substeps_per_frame: u32,
}

impl Default for PhysicsParams {
    fn default() -> Self {
        Self {
            gravity: Vec2::new(0.0, -980.0),
            damping_per_second: 0.5,
            constraint_iterations: 10,
            max_substeps_per_frame: 3,
        }
    }
}

/// Demo-specific spawn parameters (not needed at runtime by the physics step).
#[derive(Resource, Debug, Clone)]
pub struct DemoConfig {
    pub num_points: usize,
    pub ring_radius: f32,
    pub puffiness: f32,
    pub initial_vel: Vec2,
    pub particle_vis_radius: f32,
    pub default_mass: f32,
    pub default_bounciness: f32,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            num_points: 16,
            ring_radius: 50.0,
            puffiness: 1.25,
            initial_vel: Vec2::new(100.0, 0.0),
            particle_vis_radius: 5.0,
            default_mass: 1.0,
            default_bounciness: 1.0,
        }
    }
}

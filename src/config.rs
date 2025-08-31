use bevy::prelude::*;

/// Physics timing
pub const PHYSICS_HZ: f64 = 120.0;
pub const DAMPING_PER_SECOND: f32 = 0.5;

/// Point / particle defaults
pub const DEFAULT_MASS: f32 = 1.0;
pub const DEFAULT_BOUNCINESS: f32 = 1.0;
pub const PARTICLE_VIS_RADIUS: f32 = 5.0;

/// Soft-body (n-gon blob) configuration
pub const NUM_POINTS: usize = 16; // number of points in the ring
pub const RING_RADIUS: f32 = 50.0; // visual/initial radius
pub const PUFFINESS: f32 = 1.25; // scales the target area (volume preservation)

/// How many constraint solver iterations per tick
pub const CONSTRAINT_ITERATIONS: usize = 10;

/// Gravity (world units per second squared; +Y up)
pub const GRAVITY: Vec2 = Vec2::new(0.0, -980.0);

/// The initial velocity applied to all points
pub const INITIAL_VEL: Vec2 = Vec2::new(100.0, 0.0);

/// Blob is centered at the world origin
pub const CENTER: Vec2 = Vec2::ZERO;

pub const MOUSE_RADIUS: f32 = 40.0;

use bevy::{gizmos::GizmoPlugin, prelude::*};
mod config;
mod physics;

use config::PHYSICS_HZ;
use physics::PhysicsPlugin;

fn main() {
    App::new()
        // Solid black background
        .insert_resource(ClearColor(Color::BLACK))
        // Configure the fixed timestep clock (used by systems in FixedUpdate)
        .insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
        // Default plugins with window overrides for the web canvas
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                // Mount into an existing <canvas id="bevy-canvas"> (web only)
                #[cfg(target_arch = "wasm32")]
                canvas: Some("#bevy-canvas".into()),
                // Let the canvas follow its CSS parent size (web only).
                // Note: ensure the parent’s size is not driven by the canvas itself,
                // or you can get a resize feedback loop (see docs).
                #[cfg(target_arch = "wasm32")]
                fit_canvas_to_parent: true,
                // Don’t prevent default browser shortcuts (web only)
                #[cfg(target_arch = "wasm32")]
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        // Your custom physics + point systems
        .add_plugins(PhysicsPlugin)
        .run();
}

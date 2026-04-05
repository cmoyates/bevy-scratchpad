use bevy::prelude::*;
mod config;
mod physics;

use config::PHYSICS_HZ;
use physics::PhysicsPlugin;

fn main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::BLACK));
    app.insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ));

    #[cfg(target_arch = "wasm32")]
    {
        use bevy::winit::{UpdateMode, WinitSettings};
        app.insert_resource(WinitSettings {
            focused_mode: UpdateMode::Continuous,
            unfocused_mode: UpdateMode::Continuous,
        });
    }

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            #[cfg(target_arch = "wasm32")]
            canvas: Some("#bevy-canvas".into()),
            #[cfg(target_arch = "wasm32")]
            fit_canvas_to_parent: true,
            #[cfg(target_arch = "wasm32")]
            prevent_default_event_handling: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(PhysicsPlugin);

    app.run();
}

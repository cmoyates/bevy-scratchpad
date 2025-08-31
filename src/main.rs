use bevy::prelude::*;
mod config;
mod physics;

use config::PHYSICS_HZ;
use physics::PhysicsPlugin;

fn main() {
    App::new()
        // Solid black background
        .insert_resource(ClearColor(Color::BLACK))
        // Configure the fixed timestep clock (used in FixedUpdate)
        .insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
        // Bevy's core engine features
        .add_plugins(DefaultPlugins)
        // Your custom physics + point systems
        .add_plugins(PhysicsPlugin)
        .run();
}

use bevy::app::ScheduleRunnerPlugin;
use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use std::time::Duration;

use bevy_scratchpad::config::{DemoConfig, PhysicsParams, PHYSICS_HZ};
use bevy_scratchpad::physics::soft_body::{spawn_soft_body_headless, WorldBounds};
use bevy_scratchpad::physics::systems::MouseEffector;
use bevy_scratchpad::physics::PhysicsCorePlugin;

const BENCHMARK_FRAMES: u32 = 300;
const SIMULATED_FPS: f64 = 60.0;
// Canonical window size for deterministic bounds
const HALF_WIDTH: f32 = 640.0;
const HALF_HEIGHT: f32 = 360.0;

fn main() {
    // Direct trace output to traces/ directory
    std::fs::create_dir_all("traces").ok();
    // SAFETY: called before App::new() — no threads exist yet
    unsafe {
        std::env::set_var("TRACE_CHROME", "traces/trace.json");
    }

    let mut app = App::new();

    app.add_plugins(
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / SIMULATED_FPS,
        ))),
    );

    #[cfg(feature = "profile")]
    app.add_plugins(bevy::log::LogPlugin::default());

    app.insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
        .insert_resource(WorldBounds {
            half: Vec2::new(HALF_WIDTH, HALF_HEIGHT),
        })
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PhysicsCorePlugin)
        .add_systems(Startup, spawn_benchmark_scene)
        .add_systems(Update, (scripted_effector, auto_quit));

    app.run();
}

fn spawn_benchmark_scene(
    mut commands: Commands,
    demo: Res<DemoConfig>,
    physics: Res<PhysicsParams>,
) {
    let center = Vec2::new(0.0, HALF_HEIGHT - (HALF_HEIGHT * 2.0 / 3.0));

    spawn_soft_body_headless(
        &mut commands,
        center,
        demo.num_points,
        demo.ring_radius,
        demo.puffiness,
        demo.initial_vel,
        physics.gravity,
        demo.particle_vis_radius,
        demo.default_mass,
        demo.default_bounciness,
    );
}

/// Circular sweep around the body center, pressing mouse button.
fn scripted_effector(
    frame: Res<FrameCount>,
    mut effector: ResMut<MouseEffector>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
) {
    let t = frame.0 as f32 / SIMULATED_FPS as f32;
    let sweep_radius = 80.0;
    let angular_speed = 2.0; // radians per second

    let angle = t * angular_speed;
    let pos = Vec2::new(angle.cos(), angle.sin()) * sweep_radius;

    effector.prev = effector.curr;
    effector.curr = pos;

    // Press mouse button for the middle third of the benchmark
    let progress = frame.0 as f32 / BENCHMARK_FRAMES as f32;
    if (0.33..0.66).contains(&progress) {
        buttons.press(MouseButton::Left);
    } else {
        buttons.release(MouseButton::Left);
    }
}

fn auto_quit(frame: Res<FrameCount>, mut exit: MessageWriter<AppExit>) {
    if frame.0 >= BENCHMARK_FRAMES {
        println!("Benchmark complete: {} frames", frame.0);
        exit.write(AppExit::Success);
    }
}

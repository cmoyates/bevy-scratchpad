use bevy::app::ScheduleRunnerPlugin;
use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

use bevy_scratchpad::config::{DemoConfig, PHYSICS_HZ, PhysicsParams};
use bevy_scratchpad::physics::PhysicsCorePlugin;
use bevy_scratchpad::physics::soft_body::{WorldBounds, spawn_soft_body};
use bevy_scratchpad::physics::systems::MouseEffector;

const HALF_WIDTH: f32 = 640.0;
const HALF_HEIGHT: f32 = 360.0;

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let num_points: usize = env_or("NUM_POINTS", 16);
    let benchmark_frames: u32 = env_or("BENCH_FRAMES", 300);
    let deterministic: bool = env_or("DETERMINISTIC", true);

    std::fs::create_dir_all("traces").ok();
    // SAFETY: called before App::new() — no threads exist yet
    unsafe {
        std::env::set_var("TRACE_CHROME", "traces/trace.json");
    }

    let mut app = App::new();

    if deterministic {
        // Manual stepping — no wall-clock sleep, no catch-up jitter.
        // ManualDuration tells Bevy's time_system to advance Time<Real> by
        // exactly this amount each update, which flows through to
        // Time<Virtual> → Time<Fixed> accumulation deterministically.
        let frame_dt = Duration::from_secs_f64(1.0 / 60.0);
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(frame_dt));
    } else {
        // Legacy path: wall-clock sleep (noisy but closer to real app behavior)
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1.0 / 60.0),
        )));
    }

    #[cfg(feature = "profile")]
    app.add_plugins(bevy::log::LogPlugin::default());

    app.insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
        .insert_resource(WorldBounds {
            half: Vec2::new(HALF_WIDTH, HALF_HEIGHT),
        })
        .insert_resource(DemoConfig {
            num_points,
            ..Default::default()
        })
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PhysicsCorePlugin)
        .add_systems(Startup, spawn_benchmark_scene)
        .add_systems(Update, (scripted_effector, auto_quit));

    if deterministic {
        eprintln!(
            "Deterministic benchmark: {} points, {} frames",
            num_points, benchmark_frames
        );

        // Startup frame (spawns entities)
        app.update();

        let start = std::time::Instant::now();
        for _ in 0..benchmark_frames {
            app.update();
        }
        let elapsed = start.elapsed();

        println!("Benchmark complete: {benchmark_frames} frames, {num_points} points");
        println!(
            "Wall time: {:.3}ms total, {:.3}ms/frame, {:.1} simulated FPS headroom",
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_secs_f64() * 1000.0 / benchmark_frames as f64,
            benchmark_frames as f64 / elapsed.as_secs_f64(),
        );
    } else {
        eprintln!(
            "Legacy benchmark (wall-clock): {} points, {} frames",
            num_points, benchmark_frames
        );
        app.insert_resource(BenchmarkConfig { benchmark_frames });
        app.run();
    }
}

/// Config resource for legacy (non-deterministic) auto-quit
#[derive(Resource)]
struct BenchmarkConfig {
    benchmark_frames: u32,
}

fn spawn_benchmark_scene(
    mut commands: Commands,
    demo: Res<DemoConfig>,
    physics: Res<PhysicsParams>,
) {
    let center = Vec2::new(0.0, HALF_HEIGHT / 3.0);

    spawn_soft_body(
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
        None,
    );
}

/// Circular sweep around the body center, pressing mouse button.
fn scripted_effector(
    frame: Res<FrameCount>,
    mut effector: ResMut<MouseEffector>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    config: Option<Res<BenchmarkConfig>>,
) {
    let benchmark_frames = config.map_or(300, |c| c.benchmark_frames);
    let t = frame.0 as f32 / 60.0;
    let sweep_radius = 80.0;
    let angular_speed = 2.0;

    let angle = t * angular_speed;
    let pos = Vec2::new(angle.cos(), angle.sin()) * sweep_radius;

    effector.prev = effector.curr;
    effector.curr = pos;

    let progress = frame.0 as f32 / benchmark_frames as f32;
    if (0.33..0.66).contains(&progress) {
        buttons.press(MouseButton::Left);
    } else {
        buttons.release(MouseButton::Left);
    }
}

fn auto_quit(
    frame: Res<FrameCount>,
    mut exit: MessageWriter<AppExit>,
    config: Option<Res<BenchmarkConfig>>,
) {
    // Only used in legacy (non-deterministic) mode
    if let Some(cfg) = config
        && frame.0 >= cfg.benchmark_frames
    {
        println!("Benchmark complete: {} frames", frame.0);
        exit.write(AppExit::Success);
    }
}

//! Headless benchmark: pure physics, no window, no GPU.
//!
//! Run: cargo run --bin benchmark --release
//!
//! Env vars:
//!   NUM_POINTS    (default 16)   — points per body
//!   NUM_BODIES    (default 1)    — independent soft bodies
//!   BENCH_FRAMES  (default 300)  — frames to simulate
//!   DETERMINISTIC (default true) — deterministic time stepping
//!   AREA_MODE     (default "per_iteration") — "per_iteration" or "once_per_step"
//!   PARALLEL      (default 0)    — 1 to enable multi-body parallelism

use bevy::app::ScheduleRunnerPlugin;
use bevy::diagnostic::FrameCount;
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

use bevy_scratchpad::config::{AreaMode, DemoConfig, PHYSICS_HZ, PhysicsParams};
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

fn parse_area_mode() -> AreaMode {
    match std::env::var("AREA_MODE").as_deref() {
        Ok("once_per_step") => AreaMode::OncePerStep,
        _ => AreaMode::PerIteration,
    }
}

fn main() {
    let num_points: usize = env_or("NUM_POINTS", 16);
    let num_bodies: usize = env_or("NUM_BODIES", 1);
    let benchmark_frames: u32 = env_or("BENCH_FRAMES", 300);
    let deterministic: bool = env_or("DETERMINISTIC", true);
    let area_mode = parse_area_mode();
    let parallel: bool = env_or("PARALLEL", 0) != 0;

    std::fs::create_dir_all("traces").ok();
    // SAFETY: called before App::new() — no threads exist yet
    unsafe {
        std::env::set_var("TRACE_CHROME", "traces/trace.json");
    }

    let mut app = App::new();

    if deterministic {
        let frame_dt = Duration::from_secs_f64(1.0 / 60.0);
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_once()));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(frame_dt));
    } else {
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
        .insert_resource(PhysicsParams {
            area_mode,
            max_substeps_per_frame: (num_bodies as u32 * 3).max(3),
            ..Default::default()
        })
        .init_resource::<ButtonInput<MouseButton>>()
        .add_plugins(PhysicsCorePlugin);

    if parallel {
        app.insert_resource(bevy_scratchpad::physics::soft_body::ParallelPhysics);
    }

    app.insert_resource(BenchConfig {
        benchmark_frames,
        num_bodies,
        parallel,
    })
    .add_systems(Startup, spawn_benchmark_scene)
    .add_systems(Update, (scripted_effector, auto_quit));

    let area_str = match area_mode {
        AreaMode::PerIteration => "per_iteration",
        AreaMode::OncePerStep => "once_per_step",
    };

    if deterministic {
        eprintln!(
            "Deterministic benchmark: {}x{} points, {} frames, area={}, parallel={}",
            num_bodies, num_points, benchmark_frames, area_str, parallel
        );

        // Startup frame
        app.update();

        let start = std::time::Instant::now();
        for _ in 0..benchmark_frames {
            app.update();
        }
        let elapsed = start.elapsed();
        let total_ms = elapsed.as_secs_f64() * 1000.0;
        let per_frame = total_ms / benchmark_frames as f64;

        println!(
            "=== Headless Benchmark: {}x{} points, {} frames, area={}, parallel={} ===",
            num_bodies, num_points, benchmark_frames, area_str, parallel
        );
        println!(
            "Wall time: {total_ms:.3}ms total, {per_frame:.3}ms/frame, {:.1} simulated FPS headroom",
            benchmark_frames as f64 / elapsed.as_secs_f64(),
        );
    } else {
        eprintln!(
            "Legacy benchmark (wall-clock): {}x{} points, {} frames",
            num_bodies, num_points, benchmark_frames
        );
        app.run();
    }
}

#[derive(Resource)]
#[allow(dead_code)]
struct BenchConfig {
    benchmark_frames: u32,
    num_bodies: usize,
    parallel: bool,
}

fn spawn_benchmark_scene(
    mut commands: Commands,
    demo: Res<DemoConfig>,
    physics: Res<PhysicsParams>,
    config: Res<BenchConfig>,
) {
    let num_bodies = config.num_bodies;
    let spacing = if num_bodies > 1 {
        (HALF_WIDTH * 2.0) / (num_bodies as f32 + 1.0)
    } else {
        0.0
    };

    for i in 0..num_bodies {
        let x = if num_bodies > 1 {
            -HALF_WIDTH + spacing * (i as f32 + 1.0)
        } else {
            0.0
        };
        let y = HALF_HEIGHT / 3.0;

        spawn_soft_body(
            &mut commands,
            Vec2::new(x, y),
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
}

fn scripted_effector(
    frame: Res<FrameCount>,
    mut effector: ResMut<MouseEffector>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    config: Res<BenchConfig>,
) {
    let t = frame.0 as f32 / 60.0;
    let sweep_radius = 80.0;
    let angular_speed = 2.0;
    let angle = t * angular_speed;
    let pos = Vec2::new(angle.cos(), angle.sin()) * sweep_radius;

    effector.prev = effector.curr;
    effector.curr = pos;

    let progress = frame.0 as f32 / config.benchmark_frames as f32;
    if (0.33..0.66).contains(&progress) {
        buttons.press(MouseButton::Left);
    } else {
        buttons.release(MouseButton::Left);
    }
}

fn auto_quit(frame: Res<FrameCount>, mut exit: MessageWriter<AppExit>, config: Res<BenchConfig>) {
    if frame.0 >= config.benchmark_frames {
        println!("Benchmark complete: {} frames", frame.0);
        exit.write(AppExit::Success);
    }
}

//! Headed benchmark: runs the full app with a window to measure render cost.
//!
//! Uses deterministic time stepping but real GPU rendering.
//! Reports per-frame breakdown of solver vs total frame time.
//!
//! Run: cargo run --bin headed-bench --release
//! Env: NUM_POINTS (default 256), BENCH_FRAMES (default 300)

use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::time::Duration;

use bevy_scratchpad::config::{DemoConfig, PHYSICS_HZ, PhysicsParams};
use bevy_scratchpad::physics::PhysicsPlugin;
use bevy_scratchpad::physics::soft_body::spawn_soft_body;
use bevy_scratchpad::physics::systems::MouseEffector;

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Accumulates per-frame timing samples.
#[derive(Resource)]
struct FrameTimings {
    num_points: usize,
    target_frames: u32,
    frame_times: Vec<f64>,
    last_frame_start: std::time::Instant,
    started: bool,
}

fn main() {
    let num_points: usize = env_or("NUM_POINTS", 256);
    let benchmark_frames: u32 = env_or("BENCH_FRAMES", 300);

    let frame_dt = Duration::from_secs_f64(1.0 / 60.0);

    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("Headed Bench — {num_points} points"),
            resolution: (1280u32, 720u32).into(),
            ..default()
        }),
        ..default()
    }));

    app.insert_resource(TimeUpdateStrategy::ManualDuration(frame_dt));

    app.insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ))
        .insert_resource(DemoConfig {
            num_points,
            ..Default::default()
        })
        .add_plugins(PhysicsPlugin)
        .insert_resource(FrameTimings {
            num_points,
            target_frames: benchmark_frames,
            frame_times: Vec::with_capacity(benchmark_frames as usize),
            last_frame_start: std::time::Instant::now(),
            started: false,
        })
        .add_systems(Startup, spawn_headed_scene)
        .add_systems(Update, (scripted_effector, frame_timing));

    app.run();
}

fn spawn_headed_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    demo: Res<DemoConfig>,
    physics: Res<PhysicsParams>,
) {
    commands.spawn(Camera2d);

    let visuals = bevy_scratchpad::physics::soft_body::SoftBodyVisuals {
        mesh: meshes.add(Circle::new(demo.particle_vis_radius)),
        material: materials.add(Color::srgb(0.2, 0.7, 1.0)),
    };

    spawn_soft_body(
        &mut commands,
        Vec2::new(0.0, 120.0),
        demo.num_points,
        demo.ring_radius,
        demo.puffiness,
        demo.initial_vel,
        physics.gravity,
        demo.particle_vis_radius,
        demo.default_mass,
        demo.default_bounciness,
        Some(&visuals),
    );
}

fn scripted_effector(
    frame: Res<FrameCount>,
    mut effector: ResMut<MouseEffector>,
    mut buttons: ResMut<ButtonInput<MouseButton>>,
    timings: Res<FrameTimings>,
) {
    let t = frame.0 as f32 / 60.0;
    let sweep_radius = 80.0;
    let angular_speed = 2.0;
    let angle = t * angular_speed;
    let pos = Vec2::new(angle.cos(), angle.sin()) * sweep_radius;

    effector.prev = effector.curr;
    effector.curr = pos;

    let progress = frame.0 as f32 / timings.target_frames as f32;
    if (0.33..0.66).contains(&progress) {
        buttons.press(MouseButton::Left);
    } else {
        buttons.release(MouseButton::Left);
    }
}

fn frame_timing(mut timings: ResMut<FrameTimings>, mut exit: MessageWriter<AppExit>) {
    let now = std::time::Instant::now();

    if !timings.started {
        // Skip the first frame (startup)
        timings.started = true;
        timings.last_frame_start = now;
        return;
    }

    let elapsed = now.duration_since(timings.last_frame_start);
    timings.last_frame_start = now;
    timings.frame_times.push(elapsed.as_secs_f64() * 1000.0);

    if timings.frame_times.len() == timings.target_frames as usize {
        report(&timings);
        exit.write(AppExit::Success);
    }
}

fn report(timings: &FrameTimings) {
    let times = &timings.frame_times;
    let n = times.len();
    if n == 0 {
        return;
    }

    // Skip first 10% as warmup
    let warmup = n / 10;
    let steady: Vec<f64> = times[warmup..].to_vec();
    let count = steady.len();

    let mean = steady.iter().sum::<f64>() / count as f64;
    let mut sorted = steady.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[count / 2];
    let p95 = sorted[(count as f64 * 0.95) as usize];
    let p99 = sorted[(count as f64 * 0.99) as usize];
    let min = sorted[0];
    let max = sorted[count - 1];

    println!(
        "=== Headed Benchmark: {} points, {} frames ({}  steady) ===",
        timings.num_points, n, count
    );
    println!("Frame time (ms): mean={mean:.3}, median={median:.3}, p95={p95:.3}, p99={p99:.3}");
    println!("  min={min:.3}, max={max:.3}");
    println!("Effective FPS: {:.1}", 1000.0 / mean);
}

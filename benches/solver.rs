//! Criterion microbenches for the pure-math solver core.
//!
//! Run with: cargo bench --bench solver

use bevy::math::Vec2;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use bevy_scratchpad::config::AreaMode;
use bevy_scratchpad::physics::geometry::{dilation_corrections, polygon_area_signed};
use bevy_scratchpad::physics::solver_core::{
    self, BodyParams, EffectorInput, SoftBodyState, SolverScratch,
};

const SCALES: &[usize] = &[16, 64, 256, 1024, 4096];

fn make_ring_vec2(n: usize, radius: f32) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let theta = (i as f32) * std::f32::consts::TAU / (n as f32);
            Vec2::new(theta.cos(), theta.sin()) * radius
        })
        .collect()
}

fn make_ring_soa(n: usize, radius: f32) -> (Vec<f32>, Vec<f32>) {
    let mut x = Vec::with_capacity(n);
    let mut y = Vec::with_capacity(n);
    for i in 0..n {
        let theta = (i as f32) * std::f32::consts::TAU / (n as f32);
        x.push(theta.cos() * radius);
        y.push(theta.sin() * radius);
    }
    (x, y)
}

fn make_state(n: usize) -> SoftBodyState {
    SoftBodyState::new_ring(
        n,
        Vec2::ZERO,
        100.0,
        1.25,
        Vec2::new(100.0, 0.0),
        1.0 / 120.0,
        1.0,
        5.0,
        1.0,
        Vec2::new(0.0, -980.0),
    )
}

fn no_effector() -> EffectorInput {
    EffectorInput::default()
}

// -- polygon_area: Vec2 vs SoA --

fn bench_polygon_area(c: &mut Criterion) {
    let mut group = c.benchmark_group("polygon_area");
    for &n in SCALES {
        let ring = make_ring_vec2(n, 100.0);
        let (x, y) = make_ring_soa(n, 100.0);

        group.bench_with_input(BenchmarkId::new("vec2", n), &n, |b, _| {
            b.iter(|| polygon_area_signed(&ring));
        });
        group.bench_with_input(BenchmarkId::new("soa", n), &n, |b, _| {
            b.iter(|| solver_core::polygon_area_soa(&x, &y));
        });
    }
    group.finish();
}

// -- dilation_corrections: Vec2 vs inlined (via solve_iteration) --

fn bench_dilation_corrections(c: &mut Criterion) {
    let mut group = c.benchmark_group("dilation_corrections_vec2");
    for &n in SCALES {
        let ring = make_ring_vec2(n, 100.0);
        let params = BodyParams::from_ring(n, 100.0, 1.25);
        let mut corrections = Vec::new();
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                dilation_corrections(
                    &ring,
                    params.desired_area,
                    params.circumference,
                    &mut corrections,
                );
            });
        });
    }
    group.finish();
}

// -- solve_iteration --

fn bench_solve_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("solve_iteration");
    for &n in SCALES {
        let ring = make_ring_vec2(n, 100.0);
        let params = BodyParams::from_ring(n, 100.0, 1.25);
        let mut dx = vec![0.0f32; n];
        let mut dy = vec![0.0f32; n];
        let mut dw = vec![0u32; n];

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            let mut positions = ring.clone();
            b.iter(|| {
                solver_core::solve_iteration(
                    &mut positions,
                    params.chord_length,
                    params.desired_area,
                    params.circumference,
                    None,
                    &no_effector(),
                    &mut dx,
                    &mut dy,
                    &mut dw,
                );
            });
        });
    }
    group.finish();
}

// -- full_step --

fn bench_full_step(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_step");
    let mut scratch = SolverScratch::default();

    for &n in SCALES {
        group.bench_with_input(BenchmarkId::new("per_iter", n), &n, |b, &n| {
            let mut state = make_state(n);
            b.iter(|| {
                solver_core::step(
                    &mut state,
                    1.0 / 120.0,
                    0.5_f32.powf(1.0 / 120.0),
                    Vec2::new(0.0, -980.0),
                    Vec2::new(640.0, 360.0),
                    10,
                    AreaMode::PerIteration,
                    &no_effector(),
                    &mut scratch,
                );
            });
        });
        group.bench_with_input(BenchmarkId::new("once_per_step", n), &n, |b, &n| {
            let mut state = make_state(n);
            b.iter(|| {
                solver_core::step(
                    &mut state,
                    1.0 / 120.0,
                    0.5_f32.powf(1.0 / 120.0),
                    Vec2::new(0.0, -980.0),
                    Vec2::new(640.0, 360.0),
                    10,
                    AreaMode::OncePerStep,
                    &no_effector(),
                    &mut scratch,
                );
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_polygon_area,
    bench_dilation_corrections,
    bench_solve_iteration,
    bench_full_step,
);
criterion_main!(benches);

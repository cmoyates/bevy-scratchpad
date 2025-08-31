use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::point::Point;
use crate::config::*;
use crate::physics::systems::EffectorState;
use crate::physics::systems::collide_point_with_swept_effector;

/// How many Gauss–Seidel iterations to run per fixed tick (from config).
pub const CONSTRAINT_ITERATIONS: usize = crate::config::CONSTRAINT_ITERATIONS;

/// A soft body made of ring-connected `Point` particles (n-gon).
/// Stores parameters and the spawned point entity IDs.
#[derive(Component)]
pub struct SoftBody {
    pub points: Vec<Entity>,
    pub num_points: usize,
    pub radius: f32,        // nominal ring radius
    pub puffiness: f32,     // scales target area
    pub desired_area: f32,  // target polygon area
    pub circumference: f32, // 2πr
    pub chord_length: f32,  // target edge length
}

impl SoftBody {
    pub fn new(num_points: usize, radius: f32, puffiness: f32) -> Self {
        // These are also available in config as constants; we keep them here
        // so a SoftBody instance can have its own parameters if needed.
        let desired_area = std::f32::consts::PI * radius * radius * puffiness;
        let circumference = 2.0 * std::f32::consts::PI * radius;
        let chord_length = circumference / (num_points as f32);
        Self {
            points: Vec::with_capacity(num_points),
            num_points,
            radius,
            puffiness,
            desired_area,
            circumference,
            chord_length,
        }
    }
}

/// Resource: window half-extents (origin at center in Bevy 2D).
#[derive(Resource, Default, Copy, Clone, Debug)]
pub struct WorldBounds {
    pub half: Vec2,
}

/// Keep `WorldBounds` up to date (resizes / DPI changes).
pub fn update_world_bounds(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut bounds: ResMut<WorldBounds>,
) {
    if let Ok(w) = windows.single() {
        bounds.half = 0.5 * w.size();
    }
}

/// Spawn a soft body as a ring (n-gon) around `center`, each point with the same initial velocity.
/// Returns the SoftBody entity.
pub fn spawn_soft_body(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    center: Vec2,
    num_points: usize,
    ring_radius: f32,
    initial_vel: Vec2,
    gravity: Vec2,
    particle_vis_radius: f32,
    mass: f32,
    bounciness: f32,
) -> Entity {
    // visual for each point
    let mesh = meshes.add(Circle::new(particle_vis_radius));
    let mat = materials.add(Color::srgb(0.2, 0.7, 1.0));

    // encode v0 in previous_position with the fixed dt
    let dt = 1.0 / PHYSICS_HZ as f32;

    let mut soft = SoftBody::new(num_points, ring_radius, PUFFINESS);

    for i in 0..num_points {
        let theta = (i as f32) * std::f32::consts::TAU / (num_points as f32);
        let curr = center + Vec2::new(theta.cos(), theta.sin()) * ring_radius;

        let mut point = Point::with_initial_velocity(curr, initial_vel, dt, i);
        point.mass = mass;
        point.radius = particle_vis_radius;
        point.bounciness = bounciness;
        point.acceleration = gravity;

        let e = commands
            .spawn((
                // render
                Mesh2d(mesh.clone()),
                MeshMaterial2d(mat.clone()),
                Transform::from_xyz(curr.x, curr.y, 0.0),
                Visibility::Hidden, // hide individual point sprites
                // physics
                point,
            ))
            .id();

        soft.points.push(e);
    }

    commands.spawn(soft).id()
}

/// Spawn one soft body like the Python demo: origin at (WIDTH/2, HEIGHT/3) in window space,
/// converted to Bevy world coords (origin at the center). Also spawns a 2D camera if needed.
pub fn spawn_demo_like_python(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // camera (if you already spawn one elsewhere, remove this)
    commands.spawn(Camera2d);

    let Ok(win) = windows.single() else {
        return;
    };
    let half = 0.5 * win.size();

    // Python used top-left origin; Bevy 2D uses center origin with +Y up.
    // Python origin = (WIDTH/2, HEIGHT/3)  →  Bevy world:
    // x: centered ⇒ 0.0
    // y: move down from top by HEIGHT/3  ⇒  +half.y (top) - HEIGHT/3
    let origin_world = Vec2::new(0.0, half.y - (win.height() / 3.0));

    spawn_soft_body(
        &mut commands,
        &mut meshes,
        &mut materials,
        origin_world,
        NUM_POINTS,
        RING_RADIUS,
        INITIAL_VEL,
        GRAVITY,
        PARTICLE_VIS_RADIUS,
        DEFAULT_MASS,
        DEFAULT_BOUNCINESS,
    );
}

/// Fixed-timestep integration: Verlet with per-second damping, then
/// PBD-style constraints (distance + area), then write positions to `Transform`.
pub fn softbody_step(
    time: Res<Time>, // fixed clock in FixedUpdate
    bounds: Res<WorldBounds>,
    mut q_points: Query<&mut Point>,
    mut q_tf: Query<&mut Transform>,
    mut q_soft: Query<&mut SoftBody>,
    buttons: Res<ButtonInput<MouseButton>>, // for left-press state
    eff: Res<EffectorState>,                // current effector state
) {
    let dt = time.delta_secs();
    let dt2 = dt * dt;
    let half = bounds.half;

    // Convert per-second damping to per-tick factor (frame-rate independent).
    // We scale the Verlet velocity-like term (x_t - x_{t-1}) by this factor.
    let damping_per_tick = DAMPING_PER_SECOND.powf(dt);

    for soft in &mut q_soft {
        // --- 1) Verlet integrate all points; add gravity EACH tick; bounce on window AABB
        for &e in &soft.points {
            if let Ok(mut p) = q_points.get_mut(e) {
                let x_t = p.position;
                let x_tm1 = p.previous_position;

                // Accumulate this tick's forces: keep transient forces in p.acceleration,
                // and ADD constant gravity each tick (otherwise the body won't fall).
                let a = p.acceleration + GRAVITY;

                // Position-Verlet with damping on (x_t - x_{t-1})
                let vel_term = (x_t - x_tm1) * damping_per_tick;
                let mut x_tp1 = x_t + vel_term + a * dt2;

                // Inferred velocity for bounce reflection
                let mut v = x_tp1 - x_t;

                // Window bounds with per-point radius (origin at center) :contentReference[oaicite:2]{index=2}
                let left = -half.x + p.radius;
                let right = half.x - p.radius;
                let bottom = -half.y + p.radius;
                let top = half.y - p.radius;

                if x_tp1.x < left {
                    x_tp1.x = left;
                    v.x = -v.x * p.bounciness;
                }
                if x_tp1.x > right {
                    x_tp1.x = right;
                    v.x = -v.x * p.bounciness;
                }
                if x_tp1.y < bottom {
                    x_tp1.y = bottom;
                    v.y = -v.y * p.bounciness;
                }
                if x_tp1.y > top {
                    x_tp1.y = top;
                    v.y = -v.y * p.bounciness;
                }

                // Advance Verlet state; clear per-tick forces (gravity is re-added next tick)
                p.previous_position = x_tp1 - v;
                p.position = x_tp1;
                p.acceleration = Vec2::ZERO;
            }
        }

        // --- 2) Constraint solve (Gauss–Seidel): distance + area (dilation)
        // Based on Position-Based Dynamics (Jakobsen / Müller et al.). :contentReference[oaicite:3]{index=3}
        for _ in 0..CONSTRAINT_ITERATIONS {
            // 2a) Distance constraints between ring neighbors: accumulate symmetric corrections
            let mut disp_accum: Vec<Vec2> = vec![Vec2::ZERO; soft.num_points];
            let mut disp_weight: Vec<u32> = vec![0; soft.num_points];

            for i in 0..soft.num_points {
                let i_next = (i + 1) % soft.num_points;

                // read positions
                let (pi, pj) = {
                    let p_i = q_points
                        .get_mut(soft.points[i])
                        .ok()
                        .map(|p| p.position)
                        .unwrap_or(Vec2::ZERO);
                    let p_j = q_points
                        .get_mut(soft.points[i_next])
                        .ok()
                        .map(|p| p.position)
                        .unwrap_or(Vec2::ZERO);
                    (p_i, p_j)
                };

                let diff = pj - pi;
                let len = diff.length();
                if len > 0.0 && len > soft.chord_length {
                    let error = (len - soft.chord_length) * 0.5;
                    let offset = diff / len * error;
                    disp_accum[i] += offset;
                    disp_accum[i_next] += -offset;
                    disp_weight[i] += 1;
                    disp_weight[i_next] += 1;
                }
            }

            // 2b) Area (dilation) constraint to keep blob “puffy”
            let corrections = dilation_corrections(&soft, &q_points);
            for (i, c) in corrections.into_iter().enumerate() {
                disp_accum[i] += c;
                disp_weight[i] += 1;
            }

            // 2c) Apply average displacement per point
            for i in 0..soft.num_points {
                if disp_weight[i] == 0 {
                    continue;
                }
                let avg = disp_accum[i] / (disp_weight[i] as f32);
                if let Ok(mut p) = q_points.get_mut(soft.points[i]) {
                    p.position += avg;
                }
            }

            // 2d) Interleave effector collision as a projection pass (PBD contact)
            if buttons.pressed(MouseButton::Left) {
                let ra = eff.prev;
                let rb = eff.curr;
                let r = eff.radius; // no speculative padding (step 2 reverted)
                for i in 0..soft.num_points {
                    if let Ok(mut p) = q_points.get_mut(soft.points[i]) {
                        let mut pos = p.position;
                        collide_point_with_swept_effector(&mut pos, ra, rb, r);
                        p.position = pos;
                    }
                }
            }
        }

        // --- 3) Write back to Transform for rendering
        for &e in &soft.points {
            if let (Ok(p), Ok(mut tf)) = (q_points.get_mut(e), q_tf.get_mut(e)) {
                tf.translation.x = p.position.x;
                tf.translation.y = p.position.y;
            }
        }
    }
}

/// Compute per-vertex normal offsets to correct polygon area towards `desired_area`.
/// Mirrors the Python approach: use a secant across neighbors and its outward normal.
fn dilation_corrections(soft: &SoftBody, q_points: &Query<&mut Point>) -> Vec<Vec2> {
    let n = soft.num_points;
    let mut poly: Vec<Vec2> = Vec::with_capacity(n);
    for &e in &soft.points {
        let pos = q_points
            .get(e)
            .ok()
            .map(|p| p.position)
            .unwrap_or(Vec2::ZERO);
        poly.push(pos);
    }

    let area = polygon_area_signed(&poly);
    let error = soft.desired_area - area;
    let offset = if soft.circumference != 0.0 {
        error / soft.circumference
    } else {
        0.0
    };

    let mut out = vec![Vec2::ZERO; n];
    for i in 0..n {
        let prev = poly[(i + n - 1) % n];
        let next = poly[(i + 1) % n];
        let secant = next - prev;
        // outward normal like Python: (y, -x)
        let normal = if secant.length_squared() == 0.0 {
            Vec2::ZERO
        } else {
            Vec2::new(secant.y, -secant.x).normalize() * offset
        };
        out[i] = normal;
    }
    out
}

/// Signed polygon area via the shoelace-like form used in your Python code.
fn polygon_area_signed(pts: &[Vec2]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        a += (p.x - q.x) * (p.y + q.y) * 0.5;
    }
    a
}

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::point::Point;
use crate::config::*;

/// A single soft body made of `Point` particles connected in a ring.
/// Stores parameters and the spawned point `Entity` IDs.
#[derive(Component)]
pub struct SoftBody {
    pub points: Vec<Entity>,
    pub num_points: usize,
    pub radius: f32,        // nominal (visual) radius of the blob
    pub puffiness: f32,     // scales target area
    pub desired_area: f32,  // pi * r^2 * puffiness
    pub circumference: f32, // 2*pi*r
    pub chord_length: f32,  // arc-length / N (target edge length)
}

impl SoftBody {
    pub fn new(num_points: usize, radius: f32, puffiness: f32) -> Self {
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

/// Resource holding window half-extents (origin at center in Bevy 2D).
#[derive(Resource, Default, Copy, Clone, Debug)]
pub struct WorldBounds {
    pub half: Vec2,
}

/// Keep `WorldBounds` up to date (resizes, DPI changes).
pub fn update_world_bounds(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut bounds: ResMut<WorldBounds>,
) {
    if let Ok(w) = windows.single() {
        bounds.half = 0.5 * w.size();
    }
}

/// Spawn a soft body as a ring (n-gon) around `center`, each point with the same initial velocity.
/// Returns the `Entity` of the SoftBody parent.
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
                // physics
                point,
            ))
            .id();

        soft.points.push(e);
    }

    commands.spawn(soft).id()
}

pub fn softbody_step(
    time: Res<Time>, // fixed clock in FixedUpdate
    bounds: Res<WorldBounds>,
    mut q_points: Query<&mut Point>,
    mut q_tf: Query<&mut Transform>,
    mut q_soft: Query<&mut SoftBody>,
) {
    let dt = time.delta_secs();
    let dt2 = dt * dt;
    let half = bounds.half;

    // convert per-second damping to per-tick
    let damping_per_tick = DAMPING_PER_SECOND.powf(dt);

    for soft in &mut q_soft {
        // --- 1) Verlet integrate all points (re-apply gravity EACH tick), bounce on window AABB
        for &e in &soft.points {
            if let Ok(mut p) = q_points.get_mut(e) {
                let x_t = p.position;
                let x_tm1 = p.previous_position;

                // accumulate forces for this tick: keep any per-step forces in p.acceleration
                // and ADD constant gravity each tick
                let a = p.acceleration + GRAVITY;

                // position-Verlet with damping on the velocity-like term (x_t - x_{t-1})
                let vel_term = (x_t - x_tm1) * damping_per_tick;
                let mut x_tp1 = x_t + vel_term + a * dt2;

                // inferred velocity for bounce reflection
                let mut v = x_tp1 - x_t;

                // window bounds with per-point radius
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

                // advance verlet state; clear ONLY per-tick forces (gravity is re-added next tick)
                p.previous_position = x_tp1 - v;
                p.position = x_tp1;
                p.acceleration = Vec2::ZERO;
            }
        }

        // --- 2) Constraint solve (distance + area) as you already had
        for _ in 0..CONSTRAINT_ITERATIONS {
            let mut disp_accum: Vec<Vec2> = vec![Vec2::ZERO; soft.num_points];
            let mut disp_weight: Vec<u32> = vec![0; soft.num_points];

            // distance constraints (ring neighbors)
            for i in 0..soft.num_points {
                let i_next = (i + 1) % soft.num_points;
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

            // area (dilation) constraint
            let corrections = dilation_corrections(&soft, &q_points);
            for (i, c) in corrections.into_iter().enumerate() {
                disp_accum[i] += c;
                disp_weight[i] += 1;
            }

            // apply accumulated average displacement
            for i in 0..soft.num_points {
                if disp_weight[i] == 0 {
                    continue;
                }
                let avg = disp_accum[i] / (disp_weight[i] as f32);
                if let Ok(mut p) = q_points.get_mut(soft.points[i]) {
                    p.position += avg;
                }
            }
        }

        // --- 3) Write to Transform
        for &e in &soft.points {
            if let (Ok(p), Ok(mut tf)) = (q_points.get_mut(e), q_tf.get_mut(e)) {
                tf.translation.x = p.position.x;
                tf.translation.y = p.position.y;
            }
        }
    }
}

/// Compute per-vertex normals offsets that inflate/deflate to match `desired_area`.
/// This mirrors your Python `calculate_dilation`.
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
        // outward normal of the secant (per your Python: (y, -x))
        let normal = if secant.length_squared() == 0.0 {
            Vec2::ZERO
        } else {
            Vec2::new(secant.y, -secant.x).normalize() * offset
        };
        out[i] = normal;
    }
    out
}

/// Signed polygon area via the **shoelace formula** (CCW positive).
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

/// Convenience system: spawn one soft body + a 2D camera.
/// (You can remove/replace this with your own spawner as needed.)
pub fn spawn_demo_softbody(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // camera
    commands.spawn(Camera2d);

    // body
    let center = CENTER;
    let num = NUM_POINTS;
    let ring_r = RING_RADIUS;
    let mass = DEFAULT_MASS;
    let bounce = DEFAULT_BOUNCINESS;

    spawn_soft_body(
        &mut commands,
        &mut meshes,
        &mut materials,
        center,
        num,
        ring_r,
        INITIAL_VEL,
        GRAVITY,
        PARTICLE_VIS_RADIUS,
        mass,
        bounce,
    );
}

/// Optional: native-only quit via runtime cfg check
pub fn exit_on_esc_or_q_if_native(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if cfg!(not(target_arch = "wasm32")) {
        if keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyQ]) {
            exit.write(AppExit::Success);
        }
    }
}

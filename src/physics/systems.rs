use bevy::prelude::*;
use bevy_polyline::polyline::Polyline as PolylineAsset;
use bevy_polyline::prelude::*;

use crate::config::MOUSE_RADIUS;
use crate::physics::debug::BlobOutline;
use crate::physics::point::Point;
use crate::physics::soft_body::SoftBody;
use bevy::window::PrimaryWindow;

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorld(pub Vec2);

/// Dirty flag for outline updates: set by physics (FixedUpdate), consumed by Update.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct OutlineDirty(pub bool);

/// Counts how many physics substeps have occurred in the current render frame.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SubstepCounter(pub u32);

/// Reset substep counter each render frame (Update schedule runs once per frame).
pub fn reset_substep_counter(mut counter: ResMut<SubstepCounter>) {
    counter.0 = 0;
}

/// Native-only quit: press Esc or Q to exit the app.
/// (No-op on wasm32.)
pub fn exit_on_esc_or_q_if_native(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if cfg!(not(target_arch = "wasm32")) {
        if keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyQ]) {
            exit.write(AppExit::Success);
        }
    }
}

/// Update the cursor's world position each frame (2D camera).
pub fn update_cursor_world(
    windows: Query<&Window, With<PrimaryWindow>>,
    // Only use the 2D camera for screen->world mapping; ignore the 3D polyline camera
    q_cam: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut cursor: ResMut<CursorWorld>,
    mut eff: ResMut<EffectorState>,
) {
    // shift prev to old curr first
    eff.prev = eff.curr;

    let Ok(window) = windows.single() else {
        return;
    };
    if let Some(screen_pos) = window.cursor_position() {
        if let Ok((camera, cam_xform)) = q_cam.single() {
            if let Ok(world_pos) = camera.viewport_to_world_2d(cam_xform, screen_pos) {
                cursor.0 = world_pos;
                eff.curr = world_pos; // keep effector path in sync
            }
        }
    }
}

pub fn effector_swept_collision_system(
    buttons: Res<ButtonInput<MouseButton>>,
    eff: Res<EffectorState>,
    mut points: Query<&mut Point>,
) {
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    let ra = eff.prev;
    let rb = eff.curr;
    let r = eff.radius;

    for mut p in &mut points {
        let mut pos = p.position;
        collide_point_with_swept_effector(&mut pos, ra, rb, r);
        // write back to Point (your solver uses position directly)
        p.position = pos;
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct EffectorState {
    pub radius: f32,
    pub prev: Vec2,
    pub curr: Vec2,
}

impl Default for EffectorState {
    fn default() -> Self {
        Self {
            radius: MOUSE_RADIUS, // tweak as you like (or use MOUSE_RADIUS from config)
            prev: Vec2::ZERO,
            curr: Vec2::ZERO,
        }
    }
}

#[inline]
pub(crate) fn collide_point_with_swept_effector(p: &mut Vec2, seg_a: Vec2, seg_b: Vec2, r: f32) {
    let seg = seg_b - seg_a;
    let seg_len2 = seg.length_squared();
    if seg_len2 <= 1e-12 {
        // no motion this frame: fallback to circle at seg_b
        let d = *p - seg_b;
        let d2 = d.length_squared();
        if d2 < r * r && d2 > 1e-12 {
            *p = seg_b + d.normalize() * r;
        }
        return;
    }
    // closest point q on segment [seg_a, seg_b] to point p
    let t = (*p - seg_a).dot(seg) / seg_len2;
    let t = t.clamp(0.0, 1.0);
    let q = seg_a + t * seg;

    let d = *p - q;
    let d2 = d.length_squared();
    if d2 < r * r && d2 > 1e-12 {
        *p = q + d.normalize() * r; // project out to capsule boundary
    }
}

/// One-pass Chaikin smoothing for a closed polygon ring.
pub fn chaikin_closed_once(input: &[Vec2], out: &mut Vec<Vec2>) {
    out.clear();
    let n = input.len();
    if n < 3 {
        out.extend_from_slice(input);
        return;
    }
    out.reserve(n * 2);
    for i in 0..n {
        let a = input[i];
        let b = input[(i + 1) % n];
        out.push(a.lerp(b, 0.25));
        out.push(a.lerp(b, 0.75));
    }
}

/// Update the GPU polyline to trace the soft body outline.
pub fn update_blob_outline(
    q_soft: Query<&SoftBody>,
    q_points: Query<&Point>,
    mut lines: ResMut<Assets<PolylineAsset>>,
    q_outline: Query<&PolylineHandle, With<BlobOutline>>,
    mut dirty: ResMut<OutlineDirty>,
) {
    if !dirty.0 {
        return;
    }
    // Reset dirty so we only update once per render frame
    dirty.0 = false;
    let Some(soft) = q_soft.iter().next() else {
        // No softbody yet
        // info!("update_blob_outline: no SoftBody found");
        return;
    };

    // Gather current ring positions in order.
    let mut ring: Vec<Vec2> = Vec::with_capacity(soft.num_points);
    for &e in &soft.points {
        if let Ok(p) = q_points.get(e) {
            ring.push(p.position);
        }
    }

    // Smooth once with Chaikin.
    let mut smooth: Vec<Vec2> = Vec::with_capacity(ring.len() * 2);
    chaikin_closed_once(&ring, &mut smooth);
    let src = if smooth.len() >= 3 { &smooth } else { &ring };

    if let Some(handle) = q_outline.iter().next() {
        if let Some(poly) = lines.get_mut(&handle.0) {
            poly.vertices.clear();
            // reserve one extra to close the loop
            poly.vertices.reserve(src.len() + 1);
            for p in src.iter() {
                poly.vertices.push(p.extend(0.0));
            }
            // Close the ring visually by repeating the first vertex
            if let Some(first) = src.first() {
                poly.vertices.push(first.extend(0.0));
            }
            // info!("update_blob_outline: vertices={} (ring={}, smooth={})", poly.vertices.len(), ring.len(), smooth.len());
        }
    } else {
        // info!("update_blob_outline: no BlobOutline handle found");
    }
}

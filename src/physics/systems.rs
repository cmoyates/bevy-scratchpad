use bevy::prelude::*;

use crate::config::MOUSE_RADIUS;
use crate::physics::point::Point;
use bevy::window::PrimaryWindow;

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorld(pub Vec2);

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
    q_cam: Query<(&Camera, &GlobalTransform)>,
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
fn collide_point_with_swept_effector(p: &mut Vec2, seg_a: Vec2, seg_b: Vec2, r: f32) {
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

use bevy::prelude::*;

use crate::config::MOUSE_RADIUS;
use crate::physics::point::Point;
use bevy::prelude::*;
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
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if let Some(screen_pos) = window.cursor_position() {
        if let Ok((camera, cam_xform)) = q_cam.single() {
            if let Ok(ray) = camera.viewport_to_world(cam_xform, screen_pos) {
                let world_pos = ray.origin.truncate();
                cursor.0 = world_pos;
            }
        }
    }
}

/// While LMB is held, push points out of a circle around the cursor.
/// Uses your existing `Point::collide_with_mouse`.
pub fn mouse_push_points(
    buttons: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorWorld>,
    mut points: Query<&mut Point>,
) {
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    for mut p in &mut points {
        // pressed = true; uses your helper’s logic
        p.collide_with_mouse(cursor.0, true, MOUSE_RADIUS);
    }
}

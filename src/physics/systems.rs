use bevy::prelude::*;

use crate::config::MOUSE_RADIUS;
use bevy::window::PrimaryWindow;

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct CursorWorld(pub Vec2);

/// Swept-sphere mouse effector for physics collision.
#[derive(Resource, Debug, Clone, Copy)]
pub struct MouseEffector {
    pub radius: f32,
    pub prev: Vec2,
    pub curr: Vec2,
}

impl Default for MouseEffector {
    fn default() -> Self {
        Self {
            radius: MOUSE_RADIUS,
            prev: Vec2::ZERO,
            curr: Vec2::ZERO,
        }
    }
}

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
pub fn exit_on_esc_or_q_if_native(
    keys: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
) {
    if cfg!(not(target_arch = "wasm32"))
        && keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyQ]) {
            exit.write(AppExit::Success);
        }
}

/// Update the cursor's world position each frame (2D camera).
pub fn update_cursor_world(
    windows: Query<&Window, With<PrimaryWindow>>,
    q_cam: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut cursor: ResMut<CursorWorld>,
    mut effector: ResMut<MouseEffector>,
) {
    effector.prev = effector.curr;

    let Ok(window) = windows.single() else {
        return;
    };
    if let Some(screen_pos) = window.cursor_position()
        && let Ok((camera, cam_xform)) = q_cam.single()
            && let Ok(world_pos) = camera.viewport_to_world_2d(cam_xform, screen_pos) {
                cursor.0 = world_pos;
                effector.curr = world_pos;
            }
}


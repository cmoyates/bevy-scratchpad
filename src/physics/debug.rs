use crate::config::MOUSE_RADIUS;
use crate::physics::systems::CursorWorld; // where you defined CursorWorld
use bevy::prelude::*;

pub fn draw_effector_gizmo(mut gizmos: Gizmos, cursor: Res<CursorWorld>) {
    // Draw each frame (immediate-mode gizmo)
    gizmos.circle_2d(cursor.0, MOUSE_RADIUS, Color::srgb(1.0, 0.0, 0.0));
}

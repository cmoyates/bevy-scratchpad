use crate::config::MOUSE_RADIUS;
use crate::physics::systems::CursorWorld;
use bevy::prelude::*;

use crate::physics::point::Point;
use crate::physics::soft_body::SoftBody;
use crate::physics::systems::{OutlineDirty, chaikin_closed_once};

/// Cached smoothed outline vertices, rebuilt when dirty.
#[derive(Resource, Default)]
pub struct OutlineCache(pub Vec<Vec2>);

pub fn draw_effector_gizmo(
    mut gizmos: Gizmos,
    cursor: Res<CursorWorld>,
    buttons: Res<ButtonInput<MouseButton>>,
) {
    let alpha = if buttons.pressed(MouseButton::Left) {
        1.0
    } else {
        0.1
    };
    gizmos.circle_2d(cursor.0, MOUSE_RADIUS, Color::srgba(1.0, 0.0, 0.0, alpha));
}

/// Rebuild the cached outline when physics marks it dirty.
pub fn rebuild_outline_cache(
    q_soft: Query<&SoftBody>,
    q_points: Query<&Point>,
    mut dirty: ResMut<OutlineDirty>,
    mut cache: ResMut<OutlineCache>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;

    let Some(soft) = q_soft.iter().next() else {
        return;
    };

    let mut ring: Vec<Vec2> = Vec::with_capacity(soft.num_points);
    for &e in &soft.points {
        if let Ok(p) = q_points.get(e) {
            ring.push(p.position);
        }
    }

    let mut smooth: Vec<Vec2> = Vec::with_capacity(ring.len() * 2);
    chaikin_closed_once(&ring, &mut smooth);
    let src = if smooth.len() >= 3 { &smooth } else { &ring };

    cache.0.clear();
    cache.0.extend_from_slice(src);
    // Close the loop
    if let Some(&first) = src.first() {
        cache.0.push(first);
    }
}

/// Draw the cached outline using gizmos (runs every frame, cheap if cache unchanged).
pub fn draw_blob_outline(mut gizmos: Gizmos, cache: Res<OutlineCache>) {
    if cache.0.len() < 2 {
        return;
    }
    gizmos.linestrip_2d(cache.0.iter().copied(), Color::WHITE);
}

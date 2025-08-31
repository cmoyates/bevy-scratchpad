use crate::config::MOUSE_RADIUS;
use crate::physics::systems::CursorWorld; // where you defined CursorWorld
use bevy::prelude::Projection;
use bevy::prelude::*;
use bevy::render::camera::{ClearColorConfig, OrthographicProjection, ScalingMode};
use bevy_polyline::polyline::Polyline as PolylineAsset;
use bevy_polyline::prelude::*;

/// Marker for the single blob outline polyline entity.
#[derive(Component)]
pub struct BlobOutline;

pub fn draw_effector_gizmo(
    mut gizmos: Gizmos,
    cursor: Res<CursorWorld>,
    buttons: Res<ButtonInput<MouseButton>>,
) {
    // Alpha 0.25 when not pressed, 1.0 when left mouse is pressed
    let alpha = if buttons.pressed(MouseButton::Left) {
        1.0
    } else {
        0.1
    };
    gizmos.circle_2d(cursor.0, MOUSE_RADIUS, Color::srgba(1.0, 0.0, 0.0, alpha));
}

/// Spawn a persistent empty polyline and material; we'll update the vertices each physics tick.
pub fn spawn_blob_outline(
    mut commands: Commands,
    mut lines: ResMut<Assets<PolylineAsset>>,
    mut mats: ResMut<Assets<PolylineMaterial>>,
) {
    let line_handle = lines.add(PolylineAsset {
        vertices: Vec::new(),
    });
    let mat_handle = mats.add(PolylineMaterial {
        width: 3.0,
        color: LinearRgba::WHITE,
        perspective: false,
        depth_bias: -0.001,
    });

    commands.spawn((
        BlobOutline,
        PolylineBundle {
            polyline: PolylineHandle(line_handle),
            material: PolylineMaterialHandle(mat_handle),
            transform: Transform::from_xyz(0.0, 0.0, 1.0),
            ..default()
        },
    ));
}

// (removed temporary gizmo outline)

// removed temporary test polyline

/// Spawn a 3D camera so bevy_polyline's 3D render graph is active and overlays the 2D scene.
pub fn spawn_polyline_camera_3d(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1, // render after the 2D camera
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::WindowSize,
            near: -1000.0,
            far: 1000.0,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, 0.0, 1000.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

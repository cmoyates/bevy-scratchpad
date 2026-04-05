use tracing::info_span;

use crate::config::MOUSE_RADIUS;
use crate::physics::soft_body::SoftBody;
use crate::physics::solver_core;
use crate::physics::systems::{CursorWorld, OutlineDirty};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Marker component for the outline mesh entity.
#[derive(Component)]
pub struct OutlineMesh;

/// Cached smoothed outline vertices, rebuilt when dirty.
#[derive(Resource, Default)]
pub struct OutlineCache {
    pub vertices: Vec<Vec2>,
    smooth_buf: Vec<Vec2>,
}

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

/// Spawn the outline mesh entity on startup.
pub fn spawn_outline_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mesh = Mesh::new(PrimitiveTopology::LineStrip, default());
    commands.spawn((
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(materials.add(Color::WHITE)),
        Transform::default(),
        OutlineMesh,
    ));
}

/// Rebuild the cached outline vertices when physics marks dirty,
/// then update the mesh vertex buffer.
pub fn update_outline_mesh(
    q_soft: Query<&SoftBody>,
    mut dirty: ResMut<OutlineDirty>,
    cache: ResMut<OutlineCache>,
    q_outline: Query<&Mesh2d, With<OutlineMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let _span = info_span!("update_outline_mesh").entered();
    if !dirty.0 {
        return;
    }
    dirty.0 = false;

    let Some(soft) = q_soft.iter().next() else {
        return;
    };

    let cache = cache.into_inner();
    solver_core::build_outline(&soft.state, &mut cache.smooth_buf, &mut cache.vertices);

    // Update the mesh vertex data
    let Ok(mesh_handle) = q_outline.single() else {
        return;
    };
    let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
        return;
    };

    let n = cache.vertices.len();
    let positions: Vec<[f32; 3]> = cache.vertices.iter().map(|v| [v.x, v.y, 0.0]).collect();

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);

    // Line strip indices: 0, 1, 2, ..., n-1
    let indices: Vec<u32> = (0..n as u32).collect();
    mesh.insert_indices(Indices::U32(indices));
}

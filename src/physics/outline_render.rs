//! Storage-buffer-driven outline rendering via vertex pulling.
//!
//! Uploads outline positions to a `ShaderStorageBuffer` and renders
//! via a custom `Material2d` + WGSL vertex shader that pulls positions
//! from the SSBO using `@builtin(vertex_index)`.

use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::storage::ShaderStorageBuffer;
use bevy::shader::ShaderRef;
use bevy::sprite_render::Material2d;
use tracing::info_span;

use crate::physics::geometry::chaikin_closed_once;
use crate::physics::soft_body::SoftBody;
use crate::physics::systems::OutlineDirty;

const SHADER_PATH: &str = "shaders/outline_ssbo.wgsl";

/// Custom material that binds outline positions via SSBO.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct OutlineMaterial {
    #[storage(0, read_only)]
    pub positions: Handle<ShaderStorageBuffer>,
}

impl Material2d for OutlineMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER_PATH.into()
    }
}

/// Marker component for the SSBO-driven outline mesh entity.
#[derive(Component)]
pub struct SsboOutline;

/// Resource holding scratch buffers for outline building.
#[derive(Resource, Default)]
pub struct OutlineScratch {
    ring: Vec<Vec2>,
    smooth: Vec<Vec2>,
}

/// Spawn the SSBO outline mesh entity.
pub fn spawn_ssbo_outline(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut materials: ResMut<Assets<OutlineMaterial>>,
) {
    // Initial empty position buffer
    let initial_data: Vec<[f32; 2]> = vec![[0.0, 0.0]; 4];
    let ssbo = buffers.add(ShaderStorageBuffer::from(initial_data));

    let material = materials.add(OutlineMaterial { positions: ssbo });

    // Dummy mesh with LineStrip topology — vertex positions are ignored,
    // the shader pulls from the SSBO instead.
    let mut mesh = Mesh::new(PrimitiveTopology::LineStrip, default());
    let dummy_positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; 4];
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, dummy_positions);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 3]));

    commands.spawn((
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(material),
        Transform::default(),
        SsboOutline,
    ));
}

/// Upload updated outline positions to the SSBO when dirty.
pub fn update_ssbo_outline(
    q_soft: Query<&SoftBody>,
    mut dirty: ResMut<OutlineDirty>,
    mut scratch: ResMut<OutlineScratch>,
    q_outline: Query<(&Mesh2d, &MeshMaterial2d<OutlineMaterial>), With<SsboOutline>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    materials: Res<Assets<OutlineMaterial>>,
) {
    let _span = info_span!("update_ssbo_outline").entered();
    if !dirty.0 {
        return;
    }
    dirty.0 = false;

    let Some(soft) = q_soft.iter().next() else {
        return;
    };

    // Build smoothed ring from SoA state
    let n = soft.state.len();
    scratch.ring.clear();
    scratch.ring.reserve(n);
    for i in 0..n {
        scratch
            .ring
            .push(Vec2::new(soft.state.x[i], soft.state.y[i]));
    }

    // Split borrow: chaikin needs immutable ring + mutable smooth
    let scratch = scratch.into_inner();
    chaikin_closed_once(&scratch.ring, &mut scratch.smooth);
    let src = if scratch.smooth.len() >= 3 {
        &scratch.smooth
    } else {
        &scratch.ring
    };

    // Build closed outline (add first point at end)
    let vert_count = src.len() + 1;
    let position_data: Vec<[f32; 2]> = src.iter().chain(src.first()).map(|v| [v.x, v.y]).collect();

    // Update the SSBO with new position data
    let Ok((mesh_handle, mat_handle)) = q_outline.single() else {
        return;
    };

    if let Some(mat) = materials.get(&mat_handle.0)
        && let Some(buffer) = buffers.get_mut(&mat.positions)
    {
        buffer.set_data(position_data);
    }

    // Resize the dummy mesh to match vertex count
    if let Some(mesh) = meshes.get_mut(&mesh_handle.0) {
        let dummy: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0]; vert_count];
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, dummy);
        let indices: Vec<u32> = (0..vert_count as u32).collect();
        mesh.insert_indices(Indices::U32(indices));
    }
}

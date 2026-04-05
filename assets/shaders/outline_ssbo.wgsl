// Vertex-pulling outline shader: reads positions from a storage buffer
// instead of the vertex input assembler.

#import bevy_sprite::{
    mesh2d_functions as mesh_functions,
    mesh2d_vertex_output::VertexOutput,
}

// Material bind group: SSBO of 2D positions
@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<storage, read> positions: array<vec2<f32>>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
    @location(0) fallback_position: vec3<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    // Pull 2D position from storage buffer, fall back to mesh position
    let n = arrayLength(&positions);
    var local_pos: vec4<f32>;
    if vertex.vertex_index < n {
        let pos2d = positions[vertex.vertex_index];
        local_pos = vec4<f32>(pos2d.x, pos2d.y, 0.0, 1.0);
    } else {
        local_pos = vec4<f32>(vertex.fallback_position, 1.0);
    }

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh2d_position_local_to_world(
        world_from_local,
        local_pos,
    );
    out.position = mesh_functions::mesh2d_position_world_to_clip(out.world_position);

    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}

---
title: Custom Material2d bind group stale after ShaderStorageBuffer replacement
date: 2026-04-05
category: integration-issues
tags: [bevy, wgsl, custom-material, shader-storage-buffer, bind-group, material2d]
severity: high
component: physics/outline_render
symptoms:
  - SSBO vertex-pulling shader compiles but renders static/frozen data
  - Outline visible but does not respond to simulation updates
  - No errors, no crashes, just stale visuals
root_cause: Material bind group caches GPU buffer reference; replacing ShaderStorageBuffer asset creates new GPU buffer but material still points to old one
resolution: Touch material via materials.get_mut() after SSBO buffer update to force bind group recreation
---

## Problem

In Bevy 0.18, a custom `Material2d` using a `ShaderStorageBuffer` (SSBO) via `AsBindGroup` has its bind group baked at material creation time. When the underlying buffer asset is replaced via `Assets::insert()`, the GPU allocates a new buffer, but the material's cached bind group still points to the old GPU buffer handle. The shader reads stale (static) position data.

## Investigation Steps

1. **Confirmed the pipeline itself worked** -- modified the shader to ignore the SSBO and render using mesh positions directly. Outline appeared green, proving `Material2d` + `LineStrip` topology was functional.
2. **Re-enabled SSBO reads** -- outline appeared but was static (didn't respond to simulation updates).
3. **Switched from `buffer.set_data()` to `buffers.insert()`** -- expected the `AssetEvent::Modified` to propagate; outline remained static.
4. **Identified root cause** -- the material's bind group was never being invalidated. Bevy only recreates a material's bind group when the material asset itself is marked changed.
5. **Fix** -- after replacing the buffer, called `materials.get_mut()` on the material handle. This marks the material as changed, causing Bevy to rebuild its bind group pointing to the new GPU buffer.

## Root Cause

Bevy's `Material2d` caches bind groups per material asset. Replacing or mutating a `ShaderStorageBuffer` asset does not automatically invalidate the bind group of any material that references it. Only a change event on the material asset itself triggers bind group recreation. Since the SSBO handle stored in `OutlineMaterial.positions` is unchanged (same `Handle<ShaderStorageBuffer>`, different underlying GPU allocation), Bevy has no signal to rebuild the bind group.

## Solution

After inserting the new buffer, touch the material via `get_mut()` to emit a change event, forcing bind group recreation:

```rust
// Replace the SSBO with fresh position data
if let Some(mat) = materials.get(&mat_handle.0) {
    let ssbo_handle = mat.positions.clone();
    let _ = buffers.insert(&ssbo_handle, ShaderStorageBuffer::from(position_data));
} else {
    return;
}

// Touch the material so its bind group is recreated pointing to the new GPU buffer
let _ = materials.get_mut(&mat_handle.0);
```

The shader uses `@builtin(vertex_index)` to pull positions directly from the SSBO:

```wgsl
@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<storage, read> positions: array<vec2<f32>>;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let n = arrayLength(&positions);
    var local_pos: vec4<f32>;
    if vertex.vertex_index < n {
        let pos2d = positions[vertex.vertex_index];
        local_pos = vec4<f32>(pos2d.x, pos2d.y, 0.0, 1.0);
    } else {
        local_pos = vec4<f32>(vertex.fallback_position, 1.0);
    }
    // ...
}
```

The dummy mesh's vertex count must also be kept in sync with the SSBO so the GPU issues enough draw calls for all outline vertices.

## Prevention & Best Practices

**Mental model:** Materials are "compiled configurations" that cache GPU bindings at creation time. Modifying a *referenced* asset doesn't recompile the material. Only modifying the material itself triggers recompilation.

1. **If you replace an SSBO asset at runtime**, always follow with `materials.get_mut(&material_handle)` to mark the material modified.
2. **For high-frequency dynamic data**, prefer GPU writes (compute shaders, encoder writes) over asset replacement. This avoids the bind group lifecycle problem entirely.
3. **In code reviews**, flag any "asset replacement + shader/material usage" patterns. Ask: "Does this trigger bind group recreation?"

## Diagnostic Checklist

When a shader compiles but renders wrong/stale data:

- [ ] Confirm shader compiles (check `RUST_LOG=wgpu=debug`)
- [ ] Search for `.insert()` calls on the relevant `Assets<T>`
- [ ] Verify each replacement is followed by material mutation notification
- [ ] Trace all `Handle<_>` fields in your Material2d -- could they be replaced at runtime?
- [ ] Add frame-by-frame logging: asset replacement -> material mutation -> bind group recreation

## Related References

- **GitHub Issue #11** (Closed) -- "Debug SSBO vertex-pulling outline shader"
- **Phase A Handoff** (`docs/handoff/2026-04-06-phase-a-results.md`) -- documents full SSBO solution
- **Bevy Soft-Body Optimization Research** (`docs/research/Bevy Soft-Body Optimization Research.md`) -- describes vertex pulling approach
- Bevy source: `bevy_sprite_render-0.18.1/src/mesh2d/material.rs` -- `MATERIAL_2D_BIND_GROUP_INDEX = 2`, bind group recreation in `specialize()`

## Outline-only Wire Look: Plan for Agent

### 1. Add `bevy_polyline` crate and plugin

- Add to `Cargo.toml`:

  ```toml
  [dependencies]
  bevy = "0.16"
  bevy_polyline = "0.12"
  ```

- In `main.rs`, add the plugin:

  ```rust
  App::new()
    .add_plugins((DefaultPlugins, PolylinePlugin))
    .add_plugins(PhysicsPlugin)
    .run();
  ```

  (Registers `Polyline` render brush for instanced GPU line drawing.)

### 2. Implement one-pass Chaikin smoothing

- In `src/physics/systems.rs`:

  ```rust
  pub fn chaikin_closed_once(input: &[Vec2], out: &mut Vec<Vec2>) {
      out.clear();
      let n = input.len();
      if n < 3 {
          out.extend_from_slice(input);
          return;
      }
      out.reserve(n * 2);
      for i in 0..n {
          let a = input[i];
          let b = input[(i + 1) % n];
          out.push(a.lerp(b, 0.25));
          out.push(a.lerp(b, 0.75));
      }
  }
  ```

  - Chaikin’s method _never overshoots_ and stays within the polygon, ideal for dynamic blobs.

### 3. Spawn a persistent Polyline entity

- In `src/physics/debug.rs` (or `mod.rs`):

  ```rust
  #[derive(Component)] struct BlobOutline;

  pub fn spawn_blob_outline(
      mut commands: Commands,
      mut lines: ResMut<Assets<Polyline>>,
      mut mats: ResMut<Assets<PolylineMaterial>>,
  ) {
      let line_handle = lines.add(Polyline { vertices: Vec::new() });
      let mat_handle = mats.add(PolylineMaterial {
          width: 3.0,
          color: Color::WHITE,
          perspective: false,
          depth_bias: -0.001,
      });
      commands.spawn((
          BlobOutline,
          PolylineBundle {
              polyline: PolylineHandle(line_handle),
              material: PolylineMaterialHandle(mat_handle),
              transform: Transform::default(),
              ..default()
          },
      ));
  }
  ```

  - Creates a GPU‑instanced line entity to update each frame.

### 4. Update the Polyline every physics tick

- In `src/physics/systems.rs`:

  ```rust
  pub fn update_blob_outline(
      points: Query<&Point>, // in your blob perimeter order
      mut lines: ResMut<Assets<Polyline>>,
      outline_q: Query<&PolylineHandle, With<BlobOutline>>,
  ) {
      let Ok(handle) = outline_q.get_single() else { return; };
      let ring: Vec<Vec2> = gather_ring_in_order(&points);
      let mut smooth: Vec<Vec2> = Vec::with_capacity(ring.len() * 2);
      chaikin_closed_once(&ring, &mut smooth);
      let pts = if smooth.len() >= 3 { &smooth } else { &ring };
      if let Some(poly) = lines.get_mut(&handle.0) {
          poly.vertices.clear();
          poly.vertices.reserve(pts.len());
          for p in pts.iter() {
              poly.vertices.push(p.extend(0.0));
          }
      }
  }
  ```

  - Called in `FixedUpdate` lagging the physics loop.
  - **Performance tips**: only one pass, pre‑allocate vectors, fixed‑step updates to keep workload stable.

### 5. System ordering in your plugin

- In `src/physics/mod.rs`, register:

  ```rust
  app
    .add_systems(Startup, (spawn_demo, spawn_blob_outline))
    .add_systems(FixedUpdate, (
      effector_swept_collision_system,
      softbody_step,
      update_blob_outline,
    ));
  ```

  - Ensures outline reflects the current positions after physics.

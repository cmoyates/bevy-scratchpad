use bevy::prelude::*;

/// A single Verlet-integrated particle ("point").
/// Store this as a Component on the rendered entity (which also has a Transform).
#[derive(Component, Clone, Copy, Debug)]
pub struct Point {
    pub index: usize,

    /// Current position x_t (kept in sync with Transform by systems).
    pub position: Vec2,
    /// Previous position x_{t-1} (encodes velocity implicitly).
    pub previous_position: Vec2,
    /// Accumulated acceleration a_t (e.g., gravity or forces / mass).
    pub acceleration: Vec2,

    pub mass: f32,
    /// Collision/interaction radius (world units).
    pub radius: f32,
    /// Restitution used when reflecting on bounds (0..=1).
    pub bounciness: f32,
}

impl Default for Point {
    fn default() -> Self {
        Self {
            index: 0,
            position: Vec2::ZERO,
            previous_position: Vec2::ZERO,
            acceleration: Vec2::ZERO,
            mass: 1.0,
            radius: 5.0,
            bounciness: 0.5,
        }
    }
}

impl Point {
    /// Create a new point at `pos`. `previous_position` starts the same
    /// (zero initial velocity). Use `with_initial_velocity` to set v0.
    pub fn new(pos: Vec2, index: usize) -> Self {
        Self {
            position: pos,
            previous_position: pos,
            index,
            ..Default::default()
        }
    }

    /// Create with initial velocity `v0`, encoded via previous_position:
    /// x_{t-1} = x_t - v0 * dt
    pub fn with_initial_velocity(pos: Vec2, v0: Vec2, dt: f32, index: usize) -> Self {
        let mut p = Self::new(pos, index);
        p.previous_position = pos - v0 * dt;
        p
    }

    // --------------------- Setters (Python-ish) ---------------------

    /// Set position using a Vec2 and keep Verlet state consistent
    /// (matches Python: set both current and previous).
    pub fn set_position(&mut self, new_pos: Vec2) {
        self.position = new_pos;
        self.previous_position = new_pos;
    }

    /// Set position via (x, y) and keep Verlet state consistent.
    pub fn set_position_xy(&mut self, x: f32, y: f32) {
        self.set_position(Vec2::new(x, y));
    }

    /// Set previous position only (x_{t-1}).
    pub fn set_previous_position(&mut self, new_prev: Vec2) {
        self.previous_position = new_prev;
    }

    pub fn set_previous_position_xy(&mut self, x: f32, y: f32) {
        self.set_previous_position(Vec2::new(x, y));
    }

    /// Set current position only (does not touch previous_position).
    pub fn set_current_position(&mut self, pos: Vec2) {
        self.position = pos;
    }

    pub fn set_current_position_xy(&mut self, x: f32, y: f32) {
        self.set_current_position(Vec2::new(x, y));
    }

    // --------------------- Forces / external inputs ---------------------

    /// Apply a world-space force: a += F / m
    pub fn apply_force(&mut self, force: Vec2) {
        self.acceleration += force / self.mass;
    }

    /// Translate both current and previous positions by `delta`
    /// (keeps the same implicit velocity).
    pub fn move_by(&mut self, delta: Vec2) {
        self.position += delta;
        self.previous_position += delta;
    }

    // --------------------- Integration & collisions ---------------------

    /// Perform one **position-Verlet** step:
    /// x_{t+1} = 2 x_t - x_{t-1} + a * dt^2
    /// Damping multiplies the (x_t - x_{t-1}) "velocity" term.
    ///
    /// Returns the inferred velocity used this step (x_{t+1} - x_t),
    /// in case the caller wants it (e.g., for debug or effects).
    pub fn verlet_step(&mut self, dt: f32, damping: f32) -> Vec2 {
        let x_t = self.position;
        let x_tm1 = self.previous_position;

        let vel_term = (x_t - x_tm1) * damping;
        let x_tp1 = x_t + vel_term + self.acceleration * (dt * dt);

        // inferred velocity for this step
        let v = x_tp1 - x_t;

        // advance state
        self.previous_position = x_tp1 - v;
        self.position = x_tp1;

        // reset per-step acceleration (like Python)
        self.acceleration = Vec2::ZERO;

        v
    }

    /// Reflect against axis-aligned bounds with per-point radius and restitution.
    ///
    /// `half_extents` should be (window_width/2, window_height/2) in world units.
    /// This matches Bevy's default 2D camera where origin is at the window center. :contentReference[oaicite:4]{index=4}
    pub fn bounce_in_bounds(&mut self, half_extents: Vec2) {
        let mut v = self.position - self.previous_position; // current step's velocity-like term

        let left = -half_extents.x + self.radius;
        let right = half_extents.x - self.radius;
        let bottom = -half_extents.y + self.radius;
        let top = half_extents.y - self.radius;

        // Clamp & reflect X
        if self.position.x < left {
            self.position.x = left;
            v.x = -v.x * self.bounciness;
        } else if self.position.x > right {
            self.position.x = right;
            v.x = -v.x * self.bounciness;
        }

        // Clamp & reflect Y
        if self.position.y < bottom {
            self.position.y = bottom;
            v.y = -v.y * self.bounciness;
        } else if self.position.y > top {
            self.position.y = top;
            v.y = -v.y * self.bounciness;
        }

        // Rebuild previous so the next verlet step uses the reflected velocity
        self.previous_position = self.position - v;
    }

    /// Keep inside [min, max] bounds without reflection (like your Python keep_in_bounds).
    /// (This version assumes min and max are absolute corners in world coords.)
    pub fn clamp_to_bounds(&mut self, min: Vec2, max: Vec2) {
        self.position = self.position.clamp(min, max);
        self.previous_position = self.previous_position.clamp(min, max);
    }

    /// Simple mouse "push-away" helper similar to Python's `collide_with_mouse`.
    /// Returns true if it modified the position.
    pub fn collide_with_mouse(
        &mut self,
        mouse_pos: Vec2,
        mouse_pressed: bool,
        push_dist: f32,
    ) -> bool {
        if !mouse_pressed {
            return false;
        }
        let diff = self.position - mouse_pos;
        let dist2 = diff.length_squared();
        if dist2 < push_dist * push_dist && dist2 > 0.0 {
            let dir = diff.normalize_or_zero();
            self.position = mouse_pos + dir * push_dist;
            // keep verlet state consistent for this snap:
            self.previous_position = self.position;
            return true;
        }
        false
    }
}

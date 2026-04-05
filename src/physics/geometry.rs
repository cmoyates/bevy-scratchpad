use bevy::math::Vec2;

/// Signed polygon area via shoelace formula.
pub fn polygon_area_signed(pts: &[Vec2]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        a += (p.x - q.x) * (p.y + q.y) * 0.5;
    }
    a
}

/// Per-vertex normal offsets to correct polygon area towards `desired_area`.
/// Uses secant across neighbors and its outward normal.
pub fn dilation_corrections(
    polygon: &[Vec2],
    desired_area: f32,
    circumference: f32,
    out: &mut Vec<Vec2>,
) {
    let n = polygon.len();
    out.clear();
    out.resize(n, Vec2::ZERO);

    let area = polygon_area_signed(polygon);
    let error = desired_area - area;
    let offset = if circumference != 0.0 {
        error / circumference
    } else {
        0.0
    };

    for i in 0..n {
        let prev = polygon[(i + n - 1) % n];
        let next = polygon[(i + 1) % n];
        let secant = next - prev;
        // outward normal: (y, -x)
        let normal = if secant.length_squared() == 0.0 {
            Vec2::ZERO
        } else {
            Vec2::new(secant.y, -secant.x).normalize() * offset
        };
        out[i] = normal;
    }
}

/// Push `p` outside the swept sphere (capsule) defined by segment [seg_a, seg_b] with radius `r`.
#[inline]
pub fn collide_point_with_swept_effector(p: &mut Vec2, seg_a: Vec2, seg_b: Vec2, r: f32) {
    let seg = seg_b - seg_a;
    let seg_len2 = seg.length_squared();
    if seg_len2 <= 1e-12 {
        let d = *p - seg_b;
        let d2 = d.length_squared();
        if d2 < r * r && d2 > 1e-12 {
            *p = seg_b + d.normalize() * r;
        }
        return;
    }
    let t = (*p - seg_a).dot(seg) / seg_len2;
    let t = t.clamp(0.0, 1.0);
    let q = seg_a + t * seg;

    let d = *p - q;
    let d2 = d.length_squared();
    if d2 < r * r && d2 > 1e-12 {
        *p = q + d.normalize() * r;
    }
}

/// One-pass Chaikin smoothing for a closed polygon ring.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const EPSILON: f32 = 1e-4;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < EPSILON
    }

    // --- polygon_area_signed ---

    #[test]
    fn area_unit_square_ccw() {
        // CCW square: positive area
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let area = polygon_area_signed(&pts);
        assert!(approx_eq(area.abs(), 1.0), "area={area}");
    }

    #[test]
    fn area_degenerate() {
        assert_eq!(polygon_area_signed(&[]), 0.0);
        assert_eq!(polygon_area_signed(&[Vec2::ZERO]), 0.0);
        assert_eq!(polygon_area_signed(&[Vec2::ZERO, Vec2::X]), 0.0);
    }

    #[test]
    fn area_triangle() {
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(4.0, 0.0),
            Vec2::new(0.0, 3.0),
        ];
        assert!(approx_eq(polygon_area_signed(&pts).abs(), 6.0));
    }

    // --- dilation_corrections ---

    #[test]
    fn dilation_no_correction_when_area_matches() {
        let square = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let area = polygon_area_signed(&square);
        let mut out = Vec::new();
        dilation_corrections(&square, area, 4.0, &mut out);
        for c in &out {
            assert!(c.length() < EPSILON, "expected zero correction, got {c}");
        }
    }

    #[test]
    fn dilation_expands_when_area_too_small() {
        let square = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let mut out = Vec::new();
        // desired area larger than actual
        dilation_corrections(&square, 2.0, 4.0, &mut out);
        // corrections should be nonzero and point outward
        let total_magnitude: f32 = out.iter().map(|c| c.length()).sum();
        assert!(total_magnitude > EPSILON);
    }

    // --- collide_point_with_swept_effector ---

    #[test]
    fn collision_pushes_point_outside_radius() {
        let mut p = Vec2::new(0.5, 0.0);
        let seg_a = Vec2::ZERO;
        let seg_b = Vec2::ZERO;
        collide_point_with_swept_effector(&mut p, seg_a, seg_b, 1.0);
        let dist = p.distance(seg_b);
        assert!(approx_eq(dist, 1.0), "dist={dist}");
    }

    #[test]
    fn collision_no_effect_outside_radius() {
        let mut p = Vec2::new(5.0, 0.0);
        let original = p;
        collide_point_with_swept_effector(&mut p, Vec2::ZERO, Vec2::ZERO, 1.0);
        assert_eq!(p, original);
    }

    #[test]
    fn collision_swept_segment() {
        // Point near the midpoint of a segment
        let mut p = Vec2::new(0.5, 0.3);
        let seg_a = Vec2::new(0.0, 0.0);
        let seg_b = Vec2::new(1.0, 0.0);
        collide_point_with_swept_effector(&mut p, seg_a, seg_b, 1.0);
        // closest point on segment is (0.5, 0.0), so pushed to distance 1.0
        let closest = Vec2::new(0.5, 0.0);
        let dist = p.distance(closest);
        assert!(approx_eq(dist, 1.0), "dist={dist}");
    }

    // --- chaikin_closed_once ---

    #[test]
    fn chaikin_triangle_produces_hexagon() {
        let tri = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ];
        let mut out = Vec::new();
        chaikin_closed_once(&tri, &mut out);
        assert_eq!(out.len(), 6); // 3 edges * 2 points
    }

    #[test]
    fn chaikin_degenerate_passthrough() {
        let two = vec![Vec2::ZERO, Vec2::X];
        let mut out = Vec::new();
        chaikin_closed_once(&two, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Vec2::ZERO);
        assert_eq!(out[1], Vec2::X);
    }

    #[test]
    fn chaikin_preserves_closure() {
        let n = 8;
        let ring: Vec<Vec2> = (0..n)
            .map(|i| {
                let theta = (i as f32) * TAU / (n as f32);
                Vec2::new(theta.cos(), theta.sin())
            })
            .collect();
        let mut out = Vec::new();
        chaikin_closed_once(&ring, &mut out);
        // output should wrap: last point's neighbor is first point's neighbor
        assert_eq!(out.len(), n * 2);
    }
}

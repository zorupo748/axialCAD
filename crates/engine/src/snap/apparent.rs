//! Apparent intersections between a drafting ray and visible WCS lines.

use cadcraft_doc::{Drawing, EntityKind, Space};
use cadcraft_geom::{OrbitCamera, Vec2, Vec3, line_line_infinite};

use super::{SnapHit, mode, name_of};

const MIN_ANGLE_SINE: f64 = 1e-4;

/// Snap the current LINE ray to a crossing in the view. The aperture and cursor are in
/// camera coordinates. The chosen WCS point belongs to the existing line at the projected
/// crossing, so it shares the ray's screen position and the existing line's depth.
pub fn projected_apparent_line_intersection(
    doc: &Drawing,
    space: &Space,
    camera: &OrbitCamera,
    cursor: Vec2,
    aperture: f64,
    start: Vec3,
    toward: Vec3,
) -> Option<SnapHit> {
    let direction = toward - start;
    let length = direction.len();
    if !cursor.is_finite() || !aperture.is_finite() || aperture <= 0.0 || !length.is_finite() || length <= 1e-12 {
        return None;
    }

    let (right, up) = (camera.right_dir(), camera.up_dir());
    let project_direction = |d: Vec3| Vec2::new(d.dot(right), d.dot(up));
    let project = |p: Vec3| project_direction(p - camera.target);
    let origin = project(start);
    let ray = project_direction(direction) / length;
    let ray_length = ray.len();
    if !origin.is_finite() || !ray_length.is_finite() || ray_length < MIN_ANGLE_SINE {
        return None;
    }
    let ray = ray / ray_length;
    let from_origin = cursor - origin;
    let ray_distance = if from_origin.dot(ray) < 0.0 { from_origin.len() } else { ray.cross(from_origin).abs() };
    if !ray_distance.is_finite() || ray_distance > aperture {
        return None;
    }

    let mut best = None;
    let mut nearest = f64::INFINITY;
    for entity in doc.space(space)?.iter() {
        let EntityKind::Line(line) = &entity.kind else { continue };
        if !doc.is_visible(entity) {
            continue;
        }
        let a = project(line.a);
        let b = project(line.b);
        // A finite segment with a crossing inside the aperture must overlap its bounding square.
        if !a.is_finite()
            || !b.is_finite()
            || a.x.min(b.x) - cursor.x > aperture
            || cursor.x - a.x.max(b.x) > aperture
            || a.y.min(b.y) - cursor.y > aperture
            || cursor.y - a.y.max(b.y) > aperture
        {
            continue;
        }
        let segment = b - a;
        let projected_length = segment.len();
        if !projected_length.is_finite() || projected_length <= 1e-12 || ray.cross(segment).abs() < MIN_ANGLE_SINE * projected_length {
            continue;
        }
        let Some((crossing, along_ray, along_line)) = line_line_infinite(origin, origin + ray, a, b) else { continue };
        if !along_ray.is_finite() || along_ray <= 0.0 || !(-1e-9..=1.0 + 1e-9).contains(&along_line) {
            continue;
        }
        let distance = crossing.dist(cursor);
        if !distance.is_finite() || distance > aperture || distance >= nearest {
            continue;
        }
        let point3 = line.a + (line.b - line.a) * along_line;
        if point3.is_finite() {
            nearest = distance;
            best =
                Some(SnapHit { point: point3.xy(), point3: Some(point3), mode: mode::APP, name: name_of(mode::APP), viewport: None, deferred: None });
        }
    }
    best
}

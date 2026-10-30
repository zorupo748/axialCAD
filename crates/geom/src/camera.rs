use serde::{Deserialize, Serialize};

use crate::{Mat3, Vec2, Vec3, norm_angle};

/// Predefined drawing/construction planes for 2D and 3D wireframe drafting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawingPlane {
    /// Horizontal XY plane (Z = 0) - default 2D drafting plane.
    #[default]
    XY,
    /// Vertical XZ plane (Y = 0) - enables drawing with X and height (Z) changes.
    XZ,
    /// Vertical YZ plane (X = 0) - enables drawing with Y and height (Z) changes.
    YZ,
}

impl DrawingPlane {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "xy" | "top" | "plan" | "world" => Some(Self::XY),
            "xz" | "front" => Some(Self::XZ),
            "yz" | "right" | "side" => Some(Self::YZ),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::XY => "XY",
            Self::XZ => "XZ",
            Self::YZ => "YZ",
        }
    }
}

/// The three Cartesian axes in 3D WCS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis3D {
    X,
    Y,
    Z,
}

impl Axis3D {
    pub fn name(&self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }
}

/// Maximum pitch angle from the XY plane (89.5°) to prevent gimbal lock, axis inversions, and
/// upside-down camera flipping during turntable orbit navigation.
pub const MAX_ORBIT_ELEVATION: f64 = 89.5 * std::f64::consts::PI / 180.0;

/// 3D Orbit Camera (turntable navigation model compatible with AutoCAD 3DORBIT).
///
/// Orientates the view around a stable reference target point in WCS.
/// In standard Plan (Top) view, azimuth is -90° (-π/2) and elevation is +90° (+π/2),
/// looking straight down the Z axis with +X to the right and +Y upwards.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitCamera {
    /// Reference target point in WCS around which the camera orbits.
    pub target: Vec3,
    /// Azimuth angle in radians (horizontal rotation around Z axis).
    /// In Plan/Top view: -π/2 (-90°), looking towards +Y.
    pub azimuth: f64,
    /// Elevation angle in radians (pitch from XY plane towards Z axis).
    /// In Plan/Top view: +π/2 (+90°), looking straight down.
    pub elevation: f64,
    /// Visible height in drawing units.
    pub height: f64,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        OrbitCamera { target: Vec3::ZERO, azimuth: -std::f64::consts::FRAC_PI_2, elevation: std::f64::consts::FRAC_PI_2, height: 10.0 }
    }
}

impl OrbitCamera {
    /// Create a new camera in Plan (Top) view centered on `target`.
    pub fn plan(target: Vec3, height: f64) -> Self {
        OrbitCamera { target, azimuth: -std::f64::consts::FRAC_PI_2, elevation: std::f64::consts::FRAC_PI_2, height: height.max(1e-9) }
    }

    /// Whether the camera is in Plan (Top) view looking straight down the Z axis.
    pub fn is_plan(&self) -> bool {
        (self.elevation - std::f64::consts::FRAC_PI_2).abs() < 1e-3
            && (norm_angle(self.azimuth) - norm_angle(-std::f64::consts::FRAC_PI_2)).abs() < 1e-3
    }

    /// Line-of-sight unit vector pointing from target towards the camera eye.
    pub fn view_dir(&self) -> Vec3 {
        let (s_el, c_el) = self.elevation.sin_cos();
        let (s_az, c_az) = self.azimuth.sin_cos();
        Vec3::new(c_el * c_az, c_el * s_az, s_el)
    }

    /// Camera Right unit vector in the horizontal plane.
    pub fn right_dir(&self) -> Vec3 {
        let (s_az, c_az) = self.azimuth.sin_cos();
        Vec3::new(-s_az, c_az, 0.0)
    }

    /// Camera Up unit vector perpendicular to `right_dir` and `view_dir`.
    pub fn up_dir(&self) -> Vec3 {
        let (s_el, c_el) = self.elevation.sin_cos();
        let (s_az, c_az) = self.azimuth.sin_cos();
        Vec3::new(-s_el * c_az, -s_el * s_az, c_el)
    }

    /// 2D orientation matrix projecting relative (X, Y) displacements onto camera (u, v).
    pub fn orient_matrix(&self) -> Mat3 {
        let r = self.right_dir();
        let u = self.up_dir();
        Mat3 { a: r.x, b: u.x, c: r.y, d: u.y, e: 0.0, f: 0.0 }
    }

    /// Project a 3D point in WCS into 2D camera coordinates (u, v) centered at target.
    pub fn project_point(&self, p: Vec3) -> Vec2 {
        let d = p - self.target;
        Vec2::new(d.dot(self.right_dir()), d.dot(self.up_dir()))
    }

    /// Project a 2D point on the XY plane (Z = 0) into 2D camera coordinates.
    pub fn project_point2(&self, p: Vec2) -> Vec2 {
        self.project_point(Vec3::new(p.x, p.y, 0.0))
    }

    /// Unproject a 2D camera coordinate (u, v) centered at target onto the specified drawing plane
    /// with an arbitrary elevation offset.
    ///
    /// The camera ray is defined by $P(s) = P_0 + s \cdot v$, where $P_0 = \text{target} + q_x \cdot r + q_y \cdot u$
    /// and $v$ is the view direction. If the ray is near-parallel to the plane ($|v \cdot n| \le 10^{-4}$),
    /// this function returns `None` to inform the caller of depth ambiguity from this viewpoint.
    pub fn unproject_to_plane_with_offset(&self, q: Vec2, plane: DrawingPlane, offset: f64) -> Option<Vec3> {
        let r = self.right_dir();
        let u = self.up_dir();
        let v = self.view_dir();
        let p0 = self.target + r * q.x + u * q.y;

        match plane {
            DrawingPlane::XY => {
                if self.is_plan() {
                    return Some(Vec3::new(self.target.x + q.x, self.target.y + q.y, offset));
                }
                if v.z.abs() > 1e-4 {
                    let s = (offset - p0.z) / v.z;
                    let p = p0 + v * s;
                    if p.x.is_finite() && p.y.is_finite() {
                        Some(Vec3::new(p.x, p.y, offset))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            DrawingPlane::XZ => {
                if v.y.abs() > 1e-4 {
                    let s = (offset - p0.y) / v.y;
                    let p = p0 + v * s;
                    if p.x.is_finite() && p.z.is_finite() {
                        Some(Vec3::new(p.x, offset, p.z))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            DrawingPlane::YZ => {
                if v.x.abs() > 1e-4 {
                    let s = (offset - p0.x) / v.x;
                    let p = p0 + v * s;
                    if p.y.is_finite() && p.z.is_finite() {
                        Some(Vec3::new(offset, p.y, p.z))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    /// Unproject a 2D camera coordinate (u, v) centered at target onto the specified drawing plane
    /// at the given coordinate offset along the plane normal.
    ///
    /// For DrawingPlane::XY, intersects the camera ray with the plane $Z = \text{offset}$.
    /// For DrawingPlane::XZ, intersects with $Y = \text{offset}$.
    /// For DrawingPlane::YZ, intersects with $X = \text{offset}$.
    ///
    /// The camera ray is defined by $P(s) = P_0 + s \cdot v$, where $P_0 = \text{target} + q_x \cdot r + q_y \cdot u$
    /// and $v$ is the view direction. If the ray is near-parallel to the plane ($|v \cdot n| \le 10^{-4}$) or if
    /// the intersection computation overflows/yields non-finite coordinates, a stable fallback projection onto the plane is used.
    pub fn unproject_to_drawing_plane_at(&self, q: Vec2, plane: DrawingPlane, offset: f64) -> Vec3 {
        if let Some(p) = self.unproject_to_plane_with_offset(q, plane, offset) {
            if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
                return p;
            }
        }
        let r = self.right_dir();
        let u = self.up_dir();
        let p0 = self.target + r * q.x + u * q.y;
        match plane {
            DrawingPlane::XY => Vec3::new(if p0.x.is_finite() { p0.x } else { 0.0 }, if p0.y.is_finite() { p0.y } else { 0.0 }, offset),
            DrawingPlane::XZ => Vec3::new(if p0.x.is_finite() { p0.x } else { 0.0 }, offset, if p0.z.is_finite() { p0.z } else { 0.0 }),
            DrawingPlane::YZ => Vec3::new(offset, if p0.y.is_finite() { p0.y } else { 0.0 }, if p0.z.is_finite() { p0.z } else { 0.0 }),
        }
    }

    /// Unproject a 2D camera coordinate (u, v) centered at target onto the specified drawing plane.
    pub fn unproject_to_drawing_plane(&self, q: Vec2, plane: DrawingPlane) -> Vec3 {
        self.unproject_to_drawing_plane_at(q, plane, 0.0)
    }

    /// Unproject a 2D camera plane coordinate (u, v) back onto the construction plane (Z = 0).
    pub fn unproject_to_plane(&self, q: Vec2) -> Vec2 {
        self.unproject_to_drawing_plane(q, DrawingPlane::XY).xy()
    }

    /// Calculate height Z on a vertical line passing through base point `base`
    /// closest to the camera ray passing through camera coordinate `q`.
    ///
    /// Preserves world X and Y of `base` and calculates the Z coordinate corresponding to the
    /// cursor's vertical position in the screen plane.
    pub fn unproject_vertical_z(&self, q: Vec2, base: Vec3) -> f64 {
        let r = self.right_dir();
        let u = self.up_dir();
        let v = self.view_dir();
        let p0 = self.target + r * q.x + u * q.y;
        let diff = p0 - base;
        let b = v.z;
        let denom = 1.0 - b * b;
        if denom > 1e-6 {
            let num = diff.z - b * diff.dot(v);
            base.z + num / denom
        } else {
            base.z + (q.y - self.project_point(base).y)
        }
    }

    /// For a 3D base point, calculate the closest orthogonal 3D point (along X, Y, or Z axis)
    /// from the camera ray at coordinate `q`.
    pub fn closest_axis_3d(&self, q: Vec2, base: Vec3) -> (Vec3, f64) {
        let (pt, _axis, delta) = self.closest_axis_3d_with_prev(q, base, None);
        (pt, delta)
    }

    /// Calculate the closest orthogonal 3D point with explicit axis identification and hysteresis.
    ///
    /// - An axis whose projected screen length is < 0.05 units is near line-of-sight and excluded.
    /// - Candidate axes are scored by normalized directional alignment (cosine of screen angle).
    /// - Hysteresis gives a 15% bonus to `prev` to avoid erratic switching near 45° boundaries.
    /// - Ambiguities are resolved deterministically (X > Y > Z).
    pub fn closest_axis_3d_with_prev(&self, q: Vec2, base: Vec3, prev: Option<Axis3D>) -> (Vec3, Axis3D, f64) {
        let p_screen = self.project_point(base);
        let delta = q - p_screen;
        let delta_len = delta.len();

        let s_x = self.project_point(base + Vec3::new(1.0, 0.0, 0.0)) - p_screen;
        let s_y = self.project_point(base + Vec3::new(0.0, 1.0, 0.0)) - p_screen;
        let s_z = self.project_point(base + Vec3::new(0.0, 0.0, 1.0)) - p_screen;

        let len_x = s_x.len();
        let len_y = s_y.len();
        let len_z = s_z.len();

        const MIN_SCREEN_AXIS_LEN: f64 = 0.05;

        let score = |s: Vec2, len: f64, axis: Axis3D| -> f64 {
            if len < MIN_SCREEN_AXIS_LEN || delta_len < 1e-6 {
                return -1.0;
            }
            let cos_theta = delta.dot(s).abs() / (len * delta_len);
            let mut sc = cos_theta;
            if prev == Some(axis) {
                sc *= 1.15;
            }
            sc
        };

        let score_x = score(s_x, len_x, Axis3D::X);
        let score_y = score(s_y, len_y, Axis3D::Y);
        let score_z = score(s_z, len_z, Axis3D::Z);

        let chosen = if score_z >= score_x && score_z >= score_y && score_z >= 0.0 {
            Axis3D::Z
        } else if score_x >= score_y && score_x >= 0.0 {
            Axis3D::X
        } else if score_y >= 0.0 {
            Axis3D::Y
        } else if len_x >= len_y && len_x >= len_z {
            Axis3D::X
        } else if len_y >= len_z {
            Axis3D::Y
        } else {
            Axis3D::Z
        };

        match chosen {
            Axis3D::Z => {
                let z = self.unproject_vertical_z(q, base);
                (Vec3::new(base.x, base.y, z), Axis3D::Z, z - base.z)
            }
            Axis3D::X => {
                let r = self.right_dir();
                let u = self.up_dir();
                let v = self.view_dir();
                let p0 = self.target + r * q.x + u * q.y;
                let diff = p0 - base;
                let b = v.x;
                let denom = 1.0 - b * b;
                let x = if denom > 1e-6 { base.x + (diff.x - b * diff.dot(v)) / denom } else { base.x + delta.x };
                (Vec3::new(x, base.y, base.z), Axis3D::X, x - base.x)
            }
            Axis3D::Y => {
                let r = self.right_dir();
                let u = self.up_dir();
                let v = self.view_dir();
                let p0 = self.target + r * q.x + u * q.y;
                let diff = p0 - base;
                let b = v.y;
                let denom = 1.0 - b * b;
                let y = if denom > 1e-6 { base.y + (diff.y - b * diff.dot(v)) / denom } else { base.y + delta.y };
                (Vec3::new(base.x, y, base.z), Axis3D::Y, y - base.y)
            }
        }
    }

    /// Pan the camera parallel to the screen plane by mouse delta in pixels.
    ///
    /// Translates `target` along the camera's `right_dir` and `up_dir` basis vectors:
    /// $\Delta \text{target} = - \left( \frac{dx}{\text{scale}} \cdot \mathbf{r} - \frac{dy}{\text{scale}} \cdot \mathbf{u} \right)$
    /// so that world geometry follows the mouse cursor precisely in any 2D or 3D orientation.
    pub fn pan(&mut self, dx_pixels: f64, dy_pixels: f64, scale: f64) {
        if !scale.is_finite() || scale <= 1e-12 {
            return;
        }
        let du = dx_pixels / scale;
        let dv = -dy_pixels / scale;
        let r = self.right_dir();
        let u = self.up_dir();
        let d = r * du + u * dv;
        if self.is_plan() {
            self.target = Vec3::new(self.target.x - d.x, self.target.y - d.y, self.target.z);
        } else {
            self.target = self.target - d;
        }
    }

    /// Update camera orientation by dragging mouse delta in pixels.
    ///
    /// Clamping elevation strictly within `[-MAX_ORBIT_ELEVATION, MAX_ORBIT_ELEVATION]` ensures
    /// the turntable model remains stable and never flips upside down or suffers gimbal lock.
    pub fn orbit(&mut self, dx_pixels: f64, dy_pixels: f64, viewport_size: (f64, f64)) {
        let sens_x = std::f64::consts::PI / viewport_size.0.max(100.0);
        let sens_y = std::f64::consts::PI / viewport_size.1.max(100.0);
        self.azimuth = norm_angle(self.azimuth + dx_pixels * sens_x);
        self.elevation = (self.elevation - dy_pixels * sens_y).clamp(-MAX_ORBIT_ELEVATION, MAX_ORBIT_ELEVATION);
    }

    /// Set camera orientation from a 3D view direction vector (towards eye).
    pub fn set_view_dir(&mut self, dir: Vec3) {
        if dir.dot(dir) < 1e-12 {
            return;
        }
        let v = dir.normalized();
        let r_xy = (v.x * v.x + v.y * v.y).sqrt();
        let el = v.z.atan2(r_xy);
        let az = if r_xy < 1e-6 { -std::f64::consts::FRAC_PI_2 } else { v.y.atan2(v.x) };
        if r_xy < 1e-6 {
            self.elevation = if v.z >= 0.0 { std::f64::consts::FRAC_PI_2 } else { -std::f64::consts::FRAC_PI_2 };
        } else {
            self.elevation = el.clamp(-MAX_ORBIT_ELEVATION, MAX_ORBIT_ELEVATION);
        }
        self.azimuth = norm_angle(az);
    }

    /// Set camera orientation to one of AutoCAD's standard 3D view presets.
    pub fn set_preset(&mut self, preset: &str) {
        let p = preset.trim().to_ascii_lowercase();
        let pi = std::f64::consts::PI;
        let frac_pi_2 = std::f64::consts::FRAC_PI_2;
        let iso_elev = (1.0f64 / 3.0f64.sqrt()).asin();
        match p.as_str() {
            "top" | "plan" => {
                self.azimuth = -frac_pi_2;
                self.elevation = frac_pi_2;
            }
            "bottom" => {
                self.azimuth = -frac_pi_2;
                self.elevation = -frac_pi_2;
            }
            "front" => {
                self.azimuth = -frac_pi_2;
                self.elevation = 0.0;
            }
            "back" => {
                self.azimuth = frac_pi_2;
                self.elevation = 0.0;
            }
            "left" => {
                self.azimuth = pi;
                self.elevation = 0.0;
            }
            "right" => {
                self.azimuth = 0.0;
                self.elevation = 0.0;
            }
            "swiso" => {
                self.azimuth = norm_angle(-135.0 * pi / 180.0);
                self.elevation = iso_elev;
            }
            "seiso" => {
                self.azimuth = norm_angle(-45.0 * pi / 180.0);
                self.elevation = iso_elev;
            }
            "neiso" => {
                self.azimuth = norm_angle(45.0 * pi / 180.0);
                self.elevation = iso_elev;
            }
            "nwiso" => {
                self.azimuth = norm_angle(135.0 * pi / 180.0);
                self.elevation = iso_elev;
            }
            _ => {}
        }
    }

    /// Approximate name of the current closest standard view for ViewCube display.
    pub fn view_name(&self) -> &'static str {
        if self.is_plan() {
            return "TOP";
        }
        let v = self.view_dir();
        if v.z > 0.85 {
            "TOP"
        } else if v.z < -0.85 {
            "BOTTOM"
        } else if v.y < -0.7 && v.x.abs() < 0.5 {
            "FRONT"
        } else if v.y > 0.7 && v.x.abs() < 0.5 {
            "BACK"
        } else if v.x < -0.7 && v.y.abs() < 0.5 {
            "LEFT"
        } else if v.x > 0.7 && v.y.abs() < 0.5 {
            "RIGHT"
        } else if v.z > 0.3 {
            if v.x <= 0.0 && v.y <= 0.0 {
                "SW ISO"
            } else if v.x >= 0.0 && v.y <= 0.0 {
                "SE ISO"
            } else if v.x >= 0.0 && v.y >= 0.0 {
                "NE ISO"
            } else {
                "NW ISO"
            }
        } else {
            "CUSTOM"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_view_basis_and_projection() {
        let cam = OrbitCamera::plan(Vec3::new(10.0, 20.0, 0.0), 100.0);
        assert!(cam.is_plan());
        let r = cam.right_dir();
        let u = cam.up_dir();
        let v = cam.view_dir();

        // R must be +X, U must be +Y, V must be +Z
        assert!((r.x - 1.0).abs() < 1e-6);
        assert!(r.y.abs() < 1e-6);
        assert!(r.z.abs() < 1e-6);

        assert!(u.x.abs() < 1e-6);
        assert!((u.y - 1.0).abs() < 1e-6);
        assert!(u.z.abs() < 1e-6);

        assert!(v.x.abs() < 1e-6);
        assert!(v.y.abs() < 1e-6);
        assert!((v.z - 1.0).abs() < 1e-6);

        // Projecting a point relative to target matches 2D subtraction
        let p = Vec3::new(15.0, 28.0, 0.0);
        let proj = cam.project_point(p);
        assert!((proj.x - 5.0).abs() < 1e-6);
        assert!((proj.y - 8.0).abs() < 1e-6);

        // Roundtrip unproject
        let unproj = cam.unproject_to_plane(proj);
        assert!((unproj.x - 15.0).abs() < 1e-6);
        assert!((unproj.y - 28.0).abs() < 1e-6);
    }

    #[test]
    fn test_orthonormality_under_arbitrary_orbit() {
        let mut cam = OrbitCamera::default();
        // Simulate dragging orbit
        cam.orbit(150.0, -80.0, (800.0, 600.0));
        let r = cam.right_dir();
        let u = cam.up_dir();
        let v = cam.view_dir();

        // Unit lengths
        assert!((r.len() - 1.0).abs() < 1e-6);
        assert!((u.len() - 1.0).abs() < 1e-6);
        assert!((v.len() - 1.0).abs() < 1e-6);

        // Mutual orthogonality
        assert!(r.dot(u).abs() < 1e-6);
        assert!(r.dot(v).abs() < 1e-6);
        assert!(u.dot(v).abs() < 1e-6);

        // Right-handed frame: R x U = V
        let rxu = r.cross(u);
        assert!((rxu.x - v.x).abs() < 1e-6);
        assert!((rxu.y - v.y).abs() < 1e-6);
        assert!((rxu.z - v.z).abs() < 1e-6);
    }

    #[test]
    fn test_elevation_clamping_prevents_axis_flip() {
        let mut cam = OrbitCamera::default();
        // Extreme dragging downwards
        cam.orbit(0.0, 50000.0, (800.0, 600.0));
        assert!(cam.elevation >= -MAX_ORBIT_ELEVATION);
        assert!(cam.elevation <= MAX_ORBIT_ELEVATION);

        // Extreme dragging upwards
        cam.orbit(0.0, -100000.0, (800.0, 600.0));
        assert!(cam.elevation <= MAX_ORBIT_ELEVATION);
        assert!(cam.elevation >= -MAX_ORBIT_ELEVATION);

        // Up vector z component is always positive, never inverted
        assert!(cam.up_dir().z > 0.0);
    }

    #[test]
    fn test_presets_and_view_names() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("front");
        assert_eq!(cam.view_name(), "FRONT");

        cam.set_preset("top");
        assert_eq!(cam.view_name(), "TOP");

        cam.set_preset("swiso");
        assert_eq!(cam.view_name(), "SW ISO");

        cam.set_preset("seiso");
        assert_eq!(cam.view_name(), "SE ISO");

        cam.set_preset("left");
        assert_eq!(cam.view_name(), "LEFT");

        cam.set_preset("right");
        assert_eq!(cam.view_name(), "RIGHT");
    }

    #[test]
    fn test_drawing_plane_parse_and_name() {
        assert_eq!(DrawingPlane::parse("xy"), Some(DrawingPlane::XY));
        assert_eq!(DrawingPlane::parse("top"), Some(DrawingPlane::XY));
        assert_eq!(DrawingPlane::parse("world"), Some(DrawingPlane::XY));
        assert_eq!(DrawingPlane::parse("xz"), Some(DrawingPlane::XZ));
        assert_eq!(DrawingPlane::parse("front"), Some(DrawingPlane::XZ));
        assert_eq!(DrawingPlane::parse("yz"), Some(DrawingPlane::YZ));
        assert_eq!(DrawingPlane::parse("right"), Some(DrawingPlane::YZ));
        assert_eq!(DrawingPlane::parse("invalid"), None);

        assert_eq!(DrawingPlane::XY.name(), "XY");
        assert_eq!(DrawingPlane::XZ.name(), "XZ");
        assert_eq!(DrawingPlane::YZ.name(), "YZ");
    }

    #[test]
    fn test_unproject_to_drawing_plane_in_iso_view() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");
        let q = Vec2::new(5.0, 10.0);

        // XY plane: result.z must be strictly 0.0
        let p_xy = cam.unproject_to_drawing_plane(q, DrawingPlane::XY);
        assert!((p_xy.z - 0.0).abs() < 1e-12);
        // Projecting it back matches q
        let q_proj_xy = cam.project_point(p_xy);
        assert!((q_proj_xy.x - q.x).abs() < 1e-6);
        assert!((q_proj_xy.y - q.y).abs() < 1e-6);

        // XZ plane: result.y must be strictly 0.0
        let p_xz = cam.unproject_to_drawing_plane(q, DrawingPlane::XZ);
        assert!((p_xz.y - 0.0).abs() < 1e-12);
        let q_proj_xz = cam.project_point(p_xz);
        assert!((q_proj_xz.x - q.x).abs() < 1e-6);
        assert!((q_proj_xz.y - q.y).abs() < 1e-6);

        // YZ plane: result.x must be strictly 0.0
        let p_yz = cam.unproject_to_drawing_plane(q, DrawingPlane::YZ);
        assert!((p_yz.x - 0.0).abs() < 1e-12);
        let q_proj_yz = cam.project_point(p_yz);
        assert!((q_proj_yz.x - q.x).abs() < 1e-6);
        assert!((q_proj_yz.y - q.y).abs() < 1e-6);
    }

    #[test]
    fn test_unproject_parallel_ray_fallbacks_never_panic() {
        // Plan/Top view: view direction is along +Z, parallel to XZ and YZ planes
        let mut cam = OrbitCamera::default();
        cam.set_preset("top");
        let q = Vec2::new(12.0, -8.0);

        // In top view, XZ has v.y == 0
        let p_xz = cam.unproject_to_drawing_plane(q, DrawingPlane::XZ);
        assert!(p_xz.x.is_finite());
        assert_eq!(p_xz.y, 0.0);
        assert!(p_xz.z.is_finite());

        // In top view, YZ has v.x == 0
        let p_yz = cam.unproject_to_drawing_plane(q, DrawingPlane::YZ);
        assert_eq!(p_yz.x, 0.0);
        assert!(p_yz.y.is_finite());
        assert!(p_yz.z.is_finite());

        // Front view: view direction is along +Y, parallel to XY
        cam.set_preset("front");
        let p_xy = cam.unproject_to_drawing_plane(q, DrawingPlane::XY);
        assert!(p_xy.x.is_finite());
        assert!(p_xy.y.is_finite());
        assert_eq!(p_xy.z, 0.0);
    }

    #[test]
    fn test_pan_in_2d_plan_view() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("top");
        let orig_az = cam.azimuth;
        let orig_el = cam.elevation;
        let orig_ht = cam.height;

        let scale = 20.0; // 20 pixels per world unit
        cam.pan(100.0, -60.0, scale);

        // Orientation and height must be strictly unchanged
        assert_eq!(cam.azimuth, orig_az);
        assert_eq!(cam.elevation, orig_el);
        assert_eq!(cam.height, orig_ht);

        // du = 100 / 20 = 5.0 (right). dv = -(-60) / 20 = 3.0 (up).
        // target should translate by -5.0 in X and -3.0 in Y.
        assert!((cam.target.x - (-5.0)).abs() < 1e-12);
        assert!((cam.target.y - (-3.0)).abs() < 1e-12);
        assert_eq!(cam.target.z, 0.0);
    }

    #[test]
    fn test_pan_in_3d_isometric_view_shifts_projected_screen_coordinates_consistently() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");
        let orig_az = cam.azimuth;
        let orig_el = cam.elevation;
        let orig_ht = cam.height;

        let scale = 50.0;
        let pt = Vec3::new(10.0, 20.0, 30.0);

        // Project point to camera coordinates before pan
        let q_before = cam.project_point(pt);

        let dx_pixels = 40.0;
        let dy_pixels = -30.0;
        cam.pan(dx_pixels, dy_pixels, scale);

        // Orientation and zoom must never be modified by pan
        assert_eq!(cam.azimuth, orig_az);
        assert_eq!(cam.elevation, orig_el);
        assert_eq!(cam.height, orig_ht);

        // Project point after pan
        let q_after = cam.project_point(pt);

        // The displacement in camera screen units (u, v) must be exactly (dx / scale, -dy / scale)
        let expected_du = dx_pixels / scale;
        let expected_dv = -dy_pixels / scale;
        assert!((q_after.x - q_before.x - expected_du).abs() < 1e-12);
        assert!((q_after.y - q_before.y - expected_dv).abs() < 1e-12);
    }

    #[test]
    fn test_pan_different_zoom_levels_scale_proportionally() {
        let mut cam1 = OrbitCamera::default();
        cam1.set_preset("swiso");
        let mut cam2 = cam1;

        // Zoom 1: scale = 25. Zoom 2: scale = 50 (zoomed in 2x)
        cam1.pan(100.0, 50.0, 25.0);
        cam2.pan(100.0, 50.0, 50.0);

        // Moving 100px at 2x zoom moves half the world distance
        let dist1 = (cam1.target - Vec3::ZERO).len();
        let dist2 = (cam2.target - Vec3::ZERO).len();
        assert!((dist1 - 2.0 * dist2).abs() < 1e-12);
    }

    #[test]
    fn test_unproject_vertical_z_preserves_xy_and_finds_correct_height() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");
        let base = Vec3::new(10.0, 20.0, 0.0);

        // A point directly above base at height Z = 100
        let target_pt = Vec3::new(10.0, 20.0, 100.0);
        let q = cam.project_point(target_pt);

        // unproject_vertical_z from that screen coordinate must return 100.0
        let z = cam.unproject_vertical_z(q, base);
        assert!((z - 100.0).abs() < 1e-6);

        // In front view:
        cam.set_preset("front");
        let q_front = cam.project_point(target_pt);
        let z_front = cam.unproject_vertical_z(q_front, base);
        assert!((z_front - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_closest_axis_3d_selects_z_axis_on_vertical_mouse_movement() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");
        let base = Vec3::new(5.0, 15.0, 25.0);

        // Move cursor straight up from base projection
        let p_screen = cam.project_point(base);
        let q_up = Vec2::new(p_screen.x, p_screen.y + 20.0);

        let (pt, delta) = cam.closest_axis_3d(q_up, base);
        assert_eq!(pt.x, base.x);
        assert_eq!(pt.y, base.y);
        assert!(pt.z > base.z);
        assert!(delta > 0.0);
    }

    #[test]
    fn test_unproject_to_drawing_plane_at_elevations_positive_negative() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");

        // Positive elevation offset on XY
        let pt_pos = Vec3::new(30.0, 45.0, 150.0);
        let q_pos = cam.project_point(pt_pos);
        let res_pos = cam.unproject_to_drawing_plane_at(q_pos, DrawingPlane::XY, 150.0);
        assert!((res_pos.x - pt_pos.x).abs() < 1e-9);
        assert!((res_pos.y - pt_pos.y).abs() < 1e-9);
        assert_eq!(res_pos.z, 150.0);

        // Negative elevation offset on XY
        let pt_neg = Vec3::new(-120.0, 80.0, -250.0);
        let q_neg = cam.project_point(pt_neg);
        let res_neg = cam.unproject_to_drawing_plane_at(q_neg, DrawingPlane::XY, -250.0);
        assert!((res_neg.x - pt_neg.x).abs() < 1e-9);
        assert!((res_neg.y - pt_neg.y).abs() < 1e-9);
        assert_eq!(res_neg.z, -250.0);
    }

    #[test]
    fn test_unproject_to_drawing_plane_at_planes_xy_xz_yz_and_reprojection() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");

        // DrawingPlane::XY (Z = 42.0)
        let pt_xy = Vec3::new(12.5, 34.2, 42.0);
        let q_xy = cam.project_point(pt_xy);
        let unproj_xy = cam.unproject_to_drawing_plane_at(q_xy, DrawingPlane::XY, 42.0);
        assert!((unproj_xy.x - pt_xy.x).abs() < 1e-9);
        assert!((unproj_xy.y - pt_xy.y).abs() < 1e-9);
        assert_eq!(unproj_xy.z, 42.0);
        let reproj_xy = cam.project_point(unproj_xy);
        assert!((reproj_xy.x - q_xy.x).abs() < 1e-9);
        assert!((reproj_xy.y - q_xy.y).abs() < 1e-9);

        // DrawingPlane::XZ (Y = -18.5)
        let pt_xz = Vec3::new(55.0, -18.5, 90.0);
        let q_xz = cam.project_point(pt_xz);
        let unproj_xz = cam.unproject_to_drawing_plane_at(q_xz, DrawingPlane::XZ, -18.5);
        assert!((unproj_xz.x - pt_xz.x).abs() < 1e-9);
        assert_eq!(unproj_xz.y, -18.5);
        assert!((unproj_xz.z - pt_xz.z).abs() < 1e-9);
        let reproj_xz = cam.project_point(unproj_xz);
        assert!((reproj_xz.x - q_xz.x).abs() < 1e-9);
        assert!((reproj_xz.y - q_xz.y).abs() < 1e-9);

        // DrawingPlane::YZ (X = 105.0)
        let pt_yz = Vec3::new(105.0, -70.0, 30.0);
        let q_yz = cam.project_point(pt_yz);
        let unproj_yz = cam.unproject_to_drawing_plane_at(q_yz, DrawingPlane::YZ, 105.0);
        assert_eq!(unproj_yz.x, 105.0);
        assert!((unproj_yz.y - pt_yz.y).abs() < 1e-9);
        assert!((unproj_yz.z - pt_yz.z).abs() < 1e-9);
        let reproj_yz = cam.project_point(unproj_yz);
        assert!((reproj_yz.x - q_yz.x).abs() < 1e-9);
        assert!((reproj_yz.y - q_yz.y).abs() < 1e-9);
    }

    #[test]
    fn test_unproject_large_valid_coordinates_not_rejected() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");

        // Camera target shifted to large coordinates (e.g. 5e8, beyond former 1e8 limit)
        cam.target = Vec3::new(5e8, 5e8, 1e8);
        let pt = Vec3::new(5e8 + 1234.5, 5e8 + 6789.0, 1e8 + 50.0);
        let q = cam.project_point(pt);

        let unproj = cam.unproject_to_drawing_plane_at(q, DrawingPlane::XY, 1e8 + 50.0);
        assert!((unproj.x - pt.x).abs() < 1e-4);
        assert!((unproj.y - pt.y).abs() < 1e-4);
        assert_eq!(unproj.z, 1e8 + 50.0);

        // Reprojection check
        let reproj = cam.project_point(unproj);
        assert!((reproj.x - q.x).abs() < 1e-9);
        assert!((reproj.y - q.y).abs() < 1e-9);
    }

    #[test]
    fn test_unproject_parallel_and_near_parallel_returns_finite_fallback() {
        let mut cam = OrbitCamera::default();

        // View direction parallel to XY plane: "front" view (elevation = 0) -> v.z = 0
        cam.set_preset("front");
        let q = Vec2::new(10.0, 20.0);
        let unproj_xy = cam.unproject_to_drawing_plane_at(q, DrawingPlane::XY, 75.0);
        assert!(unproj_xy.x.is_finite());
        assert!(unproj_xy.y.is_finite());
        assert_eq!(unproj_xy.z, 75.0);

        // Near-parallel to XY plane: grazing angle elevation = 5e-5 < 1e-4 threshold
        cam.elevation = 5e-5;
        let unproj_grazing = cam.unproject_to_drawing_plane_at(q, DrawingPlane::XY, 75.0);
        assert!(unproj_grazing.x.is_finite());
        assert!(unproj_grazing.y.is_finite());
        assert_eq!(unproj_grazing.z, 75.0);

        // Top view: elevation = PI/2 -> view_dir.y = 0 and view_dir.x = 0
        cam.set_preset("top");
        let unproj_xz = cam.unproject_to_drawing_plane_at(q, DrawingPlane::XZ, -20.0);
        assert!(unproj_xz.x.is_finite());
        assert_eq!(unproj_xz.y, -20.0);
        assert!(unproj_xz.z.is_finite());

        let unproj_yz = cam.unproject_to_drawing_plane_at(q, DrawingPlane::YZ, 40.0);
        assert_eq!(unproj_yz.x, 40.0);
        assert!(unproj_yz.y.is_finite());
        assert!(unproj_yz.z.is_finite());
    }

    #[test]
    fn test_closest_axis_3d_hysteresis_and_deterministic_order() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");
        let base = Vec3::new(0.0, 0.0, 0.0);
        let p_screen = cam.project_point(base);

        // Movement in Z direction
        let q_z = p_screen + Vec2::new(0.0, 20.0);
        let (pt_z, axis_z, _) = cam.closest_axis_3d_with_prev(q_z, base, None);
        assert_eq!(axis_z, Axis3D::Z);
        assert_eq!(pt_z.x, 0.0);
        assert_eq!(pt_z.y, 0.0);

        // Movement along X axis
        let s_x = cam.project_point(Vec3::new(1.0, 0.0, 0.0)) - p_screen;
        let q_x = p_screen + s_x * 10.0;
        let (_, axis_x, _) = cam.closest_axis_3d_with_prev(q_x, base, None);
        assert_eq!(axis_x, Axis3D::X);

        // Hysteresis test: near the boundary between X and Z, previous axis retains selection
        let s_z = cam.project_point(Vec3::new(0.0, 0.0, 1.0)) - p_screen;
        let mid_xz = (s_x.normalized() + s_z.normalized()) * 15.0;
        let q_mid = p_screen + mid_xz;
        let (_, axis_with_x, _) = cam.closest_axis_3d_with_prev(q_mid, base, Some(Axis3D::X));
        let (_, axis_with_z, _) = cam.closest_axis_3d_with_prev(q_mid, base, Some(Axis3D::Z));
        assert_eq!(axis_with_x, Axis3D::X);
        assert_eq!(axis_with_z, Axis3D::Z);
    }

    #[test]
    fn test_unproject_to_plane_with_offset() {
        let mut cam = OrbitCamera::default();
        cam.set_preset("swiso");

        // XY plane at height offset 50.0
        let target_3d = Vec3::new(10.0, 20.0, 50.0);
        let q = cam.project_point(target_3d);
        let unproj = cam.unproject_to_plane_with_offset(q, DrawingPlane::XY, 50.0).expect("not parallel");
        assert!((unproj.x - target_3d.x).abs() < 1e-4);
        assert!((unproj.y - target_3d.y).abs() < 1e-4);
        assert!((unproj.z - 50.0).abs() < 1e-4);

        // Negative offset
        let target_neg = Vec3::new(-15.0, 30.0, -25.0);
        let q_neg = cam.project_point(target_neg);
        let unproj_neg = cam.unproject_to_plane_with_offset(q_neg, DrawingPlane::XY, -25.0).expect("not parallel");
        assert!((unproj_neg.x - target_neg.x).abs() < 1e-4);
        assert!((unproj_neg.y - target_neg.y).abs() < 1e-4);
        assert!((unproj_neg.z - (-25.0)).abs() < 1e-4);

        // Front view is parallel to XY plane (view dir points in +Y, dot with Z is 0)
        cam.set_preset("front");
        let parallel_res = cam.unproject_to_plane_with_offset(q, DrawingPlane::XY, 50.0);
        assert!(parallel_res.is_none(), "Parallel view must return None rather than explosive coordinates");
    }
}

use serde::{Deserialize, Serialize};

use crate::{Mat3, Vec2, Vec3, norm_angle};

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

    /// Unproject a 2D camera plane coordinate (u, v) back onto the construction plane (Z = 0).
    pub fn unproject_to_plane(&self, q: Vec2) -> Vec2 {
        if self.is_plan() {
            return self.target.xy() + q;
        }
        let r = self.right_dir();
        let u = self.up_dir();
        let v = self.view_dir();
        // Point on ray: P(s) = target + q.x * r + q.y * u + s * v
        // We solve for P.z == 0: target.z + q.y * u.z + s * v.z == 0
        let s = if v.z.abs() > 1e-4 { -(self.target.z + q.y * u.z) / v.z } else { 0.0 };
        let p = self.target + r * q.x + u * q.y + v * s;
        p.xy()
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
}

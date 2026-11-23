//! User Coordinate System (UCS) spatial reference frame.
//!
//! An orthonormal right-handed reference triad (origin, X, Y, Z) defining a local
//! 2D/3D coordinate system relative to the World Coordinate System (WCS).

use serde::{Deserialize, Serialize};

use crate::{EPS, Vec2, Vec3};

/// An orthonormal right-handed reference triad defining a User Coordinate System (UCS).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UcsFrame {
    pub origin: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub z_axis: Vec3,
}

impl Default for UcsFrame {
    fn default() -> Self {
        Self::WORLD
    }
}

impl UcsFrame {
    /// World Coordinate System (WCS) identity frame.
    pub const WORLD: Self = Self {
        origin: Vec3::ZERO,
        x_axis: Vec3::X,
        y_axis: Vec3::Y,
        z_axis: Vec3::Z,
    };

    pub fn new(origin: Vec3, x_axis: Vec3, y_axis: Vec3, z_axis: Vec3) -> Self {
        Self { origin, x_axis, y_axis, z_axis }
    }

    pub fn world() -> Self {
        Self::WORLD
    }

    /// True when this frame coincides with the World Coordinate System within tolerance.
    pub fn is_world(&self) -> bool {
        self.origin.near(Vec3::ZERO, EPS)
            && self.x_axis.near(Vec3::X, EPS)
            && self.y_axis.near(Vec3::Y, EPS)
            && self.z_axis.near(Vec3::Z, EPS)
    }

    /// Construct an orthonormal right-handed frame from an origin, a point on the positive X axis,
    /// and a point on the positive Y side of the XY plane.
    ///
    /// - The X axis points in the direction `(pt_x - origin)`.
    /// - The plane is spanned by `(pt_x - origin)` and `(pt_y - origin)`.
    /// - The Z axis is `(X × (pt_y - origin)).normalized()`.
    /// - The Y axis is computed as `Z × X`, ensuring strict orthogonality and preserving the positive half-plane.
    pub fn from_3_points(origin: Vec3, pt_x: Vec3, pt_y: Vec3) -> Result<Self, &'static str> {
        let vx = pt_x - origin;
        let lx = vx.len();
        if lx < EPS {
            return Err("El punto del eje X no puede coincidir con el origen.");
        }
        let x_axis = vx / lx;

        let vy = pt_y - origin;
        let ly = vy.len();
        if ly < EPS {
            return Err("El punto del plano XY no puede coincidir con el origen.");
        }

        let vz = x_axis.cross(vy);
        let lz = vz.len();
        // Collinearity test: |X x vy| / |vy| = sin(theta)
        if lz / ly < 1e-4 {
            return Err("Los tres puntos son colineales y no definen un plano.");
        }
        let z_axis = vz / lz;
        let y_axis = z_axis.cross(x_axis).normalized();

        Ok(Self { origin, x_axis, y_axis, z_axis })
    }

    /// Move origin to `new_origin` preserving axis orientation.
    pub fn with_origin(&self, new_origin: Vec3) -> Self {
        Self {
            origin: new_origin,
            x_axis: self.x_axis,
            y_axis: self.y_axis,
            z_axis: self.z_axis,
        }
    }

    /// Transform a local point `(x, y, z)` in this UCS to WCS.
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.origin + self.x_axis * local.x + self.y_axis * local.y + self.z_axis * local.z
    }

    /// Transform a 2D local point `(x, y)` on the UCS XY plane (z = 0) to WCS.
    pub fn to_world2(&self, local: Vec2) -> Vec3 {
        self.origin + self.x_axis * local.x + self.y_axis * local.y
    }

    /// Transform a WCS point to local coordinates in this UCS.
    pub fn to_local(&self, world: Vec3) -> Vec3 {
        let d = world - self.origin;
        Vec3::new(d.dot(self.x_axis), d.dot(self.y_axis), d.dot(self.z_axis))
    }

    /// Normal vector of the UCS XY plane (the Z axis).
    pub fn normal(&self) -> Vec3 {
        self.z_axis
    }

    /// Intersect a camera ray with the UCS XY plane:
    /// Ray: P(s) = ray_origin + s * ray_dir
    /// Plane: (P - origin) · z_axis = 0
    /// Returns Some(world_point) if the ray is not parallel to the plane.
    pub fn unproject_ray(&self, ray_origin: Vec3, ray_dir: Vec3) -> Option<Vec3> {
        let denom = ray_dir.dot(self.z_axis);
        if denom.abs() <= 1e-4 {
            return None; // Parallel or near-parallel to plane
        }
        let s = (self.origin - ray_origin).dot(self.z_axis) / denom;
        let p = ray_origin + ray_dir * s;
        if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
            Some(p)
        } else {
            None
        }
    }

    /// Construct an orthonormal right-handed frame from an origin and a point on the positive X axis.
    ///
    /// Preserves the reference frame's Y and Z orientations by applying the minimal rotation
    /// that aligns `ref_frame.x_axis` with `(pt_x - origin).normalized()`.
    pub fn from_2_points(origin: Vec3, pt_x: Vec3, ref_frame: &UcsFrame) -> Result<Self, &'static str> {
        let vx = pt_x - origin;
        let lx = vx.len();
        if lx < EPS {
            return Err("El punto del eje X no puede coincidir con el origen.");
        }
        let target_x = vx / lx;
        let y_axis = rotate_minimal(ref_frame.y_axis, ref_frame.x_axis, target_x);
        let z_axis = target_x.cross(y_axis).normalized();
        let y_axis = z_axis.cross(target_x).normalized();

        Ok(Self {
            origin,
            x_axis: target_x,
            y_axis,
            z_axis,
        })
    }

    /// Construct an orthonormal right-handed frame defining a single axis direction.
    ///
    /// Resolves the other two axes deterministically and right-handed using minimal rotation
    /// relative to `ref_frame`.
    pub fn from_axis_vector(
        origin: Vec3,
        axis_vec: Vec3,
        axis: crate::camera::Axis3D,
        ref_frame: &UcsFrame,
    ) -> Result<Self, &'static str> {
        let l = axis_vec.len();
        if l < EPS {
            return Err("El vector de dirección no puede tener longitud cero.");
        }
        let target = axis_vec / l;
        match axis {
            crate::camera::Axis3D::X => {
                let y = rotate_minimal(ref_frame.y_axis, ref_frame.x_axis, target);
                let z = target.cross(y).normalized();
                let y = z.cross(target).normalized();
                Ok(Self { origin, x_axis: target, y_axis: y, z_axis: z })
            }
            crate::camera::Axis3D::Y => {
                let x = rotate_minimal(ref_frame.x_axis, ref_frame.y_axis, target);
                let z = x.cross(target).normalized();
                let x = target.cross(z).normalized();
                Ok(Self { origin, x_axis: x, y_axis: target, z_axis: z })
            }
            crate::camera::Axis3D::Z => {
                let x = rotate_minimal(ref_frame.x_axis, ref_frame.z_axis, target);
                let y = target.cross(x).normalized();
                let x = y.cross(target).normalized();
                Ok(Self { origin, x_axis: x, y_axis: y, z_axis: target })
            }
        }
    }

    /// Rotate this UCS around its local X axis by `angle_rad` radians.
    pub fn rotated_x(&self, angle_rad: f64) -> Self {
        let y_axis = rotate_around_axis(self.y_axis, self.x_axis, angle_rad);
        let z_axis = self.x_axis.cross(y_axis).normalized();
        let y_axis = z_axis.cross(self.x_axis).normalized();
        Self { origin: self.origin, x_axis: self.x_axis, y_axis, z_axis }
    }

    /// Rotate this UCS around its local Y axis by `angle_rad` radians.
    pub fn rotated_y(&self, angle_rad: f64) -> Self {
        let x_axis = rotate_around_axis(self.x_axis, self.y_axis, angle_rad);
        let z_axis = x_axis.cross(self.y_axis).normalized();
        let x_axis = self.y_axis.cross(z_axis).normalized();
        Self { origin: self.origin, x_axis, y_axis: self.y_axis, z_axis }
    }

    /// Rotate this UCS around its local Z axis by `angle_rad` radians.
    pub fn rotated_z(&self, angle_rad: f64) -> Self {
        let x_axis = rotate_around_axis(self.x_axis, self.z_axis, angle_rad);
        let y_axis = self.z_axis.cross(x_axis).normalized();
        let x_axis = y_axis.cross(self.z_axis).normalized();
        Self { origin: self.origin, x_axis, y_axis, z_axis: self.z_axis }
    }

    /// Return an AutoCAD-standard predefined UCS orientation.
    ///
    /// IMPORTANT: Standard presets are always defined strictly relative to the
    /// World Coordinate System (WCS / Universal), with origin at WCS (0,0,0).
    pub fn preset(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "top" | "plan" | "superior" => Some(Self::WORLD),
            "bottom" | "inferior" => Some(Self {
                origin: Vec3::ZERO,
                x_axis: Vec3::X,
                y_axis: -Vec3::Y,
                z_axis: -Vec3::Z,
            }),
            "front" | "frontal" => Some(Self {
                origin: Vec3::ZERO,
                x_axis: Vec3::X,
                y_axis: Vec3::Z,
                z_axis: -Vec3::Y,
            }),
            "back" | "posterior" => Some(Self {
                origin: Vec3::ZERO,
                x_axis: -Vec3::X,
                y_axis: Vec3::Z,
                z_axis: Vec3::Y,
            }),
            "left" | "izquierda" => Some(Self {
                origin: Vec3::ZERO,
                x_axis: -Vec3::Y,
                y_axis: Vec3::Z,
                z_axis: -Vec3::X,
            }),
            "right" | "derecha" => Some(Self {
                origin: Vec3::ZERO,
                x_axis: Vec3::Y,
                y_axis: Vec3::Z,
                z_axis: Vec3::X,
            }),
            _ => None,
        }
    }

    /// Align UCS to a planar face defined by at least 3 corner points.
    ///
    /// - Origin is placed at `corners[0]`.
    /// - X axis points along `corners[1] - corners[0]`.
    /// - Normal Z axis is perpendicular to the face.
    /// - Supports flipping X and flipping Y.
    pub fn align_to_face(corners: &[Vec3], flip_x: bool, flip_y: bool) -> Result<Self, &'static str> {
        if corners.len() < 3 {
            return Err("Se requieren al menos 3 vértices para definir una cara.");
        }
        let o = corners[0];
        let v1 = corners[1] - o;
        let l1 = v1.len();
        if l1 < EPS {
            return Err("La arista de la cara tiene longitud nula.");
        }
        let mut x_axis = v1 / l1;

        let v2 = corners[2] - o;
        let mut z_axis = x_axis.cross(v2);
        let lz = z_axis.len();
        if lz < 1e-6 {
            return Err("Los vértices de la cara son colineales y no definen un plano.");
        }
        z_axis = z_axis / lz;
        let mut y_axis = z_axis.cross(x_axis).normalized();

        if flip_x {
            x_axis = -x_axis;
            y_axis = -y_axis;
        }
        if flip_y {
            y_axis = -y_axis;
            z_axis = -z_axis;
        }

        Ok(Self { origin: o, x_axis, y_axis, z_axis })
    }
}

/// Rotate vector `v` by the minimal rotation mapping unit vector `from` to unit vector `to`.
pub fn rotate_minimal(v: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let dot = from.dot(to).clamp(-1.0, 1.0);
    if dot > 1.0 - 1e-12 {
        return v;
    }
    if dot < -1.0 + 1e-12 {
        // 180 degrees: find an arbitrary orthogonal axis to `from`
        let perp = if from.x.abs() < 0.9 {
            Vec3::new(0.0, -from.z, from.y)
        } else {
            Vec3::new(-from.y, from.x, 0.0)
        }
        .normalized();
        return perp * (2.0 * perp.dot(v)) - v;
    }
    let axis = from.cross(to).normalized();
    let cos_t = dot;
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    v * cos_t + axis.cross(v) * sin_t + axis * (axis.dot(v) * (1.0 - cos_t))
}

/// Rotate a vector `v` around unit axis `axis` by angle `angle_rad` radians.
pub fn rotate_around_axis(v: Vec3, axis: Vec3, angle_rad: f64) -> Vec3 {
    let cos_t = angle_rad.cos();
    let sin_t = angle_rad.sin();
    v * cos_t + axis.cross(v) * sin_t + axis * (axis.dot(v) * (1.0 - cos_t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ucs_world_properties() {
        let w = UcsFrame::world();
        assert!(w.is_world());
        assert_eq!(w.origin, Vec3::ZERO);
        assert_eq!(w.x_axis, Vec3::X);
        assert_eq!(w.y_axis, Vec3::Y);
        assert_eq!(w.z_axis, Vec3::Z);
        assert_eq!(w.to_world(Vec3::new(3.0, 4.0, 5.0)), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(w.to_local(Vec3::new(3.0, 4.0, 5.0)), Vec3::new(3.0, 4.0, 5.0));
    }

    #[test]
    fn test_ucs_from_3_points_oblique_and_non_perpendicular_y() {
        // User's example:
        // Origin: (0, 0, 5)
        // Pt X: (10, 0, 5)
        // Pt Y: (0, 10, 15) -> vector is (0, 10, 10), which is NOT perpendicular to X
        let origin = Vec3::new(0.0, 0.0, 5.0);
        let pt_x = Vec3::new(10.0, 0.0, 5.0);
        let pt_y = Vec3::new(0.0, 10.0, 15.0);

        let ucs = UcsFrame::from_3_points(origin, pt_x, pt_y).expect("from 3 points succeeds");
        assert!(!ucs.is_world());
        assert_eq!(ucs.origin, origin);

        // X axis must be (1, 0, 0)
        assert!(ucs.x_axis.near(Vec3::X, 1e-9));

        // Normal Z must be (0, -1/sqrt(2), 1/sqrt(2))
        let inv_sqrt2 = 1.0 / 2.0f64.sqrt();
        assert!(ucs.z_axis.near(Vec3::new(0.0, -inv_sqrt2, inv_sqrt2), 1e-9));

        // Orthogonal Y must be (0, 1/sqrt(2), 1/sqrt(2))
        assert!(ucs.y_axis.near(Vec3::new(0.0, inv_sqrt2, inv_sqrt2), 1e-9));

        // Strict orthogonality: X . Y == 0, X . Z == 0, Y . Z == 0
        assert!(ucs.x_axis.dot(ucs.y_axis).abs() < 1e-12);
        assert!(ucs.x_axis.dot(ucs.z_axis).abs() < 1e-12);
        assert!(ucs.y_axis.dot(ucs.z_axis).abs() < 1e-12);

        // Right-handed determinant == +1
        let det = ucs.x_axis.dot(ucs.y_axis.cross(ucs.z_axis));
        assert!((det - 1.0).abs() < 1e-12);

        // Y half-plane test: dot product with (pt_y - origin) must be strictly positive
        let vy = pt_y - origin;
        assert!(ucs.y_axis.dot(vy) > 0.0);

        // Roundtrip coordinate transformation
        let local_pt = Vec3::new(4.0, 6.0, 0.0);
        let world_pt = ucs.to_world(local_pt);
        let back_local = ucs.to_local(world_pt);
        assert!(back_local.near(local_pt, 1e-9));
    }

    #[test]
    fn test_ucs_from_3_points_rejects_coincident_and_collinear() {
        let o = Vec3::new(1.0, 2.0, 3.0);
        // Coincident X
        assert!(UcsFrame::from_3_points(o, o, Vec3::new(1.0, 5.0, 3.0)).is_err());
        // Coincident Y
        assert!(UcsFrame::from_3_points(o, Vec3::new(5.0, 2.0, 3.0), o).is_err());
        // Collinear points
        let p_x = o + Vec3::new(2.0, 0.0, 0.0);
        let p_y = o + Vec3::new(10.0, 0.0, 0.0);
        assert!(UcsFrame::from_3_points(o, p_x, p_y).is_err());
    }

    #[test]
    fn test_ucs_move_origin_preserves_axes() {
        let u = UcsFrame::from_3_points(Vec3::new(0.0, 0.0, 5.0), Vec3::new(10.0, 0.0, 5.0), Vec3::new(0.0, 10.0, 15.0)).unwrap();
        let new_o = Vec3::new(100.0, -50.0, 25.0);
        let moved = u.with_origin(new_o);
        assert_eq!(moved.origin, new_o);
        assert_eq!(moved.x_axis, u.x_axis);
        assert_eq!(moved.y_axis, u.y_axis);
        assert_eq!(moved.z_axis, u.z_axis);
    }

    #[test]
    fn test_unproject_ray() {
        let ucs = UcsFrame::from_3_points(Vec3::new(0.0, 0.0, 5.0), Vec3::new(10.0, 0.0, 5.0), Vec3::new(0.0, 10.0, 15.0)).unwrap();
        // A ray straight down from (0, 0, 20) with dir (0, 0, -1)
        let ray_orig = Vec3::new(0.0, 0.0, 20.0);
        let ray_dir = Vec3::new(0.0, 0.0, -1.0);
        let hit = ucs.unproject_ray(ray_orig, ray_dir).expect("ray hits inclined plane");
        // Hit must lie on the plane: (hit - origin) . z_axis == 0
        assert!((hit - ucs.origin).dot(ucs.z_axis).abs() < 1e-12);
        // Ray parallel to plane: normal is (0, -1/sqrt(2), 1/sqrt(2)), so ray along X (1, 0, 0) is parallel
        assert!(ucs.unproject_ray(ray_orig, Vec3::X).is_none());
    }

    #[test]
    fn test_ucs_from_2_points() {
        let origin = Vec3::new(10.0, 20.0, 0.0);
        let pt_x = Vec3::new(10.0, 30.0, 0.0); // Points in +Y of WCS
        let ucs = UcsFrame::from_2_points(origin, pt_x, &UcsFrame::WORLD).expect("2 points succeeds");
        assert_eq!(ucs.origin, origin);
        assert!(ucs.x_axis.near(Vec3::Y, 1e-9));
        // Strict orthonormality and right-handedness
        assert!(ucs.x_axis.dot(ucs.y_axis).abs() < 1e-12);
        assert!(ucs.x_axis.dot(ucs.z_axis).abs() < 1e-12);
        assert!(ucs.y_axis.dot(ucs.z_axis).abs() < 1e-12);
        let det = ucs.x_axis.dot(ucs.y_axis.cross(ucs.z_axis));
        assert!((det - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_ucs_from_axis_vector() {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let z_dir = Vec3::new(0.0, 1.0, 1.0);
        let ucs = UcsFrame::from_axis_vector(origin, z_dir, crate::camera::Axis3D::Z, &UcsFrame::WORLD)
            .expect("Z axis succeeds");
        assert!(ucs.z_axis.near(z_dir.normalized(), 1e-9));
        assert!(ucs.x_axis.dot(ucs.y_axis).abs() < 1e-12);
        assert!(ucs.x_axis.dot(ucs.z_axis).abs() < 1e-12);
        let det = ucs.x_axis.dot(ucs.y_axis.cross(ucs.z_axis));
        assert!((det - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_ucs_rotations() {
        let u = UcsFrame::WORLD;
        let rot_x = u.rotated_x(std::f64::consts::FRAC_PI_2);
        assert!(rot_x.x_axis.near(Vec3::X, 1e-9));
        assert!(rot_x.y_axis.near(Vec3::Z, 1e-9));
        assert!(rot_x.z_axis.near(-Vec3::Y, 1e-9));

        let rot_z = u.rotated_z(std::f64::consts::PI);
        assert!(rot_z.x_axis.near(-Vec3::X, 1e-9));
        assert!(rot_z.y_axis.near(-Vec3::Y, 1e-9));
        assert!(rot_z.z_axis.near(Vec3::Z, 1e-9));
    }

    #[test]
    fn test_ucs_presets_are_wcs_referenced() {
        let top = UcsFrame::preset("top").expect("top preset");
        assert!(top.is_world());

        let front = UcsFrame::preset("front").expect("front preset");
        assert_eq!(front.origin, Vec3::ZERO);
        assert!(front.x_axis.near(Vec3::X, 1e-9));
        assert!(front.y_axis.near(Vec3::Z, 1e-9));
        assert!(front.z_axis.near(-Vec3::Y, 1e-9));
    }

    #[test]
    fn test_ucs_align_to_face() {
        let corners = [
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::new(5.0, 0.0, 10.0),
            Vec3::new(5.0, 5.0, 10.0),
            Vec3::new(0.0, 5.0, 10.0),
        ];
        let ucs = UcsFrame::align_to_face(&corners, false, false).expect("face align");
        assert_eq!(ucs.origin, corners[0]);
        assert!(ucs.x_axis.near(Vec3::X, 1e-9));
        assert!(ucs.y_axis.near(Vec3::Y, 1e-9));
        assert!(ucs.z_axis.near(Vec3::Z, 1e-9));

        let flipped = UcsFrame::align_to_face(&corners, true, false).expect("face align flip x");
        assert!(flipped.x_axis.near(-Vec3::X, 1e-9));
        assert!(flipped.y_axis.near(-Vec3::Y, 1e-9));
    }
}

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
}

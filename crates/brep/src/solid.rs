//! Evaluated B-Rep solid model with strict error handling and panicking protection.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use cadcraft_geom::Vec3;
use truck_meshalgo::prelude::*;
use truck_modeling::cgmath::{Point3, Rad, Vector3};
use truck_modeling::{Face, Wire, builder};
use truck_stepio::out::{StepDesign, StepDisplay, StepHeaderDescriptor, StepModel};

use crate::bounds::Bounds3;
use crate::error::BrepError;
use crate::mesh::TriangleMesh;
use crate::snapshot::BrepSnapshot;

/// Minimum safe chordal tolerance for meshing to prevent numerical divergence
/// in the Newton-Raphson intersection curve solver.
pub const MIN_SAFE_MESH_TOLERANCE: f64 = 0.01;

/// Options for boolean subtraction operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubtractOptions {
    /// Desired tolerance for boolean surface intersection.
    pub tolerance: f64,
}

impl Default for SubtractOptions {
    fn default() -> Self {
        SubtractOptions { tolerance: 0.05 }
    }
}

/// An evaluated 3D B-Rep solid body.
///
/// Encapsulates exact boundary representation (NURBS and analytical surfaces)
/// behind a pure CADCraft interface, completely hiding internal kernel types.
#[derive(Clone, Debug)]
pub struct BrepSolid {
    pub(crate) inner: truck_modeling::Solid,
}

impl BrepSolid {
    /// Create a box (cuboid) from an origin corner and extent vector.
    pub fn box_dimensions(origin: Vec3, size: Vec3) -> Result<Self, BrepError> {
        if !origin.is_finite() || !size.is_finite() {
            return Err(BrepError::InvalidGeometry("origin and size must be finite".into()));
        }
        if size.x <= 1e-9 || size.y <= 1e-9 || size.z <= 1e-9 {
            return Err(BrepError::InvalidGeometry(format!(
                "box dimensions must be strictly positive, got size=({}, {}, {})",
                size.x, size.y, size.z
            )));
        }

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let v = builder::vertex(Point3::new(origin.x, origin.y, origin.z));
            let e = builder::tsweep(&v, Vector3::new(size.x, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, size.y, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, size.z))
        }));

        match res {
            Ok(solid) => Ok(BrepSolid { inner: solid }),
            Err(_) => Err(BrepError::KernelPanic("panic while constructing box".into())),
        }
    }

    /// Create a cylinder given center of base, radius, height, and axis vector.
    pub fn cylinder(center: Vec3, radius: f64, height: f64, axis: Vec3) -> Result<Self, BrepError> {
        if !center.is_finite() || !axis.is_finite() || !radius.is_finite() || !height.is_finite() {
            return Err(BrepError::InvalidGeometry("cylinder parameters must be finite".into()));
        }
        if radius <= 1e-9 {
            return Err(BrepError::InvalidGeometry(format!("radius must be positive, got {radius}")));
        }
        if height <= 1e-9 {
            return Err(BrepError::InvalidGeometry(format!("height must be positive, got {height}")));
        }
        if axis.len() <= 1e-9 {
            return Err(BrepError::InvalidGeometry("cylinder axis vector must be non-zero".into()));
        }

        let axis_unit = axis.normalized();
        // Compute an arbitrary orthogonal vector for the base radius vertex
        let perp = if axis_unit.z.abs() < 0.9 {
            axis_unit.cross(Vec3::new(0.0, 0.0, 1.0)).normalized()
        } else {
            axis_unit.cross(Vec3::new(1.0, 0.0, 0.0)).normalized()
        };

        let p_start = center + perp * radius;
        let p_center = center;
        let sweep_vec = axis_unit * height;

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let v = builder::vertex(Point3::new(p_start.x, p_start.y, p_start.z));
            let circle_wire = builder::rsweep(
                &v,
                Point3::new(p_center.x, p_center.y, p_center.z),
                Vector3::new(axis_unit.x, axis_unit.y, axis_unit.z),
                Rad(std::f64::consts::TAU),
                4,
            );
            let cap =
                builder::try_attach_plane(&[circle_wire]).map_err(|e| BrepError::InvalidGeometry(format!("failed to attach cylinder cap: {e:?}")))?;
            let solid = builder::tsweep(&cap, Vector3::new(sweep_vec.x, sweep_vec.y, sweep_vec.z));
            Ok(BrepSolid { inner: solid })
        }));

        match res {
            Ok(r) => r,
            Err(_) => Err(BrepError::KernelPanic("panic while constructing cylinder".into())),
        }
    }

    /// Extrude a closed 3D polygon profile along a direction vector.
    pub fn extrude(points: &[Vec3], direction: Vec3) -> Result<Self, BrepError> {
        if points.len() < 3 {
            return Err(BrepError::InvalidGeometry(format!("extrusion profile requires at least 3 vertices, got {}", points.len())));
        }
        if !direction.is_finite() || direction.len() <= 1e-9 {
            return Err(BrepError::InvalidGeometry("extrusion direction must be non-zero and finite".into()));
        }
        for (i, p) in points.iter().enumerate() {
            if !p.is_finite() {
                return Err(BrepError::InvalidGeometry(format!("vertex {i} is not finite: {p:?}")));
            }
        }

        // 1. Compute Newell's polygon normal
        let n = points.len();
        let mut normal = Vec3::ZERO;
        for i in 0..n {
            let p0 = match points.get(i) {
                Some(p) => *p,
                None => continue,
            };
            let p1 = match points.get((i.saturating_add(1)) % n) {
                Some(p) => *p,
                None => continue,
            };
            normal.x += (p0.y - p1.y) * (p0.z + p1.z);
            normal.y += (p0.z - p1.z) * (p0.x + p1.x);
            normal.z += (p0.x - p1.x) * (p0.y + p1.y);
        }
        let normal_len = normal.len();
        if normal_len <= 1e-9 {
            return Err(BrepError::InvalidGeometry("extrusion profile has degenerate zero area or collinear vertices".into()));
        }
        let unit_normal = normal.normalized();

        // 2. Verify all vertices are coplanar with the polygon plane
        let p_base = match points.first() {
            Some(p) => *p,
            None => return Err(BrepError::InvalidGeometry("empty profile".into())),
        };
        for (i, p) in points.iter().enumerate() {
            let dist = (*p - p_base).dot(unit_normal).abs();
            if dist > 1e-4 {
                return Err(BrepError::InvalidGeometry(format!("vertex {i} is not coplanar with profile plane: distance {dist} exceeds tolerance")));
            }
        }

        // 3. Verify extrusion direction is not coplanar with the profile plane (which would collapse volume to zero)
        if unit_normal.dot(direction).abs() <= 1e-9 * direction.len() {
            return Err(BrepError::InvalidGeometry(
                "extrusion direction cannot be coplanar with profile plane (would create zero-volume solid)".into(),
            ));
        }

        // 4. Project into 2D plane and validate that the polygon is simple (no self-intersections or pinch points)
        let perp = if unit_normal.z.abs() < 0.9 {
            unit_normal.cross(Vec3::new(0.0, 0.0, 1.0)).normalized()
        } else {
            unit_normal.cross(Vec3::new(1.0, 0.0, 0.0)).normalized()
        };
        let u = perp;
        let v = unit_normal.cross(u);
        let pts2d: Vec<cadcraft_geom::Vec2> = points.iter().map(|p| cadcraft_geom::Vec2::new(p.dot(u), p.dot(v))).collect();

        if let Err(err_msg) = cadcraft_geom::validate_simple_polygon(&pts2d) {
            return Err(BrepError::InvalidGeometry(format!("extrusion profile self-intersects: {err_msg}")));
        }

        // Ensure base profile winding points along extrusion direction so `tsweep` produces outward normals.
        let mut ordered_pts = points.to_vec();
        if unit_normal.dot(direction) < 0.0 {
            ordered_pts.reverse();
        }

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let vertices: Vec<_> = ordered_pts.iter().map(|p| builder::vertex(Point3::new(p.x, p.y, p.z))).collect();

            let mut edges = Vec::with_capacity(vertices.len());
            for i in 0..vertices.len() {
                let next_idx = (i + 1) % vertices.len();
                edges.push(builder::line(&vertices[i], &vertices[next_idx]));
            }

            let wire = Wire::from_iter(edges);
            let face: Face =
                builder::try_attach_plane(&[wire]).map_err(|e| BrepError::InvalidGeometry(format!("profile is not planar or valid: {e:?}")))?;

            let solid = builder::tsweep(&face, Vector3::new(direction.x, direction.y, direction.z));
            Ok(BrepSolid { inner: solid })
        }));

        match res {
            Ok(r) => r,
            Err(_) => Err(BrepError::KernelPanic("panic while extruding profile".into())),
        }
    }

    /// Compute the boolean union of two solids.
    pub fn union(&self, other: &BrepSolid) -> Result<BrepSolid, BrepError> {
        let tolerance = 0.05;
        let self_clone = self.inner.clone();
        let other_clone = other.inner.clone();

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| truck_shapeops::or(&self_clone, &other_clone, tolerance)));

        match res {
            Ok(Some(solid)) => Ok(BrepSolid { inner: solid }),
            Ok(None) => Err(BrepError::NonTransversalIntersection("boolean union failed: faces are coincident, tangent, or non-transversal".into())),
            Err(_) => Err(BrepError::KernelPanic("panic in boolean union".into())),
        }
    }

    /// Compute the boolean difference (self minus tool).
    pub fn subtract(&self, tool: &BrepSolid, options: SubtractOptions) -> Result<BrepSolid, BrepError> {
        let tol = if options.tolerance.is_finite() && options.tolerance > 1e-9 { options.tolerance } else { 0.05 };

        let self_clone = self.inner.clone();
        let mut tool_clone = tool.inner.clone();

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tool_clone.not();
            truck_shapeops::and(&self_clone, &tool_clone, tol)
        }));

        match res {
            Ok(Some(solid)) => Ok(BrepSolid { inner: solid }),
            Ok(None) => Err(BrepError::NonTransversalIntersection(
                "boolean difference failed: tool and target have coplanar or non-transversal faces. \
                 Ensure cutting tools extend through boundaries (through-cut tool oversizing)."
                    .into(),
            )),
            Err(_) => Err(BrepError::KernelPanic("panic in boolean difference".into())),
        }
    }

    /// Compute the boolean intersection of two solids.
    pub fn intersect(&self, other: &BrepSolid) -> Result<BrepSolid, BrepError> {
        let tolerance = 0.05;
        let self_clone = self.inner.clone();
        let other_clone = other.inner.clone();

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| truck_shapeops::and(&self_clone, &other_clone, tolerance)));

        match res {
            Ok(Some(solid)) => Ok(BrepSolid { inner: solid }),
            Ok(None) => {
                Err(BrepError::NonTransversalIntersection("boolean intersection failed: faces are coincident, tangent, or non-transversal".into()))
            }
            Err(_) => Err(BrepError::KernelPanic("panic in boolean intersection".into())),
        }
    }

    /// Translate the solid rigidly by a 3D displacement vector without topology reconstruction.
    pub fn translate(&self, delta: Vec3) -> Result<BrepSolid, BrepError> {
        if !delta.is_finite() {
            return Err(BrepError::InvalidGeometry("translation vector must be finite".into()));
        }

        let self_clone = self.inner.clone();
        let res =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder::translated(&self_clone, Vector3::new(delta.x, delta.y, delta.z))));

        match res {
            Ok(solid) => Ok(BrepSolid { inner: solid }),
            Err(_) => Err(BrepError::KernelPanic("panic during rigid translation".into())),
        }
    }

    /// Number of connected shell boundaries in the B-Rep solid.
    pub fn boundaries_count(&self) -> usize {
        self.inner.boundaries().len()
    }

    /// Whether the B-Rep solid has zero boundary shells (empty solid).
    pub fn is_empty(&self) -> bool {
        self.inner.boundaries().is_empty()
    }

    /// Compute the 3D bounding box by triangulating with a coarse tolerance.
    pub fn bounding_box(&self) -> Bounds3 {
        match self.triangulate(0.1) {
            Ok(mesh) => mesh.bounding_box(),
            Err(_) => Bounds3::EMPTY,
        }
    }

    /// Compute volume by fine tessellation divergence theorem.
    pub fn volume(&self) -> Result<f64, BrepError> {
        let mesh = self
            .triangulate(0.02)
            .or_else(|_| self.triangulate(0.1))
            .or_else(|_| self.triangulate(0.25))?;
        Ok(mesh.signed_volume().abs())
    }

    /// Generate a 3DFACE triangulated mesh approximation for standard DXF export.
    pub fn to_face3d_mesh(&self, tolerance: f64) -> Result<Vec<cadcraft_doc::Face3d>, BrepError> {
        let mesh = self.triangulate(tolerance)?;
        Ok(mesh.to_face3d_mesh())
    }

    /// Generate a triangle mesh approximation with the specified chordal tolerance.
    pub fn triangulate(&self, tolerance: f64) -> Result<TriangleMesh, BrepError> {
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err(BrepError::InvalidGeometry(format!("meshing tolerance must be positive and finite, got {tolerance}")));
        }

        // Controlled protection against Newton convergence divergence in upstream intersection curves
        if tolerance < MIN_SAFE_MESH_TOLERANCE {
            return Err(BrepError::NumericalConvergenceFailed(format!(
                "tolerance {tolerance} is finer than safe limit {MIN_SAFE_MESH_TOLERANCE}; \
                 kernel intersection solver may fail to converge"
            )));
        }

        let self_clone = self.inner.clone();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let poly_solid = self_clone.triangulation(tolerance);
            poly_solid.to_polygon()
        }));

        let polymesh = match res {
            Ok(m) => m,
            Err(_) => {
                return Err(BrepError::NumericalConvergenceFailed("kernel panicked during surface discretization".into()));
            }
        };

        let positions: Vec<Vec3> = polymesh.positions().iter().map(|p| Vec3::new(p.x, p.y, p.z)).collect();

        let mut triangles = Vec::new();
        let mut normals = Vec::new();

        for face in polymesh.faces().face_iter() {
            if face.len() == 3 {
                let (i0, i1, i2) = (face[0].pos, face[1].pos, face[2].pos);
                triangles.push([i0, i1, i2]);
                let p0 = if let Some(p) = positions.get(i0) { p } else { Vec3::ZERO };
                let p1 = if let Some(p) = positions.get(i1) { p } else { Vec3::ZERO };
                let p2 = if let Some(p) = positions.get(i2) { p } else { Vec3::ZERO };
                let n = (p1 - p0).cross(p2 - p0).normalized();
                normals.push(n);
            } else if face.len() == 4 {
                let (i0, i1, i2, i3) = (face[0].pos, face[1].pos, face[2].pos, face[3].pos);
                triangles.push([i0, i1, i2]);
                triangles.push([i0, i2, i3]);
                let p0 = if let Some(p) = positions.get(i0) { p } else { Vec3::ZERO };
                let p1 = if let Some(p) = positions.get(i1) { p } else { Vec3::ZERO };
                let p2 = if let Some(p) = positions.get(i2) { p } else { Vec3::ZERO };
                let p3 = if let Some(p) = positions.get(i3) { p } else { Vec3::ZERO };
                let n1 = (p1 - p0).cross(p2 - p0).normalized();
                let n2 = (p2 - p0).cross(p3 - p0).normalized();
                normals.push(n1);
                normals.push(n2);
            }
        }

        Ok(TriangleMesh { positions, triangles, normals })
    }

    /// Export the solid to binary STL file.
    pub fn export_stl_binary(&self, path: &Path, tolerance: f64) -> Result<(), BrepError> {
        let mesh = self.triangulate(tolerance)?;
        let mut file = File::create(path).map_err(|e| BrepError::IoError(e.to_string()))?;
        mesh.write_stl_binary(&mut file)
    }

    /// Export the solid to standard ISO 10303-21 STEP format.
    pub fn export_step(&self, path: &Path) -> Result<(), BrepError> {
        let self_clone = self.inner.clone();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let compressed = self_clone.compress();
            let model = StepModel::from(&compressed);
            let repr = StepDesign::from_model(model);
            let display = StepDisplay::new(StepHeaderDescriptor { organization_system: "CADCraft B-Rep".to_string(), ..Default::default() }, repr);
            display.to_string()
        }));

        let step_text = match res {
            Ok(s) => s,
            Err(_) => return Err(BrepError::StepExportError("panic during STEP model serialization".into())),
        };

        let mut file = File::create(path).map_err(|e| BrepError::IoError(e.to_string()))?;
        file.write_all(step_text.as_bytes()).map_err(|e| BrepError::IoError(e.to_string()))?;
        file.flush().map_err(|e| BrepError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Serialize this B-Rep solid into a versioned snapshot.
    pub fn to_snapshot(&self) -> Result<BrepSnapshot, BrepError> {
        let payload = serde_json::to_vec(&self.inner).map_err(|e| BrepError::SerializationError(format!("JSON serialization failed: {e}")))?;
        Ok(BrepSnapshot::new(payload))
    }

    /// Reconstruct a B-Rep solid from a versioned snapshot.
    pub fn from_snapshot(snapshot: &BrepSnapshot) -> Result<Self, BrepError> {
        snapshot.validate_compatibility()?;
        let inner: truck_modeling::Solid =
            serde_json::from_slice(&snapshot.payload).map_err(|e| BrepError::SerializationError(format!("JSON deserialization failed: {e}")))?;
        Ok(BrepSolid { inner })
    }
}

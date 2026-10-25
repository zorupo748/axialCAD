//! Tessellated triangle mesh representations and STL export.

use std::collections::{HashMap, HashSet};
use std::io::Write;

use cadcraft_geom::Vec3;
use serde::{Deserialize, Serialize};

use crate::bounds::Bounds3;
use crate::error::BrepError;

/// A tessellated 3D triangle mesh.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TriangleMesh {
    /// Distinct 3D vertex positions.
    pub positions: Vec<Vec3>,
    /// Triangle indices referencing `positions`.
    pub triangles: Vec<[usize; 3]>,
    /// Normal vector for each triangle.
    pub normals: Vec<Vec3>,
}

/// Diagnostic report verifying mesh manifoldness and topological integrity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TopologyReport {
    /// Number of triangles in the mesh.
    pub triangle_count: usize,
    /// Number of unique geometric vertices after merging within tolerance.
    pub unique_vertex_count: usize,
    /// Whether all coordinates and normals are finite numbers.
    pub all_finite: bool,
    /// Number of open boundary edges (edges with exactly 1 incident face). Closed solids must have 0.
    pub open_edges: usize,
    /// Number of non-manifold edges (edges with > 2 incident faces). Manifold solids must have 0.
    pub non_manifold_edges: usize,
    /// Number of degenerate faces with zero surface area or duplicate vertex indices.
    pub degenerate_faces: usize,
    /// Whether triangle face windings/normal orientations are mutually consistent and closed.
    pub consistent_orientation: bool,
    /// Signed volume of the mesh computed via the divergence theorem.
    pub signed_volume: f64,
}

impl TriangleMesh {
    /// Check whether all vertex coordinates and normals contain strictly finite numbers.
    pub fn is_finite(&self) -> bool {
        self.positions.iter().all(|p| p.is_finite()) && self.normals.iter().all(|n| n.is_finite())
    }

    /// Compute the 3D axis-aligned bounding box.
    pub fn bounding_box(&self) -> Bounds3 {
        let mut b = Bounds3::EMPTY;
        for p in &self.positions {
            b.add_point(*p);
        }
        b
    }

    /// Convert tessellated triangles into standard 3DFACE representations.
    pub fn to_face3d_mesh(&self) -> Vec<cadcraft_doc::Face3d> {
        self.triangles
            .iter()
            .filter_map(|tri| {
                let p0 = *self.positions.get(tri[0])?;
                let p1 = *self.positions.get(tri[1])?;
                let p2 = *self.positions.get(tri[2])?;
                Some(cadcraft_doc::Face3d {
                    corners: [p0, p1, p2, p2],
                    hidden_edges: 0,
                })
            })
            .collect()
    }

    /// Compute the signed volume using the divergence theorem over oriented planar triangles.
    ///
    /// For a closed surface with outward-pointing normals, this returns the enclosed volume.
    pub fn signed_volume(&self) -> f64 {
        let mut total = 0.0;
        for tri in &self.triangles {
            let p0 = match self.positions.get(tri[0]) {
                Some(p) => *p,
                None => continue,
            };
            let p1 = match self.positions.get(tri[1]) {
                Some(p) => *p,
                None => continue,
            };
            let p2 = match self.positions.get(tri[2]) {
                Some(p) => *p,
                None => continue,
            };

            // 1/6 * det([p0, p1, p2])
            let det = p0.x * (p1.y * p2.z - p1.z * p2.y)
                    - p0.y * (p1.x * p2.z - p1.z * p2.x)
                    + p0.z * (p1.x * p2.y - p1.y * p2.x);
            total += det / 6.0;
        }
        total
    }

    /// Analyze topology and manifoldness by quantizing and unifying vertices within `merge_tol`.
    pub fn analyze_topology(&self, merge_tol: f64) -> TopologyReport {
        let tol = if merge_tol.is_finite() && merge_tol > 1e-12 {
            merge_tol
        } else {
            1e-4
        };

        let quant = |p: Vec3| -> (i64, i64, i64) {
            (
                (p.x / tol).round() as i64,
                (p.y / tol).round() as i64,
                (p.z / tol).round() as i64,
            )
        };

        let mut vertex_lut: HashMap<(i64, i64, i64), usize> = HashMap::new();
        let mut unified_indices: Vec<[usize; 3]> = Vec::with_capacity(self.triangles.len());

        for tri in &self.triangles {
            let mut idxs = [0usize; 3];
            for (i, &pos_idx) in tri.iter().enumerate() {
                let p = self.positions.get(pos_idx).copied().unwrap_or(Vec3::ZERO);
                let key = quant(p);
                let next_id = vertex_lut.len();
                let id = *vertex_lut.entry(key).or_insert(next_id);
                idxs[i] = id;
            }
            unified_indices.push(idxs);
        }

        let mut report = TopologyReport {
            triangle_count: self.triangles.len(),
            unique_vertex_count: vertex_lut.len(),
            all_finite: self.is_finite(),
            signed_volume: self.signed_volume(),
            ..Default::default()
        };

        let mut edge_counts: HashMap<(usize, usize), usize> = HashMap::new();
        let mut directed_edges: HashSet<(usize, usize)> = HashSet::new();
        let mut orientation_ok = true;

        for idxs in &unified_indices {
            let (i0, i1, i2) = (idxs[0], idxs[1], idxs[2]);
            if i0 == i1 || i1 == i2 || i2 == i0 {
                report.degenerate_faces = report.degenerate_faces.saturating_add(1);
                continue;
            }

            let edges = [(i0, i1), (i1, i2), (i2, i0)];
            for (a, b) in edges {
                let canon = if a < b { (a, b) } else { (b, a) };
                let count = edge_counts.entry(canon).or_insert(0);
                *count = count.saturating_add(1);

                if !directed_edges.insert((a, b)) {
                    orientation_ok = false;
                }
            }
        }

        for &cnt in edge_counts.values() {
            if cnt == 1 {
                report.open_edges = report.open_edges.saturating_add(1);
            } else if cnt > 2 {
                report.non_manifold_edges = report.non_manifold_edges.saturating_add(1);
            }
        }

        report.consistent_orientation = orientation_ok && (report.open_edges == 0);
        report
    }

    /// Write binary STL format to a stream.
    pub fn write_stl_binary<W: Write>(&self, w: &mut W) -> Result<(), BrepError> {
        let mut header = [0u8; 80];
        let title = b"CADCraft B-Rep Binary STL Export";
        header[..title.len()].copy_from_slice(title);
        w.write_all(&header).map_err(|e| BrepError::IoError(e.to_string()))?;

        let tri_count = u32::try_from(self.triangles.len())
            .map_err(|_| BrepError::InvalidGeometry("triangle count exceeds u32 limit".into()))?;
        w.write_all(&tri_count.to_le_bytes())
            .map_err(|e| BrepError::IoError(e.to_string()))?;

        for (idx, tri) in self.triangles.iter().enumerate() {
            let normal = self.normals.get(idx).copied().unwrap_or(Vec3::ZERO);
            let p0 = self.positions.get(tri[0]).copied().unwrap_or(Vec3::ZERO);
            let p1 = self.positions.get(tri[1]).copied().unwrap_or(Vec3::ZERO);
            let p2 = self.positions.get(tri[2]).copied().unwrap_or(Vec3::ZERO);

            let mut buf = [0u8; 50];
            buf[0..4].copy_from_slice(&(normal.x as f32).to_le_bytes());
            buf[4..8].copy_from_slice(&(normal.y as f32).to_le_bytes());
            buf[8..12].copy_from_slice(&(normal.z as f32).to_le_bytes());

            buf[12..16].copy_from_slice(&(p0.x as f32).to_le_bytes());
            buf[16..20].copy_from_slice(&(p0.y as f32).to_le_bytes());
            buf[20..24].copy_from_slice(&(p0.z as f32).to_le_bytes());

            buf[24..28].copy_from_slice(&(p1.x as f32).to_le_bytes());
            buf[28..32].copy_from_slice(&(p1.y as f32).to_le_bytes());
            buf[32..36].copy_from_slice(&(p1.z as f32).to_le_bytes());

            buf[36..40].copy_from_slice(&(p2.x as f32).to_le_bytes());
            buf[40..44].copy_from_slice(&(p2.y as f32).to_le_bytes());
            buf[44..48].copy_from_slice(&(p2.z as f32).to_le_bytes());

            // Attribute byte count = 0
            buf[48..50].copy_from_slice(&0u16.to_le_bytes());

            w.write_all(&buf).map_err(|e| BrepError::IoError(e.to_string()))?;
        }
        w.flush().map_err(|e| BrepError::IoError(e.to_string()))?;
        Ok(())
    }
}

//! Persistent 3D solid entity schema and parametric recipe contract.
//!
//! # Schema & Persistence Contract
//!
//! - **Schema Version**: Current persistent schema version is `SOLID3D_SCHEMA_VERSION = 1`.
//! - **Primitives & Operations**:
//!   - `Box { origin: Vec3, size: Vec3 }`: axis-aligned prismatic cuboid.
//!   - `Cylinder { center: Vec3, radius: f64, height: f64, axis: Vec3 }`: cylindrical solid.
//!   - `Extrusion { profile: Vec<Vec3>, direction: Vec3 }`: closed planar polygon swept linearly.
//!   - `Boolean { kind: BooleanKind, target_id: u64, tool_id: u64, oversized: bool }`: boolean operation.
//!   - `Translation { target_id: u64, delta: Vec3 }`: rigid translation of an antecedent solid.
//! - **Cross-Operation References**:
//!   Each feature node carries a monotonically assigned `id: u64`. Operations that transform or
//!   combine features reference preceding features via `target_id` and `tool_id`.
//! - **Reconstruction**:
//!   Parametric evaluation executes the sequence in topological order (or sequential list order
//!   maintaining antecedent dependencies). An evaluated B-Rep solid is reconstructed at each step.
//! - **Evolution & Deprecation / Unsupported Versions**:
//!   If a file contains an unsupported schema version (`schema_version > SOLID3D_SCHEMA_VERSION`)
//!   or an unrecognized operation tag, the document preserves the raw structure and flags
//!   the entity status as `Solid3dStatus::Failed("unsupported schema version or operation")`
//!   instead of failing deserialization or panicking.

use cadcraft_geom::Vec3;
use serde::{Deserialize, Serialize};

/// Current persistent schema version for 3D solids.
pub const SOLID3D_SCHEMA_VERSION: u32 = 1;

/// Type of boolean combination between 3D solids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BooleanKind {
    /// Additive union of solid volumes.
    Union,
    /// Subtraction of tool solid volume from target solid volume.
    Difference,
    /// Intersection retaining only the shared solid volume.
    Intersection,
}

/// A 3D parametric feature or construction operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FeatureOp {
    /// Prismatic rectangular cuboid.
    Box {
        /// Lower origin corner (x, y, z).
        origin: Vec3,
        /// Size extents along (dx, dy, dz).
        size: Vec3,
    },
    /// Cylindrical primitive.
    Cylinder {
        /// Center of the base face.
        center: Vec3,
        /// Radius of the circular cross-section.
        radius: f64,
        /// Extrusion height along axis.
        height: f64,
        /// Extrusion axis vector.
        axis: Vec3,
    },
    /// Linear sweep of a closed 2D/3D polygon.
    Extrusion {
        /// Closed boundary vertex sequence.
        profile: Vec<Vec3>,
        /// Extrusion direction and length vector.
        direction: Vec3,
    },
    /// Boolean combination of two antecedent features.
    Boolean {
        /// Kind of boolean combination.
        kind: BooleanKind,
        /// Identifier of the target solid.
        target_id: u64,
        /// Identifier of the tool solid.
        tool_id: u64,
        /// Whether tool oversizing was applied to prevent coplanar facet failures.
        oversized: bool,
    },
    /// Rigid body translation of an antecedent feature.
    Translation {
        /// Identifier of the target solid.
        target_id: u64,
        /// Displacement vector.
        delta: Vec3,
    },
}

/// A node in the parametric feature tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureNode {
    /// Unique feature identifier within the recipe.
    pub id: u64,
    /// Operation definition.
    pub op: FeatureOp,
}

/// Parametric recipe representing the construction history of a 3D model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Ordered sequence of feature operations.
    pub features: Vec<FeatureNode>,
}

impl Recipe {
    /// Create an empty recipe.
    pub fn new() -> Self {
        Recipe { features: Vec::new() }
    }

    /// Append a feature to the recipe.
    pub fn add_feature(&mut self, id: u64, op: FeatureOp) {
        self.features.push(FeatureNode { id, op });
    }

    /// Returns the maximum feature ID present, or 0 if empty.
    pub fn max_id(&self) -> u64 {
        self.features.iter().map(|f| f.id).max().unwrap_or(0)
    }

    /// Allocates the next sequential feature ID.
    pub fn next_id(&self) -> u64 {
        self.max_id().saturating_add(1)
    }
}

/// 3D axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds3 {
    /// Minimum corner (x, y, z).
    pub min: Vec3,
    /// Maximum corner (x, y, z).
    pub max: Vec3,
}

impl Default for Bounds3 {
    fn default() -> Self {
        Bounds3::EMPTY
    }
}

impl Bounds3 {
    /// An empty bounding box where `min > max`.
    pub const EMPTY: Bounds3 = Bounds3 {
        min: Vec3 {
            x: f64::INFINITY,
            y: f64::INFINITY,
            z: f64::INFINITY,
        },
        max: Vec3 {
            x: f64::NEG_INFINITY,
            y: f64::NEG_INFINITY,
            z: f64::NEG_INFINITY,
        },
    };

    /// Create from two opposite corners.
    pub fn new(a: Vec3, b: Vec3) -> Self {
        Bounds3 {
            min: Vec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            max: Vec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        }
    }

    /// True if the bounding box has non-negative volume.
    pub fn is_empty(&self) -> bool {
        !(self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z)
    }

    /// Add a point to the bounding box.
    pub fn add_point(&mut self, p: Vec3) {
        if p.is_finite() {
            self.min.x = self.min.x.min(p.x);
            self.min.y = self.min.y.min(p.y);
            self.min.z = self.min.z.min(p.z);
            self.max.x = self.max.x.max(p.x);
            self.max.y = self.max.y.max(p.y);
            self.max.z = self.max.z.max(p.z);
        }
    }

    /// Union with another bounding box.
    pub fn union(&self, other: &Bounds3) -> Bounds3 {
        if self.is_empty() {
            *other
        } else if other.is_empty() {
            *self
        } else {
            Bounds3 {
                min: Vec3::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y), self.min.z.min(other.min.z)),
                max: Vec3::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y), self.max.z.max(other.max.z)),
            }
        }
    }

    /// Size extents (dx, dy, dz).
    pub fn size(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            self.max - self.min
        }
    }

    /// Center of the bounding box.
    pub fn center(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            (self.min + self.max) * 0.5
        }
    }
}

/// Evaluation and validity state of a 3D solid entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Solid3dStatus {
    /// Evaluated successfully and geometric representation is up to date.
    Evaluated,
    /// Pending evaluation (e.g. freshly deserialized or deferred).
    Pending,
    /// Evaluation failed with structured error reason.
    Failed(String),
}

/// Persistent 3D solid entity stored in the drawing database.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Solid3d {
    /// Persistence schema version.
    pub schema_version: u32,
    /// Parametric recipe / construction history.
    pub recipe: Recipe,
    /// 3D Axis-aligned bounding box.
    pub bbox: Bounds3,
    /// Current geometric evaluation status.
    pub status: Solid3dStatus,
    /// Cached 3DFACE triangulated mesh representation for standard rendering and DXF export.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<crate::Face3d>,
}

impl Solid3d {
    /// Create a new 3D solid entity from a parametric recipe.
    pub fn new(recipe: Recipe) -> Self {
        Solid3d {
            schema_version: SOLID3D_SCHEMA_VERSION,
            recipe,
            bbox: Bounds3::EMPTY,
            status: Solid3dStatus::Pending,
            faces: Vec::new(),
        }
    }

    /// Create with an evaluated bounding box.
    pub fn with_bbox(recipe: Recipe, bbox: Bounds3) -> Self {
        Solid3d {
            schema_version: SOLID3D_SCHEMA_VERSION,
            recipe,
            bbox,
            status: Solid3dStatus::Evaluated,
            faces: Vec::new(),
        }
    }

    /// Create with an evaluated bounding box and pre-computed mesh faces.
    pub fn with_faces(recipe: Recipe, bbox: Bounds3, faces: Vec<crate::Face3d>) -> Self {
        Solid3d {
            schema_version: SOLID3D_SCHEMA_VERSION,
            recipe,
            bbox,
            status: Solid3dStatus::Evaluated,
            faces,
        }
    }

    /// Append a translation operation to the solid recipe and update the bounding box and faces.
    pub fn translate(&mut self, delta: Vec3) {
        let last_id = self.recipe.features.last().map(|f| f.id).unwrap_or(1);
        let next_id = self.recipe.next_id();
        self.recipe.add_feature(next_id, FeatureOp::Translation { target_id: last_id, delta });
        if !self.bbox.is_empty() {
            self.bbox.min = self.bbox.min + delta;
            self.bbox.max = self.bbox.max + delta;
        }
        if !self.faces.is_empty() {
            for f in &mut self.faces {
                for corner in &mut f.corners {
                    *corner = *corner + delta;
                }
            }
        }
    }
}

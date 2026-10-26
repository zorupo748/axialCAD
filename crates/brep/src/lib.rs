//! CADCraft B-Rep: Isolated boundary representation and 3D geometric modeling kernel.
//!
//! Provides a safe, pure-Rust CADCraft-native facade over the underlying `truck`
//! geometric modeling kernel, completely encapsulating kernel-specific types and
//! protecting against panics with recoverable errors.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod bounds;
pub mod error;
pub mod mesh;
pub mod recipe;
pub mod snapshot;
pub mod solid;

#[cfg(test)]
mod tests;

pub use bounds::Bounds3;
pub use error::BrepError;
pub use mesh::{TopologyReport, TriangleMesh};
pub use recipe::{BooleanKind, EvaluateRecipe, FeatureNode, FeatureOp, Recipe, SOLID3D_SCHEMA_VERSION, Solid3d, Solid3dStatus};
pub use snapshot::{BREP_FORMAT_VERSION, BrepSnapshot, KERNEL_REVISION};
pub use solid::{BrepSolid, MIN_SAFE_MESH_TOLERANCE, SubtractOptions};
